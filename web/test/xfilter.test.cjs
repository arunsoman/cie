// XFILTER — cross-chart slice & dice: the pure core under the real demo
// artifacts. The banking sample (web/public/data/report.json) is the fixture
// with real variance — four severities, ten finding kinds, twelve effects,
// Explicit AND InferredCandidate contracts. Every claim below is checked
// against independently recomputed counts, never the module's own output
// alone. Chart kinds covered: horizontal bars (severity/kind/file/effect),
// donuts (promises/certainty), row lists (findings/contracts predicates),
// and the map slice — the four receiver surfaces.
const test = require("node:test"),
  assert = require("node:assert/strict"),
  path = require("node:path");
const loader = require("./helpers/load-ts.cjs");
const cf = loader()(path.resolve(__dirname, "../src/lib/chart-filter.ts"));
const REPORT = require(path.resolve(__dirname, "../public/data/report.json")); // banking demo
const VIEWSPEC = require(path.resolve(__dirname, "../public/data/viewspec.json"));

const sevCount = (sev) => REPORT.findings.filter((f) => f.severity === sev).length;

test("nextChartFilter: toggle-release, replace, explicit clear", () => {
  const f = { dimension: "severity", value: "High", label: "Please look", origin: "severity", originLabel: "How urgent are the problems?" };
  const g = { dimension: "kind", value: "DeadCode", label: "Dead code", origin: "kind", originLabel: "What kinds of problems?" };
  assert.equal(cf.nextChartFilter(null, f), f, "no filter + click = filter set");
  assert.equal(cf.nextChartFilter(f, f), null, "same bucket again = released");
  assert.equal(cf.nextChartFilter(f, g), g, "different bucket = replaced");
  assert.equal(cf.nextChartFilter(g, null), null, "explicit clear = cleared");
  assert.equal(cf.nextChartFilter(null, null), null, "clear with nothing active stays null");
  // same value, different dimension must NOT toggle-release (it's a replace)
  assert.equal(cf.nextChartFilter(f, { ...f, dimension: "kind" }).dimension, "kind");
});

test("filterApplies matrix: each dimension only touches the universes that carry it", () => {
  const mk = (dimension) => ({ dimension, value: "x", label: "x", origin: dimension, originLabel: "t" });
  const universes = ["findings", "summaries", "contracts", "elements", "canvas"];
  const expected = {
    severity: ["findings"],
    kind: ["findings"],
    file: universes,
    effect: ["summaries"],
    authority: ["contracts"],
    certainty: ["elements", "canvas"],
  };
  for (const [dim, yes] of Object.entries(expected)) {
    for (const u of universes) {
      assert.equal(cf.filterApplies(mk(dim), u), yes.includes(u), `${dim} × ${u}`);
    }
  }
  assert.equal(cf.filterApplies(null, "findings"), false, "no filter applies nowhere");
});

test("severity rows: the emitter's bars carry raw keys and honest counts", () => {
  const rows = cf.severityRows(REPORT.findings);
  assert.deepEqual(
    rows.map((r) => r.value),
    ["Critical", "High", "Medium", "Low"],
    "raw severity keys — the exact values the predicate matches",
  );
  for (const r of rows) assert.equal(r.count, sevCount(r.value), `${r.value} count`);
  assert.equal(rows.reduce((a, r) => a + r.count, 0), REPORT.findings.length);
  assert.equal(rows[0].name, "Fix soon", "plain-words labels on the axis");
});

