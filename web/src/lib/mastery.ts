// "Master this codebase" — the engine for the guided onboarding journey.
//
// Everything here is deterministic assembly of the CIE engine's OWN facts
// (an ingest has already run; this module never invents, never guesses a
// number, and tags every conclusion):
//
//   entry points  → symbols no other symbol calls (fan-in 0 in the recorded
//                   call graph) that touch the outside world or carry a
//                   main-ish name. CONFIRMED when the engine fully read the
//                   symbol (summary complete), INFERRED otherwise — with the
//                   reason in words, since the graph only holds what the
//                   engine could trace.
//   top flows     → candidates ranked by how much code each entry point
//                   reaches. Named candidates, never "THE flows".
//   trace         → a shallow deterministic walk over that same call graph;
//                   each step cites its own reads/writes/effects/failures
//                   from the engine's summaries. Steps without a summary
//                   stay honestly INFERRED; the user's debugger is what
//                   upgrades them (YOU-CONFIRMED lives in MasteryState).
//   C4 containers → top-level directories as containers, calls crossing
//                   containers as arrows. What was grouped out is said.
//   risk          → per-file roll-up: history coupling (cochange) ×
//                   outgoing calls × findings — only real loaded data.
//   notes / docs  → ARCHITECTURE.md and NOTES.md assembled from the same
//                   sections the notes panel shows; auto sections cite
//                   their evidence, human sections stay human.
//
// State persistence: localStorage only (the read-only promise stays intact);
// export is an explicit user action. This module is pure: it never imports
// the store, so components on both the stage and the chat side can rely on it.

import type { CieReport, ConceptTree, CieDeps, CieCochange, CieOverlap, CieQuality, CieDrift } from "./cie-types";
import {
  engineFor,
  layoutFor,
  type ChatChartKind,
  type ChatChartSpec,
  type UmlNode,
  type UmlLink,
} from "./chat-chart";
// the mastery card vocabulary lives beside the chart spec — both are things
// the chat renders straight from store facts (never the model's invention)
export type { MasteryRow, TraceStep, SequenceMsg, MasteryCardData } from "./chat-chart";
import type { MasteryRow, TraceStep, SequenceMsg, MasteryCardData } from "./chat-chart";
import { effectPlain } from "./plain-words";

// ============================================================
// Mastery state — persisted per project under cie-mastery:<key>
// ============================================================

export interface MasteryCorrection {
  at: number;
  text: string;
  /** trace key + step index when the correction came from the trace loop */
  trace?: string;
  step?: number;
}

export interface TraceRecord {
  title: string;
  /** entry symbol id in report.symbols — lets the trace be rebuilt on demand */
  entryId: number;
  entryQual: string;
  at: number;
  /** when the user confirmed every drawn step ("I confirmed all of it") */
  confirmedAt?: number;
  /** step indices the user personally verified; Record<number, 1> */
  steps: Record<number, 1>;
}

export interface MasteryState {
  key: string;
  createdAt: number;
  lastVisitAt: number;
  /** station progress: prompt id → when it ran (chip ✓, "view seen"…) */
  prompts: Record<string, number>;
  traces: Record<string, TraceRecord>; // key = entry qual_name, lower-case
  corrections: MasteryCorrection[];
  /** how many blast-radius checks were actually looked at (station 5) */
  blastChecks: number;
}

const STORAGE_PREFIX = "cie-mastery:";

export function masteryKeyOf(source: "demo" | "custom" | null, label: string): string {
  return source === "custom" ? label || "your-project" : "sample";
}

export function loadMasteryState(key: string): MasteryState {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + key);
    if (raw) {
      const m = JSON.parse(raw) as MasteryState;
      if (m && m.key === key) return m;
    }
  } catch {
    /* corrupted or unavailable — fall through to a fresh state */
  }
  return freshMastery(key);
}

export function freshMastery(key: string): MasteryState {
  return {
    key,
    createdAt: Date.now(),
    lastVisitAt: Date.now(),
    prompts: {},
    traces: {},
    corrections: [],
    blastChecks: 0,
  };
}

export function saveMasteryState(m: MasteryState) {
  try {
    localStorage.setItem(STORAGE_PREFIX + m.key, JSON.stringify({ ...m, lastVisitAt: Date.now() }));
  } catch {
    /* storage unavailable — the journey keeps working in memory */
  }
}

