// Chat → a story's own diagrams: "draw the flowchart of checkout", "show
// the sequence diagram of the payment usecase". The concept twin of
// polyglot-chat: when the ask names a USECASE the engine actually induced,
// the answer draws that story's recorded extent through the B3 mappers —
// not a stand-in. Same honesty law as every fast path: instant, nothing
// invented, and a named target nothing can resolve gets a named refusal
// instead of a guessed picture.
//
// Why this runs BEFORE draw-anything (and after polyglot): draw-anything's
// flow chart is the whole project's call shape — strictly weaker evidence
// than a story's own extent when the question names one. A BARE "draw the
// flowchart" (no story named) is deliberately NOT hijacked: it falls
// through to draw-anything, whose whole-project picture is the right
// answer for that ask. Same for bare "sequence" — only the explicit
// "sequence diagram/chart/picture/view" phrase triggers here, so asks
// like "the read-add-write sequence is not atomic" reach their own paths.

import type { CieReport, ConceptNode, ConceptTree } from "./cie-types";
import {
  conceptToFlowchartSpec,
  conceptToSequenceSpec,
  NO_INTERNAL_CALLS_GAP,
  type ConceptDiagramKind,
} from "./viz/concept-diagrams";

export interface ConceptDrawResult {
  answer: string;
  /** the usecase to open on the canvas — null when there is honestly
   *  nothing of the requested story to draw (the answer says what exists) */
  conceptId: number | null;
  kind: ConceptDiagramKind;
}

// "flowchart", "flow chart" — the story's call graph. Bare "flow"/"flow
// chart of the whole project" stays with draw-anything via the target rule
// below (no story named → null).
const FLOWCHART_WORDS = /\bflow ?charts?\b/i;

// ONLY the explicit phrase — a bare "sequence" (e.g. "the read-add-write
// sequence") is not a drawing ask and must never be hijacked.
const SEQUENCE_WORDS = /\bsequence (diagrams?|charts?|pictures?|views?)\b/i;

/** An explicitly named target: "the flowchart OF checkout", "…FOR the
 *  payment story". When present, this module owns the ask — resolve it or
 *  refuse it by name, never silently fall through to a stand-in. */
const TARGET_WORDS = /\b(?:of|for|about)\s+([^?!.,;]+)/i;

const FILLER = /^(?:the|a|an|this|that|story|usecase|use case|flow|diagram)\s+/i;

export function isConceptDiagramAsk(question: string): ConceptDiagramKind | null {
  if (FLOWCHART_WORDS.test(question)) return "flowchart";
  if (SEQUENCE_WORDS.test(question)) return "sequence";
  return null;
}

interface UsecaseEntry {
  id: number;
  label: string;
  lower: string;
  symbols: number[];
}

/** The usecase index — from the A5 projection when the artifact carries
 *  it, else from the UseCase nodes (both keyed by concept id; membership
 *  is the engine's either way). */
export function usecaseIndex(
  report: CieReport | null,
  concepts: ConceptTree | null,
): UsecaseEntry[] {
  if (!concepts) return [];
  const nodes = new Map<number, ConceptNode>();
  for (const n of concepts.nodes) if (n.kind === "UseCase") nodes.set(n.id, n);
  if (report?.usecase_view?.length) {
    return report.usecase_view
      .filter((u) => nodes.has(u.id))
      .map((u) => ({ id: u.id, label: u.label, lower: u.label.toLowerCase(), symbols: u.members.map((m) => m.symbol) }));
  }
  return [...nodes.values()].map((n) => ({
    id: n.id,
    label: n.label,
    lower: n.label.toLowerCase(),
    symbols: n.symbols,
  }));
}

/** Word-bounded containment — "sh" must not match inside "show", "time"
 *  not inside "sometimes". Escapes the name (labels carry & and +). */
function containsWord(haystack: string, needle: string): boolean {
  if (needle.length < 2) return false; // a 1-char name matches nothing honestly
  const esc = needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`\\b${esc}\\b`, "i").test(haystack);
}

/** The one entry point the chat uses. Returns null when the question
 *  isn't a story-diagram ask this module can answer (the caller falls
 *  through to the other fast paths). A recognized ask with a named target
 *  that resolves to nothing gets the honest refusal with
 *  `conceptId: null` — never a silent fallthrough that would draw the
 *  whole-project stand-in as if it were the named story. */
