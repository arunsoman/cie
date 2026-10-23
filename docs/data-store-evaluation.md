# Data-Store Evaluation — faster reads, lock-free writes, and a ≤3-type consolidation

Status: DONE (DS-EVAL, backlog item 1) · Date: 2026-10-23 · **Landed: §6 steps 1–5 implemented and verified** (WAL+NORMAL, build-then-rename, build-time dup indexes + `ensureIndexes` deleted, batched tombstones, sled off default features; receipts: readonly-WAL probe `scripts/ds-eval/verify-readonly-wal.mjs`, byte-identical viewspec bundles across runs, failure-run preserves the previous DB)
Question (user): "evaluate the data store which is the best for faster reads and lock-free writes, because cie-ingest uses various datastructures but the polyglot just puts everything onto a sql table."
Constraint (user): willing to keep **at most 3 storage solution types** if that helps overall system performance.
Method: ground-truth census of every storage family in the tree (with file:line evidence) + a measured bench on the two families that actually run in production, using the shapes the code actually writes (`scripts/ds-bench/storage-bench.mjs`, raw numbers in `scripts/ds-bench/results.json`).

---

## TL;DR verdict

**Keep exactly three storage types — and fix configuration, not engines:**

1. **SQLite in WAL mode** (`journal_mode=WAL`, `synchronous=NORMAL`) for the polyglot universe — the current `journal_mode=OFF` buys ~0–13% write speed and **loses everything that matters**: crash safety, and the only mode where writers and readers never block each other (measured: C2 probe).
2. **rkyv memory-mapped snapshots** (`.cie-snap`) for the CSR graph hot path — already zero-copy with a gated ≤10 µs warm-open; this layer already meets "lock-free reads" by construction.
3. **JSON artifacts with atomic-rename writes** for the engine store layout and the web's read-only surface copies — keep (transparency/diffability is a PRD principle), but forbid the whole-file-rewrite-per-record pattern on hot paths (measured O(n²) amplification).

**Cut:** sled (dead code in production *and* effectively unmaintained upstream), the per-record JSON rewrite misuse, and do not add LMDB/RocksDB/DuckDB — no requirement is left unmet once WAL + rename-swap land. "Lock-free writes" in CIE's single-writer architecture is a **sequencing and configuration problem, not a database-engine problem**.

---

## 1. What actually exists today (census)

### 1.1 Universe A — the engine store (`<project>/.cie`)

| Family | Where | Role |
|---|---|---|
| **JSON files, atomic-rename** | `cie-store/src/lib.rs:12-21` (layout), `write_json` at `lib.rs:115-125` (tmp + `fs::rename`) | THE production store: `meta.json`, `files.json`, `labels.json`, `tombstones.json`, `revs/{rev}/snapshot.json`, plus per-feature blobs (`stack.json`, `relations.json`, org, drift, claims, concepts) |
| **rkyv mmap snapshots** | `cie-store/src/snapshot.rs:1-20` | CSR graph per revision, zero-deserialization reads, content-addressed, warm-open gate ≤10 µs (§13 item 16) |
| **HNSW vectors** | `cie-ingest/src/vector_store.rs:1-15`, `cie-store/src/hnsw.rs` | Vectors persisted as a **flat JSON blob** `{hex(record_id) -> (text, vector)}`; the hnsw_rs index is rebuilt **in memory on every load** |
| **KvStore abstraction (sled/redb/MemKv)** | `cie-store/src/kv.rs:131-138` (trait), `:254` (sled), `:318` (redb) | TASK-024 built a one-tree-per-namespace KV with an identical round-trip suite (`kv_suite`) so backends swap with zero call-site changes — **but grep confirms zero production users**: `SledStore|RedbStore` appear only inside `cie-store` itself (source + `tests/task_024_kv.rs` + `tests/task_027_tables.rs`) |
| **L0 hot cache** | `cie-store/src/kv.rs:206-252` | DashMap + FxHashMap + quick_cache LRU, gated 50 ns path — likewise unused outside cie-store |

Two structural smells in the JSON layout, both real code paths:

- **Whole-file rewrite per record**: `mark_tombstone` (`lib.rs:165-176`) loads `tombstones.json`, inserts one path, rewrites the whole file — per deleted file. The bench (A3) measures the cost curve.
- **Full parse on every open**: reading any namespace means reading and `JSON.parse`-ing the entire file (A1).

### 1.2 Universe B — the polyglot store