// ============================================================
// The call graph (from report.summaries — engine facts only)
// ============================================================

type Sym = CieReport["symbols"][number];

interface CallGraph {
  byId: Map<number, Sym>;
  sumBySym: Map<number, CieReport["summaries"][number]>;
  /** callee id → number of distinct callers the report recorded */
  fanIn: Map<number, number>;
  /** symbol id → resolved callee ids, in recorded order */
  calleesOf: Map<number, number[]>;
}

function buildCallGraph(report: CieReport): CallGraph {
  const byId = new Map<number, Sym>(report.symbols.map((s) => [Number(s.id), s]));
  const sumBySym = new Map<number, CieReport["summaries"][number]>(report.summaries.map((s) => [Number(s.sym), s]));
  const fanIn = new Map<number, number>();
  const calleeSets = new Map<number, Set<number>>();
  for (const sum of report.summaries) {
    const from = Number(sum.sym);
    for (const raw of sum.callees ?? []) {
      const to = Number(raw);
      // skip unresolved names — only calls the engine mapped to real symbols
      if (!Number.isFinite(to) || !byId.has(to)) continue;
      fanIn.set(to, (fanIn.get(to) ?? 0) + 1);
      let set = calleeSets.get(from);
      if (!set) calleeSets.set(from, (set = new Set()));
      set.add(to);
    }
  }
  const calleesOf = new Map<number, number[]>();
  for (const [from, set] of calleeSets) calleesOf.set(from, [...set]);
  return { byId, sumBySym, fanIn, calleesOf };
}

const fileOf = (report: CieReport, s: Sym | undefined): string => (s ? report.files[s.loc.file] ?? "" : "");

// ============================================================
// Entry points
// ============================================================

const MAINISH = /^(main|run|start|serve|server|app|index|listener|consumer|handler|controller|worker|scheduler)$/i;

function touchWords(g: CallGraph, id: number): string[] {
  const sum = g.sumBySym.get(id);
  const out = new Set<string>();
  for (const e of sum?.effects ?? []) out.add(effectPlain(e));
  if (sum?.writes?.length) out.add("writes somewhere");
  if (sum?.outputs?.length) out.add("produces output");
  if (sum?.exceptions?.length) out.add("raises errors");
  return [...out];
}

/** Symbols the report records calls INTO from nowhere, that still touch the
 *  outside world (or carry a conventional start name). Sorted by how much
 *  they touch; capped with the count said, not hidden. */
export function computeEntryPoints(report: CieReport | null): MasteryRow[] {
  if (!report || !report.summaries.length) return [];
  const g = buildCallGraph(report);
  const rows: { entry: MasteryRow; score: number }[] = [];
  for (const s of report.symbols) {
    if (s.kind === "Module") continue;
    const id = Number(s.id);
    if ((g.fanIn.get(id) ?? 0) !== 0) continue; // something calls this — not a start
    const sum = g.sumBySym.get(id);
    const touched = touchWords(g, id);
    const mainish = MAINISH.test(s.name);
    if (!touched.length && !mainish) continue;
    rows.push({
      score: touched.length * 10 + (sum ? 5 : 0) + (mainish ? 3 : 0),
      entry: {
        idx: 0,
        title: s.qual_name || s.name,
        detail: touched.length
          ? `nothing calls this · it touches: ${touched.slice(0, 3).join(", ")}`
          : "nothing calls this · its name matches a conventional start point",
        file: fileOf(report, s),
        byteStart: s.loc.start,
        byteEnd: s.loc.end,
        status: sum ? (sum.complete ? "confirmed" : "inferred") : "inferred",
        why: sum
          ? sum.complete
            ? undefined
            : "the engine read it but couldn't record everything it does"
          : "no behaviour summary was recorded for it",
      },
    });
  }
  rows.sort((a, b) => b.score - a.score || a.entry.title.localeCompare(b.entry.title));
  return rows.slice(0, 14).map((r, i) => ({ ...r.entry, idx: i + 1 }));
}

// ============================================================
// Top flows — candidates ranked by reachability
// ============================================================

