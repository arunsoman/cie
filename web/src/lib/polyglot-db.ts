// Server-only reader for the cie-polyglot SQLite scratch store (PRD v2.0,
// docs/prd-v2.0.md). `cie polyglot <path>` builds a FRESH store per run at
// `<tmpdir>/cie-polyglot-<fnv1a64 hex of the canonicalized project path>.db`
// (cie-cli/src/main.rs, Command::Polyglot) and promotes it atomically
// (build-then-rename, DS-EVAL §6.2), so this module always sees either the
// previous complete store or the new one — never a half-built DB. The module
// locates that same file and reads it read-only — it never runs
// `cie polyglot` itself (a real run on a large repo can take a long time),
// so the UI must tolerate "not computed yet". Strictly zero writes here:
// every index the queries need is created at build time by the Rust side
// (duplicates.rs), so the read path can never block a concurrent writer
// (DS-EVAL §6.3: the old lazy `ensureIndexes()` opened read-write and
// created indexes on first touch — a write on the read path).

import { DatabaseSync } from "node:sqlite";
import fs from "fs";
import os from "os";
import path from "path";

function fnv1a64Hex(absPath: string): string {
  let h = BigInt("0xcbf29ce484222325");
  const prime = BigInt("0x100000001b3");
  const mask = (BigInt(1) << BigInt(64)) - BigInt(1);
  for (const b of Buffer.from(absPath, "utf8")) {
    h ^= BigInt(b);
    h = (h * prime) & mask;
  }
  return h.toString(16);
}

export function polyglotDbPath(root: string): string {
  let abs = path.resolve(root);
  try {
    abs = fs.realpathSync(abs);
  } catch {
    // mirrors the CLI's `.canonicalize().unwrap_or(path)` fallback
  }
  return path.join(os.tmpdir(), `cie-polyglot-${fnv1a64Hex(abs)}.db`);
}

export function polyglotDbExists(root: string): boolean {
  try {
    return fs.existsSync(polyglotDbPath(root));
  } catch {
    return false;
  }
}

function openPolyglotDb(root: string): DatabaseSync | null {
  const p = polyglotDbPath(root);
  if (!fs.existsSync(p)) return null;
  try {
    return new DatabaseSync(p, { readOnly: true });
  } catch {
    return null;
  }
}

type Row = Record<string, unknown>;

export interface PolyglotSummary {
  dbPath: string;
  ingest: { files: number; astNodes: number; symbols: number };
  ir: {
    blocks: number;
    instructions: number;
    calls: number;
    callsResolved: number;
    callsUnresolved: number;
  };
  frameworks: { totalRoles: number; roleSites: Row[] };
  c4: { containers: Row[]; components: Row[]; relationships: Row[] };
  stateMachines: { total: number; byTier: Record<string, number>; rows: Row[] };
  workflows: { total: number; totalNodes: number; rows: Row[] };
  duplicates: { totalPairs: number; byTier: Record<string, number> };
}

