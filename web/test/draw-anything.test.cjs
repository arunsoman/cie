// draw-anything — the generic-chart-ask fix and its neighbors.
//
// "draw a chart" / "draw a diagram" used to fall through to the AI, which
// replied with a clarifying menu and drew NOTHING (the "asked for a chart,
// got no chart" defect). Now a drawing verb + an unspecified chart noun
// draws the project's structure map instantly; a specific kind still wins;
// and a question that is NOT a request ("explain this chart") still
// reaches the AI.

const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const da = load(path.resolve(__dirname, "../src/lib/draw-anything.ts"));

const dataDir = path.resolve(__dirname, "../public/data");
const report = JSON.parse(fs.readFileSync(path.join(dataDir, "report.json"), "utf8"));
const concepts = JSON.parse(fs.readFileSync(path.join(dataDir, "concepts.json"), "utf8"));
const viewspec = JSON.parse(fs.readFileSync(path.join(dataDir, "viewspec.json"), "utf8"));

// The generic asks now go to THE MODEL (the user's rule): the drawer
// returns null, the configured LLM answers and PICKS the chart kind, and
// the store fills that kind from the records (fillDiagram). No regex
// guessing on the user's words; the stand-in map is gone.
test("a bare 'draw a chart' goes to the configured LLM, not a guessed map", () => {
  for (const q of ["draw a chart", "draw a diagram", "show me a picture of the code", "can you draw a chart"]) {
    assert.equal(da.tryDrawAnything(q, report, viewspec, concepts, null, null, null), null, `${q} reaches the AI`);
  }
});

// the model picks the kind; the records fill it — every kind is fillable
test("fillDiagram fills the model's pick from the records", () => {
  const flow = da.fillDiagram("flow", report, viewspec, concepts);
  assert.ok(flow && flow.kind === "flow" && flow.nodes.length > 0);
  const uml = da.fillDiagram("uml", report, viewspec, concepts);
  assert.ok(uml && uml.nodes.length > 0);
  const dfd = da.fillDiagram("dfd", report, viewspec, concepts, "Data flow — the ask");
  assert.ok(dfd && dfd.kind === "uml" && dfd.title === "Data flow — the ask");
  assert.ok(dfd.nodes.some((n) => n.kind === "store"), "the DFD fill has the stores");
  const seq = da.fillDiagram("sequence", report, viewspec, concepts);
  assert.ok(seq && seq.kind === "sequence" && seq.msgs.length > 0, "the sequence fill has ordered messages");
  const c4 = da.fillDiagram("c4", report, viewspec, concepts, undefined, undefined, null, null, null);
  assert.ok(c4 && c4.kind === "c4", "the C4 fill draws from the file table");
  const mind = da.fillDiagram("mindmap", report, viewspec, concepts, undefined, undefined, null, 9, null);
  assert.ok(mind && mind.kind === "tree", "the mindmap fill is the concept tree");
});

test("a specific kind still outranks the generic map", () => {
  const r = da.tryDrawAnything("draw the flow chart", report, viewspec, concepts, null, null, null);
  assert.ok(r && r.chart);
  assert.equal(r.chart.kind, "flow"); // the call flow, not the structure map
  // "draw a bar chart" whose topic the store can't rank goes to the LLM —
  // the model picks the closest kind and the store fills it
  const bar = da.tryDrawAnything("draw a bar chart", report, viewspec, concepts, null, null, null);
  assert.equal(bar, null);
});

test("a question that is not a request still reaches the AI", () => {
  assert.equal(da.tryDrawAnything("explain what this chart shows", report, viewspec, concepts, null, null, null), null);
  assert.equal(da.tryDrawAnything("why is the chart empty", report, viewspec, concepts, null, null, null), null);
});

test("the filled model pick produces a gated-renderable map spec", () => {
  const r = { chart: da.fillDiagram("uml", report, viewspec, concepts, "The map of your project") };
  assert.ok(r.chart, "the uml fill builds");
  const view = require(path.join(__dirname, "helpers/load-ts.cjs"))()(
    path.resolve(__dirname, "../src/lib/viz/chart-spec.ts"),
  ).chartView(r.chart, report, concepts, viewspec);
  assert.ok(view.view, `chartView refused: ${view.violations}`);
  assert.ok(view.view.elements.length > 0);
});

// ---------- the asked KIND is honored, centered on the ask's focus ----------

// "if a dfd is asked, the dfd is drawn": an explicit DFD ask draws the
// data-flow picture from the recorded reads/writes — stores, processes,
// the unresolved reaching box — not the call-flow stand-in and not the map.
test("an explicit DFD ask draws a data-flow chart", () => {
  for (const q of ["draw the dfd", "draw a dfd", "draw the data flow diagram"]) {
    const r = da.tryDrawAnything(q, report, viewspec, concepts, null, null, null);
    assert.ok(r && r.chart, `${q} draws`);
    assert.equal(r.chart.kind, "uml");
    const kinds = new Set(r.chart.nodes.map((n) => n.kind));
    assert.ok(kinds.has("store"), `${q} draws stores`);
    assert.ok(r.chart.nodes.length > r.chart.nodes.filter((n) => n.kind === "store").length, "process boxes drawn too");
    assert.ok(r.answer.includes("data flow"));
  }
});

