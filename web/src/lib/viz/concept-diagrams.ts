// PRD v2.0 §27 (B3) — a concept's members as renderable diagrams.
//
// Two pure functions turn one concept node (from concepts.json) plus the
// report (report.json) into gate-ready viz-adapter ViewSpecs:
//
//   conceptToFlowchartSpec(concept, report)  → call graph of the extent
//   conceptToSequenceSpec(concept, report)   → cross-class message order
//
// Membership is 100% decided by the engine (the concept's recorded
// `symbols` — never by labels); what this module adds is only the
// resolution and the honest presentation rules:
//
//   shared prep: member symbols → report.symbols (names, byte spans,
//   owning container) → report.summaries (resolved callees) → the call
//   edges whose BOTH ends are inside the extent. Members are named by
//   their recorded behavior label when the concept carries one.
//
//   flowchart: nodes = member functions, edges = internal calls,
//   entry points = members no in-extent caller reaches, isolates shown
//   but flagged. Sequence: lifelines = the classes the members belong to
//   (deduped), messages = cross-class edges ordered by BFS depth from
//   the entry points, intra-class calls suppressed in v1 (disclosed).
//
//   HONESTY GUARD: when no internal call edges are recorded the spec
//   says so in `gaps` ("members act independently") — edges are never
//   fabricated, and a member the tables cannot evidence is skipped and
//   counted, never invented.
//
// Layout stays a hint, not an engine name (VIZ-10): "dagre" keeps every
// graph engine eligible. The sequence's semantic shape (lifeline
// elements + message edges) is what makes the mermaid adapter emit a
// sequenceDiagram (catalog clause B3); the BFS depth rides each message
// in `fields.depth` so any lane layout can order without re-deriving.

import { gate, recomputeRenderHash, recomputeSemanticHash } from "viz-adapter";
import type { Gap, ViewEdge, ViewSpec, ViewElement } from "viz-adapter";
import type { CieReport, ConceptNode, SymbolInfo, SummaryInfo } from "@/lib/cie-types";

export interface ConceptDiagramResult {
  view: ViewSpec | null; // null = the gate refused; violations say exactly why
  violations: string[];
}

/** The two diagrams a concept can draw (C1/C2 vocabulary). Lives here so
 *  both the lib chat seam and the components share one definition. */
export type ConceptDiagramKind = "flowchart" | "sequence";

export const NO_INTERNAL_CALLS_GAP =
  "no internal call edges recorded — members act independently";

interface Member {
  id: number;
  sym: SymbolInfo;
  summary: SummaryInfo | null;
  label: string; // behavior label when recorded, else the symbol's name
  laneKey: string; // stable lane/group key (owner symbol id or file)
  laneLabel: string; // owning class/module name, else the file path
  laneIsClass: boolean; // false = the lane is a file/module stand-in
  evidence: { file: string; start: number; end: number };
  laneEvidence: { file: string; start: number; end: number };
}

interface Prepared {
  members: Member[]; // extent order (symbol id ascending)
  byId: Map<number, Member>;
  edges: { from: number; to: number }[]; // internal calls, sorted (from, to)
  entries: number[]; // member ids no in-extent caller reaches
  skipped: number; // extent ids the tables could not evidence
}

const fnId = (id: number) => `fn:${id}`;
const laneId = (key: string) => `lane:${key}`;

function evidenceOf(report: CieReport, sym: SymbolInfo): { file: string; start: number; end: number } | null {
  const path = report.files[sym.loc?.file];
  if (!path || typeof sym.loc?.start !== "number" || typeof sym.loc?.end !== "number") return null;
  return { file: path, start: sym.loc.start, end: Math.max(sym.loc.end, sym.loc.start) };
}

/** The shared prep. Deterministic by construction: members in symbol-id
 *  order, edges sorted (from, to), entries sorted. */