export function getPolyglotSummary(root: string): PolyglotSummary | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const count = (sql: string): number =>
      (db.prepare(sql).get() as Row | undefined)?.n as number ?? 0;
    const all = (sql: string, limit: number): Row[] =>
      (db.prepare(sql).all() as Row[]).slice(0, limit);
    const groupCount = (sql: string): Record<string, number> => {
      const out: Record<string, number> = {};
      for (const r of db.prepare(sql).all() as Row[]) {
        out[String(r.tier ?? r.key)] = Number(r.n);
      }
      return out;
    };

    const calls = count("SELECT COUNT(*) n FROM calls");
    const callsResolved = count("SELECT COUNT(*) n FROM calls WHERE resolution='RESOLVED'");

    return {
      dbPath: polyglotDbPath(root),
      ingest: {
        files: count("SELECT COUNT(*) n FROM files"),
        astNodes: count("SELECT COUNT(*) n FROM ast_nodes"),
        symbols: count("SELECT COUNT(*) n FROM symbols"),
      },
      ir: {
        blocks: count("SELECT COUNT(*) n FROM ir_blocks"),
        instructions: count("SELECT COUNT(*) n FROM ir_instructions"),
        calls,
        callsResolved,
        callsUnresolved: calls - callsResolved,
      },
      frameworks: {
        totalRoles: count("SELECT COUNT(*) n FROM framework_roles"),
        roleSites: all(
          `SELECT fr.feature, fr.framework, fr.evidence_text, fr.confidence,
                  s.qual_name, f.path
           FROM framework_roles fr
           LEFT JOIN symbols s ON s.id = fr.symbol_id
           JOIN files f ON f.id = fr.file_id
           ORDER BY fr.id`,
          2000,
        ),
      },
      c4: {
        containers: all(
          `SELECT id, label, kind, confidence, provenance FROM concept_nodes WHERE kind='Container' ORDER BY id`,
          500,
        ),
        components: all(
          `SELECT id, label, kind, confidence, provenance FROM concept_nodes WHERE kind='Component' ORDER BY id`,
          2000,
        ),
        relationships: all(
          `SELECT r.kind, r.resolution, r.confidence, a.label from_label, b.label to_label
           FROM c4_relationships r
           JOIN concept_nodes a ON a.id = r.from_node
           JOIN concept_nodes b ON b.id = r.to_node
           ORDER BY r.confidence DESC`,
          2000,
        ),
      },
      stateMachines: {
        total: count("SELECT COUNT(*) n FROM state_machines"),
        byTier: groupCount("SELECT tier, COUNT(*) n FROM state_machines GROUP BY tier"),
        rows: all(
          `SELECT sm.id, sm.name, sm.tier, sm.epistemic, sm.coverage,
                  sm.counter_argument, sm.fog_reason, s.qual_name
           FROM state_machines sm
           LEFT JOIN symbols s ON s.id = sm.source_symbol_id
           ORDER BY sm.id`,
          2000,
        ),
      },
      workflows: {
        total: count("SELECT COUNT(*) n FROM workflows"),
        totalNodes: count("SELECT COUNT(*) n FROM workflow_nodes"),
        rows: all(
          `SELECT w.id, w.name, w.entry_role, w.epistemic, w.counter_argument,
                  s.qual_name,
                  (SELECT COUNT(*) FROM workflow_nodes wn WHERE wn.workflow_id = w.id) node_count
           FROM workflows w
           LEFT JOIN symbols s ON s.id = w.entry_symbol_id
           ORDER BY node_count DESC`,
          2000,
        ),
      },
      duplicates: {
        totalPairs: count("SELECT COUNT(*) n FROM duplicates"),
        byTier: groupCount("SELECT tier, COUNT(*) n FROM duplicates GROUP BY tier"),
      },
    };
  } finally {
    db.close();
  }
}

export interface DuplicateSymbolRef {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
  startByte: number;
  endByte: number;
}

export interface DuplicateRow {
  id: number;
  similarity: number;
  tier: string;
  detection: string;
  epistemic: string;
  counterArgument: string | null;
  a: DuplicateSymbolRef;
  b: DuplicateSymbolRef;
  canonical: DuplicateSymbolRef | null;
}

export interface DuplicatesPage {
  rows: DuplicateRow[];
  total: number;
  facets: { tiers: string[]; detections: string[]; epistemics: string[] };
}

// Fixed enums from cie-polyglot/src/duplicates.rs's `duplicates` DDL comment
// — hardcoded rather than `SELECT DISTINCT ... FROM duplicates`, which on a
// multi-ten-million-row table scans the whole thing per facet (~3s each)
// even though the filtered query itself is sub-millisecond with the indexes
// below.
const DUPLICATE_TIERS = ["canonical", "high", "medium", "low"];
const DUPLICATE_DETECTIONS = ["ir-hash-match", "ir-vec", "ir-shingle", "ir-vec-cosine"];
const DUPLICATE_EPISTEMICS = ["Fact", "Inference", "Hypothesis"];

