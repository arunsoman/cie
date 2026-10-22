// Chat → named diagrams: when the polyglot pass produced a bundle, a
// drawing request in Ask CIE ("draw the C4 architecture", "show me the
// state machines") is answered from the engine's OWN artifacts — not the
// store-derived stand-ins draw-anything builds. Same honesty law as every
// fast path: instant, nothing invented, and a kind the bundle doesn't hold
// gets a named "here's what it did name" answer instead of a guessed
// picture.
//
// Why this runs BEFORE draw-anything: the bundle's C4 / machines / flows /
// impact views are strictly better evidence — every box cites the file and
// bytes that earned it, the epistemic footer counts Fact/Inference/
// Hypothesis/Fog — while buildUmlSpec can only shape the raw concept tree.
// Without a bundle this module is never consulted and the old path stands.

import type { PolyglotBundle } from "./polyglot-types";
import { polyglotDetail } from "./polyglot-types";
import {
  artifactKind,
  artifactMeta,
  artifactOrder,
  mapPolyglotSpec,
  type PolyglotArtifactKind,
} from "./viz/polyglot-spec";

export interface PolyglotDrawResult {
  answer: string;
  /** the bundle key to open on the canvas — null when there is honestly
   *  nothing of the requested kind to draw (the answer says what exists) */
  artifact: string | null;
}

// ---- intent vocabularies (conservative — a near-miss falls through to the
// existing paths rather than hijacking a question this module can't answer) ----

// "draw the C4 architecture", "what's the architecture", "container
// diagram" — chart nouns count without a drawing verb, same rule
// draw-anything's CHART_NOUNS applies to "architecture".
const C4_WORDS =
  /\b(c4|architectures?|architectural|containers? and components?|container diagram|component diagram|system (context|diagram)|component model)\b/i;

// "workflows", "how calls flow", "call flow" — deliberately NOT "who calls
// whom" (that ask keeps its dedicated report-based call flow, which answers
// a pointed question better than the bundle's forward DFS picture) and NOT
// bare "pipeline" (a question like "the pipeline is slow" is not a drawing
// request and must reach the AI).
const WORKFLOW_WORDS =
  /\b(workflows?|work ?flows?|how (do|does|is|are)? ?(the )?(calls?|control|requests?|data) flows?(?:ing)?|call flow|data ?flows?|control flow)\b/i;

const MACHINE_WORDS =
  /\b(state ?machines?|state diagrams?|states? and transitions?|protocol states?|fsm)\b/i;

// Impact views are hijacked ONLY by a drawing ask ("draw the impact view",
// "the impact diagram for X") — a bare "what's the impact of changing X"
// keeps the live `cie impact --symbol` fast path in chat-panel.
const IMPACT_WORDS =
  /\bimpact (views?|diagrams?|maps?|graphs?|rings?|pictures?|chart|picture|blast)\b/i;

export function isPolyglotDrawAsk(question: string): PolyglotArtifactKind | null {
  if (C4_WORDS.test(question)) return "c4";
  if (MACHINE_WORDS.test(question)) return "state-machine";
  if (WORKFLOW_WORDS.test(question)) return "workflows";
  if (IMPACT_WORDS.test(question)) return "impact";
  return null;
}

/** keys of the requested kind, in the bundle's stable presentation order */
function keysOfKind(bundle: PolyglotBundle, kind: PolyglotArtifactKind): string[] {
  return Object.keys(bundle.specs)
    .filter((k) => artifactKind(k) === kind)
    .sort((a, b) => artifactOrder(a) - artifactOrder(b) || a.localeCompare(b));
}

/** the element count a named answer quotes — counted, never estimated */
function elementCount(bundle: PolyglotBundle, name: string): number {
  return bundle.specs[name]?.elements.length ?? 0;
}

/** the human label for one bundle key (no mapping/gating needed for text) */
function labelOf(bundle: PolyglotBundle, name: string): string {
  return artifactMeta(name, bundle.specs[name]).label;
}

/** The one entry point the chat uses. Returns null when the question isn't
 *  a named-diagram ask (the caller falls through to the other fast paths).
 *  A recognized ask the bundle can't satisfy gets the honest count answer
 *  with `artifact: null` — never a silent fallthrough that would draw the
 *  store-derived stand-in as if it were the named picture. */
export function tryPolyglotDraw(
  question: string,
  bundle: PolyglotBundle | null,
): PolyglotDrawResult | null {
  if (!bundle) return null;
  const kind = isPolyglotDrawAsk(question);
  if (!kind) return null;

  const candidates = keysOfKind(bundle, kind);
  if (candidates.length === 0) {
    const others = Object.keys(bundle.specs).length;
    return {
      answer:
        `The polyglot pass named no ${kindName(kind)} for this project, so there's no such picture to draw. ` +
        (others === 0
          ? "In fact it named nothing at all — the honest answer for this project."
          : `What it did name: ${polyglotDetail(bundle).replace(/ ready\.$/, "")}.`),
      artifact: null,
    };
  }

  // Pick by whatever the question actually names (a machine or a symbol),
  // else the first artifact of the kind in stable order.
  const named = candidates.find((k) => questionMentions(question, bundle, k));
  const name = named ?? candidates[0];
  const { label, hint } = artifactMeta(name, bundle.specs[name]);
  const { view, violations } = mapPolyglotSpec(bundle.specs[name], { artifact: name });

  const n = elementCount(bundle, name);
  const certainty = view
    ? [
        `${view.footer.facts} fact${view.footer.facts === 1 ? "" : "s"}`,
        `${view.footer.inferences} inference${view.footer.inferences === 1 ? "" : "s"}`,
        `${view.footer.hypotheses} ${view.footer.hypotheses === 1 ? "hypothesis" : "hypotheses"}`,
        ...(view.footer.fog > 0 ? [`${view.footer.fog} fog`] : []),
      ].join(", ")
    : "counts unavailable";
  const gaps = bundle.specs[name].gaps?.length ?? 0;

  let answer =
    `${label} is on the canvas — ${n} named element${n === 1 ? "" : "s"} (${certainty}). ${hint} ` +
    `Every box cites the file and bytes that earned it; click one to open the code exactly there.`;
  if (gaps > 0) answer += ` The producer counted ${gaps} thing${gaps === 1 ? "" : "s"} it left out and why — the disclosure under the canvas lists them.`;
  if (!view)
    answer = `I found the ${label} spec, but the honesty gate refused to draw it: ${violations[0] ?? "unknown clause"}. The canvas shows the named refusal — nothing is invented to fill the gap.`;
  if (candidates.length > 1 && !named && kind !== "c4" && kind !== "workflows")
    answer += ` This project has ${candidates.length} of them — the picker in Named diagrams lists the rest.`;

  return { answer, artifact: name };
}

function kindName(kind: PolyglotArtifactKind): string {
  switch (kind) {
    case "c4": return "C4 architecture";
    case "workflows": return "workflow picture";
    case "state-machine": return "state machine";
    case "impact": return "impact view";
  }
}

/** true when the question names this artifact's machine / center symbol —
 *  "the connection state machine", "the impact view of transfer_money" */
function questionMentions(question: string, bundle: PolyglotBundle, name: string): boolean {
  const q = question.toLowerCase();
  const spec = bundle.specs[name];
  if (artifactKind(name) === "state-machine") {
    const machine = name.slice("state-machine:".length).toLowerCase();
    return q.includes(machine);
  }
  if (artifactKind(name) === "impact") {
    const center = spec.elements.find((e) => e.fields && e.fields.role === "center");
    return !!center && q.includes(center.label.toLowerCase());
  }
  return false;
}
