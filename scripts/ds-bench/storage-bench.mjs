#!/usr/bin/env node
// DS-EVAL storage bench — measures the two storage families CIE actually
// runs in production, on the shapes the code actually uses.
//
//   A. JSON path  (Universe A engine store: meta.json/revs/*/snapshot.json
//                  + per-feature blobs; AND the web artifact read path
//                  web/public/data/**/*.json)
//   B. SQLite     (Universe B polyglot DB — honest shapes from
//                  cie-polyglot/src/schema.rs + ir.rs) across journal modes
//                  OFF (current) / DELETE / WAL, synchronous tuned per mode.
//   C. Concurrency probe — the "lock-free writes / readers never blocked"
//                  question: (C1) reader polling DURING an active writer;
//                  (C2) writer committing WHILE a reader holds a read tx.
//
// Usage:  node scripts/ds-bench/storage-bench.mjs [--quick]
// Output: results table on stdout + scripts/ds-bench/results.json

import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fork } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const OUT_DIR = path.join(ROOT, "scripts", "ds-bench");
const QUICK = process.argv.includes("--quick");
const results = { env: {}, A: {}, B: {}, C: {} };

const now = () => Number(process.hrtime.bigint()) / 1e6; // ms
const ms = (x) => (x >= 100 ? x.toFixed(0) : x >= 1 ? x.toFixed(1) : x.toFixed(2));

// ---------------------------------------------------------------- env
results.env = {
  node: process.version,
  cpus: os.cpus().length,
  platform: `${os.platform()} ${os.release()}`,
  date: new Date().toISOString(),
};

// ================================================================ A. JSON
function benchJson() {
  const dataDir = path.join(ROOT, "web", "public", "data");
  const files = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".json")) files.push(p);
    }
  };
  walk(dataDir);

  // A1 — read + parse every artifact (the web read path)
  let t = now();
  const perFile = [];
  let totalBytes = 0;
  for (const f of files) {
    const t0 = now();
    const bytes = fs.readFileSync(f);
    JSON.parse(bytes.toString("utf8"));
    const dt = now() - t0;
    totalBytes += bytes.length;
    perFile.push({ file: path.relative(dataDir, f), bytes: bytes.length, parseMs: +dt.toFixed(2) });
  }
  const readAllMs = now() - t;
  perFile.sort((a, b) => b.bytes - a.bytes);
  results.A = {
    files: files.length,
    totalBytes,
    maxFileBytes: perFile[0].bytes,
    readAllParseMs: +readAllMs.toFixed(1),
    slowest: perFile.slice(0, 3),
  };
  console.log(`A1  read+parse ${files.length} artifacts (${(totalBytes / 1e6).toFixed(2)} MB): ${ms(readAllMs)} ms`);

  // A2 — single-file atomic-rename write (Store::write_json pattern),
  //      payload = the largest real artifact
  const largestAbs = path.join(dataDir, perFile[0].file);
  const big = fs.readFileSync(largestAbs).toString("utf8");
  const obj = JSON.parse(big);
  const tmp = path.join(OUT_DIR, "a2.tmp.json");
  const dst = path.join(OUT_DIR, "a2.json");
  const ROUNDS = QUICK ? 10 : 30;
  let tw = now();
  for (let i = 0; i < ROUNDS; i++) {
    const bytes = Buffer.from(JSON.stringify(obj), "utf8");
    fs.writeFileSync(tmp, bytes);
    fs.renameSync(tmp, dst);
  }
  const writeMs = now() - tw;
  results.A.atomicRenameWrite = { rounds: ROUNDS, bytes: Buffer.byteLength(big), totalMs: +writeMs.toFixed(1), perWriteMs: +(writeMs / ROUNDS).toFixed(2) };
  console.log(`A2  atomic-rename write x${ROUNDS} (${(Buffer.byteLength(big) / 1e6).toFixed(2)} MB file): ${ms(writeMs)} ms total, ${ms(writeMs / ROUNDS)} ms/write`);

  // A3 — per-record whole-file rewrite (mark_tombstone pattern: load whole
  //      file, insert one entry, rewrite whole file — per record)
  const vals = Array.isArray(obj) ? obj : Object.values(obj).find(Array.isArray);
  if (Array.isArray(vals) && vals.length > 10) {
    const n = Math.min(vals.length, 200);
    let t = now();
    const store = [];
    for (let i = 0; i < n; i++) {
      store.push(vals[i]);
      fs.writeFileSync(dst, JSON.stringify(store)); // whole-file rewrite per record
    }
    const perRecordMs = now() - t;
    results.A.perRecordRewrite = { records: n, totalMs: +perRecordMs.toFixed(1), perRecordMs: +(perRecordMs / n).toFixed(3) };
    console.log(`A3  whole-file rewrite per record x${n}: ${ms(perRecordMs)} ms total, ${ms(perRecordMs / n)} ms/record (write amplification grows O(n²) in bytes)`);
  }
  fs.rmSync(dst, { force: true });
}