/** Flat, filterable, paginated view over every tier of `duplicates` (unlike
 *  getDuplicateClusters, which is graph-shaped and canonical-only) — ids
 *  resolved to symbol name/kind/path so the table reads as code, not ids. */
export function getDuplicates(
  root: string,
  opts?: {
    tier?: string;
    detection?: string;
    epistemic?: string;
    limit?: number;
    offset?: number;
  },
): DuplicatesPage | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const limit = Math.min(Math.max(opts?.limit ?? 50, 1), 500);
    const offset = Math.max(opts?.offset ?? 0, 0);

    const where: string[] = [];
    const params: Record<string, string> = {};
    if (opts?.tier) {
      where.push("d.tier = :tier");
      params.tier = opts.tier;
    }
    if (opts?.detection) {
      where.push("d.detection = :detection");
      params.detection = opts.detection;
    }
    if (opts?.epistemic) {
      where.push("d.epistemic = :epistemic");
      params.epistemic = opts.epistemic;
    }
    const whereSql = where.length ? `WHERE ${where.join(" AND ")}` : "";

    const total =
      ((db.prepare(`SELECT COUNT(*) n FROM duplicates d ${whereSql}`).get(params) as Row | undefined)
        ?.n as number) ?? 0;

    const rows = db
      .prepare(
        `SELECT d.id, d.similarity, d.tier, d.detection, d.epistemic, d.counter_argument,
                sa.id a_id, sa.qual_name a_qual, sa.kind a_kind, fa.path a_path, sa.start_byte a_start, sa.end_byte a_end,
                sb.id b_id, sb.qual_name b_qual, sb.kind b_kind, fb.path b_path, sb.start_byte b_start, sb.end_byte b_end,
                sc.id c_id, sc.qual_name c_qual, sc.kind c_kind, fc.path c_path, sc.start_byte c_start, sc.end_byte c_end
         FROM duplicates d
         JOIN symbols sa ON sa.id = d.symbol_a
         JOIN files fa ON fa.id = sa.file_id
         JOIN symbols sb ON sb.id = d.symbol_b
         JOIN files fb ON fb.id = sb.file_id
         LEFT JOIN symbols sc ON sc.id = d.canonical_symbol_id
         LEFT JOIN files fc ON fc.id = sc.file_id
         ${whereSql}
         ORDER BY d.id
         LIMIT :limit OFFSET :offset`,
      )
      .all({ ...params, limit, offset }) as Row[];

    return {
      rows: rows.map((r) => ({
        id: r.id as number,
        similarity: r.similarity as number,
        tier: r.tier as string,
        detection: r.detection as string,
        epistemic: r.epistemic as string,
        counterArgument: (r.counter_argument as string | null) ?? null,
        a: {
          symbolId: r.a_id as number,
          qualName: r.a_qual as string,
          kind: r.a_kind as string,
          path: r.a_path as string,
          startByte: r.a_start as number,
          endByte: r.a_end as number,
        },
        b: {
          symbolId: r.b_id as number,
          qualName: r.b_qual as string,
          kind: r.b_kind as string,
          path: r.b_path as string,
          startByte: r.b_start as number,
          endByte: r.b_end as number,
        },
        canonical:
          r.c_id == null
            ? null
            : {
                symbolId: r.c_id as number,
                qualName: r.c_qual as string,
                kind: r.c_kind as string,
                path: r.c_path as string,
                startByte: r.c_start as number,
                endByte: r.c_end as number,
              },
      })),
      total,
      facets: {
        tiers: DUPLICATE_TIERS,
        detections: DUPLICATE_DETECTIONS,
        epistemics: DUPLICATE_EPISTEMICS,
      },
    };
  } finally {
    db.close();
  }
}

export interface DuplicateClusterMember {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
  startByte: number;
  endByte: number;
}

export interface DuplicateCluster {
  hub: DuplicateClusterMember;
  members: DuplicateClusterMember[]; // excludes hub, capped at maxMembersPerCluster
  totalMembers: number; // before the cap
}

