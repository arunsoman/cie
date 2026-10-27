// chat-digest's possible-charts list — the honest menu the chat's
// cie_charts tool serves: a chart kind is possible only when the loaded
// data can back it, and each entry says who supplies its content.

const { test } = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const digestLib = load(path.resolve(__dirname, "../src/lib/chat-digest.ts"));

// A minimal real-shaped report: no call records, no read/write records.
function report(overrides = {}) {
  return {
    rev: 1,
    files: ["a.py"],
    symbols: [{ id: 1, kind: "Function", name: "f", qual_name: "a.f", loc: { file: 0, start: 1, end: 2 }, lang: "python", parent: 0 }],
    summaries: [{ sym: 1, inputs: [], outputs: [], reads: [], writes: [], effects: [], exceptions: [], callees: [], unresolved_callees: [], tags: [], rev: 1, complete: true }],
    findings: [],
    contracts: [],
    coverage: { files_analyzed: 1, files_total: 1, symbols_analyzed: 1, symbols_total: 1, complete: true },
    evidence: { provenance: "test" },
    warnings: [],
    ...overrides,
  };
}
const viewspec = { elements: [], edges: [], hidden: [], gaps: [] };

test("the possible charts always include the structure kinds the report backs", () => {
  const d = digestLib.buildDigest(report(), viewspec, null, "t");
  const kinds = d.chart_possibilities.map((c) => c.kind);
  for (const k of ["uml", "flow", "c4", "hbar", "donut", "stat", "table"]) assert.ok(kinds.includes(k), k + " always possible");
});

test("sequence appears only when call records exist; dfd only when data moves", () => {
  const bare = digestLib.buildDigest(report(), viewspec, null, "t").chart_possibilities.map((c) => c.kind);
  assert.ok(!bare.includes("sequence"), "no call records → no sequence promised");
  assert.ok(!bare.includes("dfd"), "no read/write records → no dfd promised");

  const withCalls = digestLib.buildDigest(
    report({ summaries: [{ sym: 1, inputs: [], outputs: [], reads: [], writes: [], effects: [], exceptions: [], callees: ["g"], unresolved_callees: [], tags: [], rev: 1, complete: true }] }),
    viewspec, null, "t",
  );
  assert.ok(withCalls.chart_possibilities.some((c) => c.kind === "sequence" && c.data_from === "store"));
  assert.ok(!withCalls.chart_possibilities.some((c) => c.kind === "dfd"), "callees alone are not data movement");

  const withMoves = digestLib.buildDigest(
    report({ summaries: [{ sym: 1, inputs: [], outputs: [], reads: ["db"], writes: [], effects: [], exceptions: [], callees: [], unresolved_callees: [], tags: [], rev: 1, complete: true }] }),
    viewspec, null, "t",
  );
  assert.ok(withMoves.chart_possibilities.some((c) => c.kind === "dfd" && c.data_from === "store"));
  assert.ok(!withMoves.chart_possibilities.some((c) => c.kind === "sequence"), "reads alone are not call records");
});

test("mindmap/tree appear only with the concept tree; count kinds are model-fed", () => {
  const bare = digestLib.buildDigest(report(), viewspec, null, "t").chart_possibilities;
  assert.ok(!bare.some((c) => c.kind === "mindmap" || c.kind === "tree"));
  assert.ok(bare.filter((c) => c.data_from === "model").every((c) => ["hbar", "donut", "stat", "table"].includes(c.kind)));
  assert.ok(bare.filter((c) => c.data_from === "store").every((c) => !["hbar", "donut", "stat", "table"].includes(c.kind)));
});