// a DFD named at a symbol scopes to that symbol's container and says so
test("a DFD of a named symbol centers the picture and names the center", () => {
  const r = da.tryDrawAnything("draw the dfd of allocate_blocks", report, viewspec, concepts, null, null, null);
  assert.ok(r && r.chart);
  assert.ok(r.answer.includes("allocate_blocks"), "the answer names what it centered on");
});

// a bare sequence-diagram ask draws a SEQUENCE — never the flow stand-in
test("a bare sequence-diagram ask draws a sequence with lanes and ordered messages", () => {
  const r = da.tryDrawAnything("draw the sequence diagram", report, viewspec, concepts, null, null, null);
  assert.ok(r && r.chart, "the sequence ask draws");
  assert.equal(r.chart.kind, "sequence");
  assert.ok(r.chart.participants.length > 1, "lifelines exist");
  assert.ok(r.chart.msgs.length > 0, "ordered messages exist");
  assert.ok(r.chart.msgs.every((m) => m.status === "confirmed"), "every arrow is a recorded fact");
});

// the flow ask stays flow — pinned — and a NAMED symbol centers it on that
// symbol's one-hop recorded neighborhood instead of the whole-project wall
test("the flow ask centers on the named symbol when there is one", () => {
  const full = da.buildFlowSpec(report);
  const focused = da.buildFocusedFlowSpec(report, [2], "auth.good_auth", 'Flow around "auth.good_auth"');
  assert.ok(full && focused);
  assert.ok(focused.nodes.length < full.nodes.length, "the slice is a real slice");
  assert.ok(focused.nodes.some((n) => n.id === "2"), "the focus itself is on the picture");
  assert.ok((focused.tell || "").includes("auth.good_auth"), "the tell names the center");
  // a focus outside every recorded call falls to the whole flow, honestly
  const ghost = da.buildFocusedFlowSpec(report, [9999], "ghost", "Ghost");
  assert.ok(ghost && ghost.nodes.length === full.nodes.length);
  assert.ok((ghost.tell || "").includes("ghost"));
});

// the refusal companion: asked kind + focus — the seam the panel calls
test("the refusal companion draws the asked kind centered on the focus", () => {
  const flow = da.buildConceptRefusalChart("flowchart", report, [31]);
  assert.ok(flow);
  assert.equal(flow.kind, "flow");
  assert.ok(flow.nodes.some((n) => n.id === "31"), "the refused symbol is on its picture");
  const seq = da.buildConceptRefusalChart("sequence", report, [2]);
  assert.ok(seq);
  assert.ok(["sequence", "flow"].includes(seq.kind)); // sequence when it can, flow named-as-itself when it can't
});

// "if the uml is asked, the uml is drawn" — and a named symbol ranks its
// own file's boxes first instead of the store's plain order
test("a UML ask with a named symbol ranks that symbol's boxes first", () => {
  const named = report.symbols.find((s) => s.name === "good_auth");
  const focusFile = report.files[named.loc.file];
  const plain = da.buildUmlSpec(report, viewspec, concepts);
  const focused = da.buildUmlSpec(
    report, viewspec, concepts, 'UML-style map — focused on "auth.good_auth"', focusFile,
  );
  assert.ok(plain && focused);
  assert.equal(focused.nodes[0].loc?.file, focusFile, "the focused map's first box is the named symbol's file");
});

// the misspelled kind is NOT found by any vocabulary — and that is the
// point: the ask goes to the configured LLM, which decides the kind; the
// store fills it. No regex guessing on the user's words.
test("a misspelled 'sequnce diagram' ask is left to the configured LLM", () => {
  assert.equal(
    da.tryDrawAnything("show me a sequnce diagram of what happens when send sms is called", report, viewspec, concepts, null, null, null),
    null,
  );
  const parsed = require(path.join(__dirname, "../src/lib/chat-chart.ts") ? path.resolve(__dirname, "../src/lib/chat-chart.ts") : "");
  const pr = require("./helpers/load-ts.cjs")()(path.resolve(__dirname, "../src/lib/chat-chart.ts"));
  const r = pr.parseChartReply('{"answer":"the send_sms story","chart":{"kind":"sequence","title":"send sms","tell":"ordered from the entry","data":null}}');
  assert.ok(r.chart, "the model's sequence pick survives sanitizing");
  assert.equal(r.chart.kind, "sequence");
  const dfdReply = pr.parseChartReply('{"answer":"here","chart":{"kind":"dfd","title":"Data flow","tell":"stores and processes","data":null}}');
  assert.equal(dfdReply.chart.kind, "dfd", "the model's dfd pick survives sanitizing");
  const seq = da.fillDiagram("sequence", report, viewspec, concepts, "send sms");
  assert.ok(seq && seq.msgs.length > 0, "what the model picked is drawn from the records");
});

// the ask's context survives spoken names: "record audit log" -> audit_log
test("the question's context resolves spoken symbol names", () => {
  const q = require("./helpers/load-ts.cjs")()(path.resolve(__dirname, "../src/lib/quality.ts"));
  const named = q.findSymbolInQuestion(report, "show me a sequence diagram of what happens when the audit log is called");
  assert.ok(named, "audit log reaches audit_log");
  assert.ok(named.name.startsWith("audit_log"), `got ${named.name}`);
  assert.equal(q.findSymbolInQuestion(report, "what did the engine trace in this project"), null, "a question naming no symbol gets no invented focus");
  assert.equal(q.findSymbolInQuestion(report, "how does the audit log work").name, "audit_log", "the spoken two-word name beats the generic 'log' symbol");
});
