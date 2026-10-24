// Persona → chat voice (RFC-0024 §14 extension): the top-bar persona now
// answers in Ask CIE too. The honesty core is persona-INVARIANT by
// construction — these tests check the composition laws from the outside:
//   1. every persona's prompt still carries the ENTIRE honesty core
//      (JSON answer contract, chart menu with engine/layout claims, chart
//      rules, STRICT RULES, evidence-fact explanations) — a persona can
//      add a voice, never weaken or replace the core;
//   2. the persona's identity/voice actually changes the prompt (all 10
//      pairs differ) and lands BEFORE the facts with extras AFTER the
//      strict rules (additive-only, no reordering of the evidence rules);
//   3. fragment caps hold — no persona can ramble past its slice;
//   4. scoped answering (RFC-0024 stage 5) survives every persona — the
//      selection-scope law outranks any voice;
//   5. unknown/missing persona ids fall back to the fresher voice
//      silently, and the fresher voice reproduces the ORIGINAL solo prompt's
//      identity lines — the default chat behavior is unchanged.
const test = require("node:test"),
  assert = require("node:assert/strict"),
  path = require("node:path");
const loader = require("./helpers/load-ts.cjs");
const cp = loader()(path.resolve(__dirname, "../src/lib/chat-persona.ts"));
const ap = loader()(path.resolve(__dirname, "../src/lib/atlas-presets.ts"));

const DIGEST = { hello: "report-facts", findings: ["one"], by_stage: {} };
const SCOPED = { ...DIGEST, selection_scope: { files: ["app.py"], named_things: ["app.save"], chips: [{ label: "save", kind: "symbol" }] } };

const PERSONAS = Object.keys(ap.PERSONA_PRESETS); // fresher, reviewer, architect, auditor, consultant