function reachableFrom(g: CallGraph, start: number): number {
  const seen = new Set<number>([start]);
  const queue = [start];
  while (queue.length) {
    const cur = queue.shift()!;
    for (const nxt of g.calleesOf.get(cur) ?? []) {
      if (!seen.has(nxt)) {
        seen.add(nxt);
        queue.push(nxt);
      }
    }
  }
  return seen.size;
}

function effectsInTree(g: CallGraph, start: number): string {
  const seen = new Set<number>([start]);
  const queue = [start];
  const eff = new Set<string>();
  while (queue.length) {
    const cur = queue.shift()!;
    for (const nxt of g.calleesOf.get(cur) ?? []) {
      if (seen.has(nxt)) continue;
      seen.add(nxt);
      queue.push(nxt);
      for (const e of g.sumBySym.get(nxt)?.effects ?? []) eff.add(effectPlain(e));
      if ((g.sumBySym.get(nxt)?.writes?.length ?? 0) > 0) eff.add("storage");
    }
  }
  return [...eff].slice(0, 3).join(", ");
}

export interface FlowCandidate {
  rows: MasteryRow[];
  honesty: string;
}

export function computeTopFlows(
  report: CieReport | null,
  deps?: CieDeps | null,
  cochange?: CieCochange | null,
  drift?: CieDrift | null,
  concepts?: ConceptTree | null
): FlowCandidate {
  if (!report || !report.summaries.length) {
    return {
      rows: [],
      honesty:
        "The report doesn't record who-calls-whom for this project, so there are no flow candidates to rank — said honestly rather than invented.",
    };
  }
  const g = buildCallGraph(report);
  const ranked = computeEntryPoints(report)
    .map((e) => {
      const sym = report.symbols.find((s) => (s.qual_name || s.name) === e.title);
      if (!sym) return null;
      return { e, sym, reach: reachableFrom(g, Number(sym.id)) };
    })
    .filter((x): x is { e: MasteryRow; sym: Sym; reach: number } => !!x && x.reach > 1)
    .sort((a, b) => b.reach - a.reach)
    .slice(0, 5)
    .map(({ e, sym, reach }, i) => ({
      ...e,
      idx: i + 1,
      entryId: Number(sym.id),
      detail: `starts here · reaches ${reach} named thing(s)${effectsInTree(g, Number(sym.id)) ? ` · touches: ${effectsInTree(g, Number(sym.id))}` : ""}`,
    }));
  void deps;
  void cochange;
  void drift;
  void concepts;
  return {
    rows: ranked,
    honesty:
      "Candidates ranked by how much code each start point can reach — a structural guess about importance, not a measured truth. Pick one; your debugger confirms what really happens.",
  };
}

// ============================================================
// The trace — a shallow deterministic walk over call records
// ============================================================

export interface TraceResult {
  steps: TraceStep[];
  /** symbols the walk reached but didn't draw (cap) */
  truncated: number;
  effectsSeen: string[];
}

const MAX_STEPS = 12;
const MAX_DEPTH = 5;

export function traceFlow(report: CieReport | null, entryId: number): TraceResult | null {
  if (!report) return null;
  const g = buildCallGraph(report);
  if (!g.byId.has(entryId)) return null;

  const steps: TraceStep[] = [];
  const truncatedSet = new Set<number>();
  const visited = new Set<number>([entryId]);
  const queue: { id: number; caller?: string; depth: number }[] = [{ id: entryId, depth: 0 }];
  const effectsSeen = new Set<string>();

  let guard = 0;
  while (queue.length && guard++ < 2000) {
    const { id, caller, depth } = queue.shift()!;
    const s = g.byId.get(id)!;
    const sum = g.sumBySym.get(id);
    const effects = [...new Set((sum?.effects ?? []).map(effectPlain))].slice(0, 3);
    const writes = [...new Set(sum?.writes ?? [])].slice(0, 2);
    const reads = [...new Set(sum?.reads ?? [])].slice(0, 2);
    const exceptions = [...new Set(sum?.exceptions ?? [])].slice(0, 2);
    for (const e of effects) effectsSeen.add(e);

    if (steps.length >= MAX_STEPS) {
      truncatedSet.add(id); // reached but not drawn — counted, never silently dropped
    } else {
      steps.push({
        idx: steps.length + 1,
        caller,
        name: s.name,
        qualName: s.qual_name || s.name,
        file: fileOf(report, s),
        byteStart: s.loc.start,
        byteEnd: s.loc.end,
        reads,
        writes,
        effects,
        exceptions,
        noSummary: !sum,
        complete: !!sum?.complete,
      });
    }
    if (depth >= MAX_DEPTH) continue;
    const children = (g.calleesOf.get(id) ?? [])
      .slice()
      .sort((a, b) => {
        const sa = g.sumBySym.has(a) ? 0 : 1;
        const sb = g.sumBySym.has(b) ? 0 : 1;
        return sa - sb || a - b;
      });
    for (const c of children) {
      if (visited.has(c)) continue;
      visited.add(c);
      queue.push({ id: c, caller: s.qual_name || s.name, depth: depth + 1 });
    }
  }
  return { steps, truncated: truncatedSet.size, effectsSeen: [...effectsSeen] };
}

