const test = require("node:test"),
  assert = require("node:assert/strict"),
  path = require("node:path");
const loader = require("./helpers/load-ts.cjs");
const src = (name) => path.resolve(__dirname, "../src", name);
const spec = () => ({
  elements: [
    {
      id: "a",
      label: "A",
      elementKind: "process",
      displayMode: "Fact",
      evidence: { file: "a.py", start: 0, end: 10 },
      fields: {},
      salience: { factors: ["STORE_RECORD"], weight: 1, whyShown: "test" },
    },
  ],
  edges: [],
  footer: {
    facts: 1,
    inferences: 0,
    hypotheses: 0,
    fog: 0,
    stale: 0,
    caption: "test",
  },
  hidden: [],
  gaps: [],
  scope: { revision: 1, environments: [], coverage: { analyzed: true } },
  semanticHash: "",
  renderHash: "",
  layoutHint: { strategy: "dagre" },
});
test("G6 host supplies adapted nodes to canvas and suppresses selection feedback", async () => {
  let cfg, graph;
  class Graph {
    constructor(options) {
      cfg = options;
      graph = this;
      this.events = {};
    }
    async render() {}
    setLayout() {}
    async setElementState() {}
    on(name, cb) {
      this.events[name] = cb;
    }
    off() {}
    destroy() {}
  }
  const load = loader({
    "@antv/g6": { Graph },
    "@antv/hierarchy": {
      compactBox() {
        throw Error("not a tree");
      },
    },
  });
  const { renderViaNegotiator } = load(src("lib/viz/g6-engine.ts"));
  const result = await renderViaNegotiator({}, spec());
  assert.equal(cfg.data.nodes[0].id, "a");
  let calls = 0;
  result.handle.onSelection((ids) => {
    calls++;
    result.handle.setSelected(ids, "programmatic");
  });
  graph.events["node:click"]({ target: { id: "a" } });
  assert.equal(calls, 1);
  assert.equal(result.handle.layout.actual, "dagre");
  const oldHash = result.handle.renderHash;
  const { g6HostRuntime } = load(src("lib/viz/g6-engine.ts"));
  await g6HostRuntime().relayout("g6", result.handle, { strategy: "grid" });
  assert.equal(result.handle.layout.actual, "grid");
  assert.notEqual(result.handle.renderHash, oldHash);
  await result.handle.dispose();
});
test("scope slicing keeps ancestry, recomputes ledgers/hashes and filters inferred edges", () => {
  const load = loader(),
    { sliceViewSpec } = load(src("lib/viz/slice.ts"));
  const v = spec();
  v.elements.push(
    {
      ...v.elements[0],
      id: "b",
      parentId: "a",
      displayMode: "Inference",
      claimId: "c",
    },
    { ...v.elements[0], id: "unrelated" },
  );
  v.edges = [
    {
      id: "ab",
      source: "a",
      target: "b",
      edgeKind: "call",
      displayMode: "Inference",
      evidence: v.elements[0].evidence,
    },
  ];
  const before = JSON.stringify(v);
  const sliced = sliceViewSpec(v, { keepIds: new Set(["b"]) });
  assert.deepEqual(
    sliced.spec.elements.map((e) => e.id),
    ["a", "b"],
  );
  assert.equal(sliced.spec.footer.inferences, 1);
  assert.equal(sliced.dropped, 1);
  assert.equal(sliced.spec.hidden[0].id, "unrelated");
  assert.ok(sliced.spec.semanticHash);
  const facts = sliceViewSpec(v, { factsOnly: true });
  assert.equal(facts.spec.edges.length, 0);
  assert.equal(JSON.stringify(v), before);
  v.elements[1].claimId = undefined;
  assert.equal(sliceViewSpec(v).spec, null);
});
test("plural context deduplicates, bounds and clears stale revisions", () => {
  const { addContext } = loader()(src("lib/chat-context.ts"));
  let chips = [];
  for (let i = 0; i < 30; i++)
    chips = addContext(chips, {
      id: String(i),
      revision: 1,
      files: [],
      label: String(i),
      kind: "test",
    });
  assert.equal(chips.length, 24);
  chips = addContext(chips, { ...chips[0], label: "updated" });
  assert.equal(chips.length, 24);
  assert.equal(chips.at(-1).label, "updated");
  assert.equal(
    addContext(chips, {
      id: "new",
      revision: 2,
      files: [],
      label: "new",
      kind: "test",
    }).length,
    1,
  );
});
test("conductor broadcasts valid IDs once and detaches disposed charts", () => {
  const load = loader(),
    { SelectionConductor, createSelectionHub } = load(
      path.resolve(__dirname, "../../viz-adapter/src/index.ts"),
    );
  const hubA = createSelectionHub(),
    hubB = createSelectionHub();
  const a = { onSelection: hubA.subscribe, setSelected: hubA.setSelected },
    b = { onSelection: hubB.subscribe, setSelected: hubB.setSelected };
  const conductor = new SelectionConductor();
  let calls = 0;
  conductor.attach(a, ["a", "b"], () => calls++);
  const off = conductor.attach(b, ["b"], () => calls++);
  hubA.setSelected(["b"], "user");
  assert.deepEqual(hubB.selected(), ["b"]);
  assert.equal(calls, 1);
  off();
  hubA.setSelected(["a"], "user");
  assert.deepEqual(hubB.selected(), ["b"]);
});
test("scoped facts exclude other files, symbols, findings, calls and ancestor rollups", () => {
  const { scopeDigest } = loader()(src("lib/scope-digest.ts"));
  const report = {
    rev: 3,
    files: ["a.py", "secret.py"],
    symbols: [
      { id: 0, name: "a", qual_name: "a", loc: { file: 0 } },
      { id: 1, name: "secret", qual_name: "secret", loc: { file: 1 } },
    ],
    summaries: [
      { sym: 0, callees: ["secret"], unresolved_callees: [] },
      { sym: 1, callees: [] },
    ],
    findings: [{ symbol: 0 }, { symbol: 1 }],
    contracts: [{ symbols: [0, 1] }],
    coverage: {},
    evidence: {},
    warnings: ["secret"],
  };
  const chips = [{ id: "a", label: "A", revision: 3, files: ["a.py"] }];
  const tree = {
    nodes: [
      { id: 0, symbols: [0, 1] },
      { id: 1, symbols: [0], parent: 0, children: [] },
    ],
    unattached_behaviors: [],
    unattached_rules: [],
  };
  const scoped = scopeDigest(report, null, tree, chips);
  assert.deepEqual(scoped.report.files, ["a.py", ""]);
  assert.equal(scoped.report.symbols.length, 1);
  assert.equal(scoped.report.findings.length, 1);
  assert.deepEqual(scoped.report.summaries[0].callees, []);
  assert.equal(scoped.report.contracts.length, 0);
  assert.deepEqual(
    scoped.concepts.nodes.map((n) => n.id),
    [1],
  );
  assert.equal(scoped.concepts.nodes[0].parent, null);
  assert.match(scoped.scope.header, /1 files over 1/);
  assert.equal(scopeDigest(report, null, null, []).report, report);
  assert.equal(
    scopeDigest(report, null, null, [{ ...chips[0], revision: 2 }]).report
      .symbols.length,
    0,
  );
  assert.equal(
    scopeDigest(report, null, null, [{ ...chips[0], files: [] }]).report.symbols
      .length,
    0,
  );
});
test("catalog bridge covers every kind without upgrading inferred boxes or edges", () => {
  const { chartView, CHAT_CATALOG } = loader()(src("lib/viz/chart-spec.ts"));
  assert.deepEqual(
    Object.keys(CHAT_CATALOG).sort(),
    [
      "hbar",
      "donut",
      "stat",
      "table",
      "uml",
      "flow",
      "tree",
      "heatmap",
      "sequence",
      "c4",
      "mindmap",
    ].sort(),
  );
  const diagram = {
    kind: "flow",
    nodes: [
      {
        id: "a",
        title: "A",
        kind: "function",
        certainty: "Inference",
        loc: { file: "a.py", byteStart: 0, byteEnd: 8 },
      },
      { id: "b", title: "B", kind: "function", loc: { file: "b.py" } },
    ],
    links: [{ from: "a", to: "b", certainty: "Inference" }],
  };
  const { view } = chartView(
    diagram,
    { rev: 1, files: [], symbols: [], coverage: { complete: false } },
    null,
  );
  assert.equal(view.elements[0].displayMode, "Inference");
  assert.equal(view.edges[0].displayMode, "Inference");
  const { gate } = loader()(
    path.resolve(__dirname, "../../viz-adapter/src/index.ts"),
  );
  gate(view);
  const aggregate = chartView(
    { kind: "stat", data: [{ name: "Total", value: 42 }] },
    null,
    null,
  );
  assert.equal(aggregate.view.elements.length, 0);
  assert.match(aggregate.view.hidden[0].reason, /No recorded source/);
  assert.match(aggregate.entries[0].label, /42/);
});
test("sequence preserves recorded order without asserting observed runtime", () => {
  const { chartView } = loader()(src("lib/viz/chart-spec.ts"));
  const { view } = chartView(
    {
      kind: "sequence",
      msgs: [
        {
          from: "A",
          to: "B",
          label: "call",
          status: "confirmed",
          loc: { file: "a.py" },
        },
        {
          from: "B",
          to: "C",
          label: "next",
          status: "inferred",
          loc: { file: "b.py" },
        },
      ],
    },
    null,
    null,
  );
  assert.equal(view.edges.length, 1);
  assert.match(view.edges[0].edgeKind, /not runtime/);
  assert.equal(view.edges[0].displayMode, "Inference");
});
test("provider route sends selection-only instructions and discards previous whole-project turns", async () => {
  let sent;
  const load = loader({
    "next/server": {
      NextResponse: {
        json: (body, init) => ({ body, status: init?.status ?? 200 }),
      },
    },
    "@/lib/llm-providers": {
      sanitizeLlmCfg: () => ({ provider: "openai", openai: {} }),
      chatOpenAi: async (_cfg, messages) => {
        sent = messages;
        return '{"answer":"Scoped answer","chart":null}';
      },
      LlmError: class extends Error {},
    },
  });
  const { POST } = load(src("app/api/chat/route.ts"));
  const body = {
    scope: { files: ["a.py"] },
    digest: { answer_scope: { files: ["a.py"] } },
    messages: [
      { role: "assistant", content: "OUTSIDE WHOLE PROJECT DETAIL" },
      { role: "user", content: "Explain the selection" },
    ],
  };
  const response = await POST({ json: async () => body });
  assert.equal(response.status, 200);
  assert.equal(sent.length, 2);
  assert.match(sent[0].content, /ONLY from the selection code space/);
  assert.doesNotMatch(JSON.stringify(sent), /OUTSIDE WHOLE PROJECT DETAIL/);
  const rejected = await POST({ json: async () => ({ ...body, digest: {} }) });
  assert.equal(rejected.status, 400);
});