export interface DuplicateClustersResult {
  tierUsed: "canonical";
  totalFamilies: number;
  totalSymbolsInvolved: number;
  clusters: DuplicateCluster[];
}

/** Groups exact-IR-hash duplicate pairs (`tier='canonical'`, the one Fact
 *  tier in the §23.2 ladder — cosine tiers are Inference/Hypothesis and, as
 *  found by inspecting real pairs, prone to false positives on short,
 *  simple methods) into connected-component families via union-find, then
 *  returns the largest families as hub+members — one representative "pick"
 *  per family, every other member of that exact-duplicate set attached to
 *  it, biggest family first. */
export function getDuplicateClusters(
  root: string,
  opts?: { maxClusters?: number; maxMembersPerCluster?: number },
): DuplicateClustersResult | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const maxClusters = opts?.maxClusters ?? 12;
    const maxMembers = opts?.maxMembersPerCluster ?? 24;

    const pairs = db
      .prepare("SELECT symbol_a, symbol_b FROM duplicates WHERE tier='canonical'")
      .all() as { symbol_a: number; symbol_b: number }[];

    const parent = new Map<number, number>();
    const find = (x: number): number => {
      let r = x;
      while ((parent.get(r) ?? r) !== r) r = parent.get(r) as number;
      let cur = x;
      while (cur !== r) {
        const next = parent.get(cur) as number;
        parent.set(cur, r);
        cur = next;
      }
      return r;
    };
    const union = (a: number, b: number) => {
      const ra = find(a);
      const rb = find(b);
      if (ra !== rb) parent.set(ra, rb);
    };
    for (const { symbol_a, symbol_b } of pairs) {
      if (!parent.has(symbol_a)) parent.set(symbol_a, symbol_a);
      if (!parent.has(symbol_b)) parent.set(symbol_b, symbol_b);
      union(symbol_a, symbol_b);
    }

    const byRoot = new Map<number, number[]>();
    for (const id of parent.keys()) {
      const r = find(id);
      const list = byRoot.get(r);
      if (list) list.push(id);
      else byRoot.set(r, [id]);
    }
    const families = Array.from(byRoot.values()).sort((a, b) => b.length - a.length);

    // Small table (9,503 symbols even on a large repo) — one join, no
    // parameter-count limits to worry about, instead of an IN(...) per family.
    const nameRows = db
      .prepare(
        `SELECT s.id, s.qual_name, s.kind, s.start_byte, s.end_byte, f.path
         FROM symbols s JOIN files f ON f.id = s.file_id`,
      )
      .all() as { id: number; qual_name: string; kind: string; start_byte: number; end_byte: number; path: string }[];
    const byId = new Map(nameRows.map((r) => [r.id, r]));

    const toMember = (id: number): DuplicateClusterMember | null => {
      const r = byId.get(id);
      if (!r) return null;
      return {
        symbolId: id,
        qualName: r.qual_name,
        kind: r.kind,
        path: r.path,
        startByte: r.start_byte,
        endByte: r.end_byte,
      };
    };

    const clusters: DuplicateCluster[] = [];
    for (const fam of families) {
      if (clusters.length >= maxClusters) break;
      const members = fam.map(toMember).filter((m): m is DuplicateClusterMember => m !== null);
      if (members.length < 2) continue;
      const [hub, ...rest] = members;
      clusters.push({ hub, members: rest.slice(0, maxMembers), totalMembers: rest.length });
    }

    return {
      tierUsed: "canonical",
      totalFamilies: families.length,
      totalSymbolsInvolved: parent.size,
      clusters,
    };
  } finally {
    db.close();
  }
}

// ---------------------------------------------------------------------------
// Generic table browser — every table `ensure_all_schemas` creates
// (cie-polyglot/src/lib.rs), whether or not this particular run populated
// it. Schema is unconditional; row counts are not (see `group` below).
// ---------------------------------------------------------------------------

export type PolyglotTableGroup =
  | "Ingest & AST"
  | "IR"
  | "Frameworks"
  | "C4 / Concepts"
  | "State machines"
  | "Workflows"
  | "Duplicates"
  | "Runtime (OTel)";