/** CONFIRMED = the engine recorded a complete summary for this step;
 *  everything else stays INFERRED with its reason attached. */
export function traceStatusStep(step: TraceStep): "confirmed" | "inferred" {
  return step.complete && !step.noSummary ? "confirmed" : "inferred";
}

export function traceStatusPlain(step: TraceStep): string {
  return step.complete && !step.noSummary
    ? "engine-confirmed"
    : step.noSummary
      ? "inferred — no summary was recorded for this symbol"
      : "inferred — the summary for this symbol is incomplete";
}

export function traceKeyOf(entryQual: string): string {
  return (entryQual || "").toLowerCase();
}

export function readsWritesPlain(st: TraceStep): string {
  const bits: string[] = [];
  if (st.reads.length) bits.push(`reads ${st.reads[0]}`);
  if (st.writes.length) bits.push(`writes ${st.writes[0]}`);
  if (!bits.length && st.effects.length) bits.push(`touches: ${st.effects.join(", ")}`);
  if (!bits.length && !st.noSummary) bits.push("local computation only");
  if (st.noSummary) bits.push("no records kept (the engine couldn't describe this step)");
  return bits.join(" · ");
}

// ============================================================
// Charts: sequence + C4 (both kinds are store-built, never model-picked)
// ============================================================

export function shortPart(f: string): string {
  return f.split("/").pop() || f;
}

/** Build the lane-per-file sequence diagram for a trace. Caller lanes resolve
 *  to the caller's own file where the walk knows it; the entry starts at
 *  "outside". `you` = step indices the user personally verified. */
export function buildSequenceSpec(title: string, steps: TraceStep[], you: Set<number>, max = 9): ChatChartSpec | null {
  if (!steps.length) return null;
  const shown = steps.slice(0, max);
  const participants: string[] = [];
  const laneOfStep = (st: TraceStep, from: boolean): string => {
    if (from) {
      const callerStep = st.caller ? shown.find((x) => x.qualName === st.caller) : undefined;
      return callerStep ? shortPart(callerStep.file) : "outside";
    }
    return shortPart(st.file);
  };
  const msgs: SequenceMsg[] = shown.map((st) => {
    const from = laneOfStep(st, true);
    const to = laneOfStep(st, false);
    if (!participants.includes(from)) participants.push(from);
    if (!participants.includes(to)) participants.push(to);
    const status: SequenceMsg["status"] = you.has(st.idx) ? "you" : traceStatusStep(st);
    return {
      from,
      to,
      label: `${st.qualName}()`,
      sub: readsWritesPlain(st),
      status,
      loc: st.file ? { file: st.file, byteStart: st.byteStart, byteEnd: st.byteEnd } : undefined,
    };
  });
  const s: ChatChartSpec = {
    kind: "sequence",
    engine: engineFor("sequence"),
    layout: layoutFor("sequence"),
    title,
    tell:
      "Arrows are calls from the report's records. Solid = the engine fully read that step · dashed amber = inferred · dashed emerald with YOU CONFIRMED = you checked it yourself.",
    source: "report.json — the call records and each step's recorded behaviour",
  };
  s.participants = participants;
  s.msgs = msgs;
  return s;
}

/** C4-style containers: top-level folders, how much lives in each, and the
 *  calls that cross container boundaries. Grouping is by the repo's own
 *  structure — and the cap is announced. */
