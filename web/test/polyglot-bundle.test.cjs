// The polyglot bundle seam, tested from the CLIENT side.
//
// Fixtures:
//   - fixtures/polyglot-bundle.json — a REAL bundle produced by the engine
//     (`cie polyglot tests/fixtures/polyglot --viewspec all`), committed so
//     this suite is hermetic. The contract under test is the one the whole
//     UI rides on: every spec the producer emitted maps through
//     lib/viz/polyglot-spec.ts and PASSES the viz-adapter gate unchanged.
//   - synthetic bundles below pin the mapper's fill-in rules (salience and
//     fields defaults, edge ids, footer counts, gap mapping, layout
//     fallback, per-artifact gate refusals).

const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const { gate } = load(path.resolve(__dirname, "../../viz-adapter/src/gate.ts"));
const {
  artifactKind,
  artifactOrder,
  artifactMeta,
  mapPolyglotSpec,
  polyglotArtifacts,
} = load(path.resolve(__dirname, "../src/lib/viz/polyglot-spec.ts"));

const fixturePath = path.resolve(__dirname, "fixtures/polyglot-bundle.json");
const hasFixture = fs.existsSync(fixturePath);

const span = { file: "src/a.py", start: 0, end: 9 };

function el(id, displayMode = "Fact", extra = {}) {
  return {
    id,
    elementKind: "symbol",
    displayMode,
    label: id,
    evidence: span,
    ...(displayMode === "Inference" || displayMode === "Hypothesis"
      ? { claimId: `C-${id}` }
      : {}),
    ...(displayMode === "Hypothesis" ? { counterArgument: "not proven" } : {}),
    ...(displayMode === "Fog" ? { fogReason: "cannot know statically" } : {}),
    ...extra,
  };
}

test("fixture: a REAL engine bundle maps and passes the gate, spec by spec", { skip: !hasFixture && "fixture not generated yet" }, () => {
  const bundle = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  assert.equal(bundle.kind, "cie-polyglot-viewspec-bundle");
  const artifacts = polyglotArtifacts(bundle, { revision: 7 });
  assert.equal(artifacts.length, Object.keys(bundle.specs).length);
  assert.equal(
    bundle["spec-count"],
    Object.keys(bundle.specs).length,
    "spec-count matches the specs object",
  );
  for (const a of artifacts) {
    assert.ok(a.view, `${a.name}: mapped spec must pass the gate (violations: ${a.violations.join("; ")})`);
    assert.deepEqual(a.violations, []);
    // the mapped view re-passes an INDEPENDENT gate run
    gate(a.view);
    // every element and edge keeps a well-formed byte-span evidence ref
    for (const e of a.view.elements) {
      assert.ok(e.evidence && e.evidence.file.length > 0, `${a.name}/${e.id}: element evidence`);
      assert.ok(typeof e.evidence.start === "number" && typeof e.evidence.end === "number");
      assert.ok(e.salience && Number.isFinite(e.salience.weight), `${a.name}/${e.id}: salience present`);
      assert.ok(e.fields && typeof e.fields === "object");
    }
    for (const e of a.view.edges) {
      assert.ok(e.id && e.id.length > 0, "edge id synthesized");
      assert.ok(e.evidence && e.evidence.file.length > 0, `${a.name}: edge evidence`);
    }
    // footer counts agree with the final element/edge sets
    const count = (m) =>
      a.view.elements.filter((x) => x.displayMode === m).length +
      a.view.edges.filter((x) => x.displayMode === m).length;
    assert.equal(a.view.footer.facts, count("Fact"));
    assert.equal(a.view.footer.inferences, count("Inference"));
    assert.equal(a.view.footer.hypotheses, count("Hypothesis"));
    assert.equal(a.view.footer.fog, count("Fog"));
  }
});

test("fixture: stable presentation order — c4, workflows, state machines, impact", { skip: !hasFixture && "fixture not generated yet" }, () => {
  const bundle = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  const artifacts = polyglotArtifacts(bundle, {});
  const kinds = artifacts.map((a) => a.kind);
  const firstOf = (k) => kinds.indexOf(k);
  if (kinds.includes("c4") && kinds.includes("workflows"))
    assert.ok(firstOf("c4") < firstOf("workflows"));
  if (kinds.includes("workflows") && kinds.includes("state-machine"))
    assert.ok(firstOf("workflows") < firstOf("state-machine"));
  if (kinds.includes("state-machine") && kinds.includes("impact"))
    assert.ok(firstOf("state-machine") < firstOf("impact"));
});

test("artifactKind classifies the closed bundle vocabulary", () => {
  assert.equal(artifactKind("c4"), "c4");
  assert.equal(artifactKind("workflows"), "workflows");
  assert.equal(artifactKind("state-machine:Order"), "state-machine");
  assert.equal(artifactKind("impact:42"), "impact");
});

test("artifactOrder is a total order over the vocabulary", () => {
  assert.ok(artifactOrder("c4") < artifactOrder("workflows"));
  assert.ok(artifactOrder("workflows") < artifactOrder("state-machine:A"));
  assert.ok(artifactOrder("state-machine:A") < artifactOrder("impact:1"));
  assert.equal(artifactOrder("impact:1"), artifactOrder("impact:9"), "all impacts share one tier");
});