interface TableSpec {
  name: string;
  group: PolyglotTableGroup;
  note: string;
}

// Whitelist — also the only thing standing between a table name and a raw
// SQL string, so getTableRows refuses any name not in this list.
const POLYGLOT_TABLES: TableSpec[] = [
  { name: "files", group: "Ingest & AST", note: "every ingested file, one row each" },
  { name: "node_kinds", group: "Ingest & AST", note: "tree-sitter grammar node-kind lookup" },
  { name: "node_text", group: "Ingest & AST", note: "grammar node text lookup" },
  { name: "uast_kinds", group: "Ingest & AST", note: "universal-AST kind lookup" },
  { name: "uast_map", group: "Ingest & AST", note: "per-language kind → universal-AST kind mapping" },
  { name: "ast_nodes", group: "Ingest & AST", note: "the parsed AST, one row per node" },
  { name: "symbols", group: "Ingest & AST", note: "named declarations found in the AST" },
  { name: "calls", group: "IR", note: "call-site edges between symbols" },
  { name: "ir_blocks", group: "IR", note: "lowered control-flow blocks, one per symbol's block" },
  { name: "ir_instructions", group: "IR", note: "lowered IR instructions inside each block" },
  { name: "ir_edges", group: "IR", note: "control-flow edges between blocks" },
  { name: "file_source_cache", group: "IR", note: "zstd-compressed source per file (blob hidden below)" },
  { name: "ir_meta", group: "IR", note: "per-symbol IR hash + vector (duplicate detection input)" },
  { name: "framework_adapters", group: "Frameworks", note: "static per-language framework-signature registry" },
  { name: "framework_roles", group: "Frameworks", note: "AST sites matching a registry signature" },
  { name: "cross_language_bindings", group: "Frameworks", note: "API-spec ↔ client-call matches" },
  { name: "concept_nodes", group: "C4 / Concepts", note: "C4 containers/components — needs a recognized manifest file" },
  { name: "concept_edges", group: "C4 / Concepts", note: "edges between concept nodes" },
  { name: "concept_node_symbols", group: "C4 / Concepts", note: "which symbols belong to which concept node" },
  { name: "c4_relationships", group: "C4 / Concepts", note: "C4 relationships between containers/components" },
  { name: "state_machines", group: "State machines", note: "detected state machines (T1 fact / T2 inference / T3 hypothesis)" },
  { name: "workflows", group: "Workflows", note: "entry-point call-chain workflows" },
  { name: "workflow_nodes", group: "Workflows", note: "nodes within each workflow" },
  { name: "workflow_edges", group: "Workflows", note: "edges within each workflow" },
  { name: "duplicates", group: "Duplicates", note: "see the Duplicate analysis view below for a resolved, filterable version" },
  { name: "duplicate_gaps", group: "Duplicates", note: "per-symbol count of pairs hidden below the reporting threshold" },
  { name: "runtime_evidence", group: "Runtime (OTel)", note: "needs an OTel trace payload (`cie polyglot --otel`) — empty otherwise" },
  { name: "runtime_unresolved", group: "Runtime (OTel)", note: "needs an OTel trace payload — empty otherwise" },
  { name: "runtime_summary", group: "Runtime (OTel)", note: "needs an OTel trace payload — empty otherwise" },
  { name: "runtime_drift", group: "Runtime (OTel)", note: "needs an OTel trace payload — empty otherwise" },
];

export interface PolyglotTableInfo {
  name: string;
  group: PolyglotTableGroup;
  note: string;
  count: number | null; // null if the table doesn't exist in this store (older schema version)
}

/** Row counts for every table `ensure_all_schemas` creates, whether or not
 *  this run populated it — the UI groups/greys out the empty ones rather
 *  than hiding them, so "0 rows" reads as a fact about this codebase, not
 *  as a missing feature. */
