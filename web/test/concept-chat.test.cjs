// C2 — the concept-chat seam, tested from the CLIENT side.
//
// "draw the flowchart of <usecase>" / "show the sequence diagram of
// <usecase>" typed into Ask CIE must draw the story's OWN recorded extent
// through the B3 mappers — never the whole-project stand-in — and the
// refusal ladder must be honest: a named target nothing resolves is
// refused BY NAME, a bare "draw the flowchart" is NOT hijacked (it stays
// with draw-anything), and a bare "sequence" ask never fires.
// Fixtures:
//   - synthetic hermetic report + concepts (two usecases over six symbols)
//   - the REAL demo artifacts (web/public/data) for an end-to-end resolve

const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const {
  tryConceptDraw,
  isConceptDiagramAsk,
  usecaseIndex,
} = load(path.resolve(__dirname, "../src/lib/concept-chat.ts"));

// ---------- hermetic fixtures ----------

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
      { id: 7, kind: "Function", name: "sh", qual_name: "scripts.sh", loc: { file: 1, start: 79, end: 80 }, lang: "python", parent: null },
    ],
    summaries: [
      { sym: 2, callees: [5], tags: [] },
      { sym: 3, callees: [] },
      { sym: 5, callees: [6] },
      { sym: 6, callees: [] },
    ],
    coverage: { files_total: 2, files_analyzed: 2, symbols_total: 7, symbols_analyzed: 4, complete: true, notes: [] },
    evidence: { rev: 7, provenance: "test", assumptions: [], unsupported: [], unresolved: [] },
    warnings: [],
  };
}

function concepts() {
  return {
    nodes: [
      { id: 9, label: "Project", kind: "Domain", parent: null, children: [10, 11, 12, 13], primary_edge: "PackageContainment", symbols: [1, 2, 3, 4, 5, 6, 7], behaviors: [], rules: [] },
      { id: 10, label: "checkout payment", kind: "UseCase", parent: 9, children: [11], primary_edge: "FcaSubsumption", symbols: [2, 3, 5], behaviors: [], rules: [], intent: ["money"] },
      { id: 11, label: "payment", kind: "UseCase", parent: 10, children: [], primary_edge: "FcaSubsumption", symbols: [5], behaviors: [], rules: [], intent: ["money"] },
      { id: 12, label: "shell story", kind: "UseCase", parent: 9, children: [], primary_edge: "FcaSubsumption", symbols: [7], behaviors: [], rules: [], intent: ["spawn"] },
      { id: 13, label: "ghost story", kind: "UseCase", parent: 9, children: [], primary_edge: "FcaSubsumption", symbols: [99, 100], behaviors: [], rules: [], intent: [] },
    ],
    unattached_behaviors: [],
    unattached_rules: [],
    dirty: false,
  };
}

// ---------- intent vocabulary ----------

test("the ask vocabulary recognizes the two diagram phrases and nothing looser", () => {
  assert.equal(isConceptDiagramAsk("draw the flowchart of checkout"), "flowchart");
  assert.equal(isConceptDiagramAsk("draw the flow chart of checkout"), "flowchart");
  assert.equal(isConceptDiagramAsk("show the sequence diagram of payment"), "sequence");
  assert.equal(isConceptDiagramAsk("the sequence chart for payment please"), "sequence");
  // not hijacked:
  assert.equal(isConceptDiagramAsk("the read-add-write sequence is not atomic"), null);
  assert.equal(isConceptDiagramAsk("the pipeline is slow"), null);
  assert.equal(isConceptDiagramAsk("draw the flow"), null); // whole-project ask stays with draw-anything
  // the vocabulary fires on the noun, but the RESOLVER falls through when
  // no story is named and nothing is asked to be drawn — that contract:
  assert.equal(tryConceptDraw("what is a flowchart anyway", report(), concepts()), null);
});

test("a bare flowchart ask falls through when no story is named", () => {
  const r = tryConceptDraw("draw the flowchart", report(), concepts());
  assert.equal(r, null); // draw-anything's whole-project picture is the right answer
  const r2 = tryConceptDraw("draw the flowchart", null, null);
  assert.equal(r2, null); // no artifacts at all — old paths stand
});

// ---------- resolution: labels, then members ----------

test("a named story resolves by label, longest label wins", () => {
  const r = tryConceptDraw("draw the flowchart of checkout payment", report(), concepts());
  assert.ok(r);
  assert.equal(r.kind, "flowchart");
  assert.equal(r.conceptId, 10);
  assert.ok(r.answer.includes("flowchart"));
  assert.ok(r.answer.includes("checkout payment"));
});