test("catalog: every atlas persona has a chat voice; fields capped and populated", () => {
  const caps = { intro: 5, voice: 6, focus: 6, chartBias: 3, extraRules: 4 };
  assert.deepEqual(
    PERSONAS.sort(),
    Object.keys(cp.CHAT_PERSONAS).sort(),
    "one ChatPersona per atlas persona — no extra voices the menu can't pick"
  );
  for (const id of Object.keys(cp.CHAT_PERSONAS)) {
    const p = cp.CHAT_PERSONAS[id];
    for (const f of Object.keys(caps)) {
      assert.ok(Array.isArray(p[f]), `${id}.${f} is an array`);
      assert.ok(p[f].length <= caps[f], `${id}.${f} within cap ${caps[f]} (got ${p[f].length})`);
      assert.ok(p[f].every((line) => typeof line === "string" && line.length >= 8), `${id}.${f} lines are real text`);
    }
    if (id !== "fresher") {
      assert.ok(p.focus.length >= 1, `${id} prioritizes something (non-default personas)`);
      assert.ok(p.extraRules.length >= 1, `${id} carries its own additional rules`);
    }
    assert.ok(p.starters.length >= 3 && p.starters.length <= 5, `${id} has 3-5 starters`);
    // questions or imperative drawing asks ("Draw the C4 architecture") —
    // both are legitimate chat starters, never statements
    assert.ok(p.starters.every((s) => s.endsWith("?") || /^(draw|show|trace|explain|teach|compare|what('s| is)|where|which)/i.test(s)), `${id} starters are questions or drawing asks`);
    assert.ok(new Set(p.starters).size === p.starters.length, `${id} starters unique`);
    assert.ok(p.voiceTag && p.tagline, `${id} labels its voice and tagline`);
  }
  // distinct short tags so the chat header/footer can't confuse two personas
  const tags = Object.values(cp.CHAT_PERSONAS).map((p) => p.voiceTag);
  assert.equal(new Set(tags).size, tags.length, "voiceTags unique");
});

// The invariance law: every marker below is part of CIE's honesty core and
// must appear in EVERY persona's prompt, word-for-word.
const CORE_MARKERS = [
  "----- BEGIN PROJECT FACTS -----",
  "----- END PROJECT FACTS -----",
  "OUTPUT FORMAT — reply with ONE JSON object and nothing else",
  '"answer": "your plain-words answer"',
  "CHART MENU — pick the kind that best answers the question",
  'engine "custom-svg", layout "fisheye-rows"',
  'engine "recharts", layout "donut+legend"',
  'engine "card", layout "hero-number"',
  'engine "grid", layout "scroll-rows"',
  'engine "custom-svg", layout "uml-boxes"',
  'engine "custom-svg", layout "call-flow"',
  "Set \"chart\": null when a chart would not help",
  "CHART RULES (the chart must never lie):",
  "Never invent, estimate or round up a number that is not there.",
  "- At most 8 rows; shortest honest chart wins.",
  "STRICT RULES:",
  '1. Answer ONLY from the JSON facts above. If the answer is not there, say plainly: "I don\'t know that from the report I have." Then suggest what they could check next',
  "2. Never invent file names, numbers, findings, promises or statistics.",
  "Never give advice that would weaken security, and never provide code that exploits a weakness.",
  "runtime_evidence: measured numbers",
  "repeated_behavior: THIS IS CIE'S REAL ANSWER",
  "cochange_facts: real git history",
  "drift_facts: real claim extraction",
];

test("honesty core: persona-invariant — every core marker present in every voice", () => {
  for (const id of Object.keys(cp.CHAT_PERSONAS)) {
    const prompt = cp.buildSystemPrompt(id, DIGEST, false);
    for (const marker of CORE_MARKERS) {
      assert.ok(prompt.includes(marker), `${id}: core marker missing → ${JSON.stringify(marker.slice(0, 60))}`);
    }
  }
});

test("composition order: persona voice before the facts, extras after the strict rules, nothing reordered", () => {
  for (const id of Object.keys(cp.CHAT_PERSONAS)) {
    const p = cp.CHAT_PERSONAS[id];
    const prompt = cp.buildSystemPrompt(id, DIGEST, false);
    const atIntro = prompt.indexOf(p.intro[0]);
    const atFacts = prompt.indexOf("----- BEGIN PROJECT FACTS -----");
    const atStrict = prompt.indexOf("STRICT RULES:");
    assert.ok(atIntro >= 0 && atFacts >= 0 && atStrict >= 0, `${id} composed`);
    assert.ok(atIntro < atFacts, `${id}: voice comes before the facts (identity first)`);
    assert.ok(prompt.lastIndexOf("drift_facts: real claim extraction") < atStrict, `${id}: evidence explanations live before STRICT RULES`);
    // persona focus, if any, sits before the facts too
    for (const focusLine of p.focus) assert.ok(prompt.indexOf(focusLine) < atFacts, `${id}: focus inside the voice half`);
    for (const biasLine of p.chartBias) assert.ok(prompt.indexOf(biasLine) > atFacts && prompt.indexOf(biasLine) < atStrict, `${id}: chart bias sits with the chart menu, not past it`);
    // extra rules continue the numbering (strict carries 7 rules → extras start at 8)
    p.extraRules.forEach((rule, i) => {
      const marker = `${8 + i}. ${rule}`;
      assert.ok(prompt.includes(marker), `${id}: extra rule numbered into the STRICT list → "${marker.slice(0, 40)}"`);
      assert.ok(prompt.indexOf(marker) > prompt.indexOf("7. If the user asks something totally unrelated"), `${id}: extras strictly after core rule 7`);
    });
  }
});

test("all ten prompts pairwise distinct — the persona really changes the prompt", () => {
  const built = {};
  for (const id of Object.keys(cp.CHAT_PERSONAS)) built[id] = cp.buildSystemPrompt(id, DIGEST, false);
  for (let i = 0; i < PERSONAS.length; i++)
    for (let j = i + 1; j < PERSONAS.length; j++) {
      const a = PERSONAS[i], b = PERSONAS[j];
      assert.notEqual(built[a], built[b], `${a} vs ${b} prompts differ`);
      assert.notEqual(built[a].split("\n")[0].split(",")[0], built[b].split("\n")[0].split(",")[0], `${a}/${b}: identities differ on the first line`);
    }
});

test("voice signatures: each persona says who it is and keeps its own rules", () => {
  assert.ok(cp.buildSystemPrompt("fresher", DIGEST).includes("You are CIE's friendly explainer"));
  const arch = cp.buildSystemPrompt("architect", DIGEST);
  assert.ok(arch.includes("You are CIE's technical architect"));
  assert.ok(arch.includes("WHAT YOU PRIORITIZE WHEN READING THE FACTS:"));
  assert.ok(arch.includes("End structural answers naming the ONE seam"));
  assert.ok(cp.buildSystemPrompt("auditor", DIGEST).includes("Never present an inference as a finding"));
  assert.ok(cp.buildSystemPrompt("reviewer", DIGEST).includes("Never soften a finding's severity"));
  assert.ok(cp.buildSystemPrompt("consultant", DIGEST).includes("independent software consultant"));
  assert.ok(cp.buildSystemPrompt("consultant", DIGEST).includes("Never invent a score, grade or rating"));
});

test("fallback: unknown/missing persona is the fresher voice — silently, no error", () => {
  assert.equal(cp.sanitizePersonaId(undefined), "fresher");
  assert.equal(cp.sanitizePersonaId(null), "fresher");
  assert.equal(cp.sanitizePersonaId("bogus"), "fresher");
  assert.equal(cp.sanitizePersonaId(123), "fresher");
  assert.equal(cp.sanitizePersonaId("auditor"), "auditor");
  const bogus = cp.buildSystemPrompt("bogus", DIGEST, false);
  assert.equal(bogus, cp.buildSystemPrompt("fresher", DIGEST, false), "unknown id composes exactly the fresher prompt");
});

test("default (fresher) regression: the original friendly-explainer identity survives verbatim", () => {
  const prompt = cp.buildSystemPrompt("fresher", DIGEST, false);
  // the two identity lines that ALWAYS were the chat's system prompt
  assert.ok(prompt.includes("You are CIE's friendly explainer. CIE is a tool that reads a software project and turns it into a plain-English health report."));
  assert.ok(prompt.includes("You are chatting with the project's owner. They may be completely non-technical — a founder, a manager, a student. Be warm, patient and clear."));
  // the language rules that lived in the old STRICT RULES live in the fresher voice now
  assert.ok(prompt.includes("Use simple words and short sentences in the answer. Avoid jargon."));
  assert.ok(prompt.includes("Keep the answer under about 150 words unless the user clearly asks for more detail."));
  // fresher adds nothing: no focus/priorities block in the default voice
  assert.ok(!prompt.includes("WHAT YOU PRIORITIZE"), "fresher has no extra focus block");
  assert.ok(!prompt.includes("WHAT THIS VIEW PREFERS"), "fresher has no chart-bias block");
});

test("scoped answering (RFC-0024 stage 5) survives every persona — scope outranks the voice", () => {
  for (const id of Object.keys(cp.CHAT_PERSONAS)) {
    const scopedPrompt = cp.buildSystemPrompt(id, SCOPED, true);
    assert.ok(scopedPrompt.includes("SELECTION SCOPE — THIS OVERRIDES EVERYTHING ELSE ABOUT RANGE:"), `${id}: scope law present`);
    assert.ok(scopedPrompt.includes("- Never silently import an out-of-scope fact to fill a gap."), `${id}: out-of-scope disclosure intact`);
    assert.ok(scopedPrompt.includes("Answer ONLY from the selection code space in this digest."), `${id}: scoped facts rule`);
    assert.ok(scopedPrompt.includes("the answer was drawn from 1 file(s) over 1 named thing(s)."), `${id}: honesty line computed from the scope`);
    assert.ok(scopedPrompt.includes(cp.CHAT_PERSONAS[id].intro[0]), `${id}: the persona still introduces itself under scoping`);
  }
  // unscoped prompts never carry the scope law
  assert.ok(!cp.buildSystemPrompt("auditor", DIGEST, false).includes("SELECTION SCOPE"));
});

// ---- §5: persona experience across the WHOLE chat — the instant paths ----

const KINDS = ["story", "quality", "diagram", "mastery", "impact"];
const CANNED = "**3 things to fix first** — the report's own ordering.\n\n- Secret in config.yml (rose)";

test("lens: every non-fresher persona frames EVERY instant kind — no persona hole anywhere in the chat", () => {
  for (const id of PERSONAS) {
    const p = cp.CHAT_PERSONAS[id];
    if (id === "fresher") continue; // identity lens — covered by the no-digit test below
    for (const kind of KINDS) {
      assert.ok(typeof p.lens[kind] === "string" && p.lens[kind].length > 12, `${id} has a lens line for ${kind}`);
      const framed = cp.voiceInstant(id, kind, CANNED);
      assert.ok(framed.startsWith(p.lens[kind] + "\n\n"), `${id}/${kind}: lens prepended verbatim`);
      assert.ok(framed.endsWith(CANNED), `${id}/${kind}: canned answer unchanged below the lens (byte-verbatim)`);
      assert.ok(framed.includes(CANNED), `${id}/${kind}: original content intact`);
    }
  }
});

test("lens law: the framing line names the lens, never a fact — no digits, and fresher is the identity lens", () => {
  for (const id of PERSONAS) for (const kind of KINDS) {
    assert.ok(!/\d/.test(cp.CHAT_PERSONAS[id].lens[kind]), `${id}/${kind}: lens has no digits — framing, not facts`);
  }
  // fresher = identity for all kinds (the canned copy IS the onboarding voice)
  for (const kind of KINDS) {
    assert.equal(cp.voiceInstant("fresher", kind, CANNED), CANNED, `fresher/${kind} byte-identical to today's answers`);
  }
});

test("lens: unknown kind / no kind passes through; distinct kinds differ", () => {
  assert.equal(cp.voiceInstant("auditor", undefined, CANNED), CANNED, "no kind → untouched (LLM path & errors)");
  const framed = cp.voiceInstant("auditor", "quality", CANNED);
  assert.notEqual(framed, CANNED);
  assert.notEqual(cp.voiceInstant("architect", "quality", CANNED), framed, "different personas, different lens on the same kind");
});

test("greeting: every persona greets in its own voice; fresher keeps the original line", () => {
  const tags = Object.values(cp.CHAT_PERSONAS).map((p) => p.greeting);
  assert.equal(new Set(tags).size, tags.length, "greetings distinct per persona");
  assert.ok(tags.every((g) => g.length >= 20), "greetings are real lines");
  assert.equal(cp.CHAT_PERSONAS.fresher.greeting, "Hi! I've read your whole project.");
});