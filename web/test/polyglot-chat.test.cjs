// Chat → named diagrams (PRD v2.0 §25) — the polyglot-chat seam, tested
// from the CLIENT side.
//
// "draw the C4 architecture" typed into Ask CIE must resolve the engine's
// OWN artifact from the bundle — never the store-derived stand-ins — and a
// kind the bundle doesn't hold must get the honest count answer instead of
// a guessed picture. Fixtures:
//   - fixtures/polyglot-bundle.json — the REAL engine bundle (hermetic)
//   - synthetic bundles below pin the pick / order / refusal rules.

const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const {
  tryPolyglotDraw,
  isPolyglotDrawAsk,
} = load(path.resolve(__dirname, "../src/lib/polyglot-chat.ts"));
const { gate } = load(path.resolve(__dirname, "../../viz-adapter/src/gate.ts"));
const { mapPolyglotSpec } = load(
  path.resolve(__dirname, "../src/lib/viz/polyglot-spec.ts"),
);

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
function edge(source, target, displayMode = "Fact") {
  return { source, target, edgeKind: "calls", displayMode, evidence: span };
}

// two machines + c4, everything Fact except one cited inference — every
// spec here must pass the gate after mapping (the chat opens them directly)
function syntheticBundle() {
  return {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 3,
    "impact-centers-truncated": false,
    specs: {
      c4: {
        elements: [el("dom-core"), el("mod-a", "Inference", { claimId: "C-1" })],
        edges: [edge("dom-core", "mod-a")],
        footer: { note: "one domain, one inferred module" },
      },
      "state-machine:Connection": {
        elements: [el("st-open"), el("st-closed", "Inference", { claimId: "C-2" })],
        edges: [edge("st-open", "st-closed")],
        footer: { tier: "T2" },
      },
      "state-machine:Order": {
        elements: [el("st-new"), el("st-paid")],
        edges: [],
        footer: { tier: "Fact" },
      },
    },
  };
}

test("intent: the four named-diagram vocabularies, and only those", () => {
  assert.equal(isPolyglotDrawAsk("draw the C4 architecture"), "c4");
  assert.equal(isPolyglotDrawAsk("what's the architecture of this project?"), "c4");
  assert.equal(isPolyglotDrawAsk("show me a container diagram"), "c4");
  assert.equal(isPolyglotDrawAsk("show me the state machines"), "state-machine");
  assert.equal(isPolyglotDrawAsk("is there a state diagram for orders?"), "state-machine");
  assert.equal(isPolyglotDrawAsk("draw the workflows"), "workflows");
  assert.equal(isPolyglotDrawAsk("how do requests flow through the system?"), "workflows");
  assert.equal(isPolyglotDrawAsk("draw the impact view"), "impact");
  assert.equal(isPolyglotDrawAsk("the impact diagram for payments"), "impact");
});

test("intent: questions that must NOT be hijacked from the existing paths", () => {
  // bare "impact" stays with the live `cie impact --symbol` fast path
  assert.equal(isPolyglotDrawAsk("what's the impact of changing transfer()?"), null);
  assert.equal(isPolyglotDrawAsk("which tests cover this?"), null);
  // "who calls whom" keeps the report-based call flow
  assert.equal(isPolyglotDrawAsk("who calls parse_config?"), null);
  // non-drawing questions reach the AI
  assert.equal(isPolyglotDrawAsk("what does parseConfig do?"), null);
  assert.equal(isPolyglotDrawAsk("the pipeline is slow — why?"), null);
  assert.equal(isPolyglotDrawAsk("are there any secrets in the repo?"), null);
  assert.equal(isPolyglotDrawAsk("draw a donut of files per directory"), null);
});

test("fixture: 'draw the C4 architecture' resolves the REAL c4 spec and the answer quotes counted evidence", { skip: !hasFixture && "fixture not generated yet" }, () => {
  const bundle = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  const r = tryPolyglotDraw("draw the C4 architecture", bundle);
  assert.ok(r, "a bundle question must be answered");
  assert.equal(r.artifact, "c4");
  assert.match(r.answer, /C4 — containers and components/);
  assert.match(r.answer, /named element/);
  assert.match(r.answer, /facts?, \d+ inference/);
  assert.match(r.answer, /cites the file and bytes/);
  // the artifact the chat opens is exactly the mapped, gated spec
  const spec = bundle.specs.c4;
  const n = spec.elements.length;
  assert.ok(r.answer.includes(`${n} named element`), `answer must quote the real count ${n}`);
});