test("a member name resolves to the one usecase that holds it", () => {
  const r = tryConceptDraw("draw the flowchart of add_item", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, 10); // symbol 2 is held only by "checkout payment"
});

test("a member in two usecases is refused with both names, not merged", () => {
  const r = tryConceptDraw("draw the flowchart of charge", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, null); // symbol 5 is in both stories
  assert.ok(r.answer.includes("checkout payment"));
  assert.ok(r.answer.includes("payment"));
});

test("a short member name never matches inside a longer word", () => {
  // "sh" exists as a symbol, but "show" contains no word-bounded "sh"
  const r = tryConceptDraw("show the sequence diagram", report(), concepts());
  assert.equal(r, null);
});

// ---------- the refusal ladder is honest ----------

test("a named target nothing resolves is refused by name, with the counted menu", () => {
  const r = tryConceptDraw("draw the flowchart of unicorn", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, null);
  assert.ok(r.answer.includes("No usecase named “unicorn”"));
  assert.ok(r.answer.includes("4 usecases")); // counted, not guessed
  assert.ok(r.answer.includes("“checkout payment”"));
});

test("a member the code holds but no usecase claims gets its own honest wording", () => {
  const r = tryConceptDraw("draw the flowchart of refund", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, null);
  assert.ok(r.answer.includes("no usecase claims it"));
});

// ---------- the refusal still honors the ask's kind and focus ----------

// The refusal never draws a story — but it hands the panel the ask's own
// context (the matched symbol ids) so the asked KIND can draw centered on
// what the user named, from the records. The focus is what the resolution
// machinery found, not a re-guess.
test("the unclaimed-member refusal carries the question's focus", () => {
  const r = tryConceptDraw("draw the flowchart of refund", report(), concepts());
  assert.ok(r);
  assert.deepEqual(r.focus, [6]); // symbol 6 = refund — the question's own context
});

test("the ambiguous-member refusal carries its focus too", () => {
  const r = tryConceptDraw("draw the flowchart of charge", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, null);
  assert.deepEqual(r.focus, [5]); // symbol 5 = charge — ambiguous but named
});

test("a named target matching NOTHING carries no focus (no invented center)", () => {
  const r = tryConceptDraw("draw the flowchart of unicorn", report(), concepts());
  assert.ok(r);
  assert.ok(r.answer.includes("No usecase named \u201cunicorn\u201d"));
  assert.ok(!r.focus); // no matched symbol — no center to claim
});