export function buildC4Spec(report: CieReport | null, title = "C4 containers — the big picture"): ChatChartSpec | null {
  if (!report || !report.files?.length) return null;
  const containerOf = (f: string): string => {
    const i = f.indexOf("/");
    return i < 0 ? "(project root)" : f.slice(0, i);
  };
  const g = buildCallGraph(report);
  const fileCount = new Map<string, number>();
  const symCount = new Map<string, number>();
  for (const f of report.files) {
    const c = containerOf(f);
    fileCount.set(c, (fileCount.get(c) ?? 0) + 1);
  }
  for (const s of report.symbols) {
    const c = containerOf(fileOf(report, s));
    if (!c) continue;
    symCount.set(c, (symCount.get(c) ?? 0) + 1);
  }
  // cross-container call counts
  const crossCalls = new Map<string, Map<string, number>>();
  for (const s of report.symbols) {
    const from = containerOf(fileOf(report, s));
    for (const callee of g.calleesOf.get(Number(s.id)) ?? []) {
      const to = containerOf(fileOf(report, g.byId.get(callee)));
      if (!to || to === from || !fileCount.has(to)) continue;
      let m = crossCalls.get(from);
      if (!m) crossCalls.set(from, (m = new Map()));
      m.set(to, (m.get(to) ?? 0) + 1);
    }
  }

  const MAX_CONTAINERS = 12;
  const order = [...fileCount.entries()].sort(
    (a, b) => (symCount.get(b[0]) ?? 0) - (symCount.get(a[0]) ?? 0) || a[0].localeCompare(b[0])
  );
  const shown = order.slice(0, MAX_CONTAINERS);
  const rest = order.length - shown.length;

  const nodes: UmlNode[] = shown.map(([name, fc]) => ({
    id: name,
    title: name,
    kind: "container",
    lines: [`${fc} file(s)`, `${symCount.get(name) ?? 0} named things`],
    certainty: "Fact" as const,
  }));
  if (rest > 0) {
    nodes.push({
      id: "…other-parts",
      title: `…and ${rest} more container(s)`,
      kind: "container",
      lines: ["smaller — grouped out of this picture, not hidden"],
      certainty: "Inference" as const,
    });
  }
  const links: UmlLink[] = [];
  for (const [from, targets] of crossCalls) {
    if (!nodes.some((n) => n.id === from)) continue;
    for (const [to, n] of [...targets.entries()].sort((a, b) => b[1] - a[1]).slice(0, 4)) {
      if (!nodes.some((x) => x.id === to)) continue;
      links.push({
        from,
        to,
        label: n > 1 ? `calls into (${n})` : "calls into",
        certainty: "Fact",
      });
    }
  }
  const s = specOf("c4", title, "Boxes are whole containers — top-level folders with their contents counted. Arrows are real calls across containers.");
  s.nodes = nodes;
  s.links = links;
  s.source = "report.json — the file table, named things, and recorded calls";
  return s;
}

function specOf(kind: ChatChartKind, title: string, tell: string): ChatChartSpec {
  return { kind, engine: engineFor(kind), layout: layoutFor(kind), title, tell };
}

// ============================================================
// "5 things a new developer should know"
// ============================================================

const FIVE_CAP = 5;