test("receiver chain: severity=High re-aggregates kind and file charts from the SAME rows", () => {
  const high = { dimension: "severity", value: "High", label: "Please look", origin: "severity", originLabel: "t" };
  const findings = cf.filteredFindings(REPORT, high);
  assert.equal(findings.length, sevCount("High"));
  assert.ok(findings.every((f) => f.severity === "High"));
  // the sibling kind chart re-aggregates from the filtered rows only
  const kinds = cf.kindRows(findings);
  assert.equal(kinds.reduce((a, r) => a + r.count, 0), findings.length, "kind chart sum = surviving findings");
  // and the sibling file chart: independently recompute the file counts
  const fileOf = (sym) => {
    const s = REPORT.symbols.find((x) => x.id === sym);
    return s ? REPORT.files[s.loc.file] : null;
  };
  const byFile = new Map();
  for (const f of findings) {
    const p = fileOf(f.symbol);
    if (p) byFile.set(p, (byFile.get(p) ?? 0) + 1);
  }
  const files = cf.fileRows(findings, REPORT);
  assert.equal(files.reduce((a, r) => a + r.count, 0), [...byFile.values()].reduce((a, b) => a + b, 0));
  for (const r of files) assert.equal(r.count, byFile.get(r.value) ?? 0, `file ${r.value}`);
  const pathSet = new Set(REPORT.files);
  assert.ok(files.every((r) => pathSet.has(r.value)), "file keys are FULL recorded paths");
  assert.ok(files.every((r) => (r.value.split("/").pop() ?? r.value).startsWith(r.name.replace("…", ""))), "labels are basenames (truncated)");
});

test("kind filter: emitter key = raw kind, predicate agrees, cross-chart sums hold", () => {
  const rows = cf.kindRows(REPORT.findings);
  const top = rows[0];
  const expected = REPORT.findings.filter((f) => f.kind === top.value).length;
  assert.equal(top.count, expected, "the clicked bar's own count");
  const findings = cf.filteredFindings(REPORT, {
    dimension: "kind", value: top.value, label: top.full, origin: "kind", originLabel: "t",
  });
  assert.equal(findings.length, expected);
  assert.ok(findings.every((f) => f.kind === top.value));
  // the severity chart re-aggregates: its buckets now sum to the survivors
  const sev = cf.severityRows(findings);
  assert.equal(sev.reduce((a, r) => a + r.count, 0), findings.length);
});

test("file filter: one dimension, four universes", () => {
  const fileOf = (sym) => {
    const s = REPORT.symbols.find((x) => x.id === sym);
    return s ? REPORT.files[s.loc.file] : null;
  };
  const rows = cf.fileRows(REPORT.findings, REPORT);
  const top = rows[0];
  const filter = { dimension: "file", value: top.value, label: top.value, origin: "file", originLabel: "t" };
  const findings = cf.filteredFindings(REPORT, filter);
  const expectedFindings = REPORT.findings.filter((f) => fileOf(f.symbol) === top.value).length;
  assert.equal(findings.length, expectedFindings, "findings universe");
  assert.ok(findings.every((f) => fileOf(f.symbol) === top.value));

  const summaries = cf.filteredSummaries(REPORT, filter);
  const expectedSummaries = REPORT.summaries.filter((s) => fileOf(s.sym) === top.value).length;
  assert.equal(summaries.length, expectedSummaries, "summaries universe");

  const contracts = cf.filteredContracts(REPORT, filter);
  const expectedContracts = REPORT.contracts.filter((c) => c.symbols.some((id) => fileOf(id) === top.value)).length;
  assert.equal(contracts.length, expectedContracts, "contracts universe");

  const els = VIEWSPEC.elements.filter((e) => e.evidence?.file === top.value);
  assert.equal(els.length, VIEWSPEC.elements.filter((e) => cf.elementMatches(e, filter)).length, "elements universe");
  // the effect chart does NOT react to a file filter when rows lack it —
  // but summaries DO carry files, so effectRows re-aggregates from survivors
  const effects = cf.effectRows(summaries);
  assert.ok(effects.every((r) => summaries.some((s) => (s.effects ?? []).includes(r.value))), "every surviving effect comes from a surviving summary");
});

test("promise donut: explicit/inferred pseudo-buckets sum to every contract", () => {
  const rows = cf.promiseRows(REPORT.contracts);
  assert.deepEqual(rows.map((r) => r.key), ["explicit", "inferred"]);
  const explicit = REPORT.contracts.filter((c) => c.authority === "Explicit").length;
  assert.equal(rows[0].value, explicit);
  assert.equal(rows[1].value, REPORT.contracts.length - explicit);
  assert.equal(rows.reduce((a, r) => a + r.value, 0), REPORT.contracts.length);
  // the filter agrees with the chart's own buckets
  const keptExplicit = cf.filteredContracts(REPORT, { dimension: "authority", value: "explicit", label: "Written by you", origin: "authority", originLabel: "t" });
  assert.equal(keptExplicit.length, explicit);
  assert.ok(keptExplicit.every((c) => c.authority === "Explicit"));
  const keptInferred = cf.filteredContracts(REPORT, { dimension: "authority", value: "inferred", label: "CIE's guess", origin: "authority", originLabel: "t" });
  assert.equal(keptInferred.length + keptExplicit.length, REPORT.contracts.length, "the two buckets partition the list");
});