// ================================================================ B. SQLite
// Honest shapes from cie-polyglot/src/schema.rs + ir.rs DDL.
const FILES = QUICK ? 1200 : 5000;
const SYMBOLS = QUICK ? 15000 : 60000;
const CALLS = QUICK ? 75000 : 300000;

const DDL = `
CREATE TABLE files (
  id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL, path TEXT NOT NULL,
  content_hash BLOB NOT NULL, language TEXT NOT NULL, revision INTEGER NOT NULL,
  size_bytes INTEGER NOT NULL, tree_sitter_grammar_version TEXT NOT NULL,
  UNIQUE (repo_id, path, revision));
CREATE INDEX idx_files_rev ON files(revision);
CREATE INDEX idx_files_hash ON files(content_hash);
CREATE TABLE symbols (
  id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL REFERENCES files(id),
  ast_node_id INTEGER NOT NULL, name TEXT NOT NULL, qual_name TEXT NOT NULL,
  kind TEXT NOT NULL, parent_symbol_id INTEGER, start_byte INTEGER NOT NULL,
  end_byte INTEGER NOT NULL, sig_end_byte INTEGER, subtree_hash BLOB NOT NULL,
  annotations TEXT);
CREATE INDEX idx_sym_file ON symbols(file_id);
CREATE INDEX idx_sym_parent ON symbols(parent_symbol_id);
CREATE INDEX idx_sym_name ON symbols(name);
CREATE INDEX idx_sym_qual ON symbols(qual_name);
CREATE INDEX idx_sym_kind ON symbols(kind);
CREATE INDEX idx_sym_span ON symbols(file_id, start_byte, end_byte);
CREATE TABLE calls (
  id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, call_node_id INTEGER NOT NULL,
  caller_symbol_id INTEGER NOT NULL, callee_symbol_id INTEGER, callee_text TEXT,
  resolution TEXT NOT NULL, stmt_index INTEGER NOT NULL, block_id INTEGER,
  call_kind TEXT NOT NULL DEFAULT 'sync', throws_set BLOB, guard_summary TEXT);
CREATE INDEX idx_calls_caller ON calls(caller_symbol_id, stmt_index);
CREATE INDEX idx_calls_callee ON calls(callee_symbol_id);
`;