export function computeFiveThings(ctx: {
  report: CieReport | null;
  concepts: ConceptTree | null;
  deps?: CieDeps | null;
  cochange?: CieCochange | null;
  overlap?: CieOverlap | null;
  drift?: CieDrift | null;
}): { items: string[]; note: string } {
  const { report, concepts, deps, cochange, overlap: _overlap, drift } = ctx;
  const items: string[] = [];
  if (report) {
    const entries = computeEntryPoints(report);
    const top = entries[0];
    if (top) items.push(`Everything starts at **${top.title}** (\`${top.file}\`) — CIE sees nothing calling it. Start every investigation there.`);

    const tally = new Map<string, number>();
    for (const s of report.summaries) for (const e of s.effects ?? []) { const p = effectPlain(e); tally.set(p, (tally.get(p) ?? 0) + 1); }
    const eff = [...tally.entries()].sort((a, b) => b[1] - a[1])[0];
    if (eff) items.push(`Its most common real-world touch is "${eff[0]}" — ${eff[1]} named thing(s) do it. A test double for that area unlocks a lot of testing.`);

    if (report.findings.length) {
      const f0 = [...report.findings].sort((a, b) => sevRank(b.severity) - sevRank(a.severity))[0];
      items.push(`${report.findings.length} problem(s) are already listed — the most severe is **${f0.message.slice(0, 80)}** (${f0.severity}). Check "Things to look at" before debugging blind.`);
    }
    if (report.coverage.files_analyzed < report.coverage.files_total) {
      items.push(`Fully analysed: ${report.coverage.files_analyzed} of ${report.coverage.files_total} files (${report.coverage.symbols_analyzed} of ${report.coverage.symbols_total} named things). ${report.coverage.notes?.[0] ?? ""}`);
    } else {
      items.push(`Everything CIE could read was fully analysed (${report.coverage.files_analyzed} files, ${report.coverage.symbols_total} named things) — the trust page names the exceptions.`);
    }
    if (deps?.table && deps.table.files_scanned > 0) {
      const eco = new Map<string, number>();
      for (const d of deps.table.deps) eco.set(d.ecosystem, (eco.get(d.ecosystem) ?? 0) + 1);
      const topEco = [...eco.entries()].sort((a, b) => b[1] - a[1])[0];
      items.push(`${deps.table.deps.length} dependencies declared (mostly ${topEco?.[0] ?? "various"}), read from ${deps.table.files_scanned} manifest file(s) — versions kept exactly as written, never resolved.`);
    }
  }
  if (concepts && items.length < FIVE_CAP) {
    const dom = concepts.nodes.find((n) => n.kind === "Domain" && n.behaviors.length);
    if (dom) items.push(`The project's whole job, in the engine's words: ${dom.behaviors.slice(0, 3).map((b) => b.kind).join("; ")}.`);
  }
  if (cochange?.edges?.length && items.length < FIVE_CAP) {
    const top = [...cochange.edges].sort((a, b) => b.support - a.support)[0];
    items.push(`**${top.a}** and **${top.b}** changed together ${top.support} time(s) in git history — a change to one almost always wants the other touched too.`);
  }
  if (drift?.aggregate && items.length < FIVE_CAP) {
    const bad = (drift.aggregate["Contradicting"] ?? 0) + (drift.aggregate["Drifted"] ?? 0) + (drift.aggregate["Missing"] ?? 0);
    if (bad > 0) items.push(`${bad} claim(s) in the project's own docs don't match the code (contradicted / drifted / missing) — trust the code over the docs, then fix the docs.`);
  }
  return {
    items: items.slice(0, FIVE_CAP),
    note: "Drafted straight from the report's own facts — edit it in Mastery notes until it reads like a colleague wrote it. That edited version is what ARCHITECTURE.md keeps.",
  };
}

function sevRank(sev: string): number {
  return sev === "Critical" ? 3 : sev === "High" ? 2 : sev === "Medium" ? 1 : 0;
}

// ============================================================
// The chat fast path — one instant, cited answer per journey prompt
// ============================================================

export interface MasteryAnswer {
  answer: string;
  card?: MasteryCardData;
  chart?: ChatChartSpec | null;
  promptKey?: string;
}

const C4_WORDS = /\bbig picture\b|\bc4\b|\bcontainers?\b/i;
const FLOWS_WORDS = /\b(top|important|critical|central) flows?\b|\bflow candidates?\b/i;
const TRACE_WORDS = /\btrace\b|\bwalk (the )?(flow|calls?|path)\b|\bfollow (the )?calls?\b/i;
const FIVE_WORDS = /\b5 things\b|\bfive things\b|\bnew (dev|developer|engineer|hire)\b|\bmust know\b|\bonboard/i;
const ENTRY_WORDS = /\bwhere does (it|this) (start|begin)\b|\bentry ?points?\b|\bstarting ?points?\b|\bwhat runs first\b|\bmain (class|function|file|entry)\b/i;
const SUMMARY_WORDS = /\bwhat (is|does) (this|it)\b|\bsummar(y|ise|ize)\b|\bexplain (this|the) (code|project|system)\b/i;

export interface MasteryCtx {
  report: CieReport | null;
  concepts: ConceptTree | null;
  deps?: CieDeps | null;
  cochange?: CieCochange | null;
  overlap?: CieOverlap | null;
  drift?: CieDrift | null;
  label: string;
}

/** The trace fast path, shared by the typed question and the flow-picker's
 *  "Trace this one →" — one deterministic build of card + sequence. */