| Family | Where | Role |
|---|---|---|
| **SQLite (rusqlite, bundled)** | `cie-polyglot/src/schema.rs:211-227` | 30 tables / 7 layers (files, ast_nodes, symbols, calls, ir_blocks/ir_meta, duplicates, frameworks, c4, …) |
| **`journal_mode=OFF` + `synchronous=OFF`** | `schema.rs:225-227` | Chosen for write speed. Consequence: **no rollback journal** — a crash mid-write leaves a corrupt DB, and readers/writers collide (§3, C-probes) |
| **Rebuild-per-run at a tmp path** | `web/src/lib/polyglot-db.ts:1-10`, `cie-cli/src/main.rs` Command::Polyglot | The CLI deletes any prior `<tmpdir>/cie-polyglot-<fnv1a64(path)>.db` and rebuilds from scratch ("honest recomputation"). The `files.content_hash UNIQUE` increment mechanism exists in the schema but is switched off by this delete-then-rebuild |
| **Web reader** | `polyglot-db.ts:37-52` | `node:sqlite` `DatabaseSync` opened `readOnly` — **but** `ensureIndexes()` first opens read-write and runs `CREATE INDEX IF NOT EXISTS` on the duplicates table: the read path performs **writes** |

### 1.3 The web surface

Static JSON artifacts under `web/public/data/**` — measured census: **33 files, 3.94 MB total, largest 1.27 MB** (`overlap.json`). Generated per run by the CLI, fetched over HTTP. This is a third "storage type" in the system even though it is derived data.

### 1.4 Real repo grounding (spring sample)

- `cie ingest /tmp/spring-ws` → rev 1: 13 files, 47 symbols, 41 findings, **wall 44.2 ms**; `.cie` store = 247 KB of JSON.
- `cie polyglot /tmp/spring-ws --viewspec all` → real DB 553 KB, journal OFF, content: 8 files parsed (Java only — **Kotlin/SQL/XML are invisible to the Lang enum**, the known coverage gap), 22 symbols, 23 calls, 119 duplicates, 24-view viewspec bundle (187 KB JSON).
- `cie report /tmp/spring-ws --json` reads the store (store-first path) and writes rep.json — works off the `.cie` store without re-walking sources.

---

## 2. The real access patterns (what the storage must serve)

1. **Writes are single-writer**: one CLI process per run writes the store; no concurrent writers exist in the architecture. "Lock-free writes" therefore cannot mean multi-writer transactions — it means: **a writer must never be blocked by readers, and readers must never observe a torn store**.
2. **Reads are read-many**: the 11 pipeline stages re-read artifacts; the web API routes re-read them per request; chat surfaces re-read per turn. Mixed workload — point lookups (symbol by id/qual-name) AND full-table scans (duplicates, viewspec, concepts inputs).
3. **Determinism**: byte-identical outputs gate the parity harness — any storage change must preserve content addressing (blake3) and stable ids.
4. **Crash story matters twice**: the user can kill a CLI run mid-write; and the web reads artifacts while a regeneration may run in another process.
5. **No new threads** (perf rule) — storage choices must not push concurrency into thread pools.

---

## 3. Measured bench

Script: `scripts/ds-bench/storage-bench.mjs` (persisted, re-runnable) · Raw: `scripts/ds-bench/results.json`.
Box: 2-cpu container, Node v24.21, warm OS cache. SQLite shapes are the **exact DDL** of `files`/`symbols`/`calls` from `schema.rs`/`ir.rs`; dataset is a scale model: 5,000 files / 60,000 symbols / 300,000 calls (a mid-size repo; the spring artifacts are 20× smaller — where relevant the real-artifact numbers are quoted too).

### A — the JSON path (production store + web artifacts)

| Probe | Result |
|---|---|
| A1 · read+parse all 33 real artifacts (3.94 MB) | **34.9 ms** |
| A2 · atomic-rename write of the 1.27 MB artifact ×30 | **3.6 ms per write** |
| A3 · whole-file rewrite per record ×200 | **0.80 ms per record** — and every record makes the file bigger, so the pattern is **O(n²) in bytes** |

Reading a JSON namespace costs a full parse of the file no matter how few records you need; writing one record costs rewriting every byte before it.

### B — SQLite journal modes (honest shapes, scale model)

| mode | bulk insert 1 tx | 30 batched txs | full scan+agg (calls) | 10k point selects (idx_sym_qual) | DB size |
|---|---|---|---|---|---|
| **OFF (current)** | 1534 ms | 1245 ms | 111 ms | 32.6 ms | 21.7 MB |
| DELETE | 1557 ms | 1358 ms | 110 ms | 30.6 ms | 21.7 MB |
| **WAL (+sync=NORMAL)** | 1730 ms (+13%) | 1408 ms (+13%) | 111 ms (=) | **11.4 ms (2.9× faster)** | 29.5 MB (+36%, WAL file included) |