function mulberry(seed) {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function buildRows() {
  const rnd = mulberry(42);
  const files = [], syms = [], calls = [];
  const langs = ["python", "typescript", "go", "rust", "java"];
  for (let f = 1; f <= FILES; f++)
    files.push([f, 1, `src/pkg${f % 200}/mod_${f}.ts`, Buffer.from(`hash${f}`), langs[f % 5], 1, 4000 + (f % 900) * 7, "tree-sitter-typescript 0.23"]);
  let sid = 0;
  const symIds = [];
  for (let f = 1; f <= FILES; f++) {
    const per = Math.round(SYMBOLS / FILES);
    for (let k = 0; k < per; k++) {
      sid++;
      symIds.push(sid);
      const base = `symbol_${sid}`;
      syms.push([sid, f, f * 100 + k, base, `com.pkg${f % 200}.mod_${f}.${base}`, k % 7 === 0 ? "class" : "function", k > 0 ? sid - 1 : null, 10 * k, 10 * k + 80, null, Buffer.from(`sub${sid}`), null]);
    }
  }
  for (let c = 1; c <= CALLS; c++) {
    const caller = symIds[(c * 7919) % symIds.length];
    const resolved = rnd() < 0.7;
    calls.push([c, (caller % FILES) + 1, caller * 3, caller, resolved ? symIds[(c * 104729) % symIds.length] : null,
      resolved ? null : `callee_text_${c}`, resolved ? "direct" : "dynamic", c % 40, null, rnd() < 0.9 ? "sync" : "async", null, rnd() < 0.1 ? "may throw" : null]);
  }
  return { files, syms, calls };
}

const MODES = [
  { name: "OFF (current)", journal: "OFF", sync: "OFF" },
  { name: "DELETE", journal: "DELETE", sync: "OFF" },
  { name: "WAL", journal: "WAL", sync: "NORMAL" },
];

function benchSqlite(rows) {
  const { files, syms, calls } = rows;
  results.B.dataset = { files: files.length, symbols: syms.length, calls: calls.length };
  results.B.modes = [];
  console.log(`B   dataset: ${files.length} files, ${syms.length} symbols, ${calls.length} calls`);

  for (const m of MODES) {
    const dbPath = path.join(OUT_DIR, `b-${m.journal}.db`);
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
    const db = new DatabaseSync(dbPath);
    db.exec(`PRAGMA journal_mode=${m.journal}; PRAGMA synchronous=${m.sync};`);
    db.exec(DDL);

    // B1 — one big transaction (bulk, like a fresh full ingest)
    let t = now();
    db.exec("BEGIN");
    const insF = db.prepare("INSERT INTO files VALUES (?,?,?,?,?,?,?,?)");
    for (const r of files) insF.run(...r);
    const insS = db.prepare("INSERT INTO symbols VALUES (?,?,?,?,?,?,?,?,?,?,?,?)");
    for (const r of syms) insS.run(...r);
    const insC = db.prepare("INSERT INTO calls VALUES (?,?,?,?,?,?,?,?,?,?,?,?)");
    for (const r of calls) insC.run(...r);
    db.exec("COMMIT");
    const bulkOneTxMs = now() - t;

    // B2 — batched transactions (30 commits — incremental ingest shape);
    //      rebuild the schema fresh, then insert calls only in 30 txs
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
    const db2 = new DatabaseSync(dbPath);
    db2.exec(`PRAGMA journal_mode=${m.journal}; PRAGMA synchronous=${m.sync};`);
    db2.exec(DDL);
    const insC2 = db2.prepare("INSERT INTO calls VALUES (?,?,?,?,?,?,?,?,?,?,?,?)");
    t = now();
    const BATCH = Math.ceil(calls.length / 30);
    for (let b = 0; b < 30; b++) {
      db2.exec("BEGIN");
      for (const r of calls.slice(b * BATCH, (b + 1) * BATCH)) insC2.run(...r);
      db2.exec("COMMIT");
    }
    const batchedMs = now() - t;

    // B3 — full-scan aggregate (duplicates-style scan over calls)
    t = now();
    db2.prepare("SELECT resolution, COUNT(*) AS n, SUM(callee_symbol_id IS NULL) AS unresolved FROM calls GROUP BY resolution").all();
    const scanMs = now() - t;

    // B4 — 10k point selects through idx_sym_qual
    const q = db2.prepare("SELECT id, qual_name, kind FROM symbols WHERE qual_name = ?");
    t = now();
    for (let i = 0; i < 10000; i++) {
      q.get(`com.pkg${i % 200}.mod_${(i % FILES) + 1}.symbol_${(i % syms.length) + 1}`);
    }
    const pointMs = now() - t;

    const sizeBytes = fs.statSync(dbPath).size + (fs.existsSync(dbPath + "-wal") ? fs.statSync(dbPath + "-wal").size : 0);
    const row = { mode: m.name, bulkOneTxMs: +bulkOneTxMs.toFixed(0), batched30TxMs: +batchedMs.toFixed(0), fullScanAggMs: +scanMs.toFixed(1), pointSelect10kMs: +pointMs.toFixed(0), dbSizeMB: +(sizeBytes / 1e6).toFixed(1) };
    results.B.modes.push(row);
    console.log(`B[${m.name.padEnd(13)}] bulk1tx=${ms(bulkOneTxMs)}ms  30tx=${ms(batchedMs)}ms  scan=${ms(scanMs)}ms  10k-pts=${ms(pointMs)}ms  size=${row.dbSizeMB}MB`);
    db.close();
    db2.close();
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
  }
}

// ================================================================ C. Concurrency
// C1: reader polls COUNT(*) every ~10ms while a writer batch-inserts.
// C2: writer tries to commit while a reader HOLDS an open read transaction.
// Orchestration: this script re-invokes itself as a child with ROLE env.

function runWriterChild(dbPath, journal, sync, batches, rowsPerBatch) {
  const db = new DatabaseSync(dbPath);
  db.exec(`PRAGMA journal_mode=${journal}; PRAGMA synchronous=${sync};`);
  db.exec("CREATE TABLE calls (id INTEGER PRIMARY KEY, caller_symbol_id INTEGER NOT NULL, callee_symbol_id INTEGER, callee_text TEXT, resolution TEXT NOT NULL, stmt_index INTEGER NOT NULL)");
  const ins = db.prepare("INSERT INTO calls VALUES (?,?,?,?,?,?)");
  for (let b = 0; b < batches; b++) {
    const t0 = now();
    db.exec("BEGIN");
    for (let i = 0; i < rowsPerBatch; i++) {
      const id = b * rowsPerBatch + i + 1;
      ins.run(id, id % 5000, id % 3 === 0 ? null : id, `callee_${id}`, id % 3 === 0 ? "dynamic" : "direct", id % 30);
    }
    db.exec("COMMIT");
    console.log(JSON.stringify({ kind: "batch", b, commitMs: +(now() - t0).toFixed(1) }));
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 25); // sleep 25ms
  }
  db.close();
  console.log(JSON.stringify({ kind: "writer-done" }));
}

