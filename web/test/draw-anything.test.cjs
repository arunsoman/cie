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

test("a bare 'draw a chart' draws the structure map — never an AI menu", () => {
  const r = da.tryDrawAnything("draw a chart", report, viewspec, concepts, null, null, null);
  assert.ok(r, "the generic ask is answered locally");
  assert.ok(r.chart, "the answer carries a chart");
  assert.equal(r.chart.kind, "uml");
  assert.ok(r.chart.nodes.length > 0, "the map has the project's parts");
});

test("'draw a diagram' and 'show me a picture of the code' draw it too", () => {
  for (const q of ["draw a diagram", "show me a picture of the code", "can you draw a chart"]) {
    const r = da.tryDrawAnything(q, report, viewspec, concepts, null, null, null);
    assert.ok(r && r.chart, `${q} draws`);
    assert.equal(r.chart.kind, "uml");
  }
});

test("a specific kind still outranks the generic map", () => {
  const r = da.tryDrawAnything("draw the flow chart", report, viewspec, concepts, null, null, null);
  assert.ok(r && r.chart);
  assert.equal(r.chart.kind, "flow"); // the call flow, not the structure map
  // "draw a bar chart" with no bar topic the demo can rank falls through
  // to the generic map — honestly, with the kinds named in the answer
  const bar = da.tryDrawAnything("draw a bar chart", report, viewspec, concepts, null, null, null);
  assert.ok(bar && bar.chart);
  assert.ok(bar.answer.includes("say the word") || bar.chart.kind === "hbar");
});

test("a question that is not a request still reaches the AI", () => {
  assert.equal(da.tryDrawAnything("explain what this chart shows", report, viewspec, concepts, null, null, null), null);
  assert.equal(da.tryDrawAnything("why is the chart empty", report, viewspec, concepts, null, null, null), null);
});

test("the real demo produces a gated-renderable map spec", () => {
  const r = da.tryDrawAnything("draw a chart", report, viewspec, concepts, null, null, null);
  assert.ok(r && r.chart);
  const view = require(path.join(__dirname, "helpers/load-ts.cjs"))()(
    path.resolve(__dirname, "../src/lib/viz/chart-spec.ts"),
  ).chartView(r.chart, report, concepts, viewspec);
  assert.ok(view.view, `chartView refused: ${view.violations}`);
  assert.ok(view.view.elements.length > 0);
});