export function tryConceptDraw(
  question: string,
  report: CieReport | null,
  concepts: ConceptTree | null,
): ConceptDrawResult | null {
  const kind = isConceptDiagramAsk(question);
  if (!kind || !report || !concepts) return null;

  const index = usecaseIndex(report, concepts);
  const q = question.toLowerCase();
  const diagramNoun = kind === "flowchart" ? "flowchart" : "sequence diagram";

  // 1 — a usecase label the question verbatim contains (longest label
  // wins: "checkout payment" beats "payment" for "…of checkout payment").
  const byLabel = index
    .filter((u) => containsWord(q, u.lower))
    .sort((a, b) => b.lower.length - a.lower.length || a.id - b.id);
  if (byLabel.length > 0) return draw(byLabel[0], kind, report, concepts, byLabel.length > 1);

  // 2 — a member the question names, whose story is unambiguous. Only the
  // MOST SPECIFIC matched name scopes the story set ("Cart.checkout"
  // beats "checkout" beats a short name that merely shares letters —
  // word-bounded, so "sh" never matches inside "show").
  const matched = new Map<string, number>(); // lower name/qual_name → symbol id
  for (const s of report.symbols) {
    if (containsWord(q, s.qual_name.toLowerCase())) matched.set(s.qual_name.toLowerCase(), s.id);
    if (containsWord(q, s.name.toLowerCase())) matched.set(s.name.toLowerCase(), s.id);
  }
  if (matched.size > 0) {
    const longest = [...matched.keys()].reduce((a, b) => (b.length > a.length ? b : a));
    const symbolId = matched.get(longest)!;
    const holding = index.filter((u) => u.symbols.includes(symbolId));
    if (holding.length === 1) return draw(holding[0], kind, report, concepts, false);
    if (holding.length > 1) {
      const names = holding
        .slice()
        .sort((a, b) => a.id - b.id)
        .map((u) => `“${u.label}”`)
        .join(", ");
      return {
        answer: `More than one usecase holds the member “${longest}”: ${names}. Name the story you mean and I'll draw its ${diagramNoun} — each story is its own picture, and I don't merge them.`,
        conceptId: null,
        kind,
      };
    }
  }

  // 3 — an explicit target nothing resolves: the named refusal. What CIE
  // names is counted, never sampled from a guess. A member the code holds
  // but no usecase claims gets its own honest wording.
  const target = TARGET_WORDS.exec(question);
  if (target) {
    const name = target[1]
      .trim()
      .replace(FILLER, "")
      .replace(/\s+(flowchart|sequence diagram|chart|diagram)$/i, "")
      .trim();
    if (index.length === 0) {
      return {
        answer: `This project's analysis named no usecases at all — there is no story to draw a ${diagramNoun} of. The functions are all in the map and the file views; a whole-project call picture is available as “draw the flow”.`,
        conceptId: null,
        kind,
      };
    }
    if (matched.size > 0) {
      const longest = [...matched.keys()].reduce((a, b) => (b.length > a.length ? b : a));
      return {
        answer: `The code does define “${longest}”, but no usecase claims it — the engine placed it in no story, so there is no ${diagramNoun} to draw for it. The named usecases: ${index
          .slice(0, 8)
          .map((u) => `“${u.label}”`)
          .join(", ")}${index.length > 8 ? "…" : ""}.`,
        conceptId: null,
        kind,
      };
    }
    const sample = index
      .slice(0, 8)
      .map((u) => `“${u.label}”`)
      .join(", ");
    return {
      answer: `No usecase named “${name}” — the engine induced ${index.length} usecase${
        index.length === 1 ? "" : "s"
      } for this project: ${sample}${index.length > 8 ? "…" : ""}. Name one of those and the ${diagramNoun} draws from its recorded members.`,
      conceptId: null,
      kind,
    };
  }

  // 4 — no story named at all: not this module's ask. draw-anything's
  // whole-project flow picture is the right answer for a bare "draw the
  // flowchart", and it stays theirs.
  return null;
}

/** Build the answer from the actual mapper output — counts and gaps are
 *  read off the same ViewSpec the canvas will render, so the words can
 *  never drift from the picture. */
function draw(
  u: UsecaseEntry,
  kind: ConceptDiagramKind,
  report: CieReport,
  concepts: ConceptTree,
  alsoOthers: boolean,
): ConceptDrawResult {
  const concept = concepts.nodes.find((n) => n.id === u.id);
  if (!concept) {
    return {
      answer: `The usecase “${u.label}” is in the projection but not in the concept table — I can't draw it without inventing membership, so I won't.`,
      conceptId: null,
      kind,
    };
  }
  const { view, violations } =
    kind === "flowchart"
      ? conceptToFlowchartSpec(concept, report)
      : conceptToSequenceSpec(concept, report);

  const diagramNoun = kind === "flowchart" ? "flowchart" : "sequence diagram";
  if (!view) {
    return {
      answer: `I found the usecase “${u.label}”, but the honesty gate refused to draw its ${diagramNoun}: ${violations[0] ?? "unknown clause"}. The canvas shows the named refusal — nothing is invented to fill the gap.`,
      conceptId: u.id,
      kind,
    };
  }

  const n = view.elements.length;
  const e = view.edges.length;
  const gaps = view.gaps ?? [];
  if (n === 0) {
    return {
      answer: `The usecase “${u.label}” has ${u.symbols.length} recorded member(s), but none could be evidenced in the symbol table — so the ${diagramNoun} is honestly empty. The canvas shows the gap, not a guess.`,
      conceptId: u.id,
      kind,
    };
  }

  const noCalls = gaps.some((g) => g.why === NO_INTERNAL_CALLS_GAP);
  let answer =
    kind === "flowchart"
      ? `“${u.label}” is on the canvas — its flowchart: ${n} member function${n === 1 ? "" : "s"}, ${e} internal call${e === 1 ? "" : "s"} the engine resolved between them. Every node cites the bytes that earned it; tap one to open the code exactly there.`
      : `“${u.label}” is on the canvas — its sequence: ${n} lifeline${n === 1 ? "" : "s"} (the classes its members belong to), ${e} cross-class message${e === 1 ? "" : "s"} ordered from the entry points outward.`;
  if (noCalls)
    answer += ` No internal calls are recorded between these members — they act independently, and the canvas says so instead of inventing arrows.`;
  if (gaps.length > (noCalls ? 1 : 0))
    answer += ` ${gaps.length} counted disclosure${gaps.length === 1 ? "" : "s"} under the canvas state${gaps.length === 1 ? "s" : ""} exactly what the picture leaves out.`;
  if (alsoOthers)
    answer += ` (Other usecases match your words too — name the one you meant if this isn't it.)`;
  return { answer, conceptId: u.id, kind };
}