test("effect filter: the clicked bar's value is the raw effect id", () => {
  const rows = cf.effectRows(REPORT.summaries);
  assert.ok(rows.length >= 10, `banking demo has real effect variance (${rows.length} kinds)`);
  const top = rows[0];
  const expected = REPORT.summaries.filter((s) => (s.effects ?? []).includes(top.value)).length;
  const summaries = cf.filteredSummaries(REPORT, { dimension: "effect", value: top.value, label: top.full, origin: "effect", originLabel: "t" });
  assert.equal(summaries.length, expected);
  assert.ok(summaries.every((s) => (s.effects ?? []).includes(top.value)));
  assert.equal(cf.filterApplies({ dimension: "effect", value: "x", label: "x", origin: "effect", originLabel: "t" }, "findings"), false, "findings charts untouched by an effect filter");
});

test("certainty donut + map slice: parent chains kept, edges gated, omissions disclosed", () => {
  const spec = {
    elements: [
      { id: "d", label: "Domain", elementKind: "domain", displayMode: "Fact", evidence: { file: "d.py", start: 0, end: 5 } },
      { id: "a", label: "A", elementKind: "module", displayMode: "Fact", evidence: { file: "a.py", start: 0, end: 5 }, parentId: "d" },
      { id: "b", label: "B", elementKind: "usecase", displayMode: "Hypothesis", evidence: { file: "b.py", start: 0, end: 5 }, parentId: "a" },
      { id: "c", label: "C", elementKind: "rule", displayMode: "Inference", evidence: { file: "a.py", start: 6, end: 9 }, parentId: "a" },
    ],
    edges: [
      { id: "e1", source: "a", target: "b", edgeKind: "has", displayMode: "Fact" },
      { id: "e2", source: "b", target: "c", edgeKind: "guards", displayMode: "Inference" },
      { id: "e3", source: "c", target: "d", edgeKind: "in", displayMode: "Inference" },
    ],
    footer: { facts: 2, inferences: 1, hypotheses: 1, fog: 0, stale: 0, caption: "cap" },
    gaps: [],
    hidden: [],
  };
  // certainty = Fact: a and its parent chain (d); b/c dropped. Edges follow
  // ENDPOINTS (the scope-slice rule) — e1 needs b, e3 needs c, so none stay.
  const fact = { dimension: "certainty", value: "Fact", label: "Proven", origin: "certainty", originLabel: "t" };
  const sliced = cf.sliceMapSpec(spec, fact);
  assert.ok(sliced, "certainty applies to the map");
  assert.deepEqual([...sliced.spec.elements.map((e) => e.id)].sort(), ["a", "d"], "kept + parent closure");
  assert.deepEqual(sliced.spec.edges, [], "edges need both ends kept");
  assert.equal(sliced.kept, 2);
  assert.equal(sliced.total, 4);
  assert.equal(sliced.spec.footer.facts, 2, "footer recomputed from the FINAL element set");
  assert.equal(sliced.spec.footer.inferences, 0);
  assert.equal(sliced.spec.footer.caption, "cap", "caption preserved");
  assert.ok(sliced.spec.gaps.some((g) => g.id === "xfilter-omissions" && g.why.includes("2 of 4")), "omissions named in the ledger");

  // certainty = Hypothesis: b survives and drags its parent chain in; the
  // a-b edge survives on endpoints (its own Fact colour still discloses)
  const hyp = { dimension: "certainty", value: "Hypothesis", label: "Not sure yet", origin: "certainty", originLabel: "t" };
  const slicedH = cf.sliceMapSpec(spec, hyp);
  assert.deepEqual([...slicedH.spec.elements.map((e) => e.id)].sort(), ["a", "b", "d"], "a kept child keeps its parents");
  assert.deepEqual(slicedH.spec.edges.map((e) => e.id), ["e1"], "endpoint rule keeps the parent link");

  // a dimension the map doesn't carry → null (map renders unchanged)
  assert.equal(cf.sliceMapSpec(spec, { dimension: "severity", value: "High", label: "x", origin: "severity", originLabel: "t" }), null);
});