test("artifactMeta derives human labels per artifact", () => {
  assert.equal(artifactMeta("c4", { elements: [], edges: [] }).label, "C4 — containers and components");
  assert.equal(
    artifactMeta("state-machine:Order", { elements: [], edges: [], footer: { tier: "Inference" } }).label,
    "State machine — Order (Inference)",
  );
  const impactSpec = {
    elements: [{ id: "center", label: "pay()", fields: { role: "center" } }],
    edges: [],
  };
  assert.equal(artifactMeta("impact:42", impactSpec).label, "Impact — pay()");
});

test("mapper fill-ins: salience, fields, edge ids, footer, gaps, scope, hashes, layout", () => {
  const spec = {
    elements: [
      el("a"),
      el("b", "Inference"),
    ],
    edges: [
      { source: "a", target: "b", edgeKind: "calls", displayMode: "Fact", evidence: span },
      { source: "b", target: "a", edgeKind: "calls", displayMode: "Fact", evidence: span },
    ],
    gaps: [{ reason: "3 unnamed callees stay counted, not shown" }],
    footer: { note: "producer note" },
    layoutHint: { strategy: "cie-state-machine" },
    scope: { artifact: "state-machine", tier: "Fact" },
  };
  const { view, violations } = mapPolyglotSpec(spec, { revision: 3, artifact: "state-machine:X" });
  assert.deepEqual(violations, []);
  // defaults
  assert.equal(view.elements[0].salience.weight, 1, "Fact default salience weight 1");
  assert.equal(view.elements[1].salience.weight, 0.6, "non-Fact default salience weight");
  assert.deepEqual(view.elements[0].fields, {});
  // edge ids unique and stable
  assert.notEqual(view.edges[0].id, view.edges[1].id);
  assert.ok(view.edges[0].id.includes("a->b"));
  // footer counts elements AND edges (a Fact + 2 Fact edges = 3, b Inference)
  assert.equal(view.footer.facts, 3);
  assert.equal(view.footer.inferences, 1);
  assert.equal(view.footer.caption, "producer note");
  assert.equal(view.footer.stale, 0);
  // gaps mapped to the adapter shape
  assert.equal(view.gaps.length, 1);
  assert.equal(view.gaps[0].why, "3 unnamed callees stay counted, not shown");
  assert.equal(view.gaps[0].area, "state-machine:X");
  // scope + hashes
  assert.equal(view.scope.revision, 3);
  assert.ok(view.semanticHash.length > 0);
  assert.ok(view.renderHash.length > 0);
  // producer-named layout falls back to the supported baseline
  assert.equal(view.layoutHint.strategy, "dagre");
  // the mapped view passes the gate unmodified
  gate(view);
});

test("producer layoutHint passes through when supported; edgeless never dags", () => {
  // with edges, the producer's supported strategy rides unchanged
  const withEdges = {
    elements: [el("a"), el("b")],
    edges: [{ source: "a", target: "b", edgeKind: "calls", displayMode: "Fact", evidence: span }],
    layoutHint: { strategy: "dagre" },
  };
  assert.equal(mapPolyglotSpec(withEdges).view.layoutHint.strategy, "dagre");
  // the edgeless rule is total: even an explicitly requested layering
  // layout is replaced on an edgeless graph (nothing to layer — a grid
  // places every node and always settles).
  const elk = { elements: [el("a")], edges: [], layoutHint: { strategy: "elk-layered" } };
  assert.equal(mapPolyglotSpec(elk).view.layoutHint.strategy, "grid");
  // an edgeless spec never gets a layering layout: dagre with zero edges
  // never settles (the same rule chartView applies) — honest grid instead.
  const edgeless = { elements: [el("a")], edges: [], layoutHint: { strategy: "dagre" } };
  assert.equal(mapPolyglotSpec(edgeless).view.layoutHint.strategy, "grid");
});

test("a spec the gate refuses is a NAMED result, never a render", () => {
  // non-Fact without a claim: the producer's own mirror refuses this too —
  // but the mapper must not silently upgrade it either.
  const bad = {
    elements: [{ ...el("x", "Inference"), claimId: undefined }],
    edges: [],
  };
  const { view, violations } = mapPolyglotSpec(bad);
  assert.equal(view, null);
  assert.ok(violations.some((v) => v.includes("NON_FACT_WITHOUT_CLAIM")));
  // an unevidenced edge is refused with its stable code
  const badEdge = {
    elements: [el("a")],
    edges: [{ source: "a", target: "a", edgeKind: "calls", displayMode: "Fact", evidence: { file: "", start: 0, end: 0 } }],
  };
  const res = mapPolyglotSpec(badEdge);
  assert.equal(res.view, null);
  assert.ok(res.violations.some((v) => v.includes("EDGE_WITHOUT_EVIDENCE")));
});

test("one refused spec never takes the bundle down", () => {
  const bundle = {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 2,
    "impact-centers-truncated": false,
    specs: {
      c4: { elements: [el("dom")], edges: [] },
      "state-machine:Broken": {
        elements: [{ ...el("s", "Inference"), claimId: undefined }],
        edges: [],
      },
    },
  };
  const artifacts = polyglotArtifacts(bundle, {});
  assert.equal(artifacts.length, 2);
  const broken = artifacts.find((a) => a.name === "state-machine:Broken");
  const good = artifacts.find((a) => a.name === "c4");
  assert.equal(broken.view, null);
  assert.ok(broken.violations.length > 0);
  assert.ok(good.view);
  assert.deepEqual(good.violations, []);
});