export function getPolyglotTableInventory(root: string): PolyglotTableInfo[] | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    return POLYGLOT_TABLES.map((t) => {
      let count: number | null = null;
      try {
        count = ((db.prepare(`SELECT COUNT(*) n FROM "${t.name}"`).get() as Row | undefined)?.n as number) ?? 0;
      } catch {
        count = null;
      }
      return { ...t, count };
    });
  } finally {
    db.close();
  }
}

export interface PolyglotTablePage {
  table: string;
  columns: string[];
  rows: Record<string, unknown>[];
  total: number;
}

const BLOB_COLUMN_HINT = /^(ir_hash|ir_vec|operands|zstd_bytes|content_hash|subtree_hash|throws_set)$/;

function cellValue(v: unknown, colName: string): unknown {
  if (v instanceof Uint8Array) return `<binary ${v.length} bytes>`;
  if (typeof v === "string" && v.length > 2000) return `${v.slice(0, 2000)}… (${v.length} chars)`;
  if (BLOB_COLUMN_HINT.test(colName) && v != null) return `<binary>`;
  return v;
}

/** Generic paginated raw-row view over any table in POLYGLOT_TABLES — `table`
 *  is validated against that whitelist before ever touching a SQL string, and
 *  every table here has a rowid (none are WITHOUT ROWID), so plain
 *  `ORDER BY rowid` pagination works uniformly without per-table knowledge
 *  of primary keys. */
export function getPolyglotTableRows(
  root: string,
  table: string,
  opts?: { limit?: number; offset?: number },
): PolyglotTablePage | null {
  if (!POLYGLOT_TABLES.some((t) => t.name === table)) return null;
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const limit = Math.min(Math.max(opts?.limit ?? 50, 1), 200);
    const offset = Math.max(opts?.offset ?? 0, 0);
    const total = ((db.prepare(`SELECT COUNT(*) n FROM "${table}"`).get() as Row | undefined)?.n as number) ?? 0;
    const rows = db
      .prepare(`SELECT * FROM "${table}" ORDER BY rowid LIMIT :limit OFFSET :offset`)
      .all({ limit, offset }) as Row[];
    const columns = rows.length ? Object.keys(rows[0]) : [];
    return {
      table,
      columns,
      rows: rows.map((r) => {
        const out: Record<string, unknown> = {};
        for (const c of columns) out[c] = cellValue(r[c], c);
        return out;
      }),
      total,
    };
  } finally {
    db.close();
  }
}

// ---------------------------------------------------------------------------
// Function IR viewer — one symbol's lowered CFG (ir_blocks/ir_instructions/
// ir_edges) plus its outgoing calls, name-resolved. `operands` is not
// surfaced: the lowering pass never fills it in (always `vec![]` — see
// cie-polyglot/src/ir.rs's `Cfg::emit`), so there's nothing to decode yet.
// ---------------------------------------------------------------------------

export interface SymbolSearchResult {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
}

/** Substring search over symbol qual_name, shortest match first — a tiny
 *  table (≤10K rows even on a large repo), so a LIKE scan is plenty fast. */
export function searchPolyglotSymbols(root: string, query: string, limit = 30): SymbolSearchResult[] | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const q = `%${query.replace(/[%_\\]/g, (c) => `\\${c}`)}%`;
    const rows = db
      .prepare(
        `SELECT s.id, s.qual_name, s.kind, f.path
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.qual_name LIKE :q ESCAPE '\\'
         ORDER BY length(s.qual_name) ASC
         LIMIT :limit`,
      )
      .all({ q, limit }) as Row[];
    return rows.map((r) => ({
      symbolId: r.id as number,
      qualName: r.qual_name as string,
      kind: r.kind as string,
      path: r.path as string,
    }));
  } finally {
    db.close();
  }
}

