// B3 — the concept-diagrams seam, tested from the CLIENT side.
//
// `conceptToFlowchartSpec` / `conceptToSequenceSpec` turn one concept
// node (concepts.json) + the report (report.json) into gate-ready
// ViewSpecs. Fixtures are synthetic and hermetic: a two-class Python
// project (Cart, Pay) plus a module-level function, with resolved call
// edges in the summaries. Gates:
//   * flowchart: extent members as nodes (behavior-label-named), internal
//     calls as edges, entries/isolates flagged, byte-span evidence mapped
//     through report.files
//   * sequence: deduped class lifelines (entries' classes first),
//     cross-class messages ordered by BFS depth from the entries,
//     intra-class calls suppressed AND disclosed
//   * honesty: no fabricated edges — the no-internal-calls gap, the
//     skipped-member gap, byte-identical determinism
//   * the mermaid adapter reads the shapes: flowchart text for the call
//     graph, sequenceDiagram text for the lifeline+message shape

const { test } = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const { gate, verifySemanticHash, mermaidAdapt } = load(
  path.resolve(__dirname, "../../viz-adapter/src/index.ts"),
);
const { conceptToFlowchartSpec, conceptToSequenceSpec, NO_INTERNAL_CALLS_GAP } = load(
  path.resolve(__dirname, "../src/lib/viz/concept-diagrams.ts"),
);

// ---------- fixtures ----------

function report() {
  return {
    rev: 7,
    files: ["shop/cart.py", "shop/pay.py"],
    symbols: [
      { id: 1, kind: "Class", name: "Cart", qual_name: "Cart", loc: { file: 0, start: 0, end: 100 }, lang: "python", parent: null },
      { id: 2, kind: "Method", name: "add_item", qual_name: "Cart.add_item", loc: { file: 0, start: 20, end: 60 }, lang: "python", parent: 1 },
      { id: 3, kind: "Method", name: "checkout", qual_name: "Cart.checkout", loc: { file: 0, start: 61, end: 99 }, lang: "python", parent: 1 },
      { id: 4, kind: "Class", name: "Pay", qual_name: "Pay", loc: { file: 1, start: 0, end: 80 }, lang: "python", parent: null },
      { id: 5, kind: "Method", name: "charge", qual_name: "Pay.charge", loc: { file: 1, start: 10, end: 50 }, lang: "python", parent: 4 },
      { id: 6, kind: "Function", name: "refund", qual_name: "refund", loc: { file: 1, start: 51, end: 79 }, lang: "python", parent: null },
    ],
    summaries: [
      { sym: 2, callees: [5, 5], tags: [] }, // parallel call sites collapse
      { sym: 3, callees: [] },
      { sym: 5, callees: [6] },
      { sym: 6, callees: [] },
    ],
    coverage: { files_total: 2, files_analyzed: 2, symbols_total: 6, symbols_analyzed: 4, complete: true, notes: [] },
    evidence: { rev: 7, provenance: "test", assumptions: [], unsupported: [], unresolved: [] },
    warnings: [],
  };
}

// extent {2,3,5,6}: entries {2,3}, internal calls 2->5, 5->6, isolate {3}
function concept() {
  return {
    id: 9,
    label: "money movement",
    kind: "UseCase",
    parent: 0,
    children: [],
    primary_edge: "FcaSubsumption",
    symbols: [6, 5, 2, 3], // deliberately unsorted — the module sorts
    behaviors: [
      { summary: 2, kind: "Add item to cart", confidence: 1 },
      { summary: 5, kind: "Move money via charge", confidence: 1 },
    ],
  };
}

const ok = (r) => {
  assert.deepEqual(r.violations, [], `gate refused: ${r.violations}`);
  assert.ok(r.view);
  assert.equal(verifySemanticHash(r.view), true, "semanticHash must verify");
  return r.view;
};

// ---------- flowchart ----------

test("flowchart: extent members, behavior labels, internal calls, entries and isolates", () => {
  const v = ok(conceptToFlowchartSpec(concept(), report()));
  assert.deepEqual(
    v.elements.map((e) => e.id),
    ["fn:2", "fn:3", "fn:5", "fn:6"],
    "extent order (symbol id ascending), skipped nothing",
  );
  assert.deepEqual(
    v.elements.map((e) => e.label),
    ["Add item to cart", "checkout", "Move money via charge", "refund"],
    "behavior labels where recorded, symbol names otherwise",
  );
  assert.deepEqual(
    v.edges.map((e) => `${e.source}->${e.target}`),
    ["fn:2->fn:5", "fn:5->fn:6"],
    "only calls whose BOTH ends are in the extent; parallel sites collapse",
  );
  const byId = Object.fromEntries(v.elements.map((e) => [e.id, e]));
  assert.equal(byId["fn:2"].fields.entry, true, "add_item has no in-extent caller");
  assert.equal(byId["fn:3"].fields.entry, true, "checkout has no in-extent caller");
  assert.equal(byId["fn:3"].fields.isolate, true, "checkout neither calls nor is called");
  assert.equal(byId["fn:6"].fields.entry, undefined, "refund IS called in-extent");
  assert.equal(byId["fn:2"].evidence.file, "shop/cart.py", "byte-span file resolved through report.files");
  assert.equal(byId["fn:5"].evidence.start, 10);
  assert.equal(v.edges[0].evidence.file, "shop/cart.py", "an edge cites the caller's bytes");
  assert.equal(v.footer.facts, v.elements.length + v.edges.length);
  assert.deepEqual(v.gaps, [], "nothing to disclose on the happy path");
  assert.equal(v.layoutHint.strategy, "dagre");
  assert.equal(v.scope.revision, 7);
});