export function buildTraceAnswer(report: CieReport, entryId: number): MasteryAnswer | null {
  const sym = report.symbols.find((s) => Number(s.id) === entryId);
  if (!sym) return null;
  const qual = sym.qual_name || sym.name;
  const res = traceFlow(report, entryId);
  if (!res) return null;
  const confirmed = res.steps.filter((s) => traceStatusStep(s) === "confirmed").length;
  const card: MasteryCardData = {
    kind: "trace",
    title: `Trace: ${qual}`,
    steps: res.steps,
    truncated: res.truncated,
    entryQual: qual,
    entryId,
    traceKey: traceKeyOf(qual),
    note:
      `${confirmed} of ${res.steps.length} drawn step(s) engine-confirmed` +
      (res.truncated ? ` — ${res.truncated} more reached but not drawn (cap, honestly counted)` : "") +
      ".",
  };
  // the sequence diagram is opened from the CARD, rebuilt with the live
  // verification state — the canvas never shows a stale verify pass
  const seq = buildSequenceSpec(`Sequence — ${qual}`, res.steps, new Set());
  return {
    answer:
      `Traced **${qual}** through the report's own call records: ${res.steps.length} step(s), ${confirmed} engine-confirmed` +
      (res.truncated ? `, ${res.truncated} more reached but not drawn` : "") +
      ".\n\nEvery step shows just what the engine recorded for that symbol. Steps marked inferred are paths concluded without a complete description — your debugger is what upgrades them, and the card below is where you record that.",
    card,
    chart: seq,
    promptKey: "trace",
  };
}

export function tryAnswerMastery(question: string, ctx: MasteryCtx): MasteryAnswer | null {
  const { report } = ctx;
  if (!report) return null;
  const has = (re: RegExp) => re.test(question);

  // trace first — the most specific journey ask
  if (has(TRACE_WORDS)) {
    const target = findTraceTarget(report, question) ?? (() => {
      const flow = computeTopFlows(report);
      if (flow.rows.length === 1 && flow.rows[0].entryId != null) {
        const id = flow.rows[0].entryId;
        const sym = report.symbols.find((s) => Number(s.id) === id);
        return sym ? { id, qual: sym.qual_name, title: sym.qual_name } : null;
      }
      return null;
    })();
    if (!target) {
      return {
        answer:
          "I can trace any recorded call path — name the thing to start from, or first ask **where does it start from?** and trace one of the start points.",
        promptKey: "trace",
      };
    }
    return buildTraceAnswer(report, target.id);
  }

  if (has(FIVE_WORDS)) {
    const five = computeFiveThings(ctx);
    return {
      answer: "The onboarding draft — every line comes from the report's own facts. Mastery notes holds it; your edits turn it into the doc.",
      card: { kind: "fivethings", title: "5 things a new developer should know", items: five.items, note: five.note },
      promptKey: "fivethings",
    };
  }

  if (has(ENTRY_WORDS)) {
    const rows = computeEntryPoints(report);
    return {
      answer: rows.length
        ? `Where things start, from the report's own call graph: ${rows.length} named thing(s) that nothing else calls and that touch the outside world. Click any row to read the real code.`
        : "No obvious start point shows up — nothing with zero callers that touches the outside world either. Honest for this shape of project; try **the top flows** next.",
      card: {
        kind: "entrypoints",
        title: "Where this system starts",
        entryRows: rows,
        note: "Each row: fan-in 0 within the recorded calls + a real-world touch (or a conventional start name). The graph holds only what the engine could trace — inferred steps say why.",
      },
      promptKey: "entry",
    };
  }

  if (has(FLOWS_WORDS)) {
    const flow = computeTopFlows(report);
    if (!flow.rows.length) return { answer: flow.honesty, promptKey: "flows" };
    return {
      answer:
        "Ranked flow candidates — pick one and it traces step by step below. Not “THE flows”: a structural ranking, labelled as the guess about importance it is.",
      card: { kind: "flows", title: "Top flow candidates", entryRows: flow.rows, note: flow.honesty },
      promptKey: "flows",
    };
  }

  if (has(C4_WORDS)) {
    const spec = buildC4Spec(report);
    if (!spec) return null;
    return {
      answer:
        "The big picture, from the file table and the recorded calls: containers (top-level folders), what lives in each, and which container calls into which. " +
        "One honest note: a full C4 model also maps runtime deployment — the report holds static structure only, so that layer stays named-as-absent.",
      chart: spec,
      promptKey: "c4",
    };
  }

  if (has(SUMMARY_WORDS)) return { answer: summaryAnswer(ctx), promptKey: "summary" };
  return null;
}