test("map slice on the REAL viewspec: file filter keeps only that file's bubbles (+parents)", () => {
  const byFile = new Map();
  for (const e of VIEWSPEC.elements) byFile.set(e.evidence.file, (byFile.get(e.evidence.file) ?? 0) + 1);
  const top = [...byFile.entries()].sort((a, b) => b[1] - a[1])[0][0];
  const sliced = cf.sliceMapSpec(VIEWSPEC, { dimension: "file", value: top, label: top, origin: "file", originLabel: "t" });
  assert.ok(sliced && sliced.kept > 0, "the busiest file keeps bubbles");
  assert.equal(sliced.kept, sliced.spec.elements.length);
  // every omitted element is honestly absent; every kept element either
  // matches or is an ancestor of a matcher
  const matchers = new Set(VIEWSPEC.elements.filter((e) => e.evidence.file === top).map((e) => e.id));
  const byId = new Map(VIEWSPEC.elements.map((e) => [e.id, e]));
  const isAncestorOfMatcher = (el) => {
    for (const m of matchers) {
      let cur = byId.get(m);
      while (cur) {
        if (cur.id === el.id) return true;
        cur = cur.parentId ? byId.get(cur.parentId) : undefined;
      }
    }
    return false;
  };
  for (const e of sliced.spec.elements) {
    assert.ok(matchers.has(e.id) || isAncestorOfMatcher(e), `${e.id} matches or is an ancestor of one`);
  }
  assert.equal(sliced.total, VIEWSPEC.elements.length, "total is the unfiltered map size");
  assert.ok(sliced.spec.edges.every((e) => {
    const ids = new Set(sliced.spec.elements.map((x) => x.id));
    return ids.has(e.source) && ids.has(e.target);
  }), "edges only between kept ends");
  assert.equal(cf.sliceMapSpec(VIEWSPEC, { dimension: "authority", value: "explicit", label: "x", origin: "authority", originLabel: "t" }), null, "non-applying dimension leaves the map alone");
});

test("certainty rows count elements AND edges, from the sliced spec when filtered", () => {
  const rows = cf.certaintyRows(VIEWSPEC);
  const total = VIEWSPEC.elements.length + VIEWSPEC.edges.length;
  assert.equal(rows.reduce((a, r) => a + r.value, 0), total, "sum covers every element and edge");
  // the real banking viewspec: every ELEMENT is an Inference, but its edges
  // carry both Fact and Inference — the donut shows both, honestly
  const elModes = new Set(VIEWSPEC.elements.map((e) => e.displayMode));
  assert.deepEqual([...elModes], ["Inference"]);
  const inferenceRow = rows.find((r) => r.key === "Inference");
  const factRow = rows.find((r) => r.key === "Fact");
  assert.equal(inferenceRow.value, VIEWSPEC.elements.length + VIEWSPEC.edges.filter((e) => e.displayMode === "Inference").length);
  assert.equal(factRow.value, VIEWSPEC.edges.filter((e) => e.displayMode === "Fact").length);
  // slicing the map to "Fact" keeps only Fact ELEMENTS — zero here: the
  // named empty case on the map page
  const sliced = cf.sliceMapSpec(VIEWSPEC, { dimension: "certainty", value: "Fact", label: "Proven", origin: "certainty", originLabel: "t" });
  assert.equal(sliced.kept, 0, "named empty: nothing on the map is Fact");
});

test("disclosures: countText, filterNoun, filterDestination", () => {
  assert.equal(cf.countText(12, 40), "12 of 40");
  assert.equal(cf.filterNoun("severity"), "problems");
  assert.equal(cf.filterNoun("file"), "problems");
  assert.equal(cf.filterNoun("effect"), "side effects");
  assert.equal(cf.filterNoun("authority"), "promises");
  assert.equal(cf.filterNoun("certainty"), "map things");
  assert.equal(cf.filterDestination("kind"), "fixes");
  assert.equal(cf.filterDestination("effect"), "does");
  assert.equal(cf.filterDestination("authority"), "promises");
  assert.equal(cf.filterDestination("certainty"), "map");
});