function prepare(concept: ConceptNode, report: CieReport): Prepared {
  const symById = new Map<number, SymbolInfo>();
  for (const s of report.symbols) symById.set(s.id, s);
  const summaryBySym = new Map<number, SummaryInfo>();
  for (const s of report.summaries) summaryBySym.set(s.sym, s);
  const behaviorLabel = new Map<number, string>();
  for (const b of concept.behaviors ?? []) behaviorLabel.set(b.summary, b.kind);

  const extent = [...new Set(concept.symbols)].sort((a, b) => a - b);
  const members: Member[] = [];
  const byId = new Map<number, Member>();
  let skipped = 0;

  for (const id of extent) {
    const sym = symById.get(id);
    const evidence = sym ? evidenceOf(report, sym) : null;
    if (!sym || !evidence) {
      skipped += 1; // the extent names it; the tables cannot evidence it
      continue;
    }
    // Owning container: the recorded parent when resolvable, else the
    // member's own file as the module stand-in. Never guessed further.
    const parent = sym.parent;
    const ownerSym =
      (parent === null || parent === undefined ? undefined : symById.get(parent as number)) ??
      undefined;
    const ownerSelf = ownerSym && ownerSym.id === sym.id;
    const useOwner = ownerSym && !ownerSelf ? ownerSym : undefined;
    const laneEvidence = useOwner ? evidenceOf(report, useOwner) ?? evidence : evidence;
    const member: Member = {
      id,
      sym,
      summary: summaryBySym.get(id) ?? null,
      label: behaviorLabel.get(id) ?? sym.name,
      laneKey: useOwner ? `sym:${useOwner.id}` : `file:${sym.loc.file}`,
      laneLabel: useOwner ? useOwner.name : report.files[sym.loc.file] ?? "unknown file",
      laneIsClass: useOwner !== undefined && useOwner.kind !== "Module",
      evidence,
      laneEvidence,
    };
    members.push(member);
    byId.set(id, member);
  }

  const extentSet = new Set(byId.keys());
  const seen = new Set<string>();
  const edges: { from: number; to: number }[] = [];
  for (const m of members) {
    const callees = m.summary?.callees ?? [];
    for (const raw of callees) {
      const to = typeof raw === "number" ? raw : Number(raw);
      if (!extentSet.has(to)) continue; // both ends must be inside the extent
      const key = `${m.id}->${to}`;
      if (seen.has(key)) continue; // parallel call sites collapse to one edge
      seen.add(key);
      edges.push({ from: m.id, to });
    }
  }
  edges.sort((a, b) => a.from - b.from || a.to - b.to);

  const called = new Set(edges.map((e) => e.to));
  const entries = members.filter((m) => !called.has(m.id)).map((m) => m.id);

  return { members, byId, edges, entries, skipped };
}

function gapsFor(prep: Prepared, area: string, extra: string[] = []): Gap[] {
  const gaps: Gap[] = [];
  for (const why of extra) {
    gaps.push({ id: `gap:${area}:${gaps.length}`, area, why });
  }
  if (prep.edges.length === 0) {
    gaps.push({ id: `gap:${area}:${gaps.length}`, area, why: NO_INTERNAL_CALLS_GAP });
  }
  if (prep.skipped > 0) {
    gaps.push({
      id: `gap:${area}:${gaps.length}`,
      area,
      why: `${prep.skipped} extent member(s) could not be evidenced by the symbol table and are not shown`,
    });
  }
  return gaps;
}

function finish(
  elements: ViewElement[],
  edges: ViewEdge[],
  gaps: Gap[],
  caption: string,
  revision: number,
): ConceptDiagramResult {
  const count = (mode: string) =>
    elements.filter((e) => e.displayMode === mode).length +
    edges.filter((e) => e.displayMode === mode).length;
  const view: ViewSpec = {
    elements,
    edges,
    footer: {
      facts: count("Fact"),
      inferences: count("Inference"),
      hypotheses: count("Hypothesis"),
      fog: count("Fog"),
      stale: 0,
      caption,
    },
    gaps,
    hidden: [], // every evidenced member is shown — the extent IS the view
    scope: { revision, environments: [], coverage: { analyzed: true } },
    layoutHint: { strategy: "dagre" },
    semanticHash: "",
    renderHash: "",
  };
  view.semanticHash = recomputeSemanticHash(view);
  view.renderHash = recomputeRenderHash(view, "");
  try {
    gate(view);
    return { view, violations: [] };
  } catch (err) {
    return { view: null, violations: [err instanceof Error ? err.message : String(err)] };
  }
}

const FACT_SALIENCE = (whyShown: string, weight: number) => ({
  factors: ["concept-extent"],
  weight,
  whyShown,
});

/** The concept's extent as a call graph: member functions named by their
 *  recorded behavior labels, internal calls as edges, entry points and
 *  isolates flagged in `fields`. With no internal calls the view still
 *  renders every member — and the gap says exactly what is missing. */
export function conceptToFlowchartSpec(
  concept: ConceptNode,
  report: CieReport,
): ConceptDiagramResult {
  const prep = prepare(concept, report);
  const kind = concept.kind ?? "UseCase";
  const elements: ViewElement[] = prep.members.map((m) => {
    const isEntry = prep.entries.includes(m.id);
    const hasOut = prep.edges.some((e) => e.from === m.id);
    const isolate = isEntry && !hasOut;
    return {
      id: fnId(m.id),
      elementKind: "function",
      displayMode: "Fact" as const,
      label: m.label,
      evidence: m.evidence,
      salience: FACT_SALIENCE(
        `recorded member of ${kind} "${concept.label}"`,
        isolate ? 0.4 : isEntry ? 1 : 0.7,
      ),
      fields: {
        symbol: m.id,
        owner: m.laneLabel,
        ...(isEntry ? { entry: true } : {}),
        ...(isolate ? { isolate: true } : {}),
      },
    };
  });
  const edges: ViewEdge[] = prep.edges.map((e) => ({
    id: `call:${e.from}->${e.to}`,
    source: fnId(e.from),
    target: fnId(e.to),
    edgeKind: "call",
    displayMode: "Fact" as const,
    // The caller's own bytes evidence the edge: the callee list lives in
    // the caller's recorded summary.
    evidence: prep.byId.get(e.from)!.evidence,
  }));
  return finish(
    elements,
    edges,
    gapsFor(prep, `flowchart:${concept.id}`),
    `Call shape of ${kind} "${concept.label}" — nodes are its recorded members, edges the resolved calls between them; entries carry no in-concept caller.`,
    report.rev,
  );
}