function runReaderChild(dbPath, durationMs, pollMs) {
  const db = new DatabaseSync(dbPath);
  const t0 = now();
  let ok = 0, busy = 0, err = 0, last = 0, dirty = 0, maxWaitMs = 0;
  while (now() - t0 < durationMs) {
    const a = now();
    try {
      const row = db.prepare("SELECT COUNT(*) AS n FROM calls").get();
      ok++;
      if (row.n < last) dirty++;
      last = row.n;
    } catch (e) {
      if (/busy|locked/i.test(String(e.message))) busy++; else err++;
    }
    const spent = now() - a;
    if (spent > maxWaitMs) maxWaitMs = spent;
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, pollMs);
  }
  db.close();
  console.log(JSON.stringify({ kind: "reader-done", ok, busy, err, dirty, maxWaitMs: +maxWaitMs.toFixed(1), last }));
}

async function benchConcurrency() {
  const self = fileURLToPath(import.meta.url);
  const runChild = (role, dbPath, journal, sync, extra = {}) =>
    new Promise((resolve) => {
      const child = fork(self, [], { env: { ...process.env, ROLE: role, DBPATH: dbPath, JOURNAL: journal, SYNC: sync, ...extra }, silent: true });
      const events = [];
      child.stdout.on("data", (d) => {
        for (const line of d.toString().split("\n")) {
          if (!line.trim()) continue;
          try { events.push(JSON.parse(line)); } catch { /* ignore */ }
        }
      });
      child.stderr.on("data", (d) => events.push({ kind: "stderr", msg: d.toString().slice(0, 300) }));
      child.on("exit", () => resolve(events));
    });

  const sleep = (x) => new Promise((r) => setTimeout(r, x));
  results.C.probes = [];
  const BATCHES = QUICK ? 8 : 16, RPBLK = 5000, DUR = QUICK ? 3000 : 5000;

  for (const m of MODES) {
    const dbPath = path.join(OUT_DIR, `c-${m.journal}.db`);
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });

    // C1 — concurrent reader during writer
    const writerP = runChild("writer", dbPath, m.journal, m.sync, { BATCHES: String(BATCHES), RPBLK: String(RPBLK) });
    await sleep(80); // let the writer create the schema
    const readerP = runChild("reader", dbPath, m.journal, m.sync, { DUR: String(DUR), POLL: "10" });
    const [wEv, rEv] = await Promise.all([writerP, readerP]);
    const wBatches = wEv.filter((e) => e.kind === "batch");
    const r = rEv.find((e) => e.kind === "reader-done") || { ok: 0, busy: 0, err: 0, dirty: 0, maxWaitMs: -1 };
    const row = {
      probe: "C1", mode: m.name, writerBatches: wBatches.length,
      avgCommitMs: wBatches.length ? +(wBatches.reduce((s, e) => s + e.commitMs, 0) / wBatches.length).toFixed(1) : null,
      readerOk: r.ok, readerBusy: r.busy, readerErr: r.err, readerSawDirty: r.dirty, readerMaxWaitMs: r.maxWaitMs,
    };
    results.C.probes.push(row);
    console.log(`C1[${m.name.padEnd(13)}] writer ${wBatches.length} batches (avg commit ${row.avgCommitMs} ms) | reader ok=${r.ok} busy=${r.busy} err=${r.err} dirtyReads=${r.dirty} maxWait=${r.maxWaitMs}ms`);
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
  }

  // C2 — writer blocked by a HOLD-OPEN reader (rollback modes vs WAL)
  for (const m of MODES) {
    const dbPath = path.join(OUT_DIR, `c2-${m.journal}.db`);
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
    const seed = new DatabaseSync(dbPath);
    seed.exec(`PRAGMA journal_mode=${m.journal}; PRAGMA synchronous=${m.sync};`);
    seed.exec("CREATE TABLE calls (id INTEGER PRIMARY KEY, caller_symbol_id INTEGER NOT NULL, callee_symbol_id INTEGER, callee_text TEXT, resolution TEXT NOT NULL, stmt_index INTEGER NOT NULL)");
    seed.prepare("INSERT INTO calls VALUES (1,1,1,'x','direct',0)").run();
    seed.close();

    const holder = new DatabaseSync(dbPath);
    holder.exec("BEGIN");
    holder.prepare("SELECT COUNT(*) FROM calls").get(); // read tx now open + active

    const writer = new DatabaseSync(dbPath);
    writer.exec(`PRAGMA journal_mode=${m.journal}; PRAGMA synchronous=${m.sync};`);
    const t0 = now();
    let outcome = "committed";
    try {
      writer.exec("BEGIN");
      writer.prepare("INSERT INTO calls VALUES (2,2,2,'y','direct',1)").run();
      writer.exec("COMMIT");
    } catch (e) {
      outcome = String(e.message).slice(0, 80);
    }
    const writerMs = now() - t0;
    holder.exec("ROLLBACK");
    holder.close();
    writer.close();
    const row = { probe: "C2", mode: m.name, writerBlockedByReader: outcome !== "committed", outcome, writerMs: +writerMs.toFixed(1) };
    results.C.probes.push(row);
    console.log(`C2[${m.name.padEnd(13)}] writer-with-reader-holding: ${outcome} (${ms(writerMs)} ms)`);
    for (const suf of ["", "-wal", "-shm"]) fs.rmSync(dbPath + suf, { force: true });
  }
}

// ================================================================ main
const role = process.env.ROLE;
if (role === "writer") { runWriterChild(process.env.DBPATH, process.env.JOURNAL, process.env.SYNC, Number(process.env.BATCHES || 16), Number(process.env.RPBLK || 5000)); }
else if (role === "reader") { runReaderChild(process.env.DBPATH, Number(process.env.DUR), Number(process.env.POLL)); }
else {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  console.log(`DS-EVAL storage bench — node ${process.version}, ${os.cpus().length} cpus`);
  benchJson();
  benchSqlite(buildRows());
  await benchConcurrency();
  fs.writeFileSync(path.join(OUT_DIR, "results.json"), JSON.stringify(results, null, 2));
  console.log(`\nresults -> scripts/ds-bench/results.json`);
}