test("a story whose extent is unevidenced draws honestly empty, not a guess", () => {
  const r = tryConceptDraw("draw the flowchart of ghost story", report(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, 13); // the canvas opens — the gap is the picture
  assert.ok(r.answer.includes("honestly empty"));
});

test("a project with no usecases says so when a target is named", () => {
  const empty = { nodes: [concepts().nodes[0]], unattached_behaviors: [], unattached_rules: [], dirty: false };
  const r = tryConceptDraw("draw the sequence diagram of payment", report(), empty);
  assert.ok(r);
  assert.equal(r.conceptId, null);
  assert.ok(r.answer.includes("named no usecases at all"));
});

// ---------- the words never drift from the picture ----------

test("the answer quotes the same counts the canvas will render", () => {
  const rep = report();
  const r = tryConceptDraw("draw the flowchart of checkout payment", rep, concepts());
  assert.ok(r && r.conceptId === 10);
  // extent {2,3,5}: the mapper's numbers — refund (6) belongs to no story
  assert.ok(r.answer.includes("3 member functions"));
  assert.ok(r.answer.includes("1 internal call"));
});

test("the sequence answer speaks lifelines and messages", () => {
  const r = tryConceptDraw("show the sequence diagram of checkout payment", report(), concepts());
  assert.ok(r);
  assert.equal(r.kind, "sequence");
  assert.equal(r.conceptId, 10);
  assert.ok(r.answer.includes("lifeline"));
  assert.ok(r.answer.includes("cross-class message"));
});

// ---------- the real demo artifacts ----------

test("the real demo data resolves a real story and the mapper gates it through", () => {
  const dataDir = path.resolve(__dirname, "../public/data");
  const rep = JSON.parse(fs.readFileSync(path.join(dataDir, "report.json"), "utf8"));
  const con = JSON.parse(fs.readFileSync(path.join(dataDir, "concepts.json"), "utf8"));
  const index = usecaseIndex(rep, con);
  assert.ok(index.length > 0, "the demo artifact carries usecases");
  const story = index.find((u) => u.symbols.length >= 2);
  assert.ok(story, "at least one usecase has two or more members");
  const r = tryConceptDraw(`draw the flowchart of ${story.label}`, rep, con);
  assert.ok(r);
  assert.equal(r.conceptId, story.id);
  assert.ok(r.view_gate === undefined); // no leaked internals
  assert.ok(r.answer.length > 40);
  // the answer must quote the mapper's element count for this story
  assert.ok(new RegExp(`${story.symbols.length} member function`).test(r.answer) || r.answer.includes("honestly empty"));
});

test("the real demo data answers a sequence ask for a multi-class story", () => {
  const dataDir = path.resolve(__dirname, "../public/data");
  const rep = JSON.parse(fs.readFileSync(path.join(dataDir, "report.json"), "utf8"));
  const con = JSON.parse(fs.readFileSync(path.join(dataDir, "concepts.json"), "utf8"));
  const index = usecaseIndex(rep, con);
  const story = index.find((u) => u.symbols.length >= 2);
  const r = tryConceptDraw(`show the sequence diagram of ${story.label}`, rep, con);
  assert.ok(r);
  assert.equal(r.kind, "sequence");
  assert.equal(r.conceptId, story.id);
});

// ---------- the words people actually type (the "not drawing one" fixes) ----------

/** The hermetic report carrying the A5 projection — chains and class cuts
 *  the fallback index can't see, so the narrowest-story and class-scoping
 *  rules are exercised the way the real artifact drives them. */
function reportWithView() {
  const r = report();
  const link = (id, label, kind) => ({ id, label, kind });
  const member = (symbol, name, qual, fileIdx, line, owner) => ({
    symbol, name, qual_name: qual, file: r.files[fileIdx], line, owner,
  });
  r.usecase_view = [
    {
      id: 10, label: "checkout payment", intent: ["money"], confidence: 0.8,
      coverage_ratio: 1, coverage_complete: true, provenance: "fca:godin-v1",
      chain: [link(9, "Project", "Domain")], narrower: [11],
      members: [member(2, "add_item", "Cart.add_item", 0, 3, "Cart"), member(3, "checkout", "Cart.checkout", 0, 4, "Cart"), member(5, "charge", "Pay.charge", 1, 2, "Pay")],
      member_classes: [{ id: 1, label: "Cart", kind: "Class", members: [2, 3] }],
    },
    {
      id: 11, label: "payment", intent: ["money"], confidence: 0.8,
      coverage_ratio: 1, coverage_complete: true, provenance: "fca:godin-v1",
      chain: [link(10, "checkout payment", "UseCase"), link(9, "Project", "Domain")], narrower: [],
      members: [member(5, "charge", "Pay.charge", 1, 2, "Pay")],
      member_classes: [{ id: 4, label: "Pay", kind: "Class", members: [5] }],
    },
    {
      id: 12, label: "shell story", intent: ["spawn"], confidence: 0.8,
      coverage_ratio: 1, coverage_complete: true, provenance: "fca:godin-v1",
      chain: [link(9, "Project", "Domain")], narrower: [],
      members: [member(7, "sh", "scripts.sh", 1, 5, null)],
      member_classes: [],
    },
  ];
  return r;
}

test("a word of a camelCase name scopes to the narrowest chained story", () => {
  // "charge" is held by BOTH stories, but "payment" (#11) is the narrower
  // one and its chain carries "checkout payment" — most-specific-wins
  // draws #11 and says the broader match stands, instead of refusing.
  const r = tryConceptDraw("draw the flowchart of charge", reportWithView(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, 11);
  assert.ok(r.answer.includes("payment"));
  assert.ok(r.answer.includes("Other usecases match your words too"));
});

test("a token of a class-cut name scopes to the stories that class cuts", () => {
  // "Cart" is a container no extent contains; the stories it cuts across
  // are the honest scope of "draw the flowchart of the cart".
  const r = tryConceptDraw("draw the flowchart of the cart", reportWithView(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, 10); // only #10's class cut holds Cart
});

test("a label's distinctive words match in any order, theme words dropped", () => {
  // word order free: "payment checkout" verbatim is absent, the token AND
  // still reaches the story the engine named "checkout payment"
  const r = tryConceptDraw("draw the flowchart of payment checkout", reportWithView(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, 10); // longest matching label wins over "payment"
});

test("a token nothing relates to is still refused, never guessed", () => {
  const r = tryConceptDraw("draw the flowchart of unicorn horn", reportWithView(), concepts());
  assert.ok(r);
  assert.equal(r.conceptId, null);
  assert.ok(r.answer.includes("No usecase named"));
});