/** BFS depth from the entry set over the internal call graph — the
 *  sequence's message order (nearer the entries first). */
function depthFromEntries(prep: Prepared): Map<number, number> {
  const depth = new Map<number, number>();
  const queue: number[] = [];
  for (const id of prep.entries) {
    if (!depth.has(id)) {
      depth.set(id, 0);
      queue.push(id);
    }
  }
  while (queue.length > 0) {
    const cur = queue.shift()!;
    for (const e of prep.edges.filter((e) => e.from === cur)) {
      if (!depth.has(e.to)) {
        depth.set(e.to, (depth.get(cur) ?? 0) + 1);
        queue.push(e.to);
      }
    }
  }
  return depth;
}

/** The concept's extent as a message sequence: lifelines are the classes
 *  its members belong to (deduped), messages are the CROSS-CLASS call
 *  edges ordered by BFS depth from the entry points. Intra-class calls
 *  are suppressed in v1 and disclosed when they were the only edges.
 *  Element/edge order IS the reading order — the layout hint stays
 *  engine-neutral. */
export function conceptToSequenceSpec(
  concept: ConceptNode,
  report: CieReport,
): ConceptDiagramResult {
  const prep = prepare(concept, report);
  const kind = concept.kind ?? "UseCase";

  // Lane order: the entries' lanes first (entry id order), then the rest
  // by first member id — the reading order of the diagram.
  const laneOrder: string[] = [];
  const pushLane = (key: string) => {
    if (!laneOrder.includes(key)) laneOrder.push(key);
  };
  for (const entryId of prep.entries) {
    const m = prep.byId.get(entryId);
    if (m) pushLane(m.laneKey);
  }
  for (const m of prep.members) pushLane(m.laneKey);
  const laneByKey = new Map<string, Member>();
  for (const m of prep.members) if (!laneByKey.has(m.laneKey)) laneByKey.set(m.laneKey, m);

  const elements: ViewElement[] = laneOrder.map((key) => {
    const m = laneByKey.get(key)!;
    return {
      id: laneId(key),
      elementKind: "lifeline",
      displayMode: "Fact" as const,
      label: m.laneLabel,
      evidence: m.laneEvidence,
      salience: FACT_SALIENCE(
        `owns recorded member(s) of ${kind} "${concept.label}"`,
        m.laneIsClass ? 1 : 0.6,
      ),
      fields: {
        laneOf: m.laneIsClass ? "class" : "module",
        members: prep.members.filter((x) => x.laneKey === key).map((x) => x.id),
      },
    };
  });

  const depth = depthFromEntries(prep);
  const cross = prep.edges.filter((e) => prep.byId.get(e.from)!.laneKey !== prep.byId.get(e.to)!.laneKey);
  const intraCount = prep.edges.length - cross.length;
  cross.sort(
    (a, b) =>
      (depth.get(a.from) ?? Number.MAX_SAFE_INTEGER) - (depth.get(b.from) ?? Number.MAX_SAFE_INTEGER) ||
      a.from - b.from ||
      a.to - b.to,
  );

  const extraGaps: string[] = [];
  if (prep.edges.length > 0 && cross.length === 0) {
    extraGaps.push("every recorded call stays inside one class — no cross-class messages to sequence");
  }
  if (intraCount > 0 && cross.length > 0) {
    extraGaps.push(`${intraCount} intra-class call(s) suppressed in this view (v1 sequences cross-class messages only)`);
  }

  const edges: ViewEdge[] = cross.map((e) => {
    const from = prep.byId.get(e.from)!;
    const to = prep.byId.get(e.to)!;
    return {
      id: `msg:${e.from}->${e.to}`,
      source: laneId(from.laneKey),
      target: laneId(to.laneKey),
      // edgeKind is the open kind vocabulary: the `message:` family
      // registers the sequence-message edge, the text after the prefix is
      // the message itself (the callee's recorded name/behavior label) —
      // this is what the mermaid adapter renders.
      edgeKind: `message:${to.label}`,
      displayMode: "Fact" as const,
      evidence: from.evidence,
    };
  });

  return finish(
    elements,
    edges,
    gapsFor(prep, `sequence:${concept.id}`, extraGaps),
    `Message order of ${kind} "${concept.label}" — lifelines are the classes its members belong to, ordered from the entry points outward (fields.depth carries the BFS depth).`,
    report.rev,
  );
}