test("flowchart: deterministic — two builds serialize byte-identically", () => {
  const a = conceptToFlowchartSpec(concept(), report());
  const b = conceptToFlowchartSpec(concept(), report());
  assert.equal(JSON.stringify(a), JSON.stringify(b));
});

// ---------- sequence ----------

test("sequence: deduped lifelines (entries' classes first), messages ordered by BFS depth", () => {
  const v = ok(conceptToSequenceSpec(concept(), report()));
  assert.deepEqual(
    v.elements.map((e) => e.label),
    ["Cart", "Pay", "shop/pay.py"],
    "Cart first (both entries), then Pay, then the module stand-in for refund",
  );
  assert.deepEqual(
    v.elements.map((e) => e.elementKind),
    ["lifeline", "lifeline", "lifeline"],
  );
  const lanes = Object.fromEntries(v.elements.map((e) => [e.label, e]));
  assert.equal(lanes.Cart.fields.laneOf, "class");
  assert.deepEqual(lanes.Cart.fields.members, [2, 3]);
  assert.equal(lanes["shop/pay.py"].fields.laneOf, "module");
  assert.deepEqual(
    v.edges.map((e) => `${e.source}|${e.target}|${e.edgeKind}`),
    [
      "lane:sym:1|lane:sym:4|message:Move money via charge", // depth 0 (entry's class)
      "lane:sym:4|lane:file:1|message:refund", // depth 1
    ],
    "cross-class messages only, nearest the entries first, message text = callee's recorded name",
  );
  assert.deepEqual(v.gaps, [], "both edges cross classes — nothing suppressed");
});

test("sequence: deterministic — two builds serialize byte-identically", () => {
  const a = conceptToSequenceSpec(concept(), report());
  const b = conceptToSequenceSpec(concept(), report());
  assert.equal(JSON.stringify(a), JSON.stringify(b));
});

test("sequence: intra-class-only calls are suppressed AND disclosed", () => {
  const rep = report();
  rep.summaries = [{ sym: 3, callees: [2] }]; // checkout -> add_item, same class
  const c = concept();
  c.symbols = [2, 3];
  c.behaviors = [];
  const v = ok(conceptToSequenceSpec(c, rep));
  assert.deepEqual(v.edges, [], "no cross-class messages exist");
  assert.equal(v.elements.length, 1, "one lifeline: Cart");
  assert.ok(
    v.gaps.some((g) => g.why.includes("no cross-class messages to sequence")),
    `the suppression is named: ${JSON.stringify(v.gaps)}`,
  );
  // the flowchart of the same concept KEEPS the edge — suppression is a
  // sequence-presentation rule, never a data loss
  const f = ok(conceptToFlowchartSpec(c, rep));
  assert.equal(f.edges.length, 1);
});

// ---------- honesty guards ----------

test("no internal call edges: members still render, the gap says they act independently", () => {
  const c = concept();
  c.symbols = [3, 6]; // checkout, refund — neither calls anything in-extent
  c.behaviors = [];
  const rep = report();
  const f = ok(conceptToFlowchartSpec(c, rep));
  assert.equal(f.elements.length, 2, "both members shown");
  assert.equal(f.edges.length, 0, "no edges invented");
  assert.ok(f.gaps.some((g) => g.why === NO_INTERNAL_CALLS_GAP));
  assert.ok(f.elements.every((e) => e.fields.isolate), "both are isolates, flagged");
  const s = ok(conceptToSequenceSpec(c, rep));
  assert.equal(s.elements.length, 2, "both lifelines shown");
  assert.equal(s.edges.length, 0);
  assert.ok(s.gaps.some((g) => g.why === NO_INTERNAL_CALLS_GAP));
});

test("extent members the tables cannot evidence are skipped and counted, never invented", () => {
  const c = concept();
  c.symbols = [2, 999]; // 999 is not in the symbol table
  const f = ok(conceptToFlowchartSpec(c, report()));
  assert.equal(f.elements.length, 1);
  assert.ok(f.gaps.some((g) => g.why.includes("1 extent member(s) could not be evidenced")));
});

// ---------- the mermaid adapter reads the shapes ----------

test("mermaid adapt: the flowchart shape renders a flowchart with Fact arrows", () => {
  const v = ok(conceptToFlowchartSpec(concept(), report()));
  const out = mermaidAdapt(v);
  assert.equal(out.kind, "flowchart");
  assert.ok(out.text.startsWith("flowchart TD"));
  assert.ok(out.text.includes("fn_2 --> |call| fn_5"), `solid Fact arrow: ${out.text}`);
  assert.ok(out.text.includes("[Add item to cart]"), "behavior label in the node");
});

test("mermaid adapt: the lifeline+message shape renders a sequenceDiagram (catalog B3)", () => {
  const v = ok(conceptToSequenceSpec(concept(), report()));
  const out = mermaidAdapt(v);
  assert.equal(out.kind, "sequenceDiagram");
  assert.ok(out.text.startsWith("sequenceDiagram"));
  assert.ok(out.text.includes("participant"), "lifelines become participants");
  assert.ok(
    out.text.includes("Move money via charge"),
    "the message text is the callee's recorded name",
  );
});