WAL costs ~13% on the bulk write path and **returns 2.9× on point reads** (the web's per-request lookups live exactly there). Scans are unchanged. On the smaller quick dataset the same point-select ratio held (26.9 ms vs 61.1 ms).

### C — concurrency probes (the "lock-free" question)

**C1 — reader polling `COUNT(*)` every 10 ms while a writer batch-commits (16 × 5,000 rows):**

| mode | writer batches | reader ok | reader busy | dirty reads |
|---|---|---|---|---|
| OFF | 15 (avg commit 5 ms) | 471 | 0 | 0 |
| DELETE | 16 | 468 | 1 | 0 |
| WAL | 16 | 465 | 0 | 0 |

At this write volume the collision window is small — but it is not zero (DELETE caught one BUSY), and it grows with batch size.

**C2 — writer tries to commit while a reader holds an open read transaction (the structural test):**

| mode | outcome |
|---|---|
| **OFF (current)** | `database is locked` — writer fails immediately |
| DELETE | `database is locked` — writer fails immediately |
| **WAL** | **committed** (0.07 ms) |

This is the decisive result: under the current `journal_mode=OFF`, **any long-lived reader (a web route mid-query, `ensureIndexes`, a chat turn) blocks the writer's commit**; under WAL, writers and readers proceed independently — SQLite's MVCC snapshot isolation does exactly what "lock-free" needs here.

**JSON atomic-rename, for comparison, is structurally old-or-new**: `fs::rename` is atomic, so a reader either sees the complete previous file or the complete new one — never a torn read. Universe A already satisfies "readers never blocked, never torn"; its costs are the O(n²) rewrite and the full-parse read (§3-A), not concurrency.

---

## 4. Candidate matrix

| Candidate | Verdict | Evidence / reasoning |
|---|---|---|
| **SQLite WAL + synchronous=NORMAL** | **ADOPT** (Universe B) | Measured: +13% writes, 2.9× point reads, only mode where writer ≠ blocked by reader (C2) and reader ≠ blocked by writer (MVCC). Crash-safe: WAL recovery on next open (with `synchronous=NORMAL`, an app crash loses nothing; an OS crash may lose the last transactions — acceptable for a scratch store that rebuilds deterministically). Already in-tree (`rusqlite`), already read by production Node (`node:sqlite`). Zero new dependencies. |
| SQLite journal OFF (current) | REPLACE | Tie on bulk writes, 3× slower points, no crash safety (no rollback journal → corruption on mid-write kill), writer↔reader blocking (C2). |
| JSON atomic-rename (Universe A layout) | **KEEP** with discipline | 3.6 ms/write @1.27 MB, atomic old-or-new for readers, human-diffable (PRD transparency principle), zero deps. Fix the per-record rewrite misuse (A3) and accept full-parse reads (34.9 ms for the whole artifact set is nothing). |
| rkyv mmap snapshots | **KEEP** | Zero-copy graph reads, ≤10 µs warm-open gate already in the tree and tested; content-addressed; exactly the right tool for the one truly hot structure (the CSR graph). |
| sled | **CUT** | Zero production users (grep-verified); upstream effectively unmaintained (pinned `"0.34"`, last release 2021). Keeping dead, unmaintained code as a *default* backend is a liability, not an option. |
| redb | KEEP BEHIND FLAG (dormant) | In-tree, `kv_suite`-verified, maintained, single-file, MVCC. No production user today → do not promote; it is the designated fallback if a point-KV namespace ever becomes hot (e.g., vector ids). Cost: one cargo feature. |
| LMDB | REJECT (for now) | The canonical lock-free-reads store (mmap B-tree + COW), but it overlaps what WAL + rename already give CIE's single-writer pattern; adds a C dependency; cited-not-benched. Revisit only if a genuine multi-writer requirement appears. |
| RocksDB | REJECT | LSM engine for write-heavy mutable workloads; CIE is write-once/read-many; heavy dependency, poor fit. |
| DuckDB | REJECT (for now) | Columnar OLAP would only fit the duplicates/concepts scans; scans measured 111 ms at 300k rows — no pain. Revisit for cross-repo analytics if that ever exists. |

---

## 5. The recommendation, mapped to the user's question

"Which store is best for **faster reads** and **lock-free writes**?"

- **Faster reads**: WAL makes the polyglot point path **2.9× faster** (measured); Universe A's hot graph path is already zero-copy mmap; the web artifacts are static files behind HTTP caches. Nothing in the system needs a different engine to read faster.
- **Lock-free writes**: with the CLI as the single writer, the requirement decomposes into (a) writers never blocked by readers — **WAL** (measured in C2) for Universe B, already true for Universe A via atomic rename; (b) readers never see torn state — rename-swap for both universes (Universe A does it today; Universe B must adopt it, §6.2).
- **≤3 storage types**: SQLite-WAL, rkyv snapshots, JSON artifacts. Sled exits; LMDB/RocksDB/DuckDB never enter.

---

## 6. Migration sketch (ordered, each step independently shippable)

1. **Two pragmas** in `cie-polyglot/src/schema.rs::open_db`: `journal_mode=WAL`, `synchronous=NORMAL` (replacing OFF/OFF). Expected: +13% bulk write (bench B), 2.9× point reads, crash recovery for free. Watch the byte-parity harness — pragmas do not change query results, only durability/concurrency.
2. **Build-then-rename in the CLI's Polyglot command**: write to `<final>.building`, close the connection (WAL checkpoints cleanly on close), then `fs::rename` over the final path. Readers with an open handle keep the old inode; new opens see the complete new DB. This deletes the "web reads a half-built DB" window that exists today (delete-then-rebuild-in-place at the same tmp path).
3. **Move the duplicates indexes into the build**: extend `DDL` with `idx_dup_tier/detection/epistemic` (currently created lazily by the web's `ensureIndexes()`), then delete `ensureIndexes` — the web read path becomes purely `readOnly` with zero writes-on-read.
4. **Tombstones off the hot path**: `mark_tombstone` stops rewriting `tombstones.json` per deleted file — append to a per-revision ledger (`tombstones/{rev}.json`, merged at snapshot build) or fold into the rev's snapshot write. Kills the measured O(n²) pattern.
5. **Sled removal (cleanup)**: drop `backend-sled` from default features (keep `MemKv` always + `backend-redb` optional; `kv_suite` stays green across both). Deletes an unmaintained dependency from the default build with zero call-site changes, by design of TASK-024.
6. **Optional, later**: if a namespace ever needs true point-KV (vector ids, evidence by hash at scale), put it behind the existing `KvStore` trait on redb — the swap needs zero call-site changes. Not needed by anything measured today.

Steps 1–3 are the performance-relevant core and total roughly a dozen lines across two files plus the index DDL; steps 4–6 are hygiene.

---

## 7. Honesty — what this evaluation does NOT claim

- Warm-cache numbers on a 2-cpu container; cold-disk and 1M-symbol monorepo behavior are unmeasured. The ratios (WAL vs OFF) are expected to hold; the absolute milliseconds are not portable.
- The scale model (5k files / 60k symbols / 300k calls) is 20× the spring artifacts; where real artifacts were measured (A1–A3, ingest wall, DB size) they are quoted separately.
- sled/redb runtime numbers are absent **because they are irrelevant to the decision**: no production code path uses them (grep-verified). Their disposition (§4) rests on maintenance status and the census, not on benchmarks.
- LMDB/RocksDB/DuckDB characteristics are cited from their documented designs, not measured here; they are rejected on fit, not on speed.
- C1's reader contention is near-zero at this scale; the structural guarantee comes from C2, which is scale-independent.
- The polyglot language-coverage gap visible in the spring run (8 of 13 files parsed — Kotlin/SQL/XML outside the Lang enum) is a separate, already-tracked issue and does not affect storage choice.

## Receipts

- Bench script (persisted, re-runnable): `scripts/ds-bench/storage-bench.mjs` · raw results: `scripts/ds-bench/results.json`
- `cie-store/src/lib.rs` (JSON layout, `write_json` tmp+rename, tombstones) · `cie-store/src/snapshot.rs` (rkyv mmap, gates) · `cie-store/src/kv.rs` (KvStore trait, sled/redb/MemKv, L0 cache, `kv_suite`) · `cie-store/tests/task_024_kv.rs`, `tests/task_027_tables.rs` (the only backend references)
- `cie-polyglot/src/schema.rs:211-227` (`open_db`, journal OFF) · `cie-polyglot/src/ir.rs:37-95` (calls/ir DDL) · `web/src/lib/polyglot-db.ts:1-52` (tmp-path location, `ensureIndexes` write-on-read, readOnly opens)
- `cie-ingest/src/vector_store.rs:1-15` (vector JSON blob + in-memory HNSW rebuild)
- Workspace `Cargo.toml:71-72` (sled "0.34", redb "2")
- Real-run artifacts: `.cie` store 247 KB (rev 1, 47 symbols, wall 44.2 ms) · polyglot DB 553 KB journal-OFF at `<tmpdir>/cie-polyglot-<fnv1a64>.db` · web artifact census 33 files / 3.94 MB