test("fixture: every artifact the chat can resolve passes an INDEPENDENT gate run", { skip: !hasFixture && "fixture not generated yet" }, () => {
  const bundle = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  for (const key of Object.keys(bundle.specs)) {
    const r = tryPolyglotDraw(`show me the ${key.replace("state-machine:", "").replace("impact:", "")} ${key.startsWith("state-machine:") ? "state machine" : key.startsWith("impact:") ? "impact view" : key === "c4" ? "architecture" : "workflows"}`, bundle);
    if (!r || r.artifact !== key) continue;
    const spec = bundle.specs[key];
    // re-map exactly as the stage does and re-run the gate
    const { view, violations } = mapPolyglotSpec(spec, { artifact: key });
    assert.deepEqual(violations, [], `${key} must map clean`);
    assert.ok(view, `${key} must pass the gate`);
    gate(view);
  }
});

test("default pick: many machines → the first in stable order, count disclosed", () => {
  const r = tryPolyglotDraw("show me the state machines", syntheticBundle());
  assert.ok(r);
  assert.equal(r.artifact, "state-machine:Connection", "Connection sorts before Order");
  assert.match(r.answer, /This project has 2 of them/);
});

test("named pick: 'the Connection state machine' resolves THAT machine", () => {
  const r = tryPolyglotDraw("draw the Connection state machine", syntheticBundle());
  assert.ok(r);
  assert.equal(r.artifact, "state-machine:Connection");
  assert.match(r.answer, /State machine — Connection \(T2\)/);
});

test("workflows and c4 answer straight from the bundle", () => {
  const b = syntheticBundle();
  const wf = tryPolyglotDraw("draw the workflows", b);
  assert.ok(wf);
  assert.equal(wf.artifact, null, "this synthetic bundle holds no workflows spec — the ask is honestly refused");
  assert.match(wf.answer, /named no workflow picture/);
  const c4 = tryPolyglotDraw("what's the architecture?", b);
  assert.ok(c4 && c4.artifact === "c4");
  assert.match(c4.answer, /2 facts, 1 inference, 0 hypotheses/, "footer counts include edges");
});

test("impact named pick: the question's symbol resolves THAT view", () => {
  const bundle = {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 2,
    "impact-centers-truncated": false,
    specs: {
      "impact:1": {
        elements: [el("sym-transfer", "Fact", { fields: { role: "center" } }), el("caller-1")],
        edges: [edge("caller-1", "sym-transfer")],
      },
      "impact:2": {
        elements: [el("sym-ledger", "Fact", { fields: { role: "center" } })],
        edges: [],
      },
    },
  };
  const r = tryPolyglotDraw("draw the impact view of sym-transfer", bundle);
  assert.ok(r);
  assert.equal(r.artifact, "impact:1");
  const plain = tryPolyglotDraw("draw the impact view", bundle);
  assert.ok(plain && plain.artifact === "impact:1", "un-named ask takes the first in stable order");
});

test("kind miss: no machines in the bundle → the honest count answer, nothing drawn", () => {
  const c4Only = {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 1,
    "impact-centers-truncated": false,
    specs: { c4: { elements: [el("dom-core")], edges: [] } },
  };
  const r = tryPolyglotDraw("draw the state machine", c4Only);
  assert.ok(r);
  assert.equal(r.artifact, null);
  assert.match(r.answer, /named no state machine/);
  assert.match(r.answer, /C4 components/);
  // and the same honest refusal for a kind the synthetic full bundle lacks
  const r2 = tryPolyglotDraw("draw the impact view", syntheticBundle());
  assert.ok(r2 && r2.artifact === null);
  assert.match(r2.answer, /2 state machine\(s\)/);
});

test("empty bundle: the honest nothing at all", () => {
  const bundle = {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 0,
    "impact-centers-truncated": false,
    specs: {},
  };
  const r = tryPolyglotDraw("draw the C4 architecture", bundle);
  assert.ok(r);
  assert.equal(r.artifact, null);
  assert.match(r.answer, /named nothing at all/);
});

test("no bundle: the module never answers — the old paths stand", () => {
  assert.equal(tryPolyglotDraw("draw the C4 architecture", null), null);
});

test("gate refusal: a broken spec is a NAMED result, never a silently drawn lie", () => {
  const bundle = {
    kind: "cie-polyglot-viewspec-bundle",
    "spec-count": 1,
    "impact-centers-truncated": false,
    specs: {
      c4: {
        // built raw — no claimId on a non-Fact element → NON_FACT_WITHOUT_CLAIM
        elements: [{ id: "bad", elementKind: "symbol", displayMode: "Inference", label: "bad", evidence: span }],
        edges: [],
      },
    },
  };
  const r = tryPolyglotDraw("draw the C4 architecture", bundle);
  assert.ok(r);
  assert.equal(r.artifact, "c4", "the canvas still opens — to show the named refusal");
  assert.match(r.answer, /honesty gate refused/);
  assert.match(r.answer, /NON_FACT_WITHOUT_CLAIM/);
});