export interface FunctionIrInstruction {
  instIndex: number;
  op: string;
  spanStart: number;
  spanEnd: number;
}
export interface FunctionIrBlock {
  id: number;
  blockIndex: number;
  kind: string;
  instructions: FunctionIrInstruction[];
}
export interface FunctionIrEdge {
  fromBlockIndex: number;
  toBlockIndex: number;
  kind: string;
}
export interface FunctionIrCall {
  calleeText: string | null;
  calleeQualName: string | null;
  resolution: string;
  callKind: string;
  stmtIndex: number;
}
export interface FunctionIr {
  symbol: { symbolId: number; qualName: string; kind: string; path: string; startByte: number; endByte: number };
  blocks: FunctionIrBlock[];
  edges: FunctionIrEdge[];
  calls: FunctionIrCall[];
}

export function getFunctionIr(root: string, symbolId: number): FunctionIr | null {
  const db = openPolyglotDb(root);
  if (!db) return null;
  try {
    const symRow = db
      .prepare(
        `SELECT s.id, s.qual_name, s.kind, f.path, s.start_byte, s.end_byte
         FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.id = :id`,
      )
      .get({ id: symbolId }) as Row | undefined;
    if (!symRow) return null;

    const blockRows = db
      .prepare(
        `SELECT bl.id, bl.block_index, bl.kind block_kind, i.inst_index, i.op, i.span_start, i.span_end
         FROM ir_blocks bl
         LEFT JOIN ir_instructions i ON i.block_id = bl.id
         WHERE bl.symbol_id = :id
         ORDER BY bl.block_index, i.inst_index`,
      )
      .all({ id: symbolId }) as Row[];

    const blocksByIndex = new Map<number, FunctionIrBlock>();
    const blockIndexById = new Map<number, number>();
    for (const r of blockRows) {
      const blockIndex = r.block_index as number;
      blockIndexById.set(r.id as number, blockIndex);
      let blk = blocksByIndex.get(blockIndex);
      if (!blk) {
        blk = { id: r.id as number, blockIndex, kind: r.block_kind as string, instructions: [] };
        blocksByIndex.set(blockIndex, blk);
      }
      if (r.inst_index != null) {
        blk.instructions.push({
          instIndex: r.inst_index as number,
          op: r.op as string,
          spanStart: r.span_start as number,
          spanEnd: r.span_end as number,
        });
      }
    }
    const blocks = Array.from(blocksByIndex.values()).sort((a, b) => a.blockIndex - b.blockIndex);

    const edgeRows = db
      .prepare(
        `SELECT e.from_block_id, e.to_block_id, e.kind
         FROM ir_edges e
         JOIN ir_blocks b ON b.id = e.from_block_id
         WHERE b.symbol_id = :id`,
      )
      .all({ id: symbolId }) as Row[];
    const edges: FunctionIrEdge[] = edgeRows
      .map((r) => {
        const from = blockIndexById.get(r.from_block_id as number);
        const to = blockIndexById.get(r.to_block_id as number);
        if (from == null || to == null) return null;
        return { fromBlockIndex: from, toBlockIndex: to, kind: r.kind as string };
      })
      .filter((e): e is FunctionIrEdge => e !== null);

    const callRows = db
      .prepare(
        `SELECT c.callee_text, s2.qual_name callee_qual, c.resolution, c.call_kind, c.stmt_index
         FROM calls c
         LEFT JOIN symbols s2 ON s2.id = c.callee_symbol_id
         WHERE c.caller_symbol_id = :id
         ORDER BY c.stmt_index`,
      )
      .all({ id: symbolId }) as Row[];
    const calls: FunctionIrCall[] = callRows.map((r) => ({
      calleeText: (r.callee_text as string | null) ?? null,
      calleeQualName: (r.callee_qual as string | null) ?? null,
      resolution: r.resolution as string,
      callKind: r.call_kind as string,
      stmtIndex: r.stmt_index as number,
    }));

    return {
      symbol: {
        symbolId: symRow.id as number,
        qualName: symRow.qual_name as string,
        kind: symRow.kind as string,
        path: symRow.path as string,
        startByte: symRow.start_byte as number,
        endByte: symRow.end_byte as number,
      },
      blocks,
      edges,
      calls,
    };
  } finally {
    db.close();
  }
}