/** "What is this system?" — instant, cited summary (also the CIE side of the
 *  checkpoint reveal and the summary section of the notes). */
export function summaryAnswer(ctx: MasteryCtx): string {
  const { report, concepts, deps, drift, label } = ctx;
  if (!report) return "";
  const bits: string[] = [];
  const dom = concepts?.nodes.find((n) => n.kind === "Domain" && n.behaviors.length);
  if (dom) bits.push(`**${label}** is a system whose parts do: ${dom.behaviors.slice(0, 4).map((b) => b.kind).join("; ")}.`);
  else {
    const langs = [...new Set(report.symbols.map((s) => s.lang))].slice(0, 3).join(", ");
    bits.push(`**${label}** — ${report.files.length} file(s), ${report.symbols.length} named things (${langs}). Everything indexed is loadable evidence; questions are answered from it, not around it.`);
  }
  const entries = computeEntryPoints(report);
  if (entries.length) bits.push(`It starts at **${entries[0].title}** (\`${entries[0].file}\`) — ask *where does it start from?* for the whole list.`);
  if (deps?.table && deps.table.files_scanned > 0)
    bits.push(`It declares **${deps.table.deps.length}** dependencies across ${deps.table.files_scanned} manifest(s), versions exactly as written.`);
  if (drift?.aggregate) {
    const bad = (drift.aggregate["Contradicting"] ?? 0) + (drift.aggregate["Drifted"] ?? 0);
    if (bad) bits.push(`Careful: ${bad} claim(s) in the project's own docs contradict or drift from the code — the Trust page names them.`);
  }
  bits.push("*Each sentence above is a fact from the loaded engine exports — verify what matters against the code; the journey exists to make that easy.*");
  return bits.join("\n\n");
}

/** Trace-target resolution from words. Deterministic (whole qual-name hits
 *  rank above bare names), with a real threshold so a stray word can't
 *  misfire a trace. */
export function findTraceTarget(report: CieReport, q: string): { id: number; qual: string; title: string } | null {
  if (!q || q.length > 400) return null;
  const lower = q.toLowerCase();
  let best: { s: Sym; score: number } | null = null;
  for (const s of report.symbols) {
    const qual = (s.qual_name || s.name).toLowerCase();
    const nm = s.name.toLowerCase();
    let score = 0;
    if (qual.length > 2 && lower.includes(qual)) score += 100;
    if (nm.length > 2 && lower.includes(nm)) score += 55;
    for (const t of new Set(lower.split(/[^a-z0-9_]+/))) {
      if (t.length > 2 && qual.split(/[_.]/).includes(t)) score += 20;
    }
    if (score > (best?.score ?? 0)) best = { s, score };
  }
  if (!best || best.score < 40) return null;
  return { id: Number(best.s.id), qual: best.s.qual_name || best.s.name, title: best.s.qual_name || best.s.name };
}

export function resolveSymbolAt(
  report: CieReport | null,
  name: string | undefined,
  file: string | undefined,
  byteStart?: number
): Sym | null {
  if (!report || !name) return null;
  const lower = name.toLowerCase();
  const inFile = (s: Sym) => !file || (report.files[s.loc.file] ?? "") === file || file.endsWith("/" + (report.files[s.loc.file] ?? ""));
  if (byteStart != null) {
    // prefer the function/class containing the byte — the whole-file Module
    // symbol also spans it and would otherwise shadow the real target
    const hit =
      report.symbols.find((s) => s.kind !== "Module" && inFile(s) && s.loc.start <= byteStart && byteStart < s.loc.end) ??
      report.symbols.find((s) => inFile(s) && s.loc.start <= byteStart && byteStart < s.loc.end);
    if (hit) return hit;
  }
  return (
    report.symbols.find((s) => inFile(s) && s.kind !== "Module" && (s.qual_name || s.name).toLowerCase() === lower) ??
    report.symbols.find((s) => inFile(s) && s.kind !== "Module" && s.name.toLowerCase() === lower) ??
    null
  );
}
