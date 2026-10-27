// "Draw anything" — when the visitor asks to SEE something, CIE builds the
// picture directly from the indexed store (report + viewspec + concepts),
// with no AI round-trip. Nothing is invented: every box, bar, number and
// arrow traces back to the loaded exports, and every picture names what it
// was built from plus what it leaves out (the honesty rules).
//
// Why this exists: Ask CIE used to keep a tiny menu of four drawings, so a
// request like "show me the UML diagram" got an honest but useless
// "I can't draw that". The store always held enough shape (files, named
// things, jobs, rules, connections, calls) to draw structure diagrams — the
// menu was just too small. This module widens the menu to "anything the
// store can honestly support", instantly.

import type { CieReport, CieViewSpec, ConceptTree, VizElement, CieDeps, CieRuntime, CieQuality } from "./cie-types";
import {
  engineFor,
  layoutFor,
  type ChatChartKind,
  type ChatChartSpec,
  type ElementLoc,
  type SequenceMsg,
  type UmlNode,
  type UmlLink,
} from "./chat-chart";
import { buildConceptTreeSpec, buildCoverageHeatmapSpec, findSymbolInQuestion } from "./quality";
import { buildCallGraph, buildC4Spec, fileOf } from "./mastery";
import { buildDataFlowSpec, containerOfPath } from "./atlas";
import type { TechStack } from "./stack-types";
import {
  findingTitle,
  obligationTitle,
  authorityPlain,
  certaintyPlain,
  effectPlain,
  edgeKindPlain,
  severityPlain,
} from "./plain-words";

const cut = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

function spec(kind: ChatChartKind, title: string, tell: string): ChatChartSpec {
  return { kind, engine: engineFor(kind), layout: layoutFor(kind), title, tell };
}

// ---------- intent detection (simple word matching, conservative) ----------

const DRAW_HINT =
  /\b(draw|drawing|show|see|render|diagram|picture|visuali[sz]e|plot|graph|uml|chart|map|table|list|flowchart)\b/i;
const CANT_WORDS =
  /\b(timeline|gantt|calendar|roadmap|burndown|video|animation|screenshot|photo|image)\b/i;
// Chart-shaped asks that carry no explicit "draw/show" verb.
const CHART_NOUNS =
  /\b(uml|architecture|class diagram|structure diagram|component diagram|erd|flow|flowchart|call ?graph|call chain|who calls|bubbles?)\b/i;
// Vague chart words ("split", "list", "compare") only count when the ask is
// clearly about THIS project — otherwise they fall through to the AI.
const PROJECT_CONTEXT =
  /\b(problem|finding|issue|bug|severity|urgency|promise|contract|rule|certain|sure|trust|guess|proven|written|secret|password|named things?|symbol|code)\b/i;
const UML_WORDS =
  /\b(uml|class diagram|structure diagram|architecture|component diagram|module diagram|erd|entity relationship|how (is|the) ?\w* ?(code|project|app)? ?(is )?(organis|organiz)|code shape|box(es)? and (lines|arrows))\b/i;
const FLOW_WORDS =
  /\b(flow|flowchart|sequence|pipeline|call ?graph|who calls|call chain|call flow|data ?flow)\b/i;
const MAP_WORDS = /\b(map|bubbles?)\b/i;
const DONUT_WORDS = /\b(donut|doughnut|pie|proportion|breakdown|percent|ratio|split)\b/i;
const BAR_WORDS = /\b(bar|compare|comparison|most|least|ranking|rank|per file|each file|worst)\b/i;
const TABLE_WORDS = /\b(table|list|which files|show (me )?(all|every))\b/i;
const STAT_WORDS = /\b(how many|how much|number of|count|total)\b/i;
// Runtime-evidence vocabularies — answered only when the loaded project has
// collected runtime data; otherwise they fall through to honest-no answers.
const DEPS_WORDS = /\b(depend|dependencies|deps|package|pom|gradle|cargo|requirements|composer|nuget|manifest|librar(y|ies))\b/i;
const COVERAGE_WORDS = /\b(coverage|tests? (cover|touch|reach)|which lines are tested|test coverage)\b/i;
const CPU_WORDS = /\b(cpu|hotspot|profile|profiler|where.*(time|slow)|slowest)\b/i;
const MEMORY_WORDS = /\b(memory|allocation|heap|bytes|tracemalloc)\b/i;
const RACE_WORDS = /\b(race|lost updates?|data race|atomic)\b/i;
const DEADLOCK_WORDS = /\bdeadlocks?\b/i;
// A bare "overlap" used to be in here too, but that word now also means
// capability overlap (cie overlap, buildCompanionChart) — require it to
// actually be about threads/time so a real capability-overlap question
// doesn't get silently hijacked into the runtime-concurrency chart.
const CONCURRENCY_WORDS = /\b(concurren|threads? (ran|run|overlap)|overlap(ping)? (thread|time|run)|parallel)\b/i;

// A GENERIC drawing request — used only in the guard: a drawing verb with
// a chart noun whose kind NO vocabulary resolved is not this module's ask
// anymore. It was used to draw the stand-in structure map ("without a kind
// this project can answer with"); that answered a DIFFERENT question than
// the one typed whenever the ask carried a kind the vocabularies misspelled
// or didn't hold. Per the charter, such a whole ask goes to THE MODEL now:
// the configured LLM answers and picks the chart kind; the store fills the
// kind from the records (fillDiagram below). Nothing is drawn here — no
// fuzzy guessing on the user's words either.

export interface DrawResult {
  answer: string;
  chart: ChatChartSpec | null; // null → honest answer with no picture
}

/**
 * The one entry point the chat uses. Returns null when the question doesn't
 * look like a drawing request — the AI path handles those as before.
 */
export function tryDrawAnything(
  question: string,
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null,
  focus?: VizElement | null,
  deps?: CieDeps | null,
  runtime?: CieRuntime | null
): DrawResult | null {
  if (!report) return null;
  const q = question.toLowerCase();
  // Fast-path guard: explicit drawing words, the diagram vocabularies
  // themselves ("who calls whom", "what's the architecture"), or a counting /
  // comparing ask ("how many", "most", "breakdown", "list"). Anything else —
  // explanations, opinions, how-tos — still goes to the AI.
  if (
    !DRAW_HINT.test(q) &&
    !CHART_NOUNS.test(q) &&
    !DONUT_WORDS.test(q) &&
    !BAR_WORDS.test(q) &&
    !TABLE_WORDS.test(q) &&
    !STAT_WORDS.test(q) &&
    !DEPS_WORDS.test(q) &&
    !COVERAGE_WORDS.test(q) &&
    !CPU_WORDS.test(q) &&
    !MEMORY_WORDS.test(q) &&
    !RACE_WORDS.test(q) &&
    !DEADLOCK_WORDS.test(q) &&
    !CONCURRENCY_WORDS.test(q)
  ) {
    return null;
  }

  // Things the store genuinely cannot support — say so, and offer the menu.
  if (CANT_WORDS.test(q)) {
    const what = /\b(timeline|gantt|calendar|roadmap|burndown)\b/i.test(q)
      ? "dates or a schedule"
      : "images or history";
    return {
      answer:
        `I can't draw that one honestly — the store doesn't hold ${what}. ` +
        "Pictures I can draw from here: a UML-style map of the code, a who-calls-whom flow, bars, a donut, one big number, a small table, dependency charts, and measured runtime evidence (CPU, memory, coverage, races, deadlocks). Just ask for one.",
      chart: null,
    };
  }

  // ---- measured runtime evidence (only from collected data — never invented)
  if (COVERAGE_WORDS.test(q)) {
    return runtimeAnswer(runtime, "coverage", q);
  }
  if (DEADLOCK_WORDS.test(q)) {
    return runtimeAnswer(runtime, "deadlock", q);
  }
  if (RACE_WORDS.test(q)) {
    return runtimeAnswer(runtime, "race", q);
  }
  if (CPU_WORDS.test(q)) {
    return runtimeAnswer(runtime, "cpu", q);
  }
  if (MEMORY_WORDS.test(q)) {
    return runtimeAnswer(runtime, "memory", q);
  }
  if (CONCURRENCY_WORDS.test(q)) {
    return runtimeAnswer(runtime, "concurrency", q);
  }

  // ---- dependencies (manifest facts, as written)
  if (DEPS_WORDS.test(q)) {
    return depsAnswer(deps);
  }

  // ---- DFDs — the data-flow diagram from the recorded reads/writes ----
  // Explicit asks only ("dfd", "data flow diagram"): a bare "data flow"
  // stays with the who-calls-whom branch below, as it always did. With a
  // named focus the picture scopes to the focus's container; with none it
  // is the whole project's recorded data flow. When no reads/writes/schema
  // exist, the honest closest shape is the (possibly focused) call flow —
  // named AS ITSELF, never drawn as if it were the DFD.
  if (DFD_WORDS.test(q)) {
    const focusSym = askFocusSymbol(report, question, focus);
    const container = focusSym ? containerOfPath(fileOf(report, focusSym)) : null;
    const scoped =
      focusSym && container && container !== "(project root)"
        ? buildDataFlowSpec(report, container, `Data flow inside ${container}`)
        : null;
    const dfd = scoped ?? buildDataFlowSpec(report, null, "Data flow — readers, writers, stores");
    if (dfd) return { answer: dfdAnswer(dfd, focusSym?.qual_name), chart: dfd };
    const flow = buildFocusedFlowSpec(
      report,
      focusSym ? [Number(focusSym.id)] : [],
      focusSym?.qual_name ?? "the thing your words name",
      focusSym ? `Flow around “${focusSym.qual_name}”` : "Who calls whom",
    );
    if (flow)
      return {
        answer:
          `The summaries record no reads or writes — no data store exists for an honest data-flow picture, so the DFD has nothing to stand on. ` +
          `The closest honest drawing is the call flow${focusSym ? ` around “${focusSym.qual_name}”` : ""} — here it is, drawn as itself, not as a DFD.`,
        chart: flow,
      };
  }

  // Sequence by its explicit name — the kind is honored, not swapped for
  // the flow stand-in. (A sequence-diagram phrase that names a story is
  // already owned by the concept seam; this branch handles the bare and
  // concepts-less cases.)
  if (SEQUENCE_DIAGRAM_WORDS.test(q)) {
    const focusSym = askFocusSymbol(report, question, focus);
    const seq = buildProjectSequenceSpec(
      report,
      focusSym ? [Number(focusSym.id)] : [],
      focusSym?.qual_name ?? "the thing your words name",
      focusSym ? `Sequence around “${focusSym.qual_name}”` : "Cross-file message order",
    );
    if (seq) return { answer: projectSequenceAnswer(seq, focusSym?.qual_name), chart: seq };
    const built = buildUmlSpec(report, viewspec, concepts);
    return {
      answer:
        "No recorded calls — there's no message order to sequence honestly. " +
        "The closest honest picture is the structure map — here it is.",
      chart: built,
    };
  }

  // Structure diagrams (UML-style) — the most asked-for shape. When the
  // question names a symbol (or a map bubble is tapped), the map slices to
  // that file's container — focus first — instead of the whole-project top.
  if (UML_WORDS.test(q)) {
    const focusSym = askFocusSymbol(report, question, focus);
    const focusFile = focus?.evidence?.file ?? (focusSym ? fileOf(report, focusSym) : undefined);
    const container = focusFile ? containerOfPath(focusFile) : "(project root)";
    const built = focusFile
      ? buildUmlSpec(
          report,
          viewspec,
          concepts,
          container === "(project root)" ? `UML-style map — focused on “${focusSym?.qual_name}”` : `UML-style map inside ${container}`,
          focusFile,
        )
      : buildUmlSpec(report, viewspec, concepts);
    if (built) return { answer: umlAnswer(built), chart: built };
  }

  // Flow (who calls whom). When the question names a symbol, the flow
  // centers on it — one recorded hop in and out — instead of a wall of
  // every call the project holds.
  if (FLOW_WORDS.test(q)) {
    const focusSym = askFocusSymbol(report, question, focus);
    if (focusSym) {
      const focused = buildFocusedFlowSpec(
        report,
        [Number(focusSym.id)],
        focusSym.qual_name,
        `Flow around “${focusSym.qual_name}”`,
      );
      if (focused) return { answer: flowAnswer(focused), chart: focused };
    }
    const flow = buildFlowSpec(report);
    if (flow) return { answer: flowAnswer(flow), chart: flow };
    // No call records — be honest, then still show the closest honest shape.
    const built = buildUmlSpec(report, viewspec, concepts);
    return {
      answer:
        "The report doesn't record who-calls-whom for this project, so there's no call flow to draw. " +
        "The closest honest picture is the structure map — here it is.",
      chart: built,
    };
  }

  // The map, by name — same picture as the UML-style structure diagram.
  if (MAP_WORDS.test(q)) {
    const built = buildUmlSpec(report, viewspec, concepts, "The map of your code");
    if (built) return { answer: umlAnswer(built, true), chart: built };
  }

  // Splits → donut.
  if (DONUT_WORDS.test(q)) {
    const d = donutFor(q, report, viewspec);
    if (d) return { answer: donutAnswer(d, report), chart: d };
  }

  // Comparisons → bars.
  if (BAR_WORDS.test(q)) {
    const b = barFor(q, report);
    if (b) return { answer: barAnswer(b, report), chart: b };
  }

  // Lists → table.
  if (TABLE_WORDS.test(q)) {
    const t = tableFor(q, report);
    if (t) return { answer: t.answer, chart: t.chart };
  }

  // Counts → one big number.
  if (STAT_WORDS.test(q)) {
    const s = statFor(q, report);
    if (s) return { answer: s.answer, chart: s.chart };
  }

  // A GENERIC chart ask — "draw a chart", "show me a diagram" — no kind
  // Nothing matched — INCLUDING a drawing ask whose kind no vocabulary
  // resolved (the misspelled kinds): the configured LLM answers that one
  // (it picks the kind, the store fills it from the records — see the
  // AI leg in chat-panel + fillDiagram below). Nothing is drawn here.
  return null;
}

// ---------- UML-style structure diagram (from viewspec + report + concepts) ----------

/** "Concept:auth+crypto:authorization & uses crypto" → "authorization & uses crypto" */
function prettyConceptLabel(label: string): string {
  const i = label.lastIndexOf(":");
  return i >= 0 ? label.slice(i + 1) : label;
}

const basename = (p: string) => p.split("/").pop() ?? p;

export function buildUmlSpec(
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null,
  title = "UML-style map of your project",
  focusFile?: string
): ChatChartSpec | null {
  if (!report || !viewspec?.elements?.length) return null;

  const MAX_NODES = 26;
  // The focus's file boxes rank first, then its container's — the map
  // centers on the question's context instead of the store's plain order.
  let els = viewspec.elements;
  if (focusFile) {
    const inFocus = els.filter((e) => e.evidence?.file === focusFile);
    if (inFocus.length) {
      const container = containerOfPath(focusFile);
      const rest = els.filter((e) => e.evidence?.file !== focusFile);
      const same =
        container === "(project root)"
          ? rest
          : rest.filter((e) => containerOfPath(e.evidence?.file ?? "") === container);
      els = [...inFocus, ...same];
    }
  }
  els = els.slice(0, MAX_NODES);

  // symbol lookup: functions per file, keyed by file index
  const fnsByFile = new Map<number, { name: string; kind: string }[]>();
  for (const s of report.symbols) {
    if (s.kind === "Module") continue;
    const list = fnsByFile.get(s.loc.file) ?? [];
    list.push({ name: s.name, kind: s.kind });
    fnsByFile.set(s.loc.file, list);
  }
  // file index by basename (viewspec elements carry plain file names)
  const fileIdx = new Map<string, number>();
  report.files.forEach((f, i) => fileIdx.set(basename(f), i));
  // module concept nodes by file basename → behaviour (job) lines
  const jobsByFile = new Map<string, string[]>();
  if (concepts) {
    for (const n of concepts.nodes) {
      if (n.kind === "Module" && n.behaviors?.length) {
        jobsByFile.set(n.label, n.behaviors.map((b) => b.kind));
      }
    }
  }

  const nodes: UmlNode[] = els.map((e) => {
    const node: UmlNode = {
      id: e.id,
      title:
        e.elementKind === "usecase"
          ? prettyConceptLabel(e.label)
          : e.elementKind === "rule" || e.elementKind === "invariant"
            ? cut(e.label, 46)
            : e.label,
      kind: e.elementKind,
      certainty: (e.displayMode as UmlNode["certainty"]) ?? "Inference",
    };
    // the engine's own evidence span — kept, not dropped: tapping the box
    // opens the exact file (line resolved when the content is available)
    if (e.evidence?.file) {
      node.loc = { file: e.evidence.file, byteStart: e.evidence.start, byteEnd: e.evidence.end };
    }
    if (e.elementKind === "module") {
      const idx = fileIdx.get(basename(e.evidence?.file ?? e.label)) ?? fileIdx.get(basename(e.label));
      const fns = idx != null ? (fnsByFile.get(idx) ?? []) : [];
      const jobs = jobsByFile.get(basename(e.label)) ?? [];
      const lines = [
        ...jobs.slice(0, 2).map((j) => cut(j, 30)),
        ...fns.slice(0, 3).map((f) => f.name),
      ].slice(0, 4);
      node.lines = lines.length ? lines : undefined;
      const shown = jobs.slice(0, 2).length + fns.slice(0, Math.max(0, 4 - Math.min(2, jobs.length))).length;
      if (jobs.length + fns.length > shown) node.more = jobs.length + fns.length - shown;
    } else if (e.elementKind === "usecase") {
      // UseCase nodes carry no behaviours of their own in the store — leave
      // the compartment empty rather than borrowing another node's jobs
      // (that would put "Authorize" on the "prints" box, which misleads).
    }
    return node;
  });

  const ids = new Set(nodes.map((n) => n.id));
  const links: UmlLink[] = viewspec.edges
    .filter((e) => ids.has(e.source) && ids.has(e.target))
    .map((e) => ({
      from: e.source,
      to: e.target,
      label: cut(edgeKindPlain(e.edgeKind), 22),
      certainty: (e.displayMode as UmlLink["certainty"]) ?? "Fact",
      dashed: e.edgeKind === "cluster_cohesion" || e.edgeKind === "fca_subsumption" || e.displayMode !== "Fact",
    }));

  const s = spec("uml", title, "Boxes are parts of your code with their jobs listed. Arrows are how they connect.");
  s.nodes = nodes;
  s.links = links;
  s.source = "viewspec.json (the map) + report.json (files and named things)";
  return s;
}

function umlAnswer(built: ChatChartSpec, isMap = false): string {
  const nodes = built.nodes ?? [];
  const byKind = (k: string) => nodes.filter((n) => n.kind === k).length;
  const parts: string[] = [];
  const mods = byKind("module");
  const ucs = byKind("usecase");
  const rules = byKind("rule") + byKind("invariant");
  const domains = byKind("domain");
  if (domains) parts.push(`${domains} big picture`);
  if (mods) parts.push(`${mods} code file${mods === 1 ? "" : "s"}`);
  if (ucs) parts.push(`${ucs} thing${ucs === 1 ? "" : "s"} it does`);
  if (rules) parts.push(`${rules} rule${rules === 1 ? "" : "s"} to keep`);
  const links = built.links?.length ?? 0;

  const name = isMap ? "map" : "UML-style picture";
  return (
    `Here is the ${name} of your project — drawn straight from what CIE indexed, nothing added: ` +
    `${nodes.length} boxes (${parts.join(", ")}) and ${links} connections.\n\n` +
    (isMap
      ? ""
      : "One honest note: this is close to a UML class diagram, but not the full thing. Real UML also lists the data each class stores (its fields). The store doesn't hold that yet, so each box lists what that part does instead.\n\n") +
    "Green = proven (checked directly). Amber = CIE's good guess — worth checking. " +
    'Press "Show this chart big on the canvas" to zoom in and follow the arrows.'
  );
}

// ---------- flow (who calls whom) ----------

export function buildFlowSpec(report: CieReport | null): ChatChartSpec | null {
  if (!report) return null;
  // Call targets are symbol ids; the engine has emitted both numbers and
  // strings across versions, so coerce defensively and skip anything unknown.
  const byId = new Map<number, typeof report.symbols[number]>(report.symbols.map((s) => [Number(s.id), s]));
  const links: UmlLink[] = [];
  const involved = new Set<number>();
  for (const sum of report.summaries) {
    for (const rawCallee of sum.callees ?? []) {
      const callee = Number(rawCallee);
      if (!Number.isFinite(callee)) continue;
      const from = byId.get(Number(sum.sym));
      const to = byId.get(callee);
      if (!from || !to || from.kind === "Module" || to.kind === "Module") continue;
      links.push({ from: String(sum.sym), to: String(callee), label: "calls", certainty: "Fact" });
      involved.add(Number(sum.sym));
      involved.add(callee);
    }
  }
  if (!links.length) return null;

  const effectsBySym = new Map<number, string[]>();
  for (const sum of report.summaries) {
    if (sum.effects?.length) {
      effectsBySym.set(sum.sym, [...(effectsBySym.get(sum.sym) ?? []), ...sum.effects]);
    }
  }

  const nodes: UmlNode[] = [...involved].map((id) => {
    const s = byId.get(id)!;
    const effs = [...new Set(effectsBySym.get(id) ?? [])].slice(0, 2).map(effectPlain);
    const node: UmlNode = {
      id: String(id),
      title: s.qual_name,
      kind: "function",
      lines: effs.length ? effs : undefined,
      certainty: "Fact" as const,
    };
    // byte offsets from the report — the panel resolves them to a line only
    // when the file's content is really available (never a guessed line)
    const file = report.files[s.loc?.file];
    if (file) node.loc = { file, byteStart: s.loc.start, byteEnd: s.loc.end };
    return node;
  });

  const s = spec("flow", "Who calls whom", "An arrow means \u201ccalls\u201d. Only calls the report recorded are drawn.");
  s.nodes = nodes;
  s.links = links;
  s.source = "report.json — the call records (summaries)";
  return s;
}

function flowAnswer(built: ChatChartSpec): string {
  const n = built.nodes?.length ?? 0;
  const m = built.links?.length ?? 0;
  return (
    `Here is the call flow — who calls whom — from the report: ${n} named things and ${m} call arrows. ` +
    "An arrow means \u201ccalls\u201d.\n\n" +
    "Honest note: this shows only calls the report recorded. If code reaches something in a way CIE couldn't trace, that arrow won't be here. " +
    'Press "Show this chart big on the canvas" to zoom in.'
  );
}

// ---------- the asked kind, centered on the question's own focus ----------
//
// The seam rule is: a NAMED target nothing resolves gets a refusal, never
// a stand-in posed as the story. But a refusal that shows no picture at
// all hides the one thing the records CAN prove: the calls around what
// the question named. These builders give every refusal — and every
// generic ask — the KIND the question asked for (flowchart → flow,
// sequence → sequence, DFD → the recorded data flow, UML → the structure
// map), centered on the focus the question names. Every box and arrow
// stays a recorded fact; what the records don't hold is counted, never
// invented. The tell always says which kind is on the canvas — the
// closest-honest fallback (e.g. a flow offered when no sequence exists)
// is named AS ITSELF, never drawn as if it were the kind asked for.

const DFD_WORDS = /\bdfds?\b|\bdata ?flow ?diagrams?\b/i;

// ONLY the explicit phrase — a bare "sequence" is not a drawing ask.
// ONLY the explicit phrase — a bare "sequence" is not a drawing ask.
// A MISSPELLED kind ("sequnce diagram") deliberately matches NOTHING
// here: no fuzzy guessing on the user's words — that ask goes to the
// configured LLM (the generic fast path is gone), which decides the kind
// and the store fills it from the records.
const SEQUENCE_DIAGRAM_WORDS = /\bsequence (diagrams?|charts?|pictures?|views?)\b/i;
const FOCUS_SLICE_CAP = 30;
const SEQUENCE_LANE_CAP = 10;

/** What the question is actually about: the map bubble the user tapped
 *  (fields.symbol, set by the concept/map views) first, then the longest
 *  symbol name the question carries — the same focus rule the companion
 *  chart and the AI leg share. */
function askFocusSymbol(report: CieReport | null, question: string, tapFocus?: VizElement | null): CieReport["symbols"][number] | null {
  if (tapFocus?.fields?.symbol != null) {
    const id = Number(tapFocus.fields.symbol);
    const hit = report?.symbols.find((s) => Number(s.id) === id);
    if (hit) return hit;
  }
  return findSymbolInQuestion(report, question);
}

/** The call flow sliced to the focus's one-hop neighborhood (its recorded
 *  callees and callers). When the focus enters no recorded call at all,
 *  the whole flow is returned — it is still the honest picture — with the
 *  counted disclosure carried in the tell. Never a guessed node. */
export function buildFocusedFlowSpec(
  report: CieReport | null,
  focusIds: number[],
  focusLabel: string,
  title: string,
): ChatChartSpec | null {
  const full = buildFlowSpec(report);
  if (!full || !full.nodes?.length) return null;
  if (!focusIds.length) return full;
  const focus = new Set(focusIds.map(String));
  const present = new Set(full.nodes.map((n) => n.id));
  if ([...focus].every((id: string) => !present.has(id))) {
    return {
      ...full,
      title,
      tell:
        `${full.tell ?? ""} “${focusLabel}” enters no recorded call ` +
        "— the picture is the whole flow, and the named thing is genuinely absent from it (counted, not hidden).",
    };
  }
  const keep = new Set(focus);
  for (const l of full.links ?? []) {
    if (focus.has(String(l.from))) keep.add(String(l.to));
    if (focus.has(String(l.to))) keep.add(String(l.from));
  }
  let nodes = full.nodes.filter((n) => keep.has(n.id));
  let links = (full.links ?? []).filter((l) => keep.has(String(l.from)) && keep.has(String(l.to)));
  let hidden = 0;
  if (nodes.length > FOCUS_SLICE_CAP) {
    const degree = new Map<string, number>();
    for (const l of links) {
      degree.set(String(l.from), (degree.get(String(l.from)) ?? 0) + 1);
      degree.set(String(l.to), (degree.get(String(l.to)) ?? 0) + 1);
    }
    const ranked = [...nodes].sort(
      (a, b) => (degree.get(b.id) ?? 0) - (degree.get(a.id) ?? 0) || Number(b.id) - Number(a.id),
    );
    const kept = new Set(ranked.slice(0, FOCUS_SLICE_CAP).map((n) => n.id));
    hidden = nodes.length - kept.size;
    nodes = nodes.filter((n) => kept.has(n.id));
    links = links.filter((l) => kept.has(String(l.from)) && kept.has(String(l.to)));
  }
  return {
    ...full,
    nodes,
    links,
    title,
    tell:
      `Centered on “${focusLabel}” — what the question names: one hop of its recorded calls in and out. ` +
      "An arrow means “calls”; only recorded calls are drawn, and dynamic dispatch the engine couldn't trace is counted wherever it was disclosed." +
      (hidden ? ` ${hidden} neighbor(s) beyond the ${FOCUS_SLICE_CAP}-box cap stay off, counted not shown.` : ""),
  };
}

/** The recorded calls as a message sequence — whole-project or sliced to a
 *  focus's one-hop neighborhood. Lifelines are the files the symbols live
 *  in (lane cap, counted when cut); arrows are cross-lane calls ordered by
 *  BFS depth from the entries (no in-slice incoming edge). Intra-lane
 *  calls and beyond-cap edges are counted in the tell, never silently
 *  dropped. Null when no recorded cross-lane order can honestly exist. */
export function buildProjectSequenceSpec(
  report: CieReport | null,
  focusIds: number[] = [],
  focusLabel: string,
  title: string,
): ChatChartSpec | null {
  if (!report?.summaries?.length) return null;
  const g = buildCallGraph(report);
  const edges: { from: number; to: number }[] = [];
  const nodeIds = new Set<number>();
  for (const sum of report.summaries) {
    const from = Number(sum.sym);
    const fromSym = g.byId.get(from);
    if (!fromSym || fromSym.kind === "Module") continue;
    for (const raw of sum.callees ?? []) {
      const to = Number(raw);
      const toSym = Number.isFinite(to) ? g.byId.get(to) : undefined;
      if (!toSym || toSym.kind === "Module") continue;
      edges.push({ from, to });
      nodeIds.add(from);
      nodeIds.add(to);
    }
  }
  if (!edges.length) return null;

  // Focus slice — one hop in and out of what the question named; when the
  // focus enters no recorded call the whole recorded graph sequences.
  let sliceIds = nodeIds;
  let sliced = false;
  if (focusIds.length) {
    const focus = new Set(focusIds.map(Number).filter((id) => nodeIds.has(id)));
    if (focus.size) {
      const slicedSet = new Set(focus);
      for (const e of edges) {
        if (focus.has(e.from)) slicedSet.add(e.to);
        if (focus.has(e.to)) slicedSet.add(e.from);
      }
      sliceIds = slicedSet;
      sliced = true;
    }
  }
  const inSlice = edges.filter((e) => sliceIds.has(e.from) && sliceIds.has(e.to));

  const laneOf = (id: number): string => {
    const file = fileOf(report, g.byId.get(id));
    return file ? file.split("/").pop() || file : "(recorded calls)";
  };
  const calledInSlice = new Set(inSlice.map((e) => e.to));
  const entries = [...sliceIds].filter((id) => !calledInSlice.has(id));
  // Reading order: BFS depth from the entries, same rule as the story's
  // own sequence diagrams.
  const depth = new Map<number, number>();
  const queue = entries.map((id) => (depth.set(id, 0), id));
  while (queue.length) {
    const cur = queue.shift()!;
    for (const e of inSlice) {
      if (e.from !== cur || depth.has(e.to)) continue;
      depth.set(e.to, (depth.get(cur) ?? 0) + 1);
      queue.push(e.to);
    }
  }
  inSlice.sort(
    (a, b) =>
      (depth.get(a.from) ?? Number.MAX_SAFE_INTEGER) - (depth.get(b.from) ?? Number.MAX_SAFE_INTEGER) ||
      a.from - b.from ||
      a.to - b.to,
  );

  const cross = inSlice.filter((e) => laneOf(e.from) !== laneOf(e.to));
  if (!cross.length) return null; // no cross-lane message order — honest null
  const laneRank = new Map<string, number>();
  for (const e of cross) {
    laneRank.set(laneOf(e.from), (laneRank.get(laneOf(e.from)) ?? 0) + 1);
    laneRank.set(laneOf(e.to), (laneRank.get(laneOf(e.to)) ?? 0) + 1);
  }
  const laneOrder = [...laneRank.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const keptLanes = new Set(laneOrder.slice(0, SEQUENCE_LANE_CAP).map(([l]) => l));
  const dropped = inSlice.length - cross.filter((e) => keptLanes.has(laneOf(e.from)) && keptLanes.has(laneOf(e.to))).length;
  const hiddenLanes = Math.max(0, laneOrder.length - SEQUENCE_LANE_CAP);
  const shown = cross.filter((e) => keptLanes.has(laneOf(e.from)) && keptLanes.has(laneOf(e.to)));
  if (!shown.length) return null;

  const participants: string[] = [];
  const pushLane = (l: string) => {
    if (!keptLanes.has(l)) return;
    if (!participants.includes(l)) participants.push(l);
  };
  for (const id of entries) pushLane(laneOf(id));
  for (const e of shown) {
    pushLane(laneOf(e.from));
    pushLane(laneOf(e.to));
  }
  const msgs: SequenceMsg[] = shown.map((e) => {
    const fromSym = g.byId.get(e.from)!;
    const toSym = g.byId.get(e.to)!;
    const file = report.files[toSym.loc.file];
    const effects = [...new Set((g.sumBySym.get(e.to)?.effects ?? []).map(effectPlain))].slice(0, 2);
    return {
      from: laneOf(e.from),
      to: laneOf(e.to),
      label: cut(toSym.qual_name || toSym.name, 40),
      sub: effects.length ? effects.join(" · ") : undefined,
      status: "confirmed" as const,
      loc: file ? { file, byteStart: toSym.loc.start, byteEnd: toSym.loc.end } : undefined,
    };
  });
  const s: ChatChartSpec = {
    kind: "sequence",
    engine: engineFor("sequence"),
    layout: layoutFor("sequence"),
    title,
    tell:
      `Lifelines are the files recorded symbols live in; arrows are the cross-lane calls the report recorded, ordered from the entries outward. ` +
      (sliced ? `Sliced one hop from “${focusLabel}” — what the question names. ` : "") +
      (dropped ? `${dropped} call(s) inside one file or beyond the ${SEQUENCE_LANE_CAP}-lane cap stay off, counted not shown. ` : "") +
      (hiddenLanes ? `${hiddenLanes} lane(s) beyond the cap stay off, counted not shown. ` : ""),
    source: "report.json — the call records (summaries)",
  };
  s.participants = participants;
  s.msgs = msgs;
  return s;
}

/** The refusal companion: the asked KIND, centered on the matched focus.
 *  A sequence ask whose records hold no cross-lane order falls to the
 *  focused flow — named as itself, never posed as the sequence. */
export function buildConceptRefusalChart(
  kind: "flowchart" | "sequence",
  report: CieReport | null,
  focusIds: number[] = [],
): ChatChartSpec | null {
  if (!report) return null;
  const focusSym = focusIds.length
    ? report.symbols.find((s) => Number(s.id) === Number(focusIds[0])) ?? null
    : null;
  const label = focusSym?.qual_name || "the thing your words name";
  if (kind === "flowchart")
    return buildFocusedFlowSpec(report, focusIds, label, focusSym ? `Flow around “${label}”` : "Who calls whom");
  const seq = buildProjectSequenceSpec(report, focusIds, label, focusSym ? `Sequence around “${label}”` : "Cross-file message order");
  if (seq) return seq;
  const flow = buildFocusedFlowSpec(report, focusIds, label, focusSym ? `Flow around “${label}”` : "Who calls whom");
  if (flow)
    return {
      ...flow,
      tell: `${flow.tell ?? ""} The sequence had no recorded cross-lane order to draw — this flow is the closest honest picture, drawn as itself, not as a sequence.`,
    };
  return null;
}

function dfdAnswer(built: ChatChartSpec, focusName?: string): string {
  const nodes = built.nodes ?? [];
  const stores = nodes.filter((n) => n.kind === "store").length;
  const externals = nodes.filter((n) => n.kind === "external").length;
  const processes = nodes.filter((n) => n.kind !== "store" && n.kind !== "external").length;
  const links = built.links?.length ?? 0;
  const centered = focusName ? ` centered on “${focusName}” — what your question names` : "";
  return (
    `Here is the data flow${centered}, drawn from the records: ${processes} process box(es), ${stores} store(s), ` +
    `${externals} unresolved target(s) and ${links} recorded read/write arrow(s). ` +
    "Solid arrows are reads and writes the summaries recorded; dashed ones are names the engine couldn't resolve, drawn as an honest reaching box. " +
    "This is the recorded data flow — not a full DFD methodology diagram, and nothing is invented to fill the classic shapes."
  );
}

function projectSequenceAnswer(built: ChatChartSpec, focusName?: string): string {
  const lanes = built.participants?.length ?? 0;
  const msgs = built.msgs?.length ?? 0;
  const centered = focusName ? ` — centered on “${focusName}”, what your question names` : "";
  return (
    `Here is the message order${centered} from the report: ${lanes} lifeline${lanes === 1 ? "" : "s"} (the files recorded symbols live in) ` +
    `and ${msgs} cross-file call${msgs === 1 ? "" : "s"}, ordered from the entries outward.\n\n` +
    "Honest note: this is the recorded call order, not a runtime trace — if code dispatches in a way CIE couldn't trace, that arrow won't be here. " +
    'Press "Show this chart big on the canvas" to zoom in.'
  );
}

// ---------- dependencies (manifest facts, as written) ----------

export function buildDepsSpec(deps: CieDeps | null | undefined): ChatChartSpec | null {
  const table = deps?.table;
  if (!table || table.files_scanned === 0 || !table.deps.length) return null;
  const byEco = new Map<string, number>();
  for (const d of table.deps) byEco.set(d.ecosystem, (byEco.get(d.ecosystem) ?? 0) + 1);
  const names: Record<string, string> = {
    pypi: "Python (pip)", npm: "npm", cargo: "Rust", go: "Go",
    maven: "Maven", gradle: "Gradle", composer: "PHP", nuget: ".NET",
  };
  const data = [...byEco.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([eco, v]) => ({ name: cut(names[eco] ?? eco, 22), value: v, color: eco === "pypi" ? "emerald" : "stone" }));
  const s = spec("donut", "Declared packages per ecosystem", `Counted straight from ${table.files_scanned} manifest file(s), versions as written.`);
  s.data = data;
  s.center = "packages";
  s.source = "deps.json (the cie deps export)";
  return s;
}

function depsAnswer(deps: CieDeps | null | undefined): DrawResult {
  const table = deps?.table;
  if (!table || table.files_scanned === 0 || !table.deps.length) {
    return {
      answer:
        "I can't show dependencies for this project yet — no manifest files were found (or none were loaded). " +
        "CIE reads pom.xml, build.gradle(.kts), package.json, requirements.txt, pyproject.toml, Cargo.toml, go.mod, composer.json and *.csproj. " +
        "When your project has one, connect it and ask again — I'll draw exactly what it declares, nothing resolved, nothing guessed.",
      chart: null,
    };
  }
  const built = buildDepsSpec(deps)!;
  const unparsed = table.unparsed.length ? ` ${table.unparsed.length} manifest-looking file(s) could not be followed and are named in the Dependencies page.` : "";
  return {
    answer:
      `Here is what the project declares it needs: ${table.deps.length} package(s) across ${table.files_scanned} manifest file(s). ` +
      "Versions are exactly as written — CIE doesn't download packages, resolve transitive trees, or judge compatibility." + unparsed +
      ' Press "Show this chart big on the canvas" to read it comfortably.',
    chart: built,
  };
}

// ---------- runtime evidence (measured, never invented) ----------

function fmtBytes(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)} MB`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)} kB`;
  return `${n} B`;
}

function runtimeAnswer(
  runtime: CieRuntime | null | undefined,
  which: "coverage" | "cpu" | "memory" | "deadlock" | "race" | "concurrency",
  _q: string
): DrawResult {
  const noData = (what: string): DrawResult => ({
    answer:
      `I can't show ${what} for this project — there is no collected runtime evidence loaded. ` +
      "CIE never invents runtime data, so this is an honest no. The sample project collects its own " +
      "with scripts/collect_runtime.py (cProfile, tracemalloc, thread timestamps, faulthandler stacks, " +
      "race trials and coverage.py); run a collector on your project and these charts come alive.",
    chart: null,
  });
  const d = runtime?.datasets;
  if (!d) return noData("that");

  if (which === "coverage") {
    const cov = d.coverage;
    if (!cov?.files.length) return noData("test coverage");
    const s = spec("hbar", "Test coverage by file (measured)", `Percent of lines the project's own tests touched — ${cov.tool}.`);
    s.data = cov.files.map((f) => ({
      name: cut(f.file, 20),
      value: f.percent,
      color: f.percent >= 70 ? "emerald" : f.percent > 0 ? "amber" : "rose",
    }));
    s.source = `runtime.json — ${cov.tool}`;
    const zeros = cov.files.filter((f) => f.percent === 0).length;
    return {
      answer:
        `Here is the measured test coverage (${cov.tool}): ${cov.totals.percent}% of the lines the run could see. ` +
        (zeros ? `${zeros} file(s) sit at 0 because no test imports them — listed honestly, not faked. ` : "") +
        ' Press "Show this chart big on the canvas" to read it.',
      chart: s,
    };
  }

  if (which === "cpu") {
    const cpu = d.cpu;
    if (!cpu?.top_functions.length) return noData("a CPU profile");
    const s = spec("hbar", "CPU hotspots (measured)", `Cumulative milliseconds per function — ${cpu.tool}.`);
    s.data = cpu.top_functions.slice(0, 8).map((f) => ({
      name: cut(f.function, 22),
      value: Math.round(f.cum_ms),
      color: "amber",
      ...(f.file ? { loc: { file: f.file, line: f.line } as ElementLoc } : {}),
    }));
    s.source = `runtime.json — ${cpu.tool}`;
    const top = cpu.top_functions[0];
    return {
      answer:
        `Here is where the time went, measured for real (${cpu.tool}): ${top.function} tops the list with ${Math.round(top.cum_ms)} ms cumulative. ` +
        "Measured on the collecting machine — a rerun gives different numbers, and that is what honest measurement looks like.",
      chart: s,
    };
  }

  if (which === "memory") {
    const mem = d.memory;
    if (!mem?.top_sites.length) return noData("a memory profile");
    const s = spec("hbar", "Memory top sites (measured)", `Bytes per allocation site — ${mem.tool}.`);
    s.data = mem.top_sites.slice(0, 8).map((site) => ({ name: cut(site.where, 24), value: site.bytes, color: "stone" }));
    s.source = `runtime.json — ${mem.tool}`;
    return {
      answer:
        `Here is where the bytes went (${mem.tool}): live ${fmtBytes(mem.allocated_bytes)}, peak ${fmtBytes(mem.peak_bytes)}. ` +
        "tracemalloc only sees what it traced — the note on the Runtime page says exactly that.",
      chart: s,
    };
  }

  if (which === "deadlock") {
    const dead = d.deadlock;
    if (!dead) return noData("a deadlock capture");
    if (!dead.detected) {
      return {
        answer: "No deadlock was captured in this run — said honestly. It does not mean the code is deadlock-free; it means this particular capture saw none.",
        chart: null,
      };
    }
    const s = spec("table", "Captured deadlock — the waits-for cycle", `Each thread holds one lock and waits for the other — ${dead.tool}.`);
    s.columns = ["Thread", "Holds", "Waits for", "Blocked at"];
    s.rows = dead.threads.map((t) => [
      t.thread,
      t.holds ?? "?",
      t.wants ?? "?",
      t.blocked_at ? `${t.blocked_at.file}:${t.blocked_at.line}` : "stack missing",
    ]);
    s.source = `runtime.json — ${dead.tool}`;
    return {
      answer:
        `Yes — a real deadlock was captured: ${dead.cycle.join(" ⇄ ")}. Both stacks were dumped while the threads ` +
        "were blocked at the acquire lines, and the cycle was computed from the waits-for data they wrote — not declared by hand. One capture proves this run deadlocked; recurring deadlocks need production thread-dump monitoring.",
      chart: s,
    };
  }

  if (which === "race") {
    const race = d.race;
    if (!race) return noData("race trials");
    const s = spec("stat", "Lost updates, measured", `${race.lost_widened} lost in the widened window, ${race.lost_natural} naturally — ${race.tool}.`);
    s.data = [{ name: race.lost_natural === 0 ? "lost naturally (clean run — races are timing-dependent)" : "lost naturally", value: race.lost_natural }];
    s.unit = "updates";
    s.source = `runtime.json — ${race.tool}`;
    return {
      answer:
        `Measured, not imagined: the unguarded counter lost ${race.lost_widened} updates across the widened trials ` +
        `and ${race.lost_natural} naturally. The widened window (yielding the GIL between read and write) is fully disclosed — ` +
        "it proves the read-add-write sequence is not atomic. A clean natural run would NOT prove the race is gone.",
      chart: s,
    };
  }

  // concurrency
  const conc = d.concurrency;
  if (!conc?.spans.length) return noData("a thread timeline");
  const s = spec("stat", "Measured thread overlap", `${conc.tasks} tasks on ${conc.workers} threads — ${conc.tool}.`);
  s.data = [{ name: "tasks ran at once (overlap ×)", value: conc.observed_overlap }];
  s.unit = "×";
  s.source = `runtime.json — ${conc.tool}`;
  return {
    answer:
      `Measured from real timestamps: ${conc.tasks} tasks across ${conc.workers} threads finished in ${conc.makespan_ms} ms wall time, ` +
      `while the tasks themselves sum to ${conc.sum_of_task_ms} ms — so threads genuinely ran ${conc.observed_overlap}× at once during this capture. ` +
      "The full span-by-span timeline lives on the Runtime page.",
    chart: s,
  };
}

// ---------- LLM-path refill: the model picks the kind, the store fills it ----------

export function fillDiagram(
  kind: ChatChartKind,
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null,
  title?: string,
  tell?: string,
  quality?: CieQuality | null,
  focusId?: number | null,
  stack?: TechStack | null
): ChatChartSpec | null {
  const applyTitle = (built: ChatChartSpec | null): ChatChartSpec | null => {
    if (built) {
      if (title) built.title = title;
      if (tell) built.tell = tell;
    }
    return built;
  };
  if (kind === "uml") return applyTitle(buildUmlSpec(report, viewspec, concepts));
  if (kind === "flow") return applyTitle(buildFlowSpec(report));
  // The data-flow diagram: stores + readers + writers from the recorded
  // reads/writes (plus the schema files), the unresolved reaching box as
  // the honest external. Every box is a record.
  if (kind === "dfd") return report ? applyTitle(buildDataFlowSpec(report, null, title ?? "Data flow — readers, writers, stores")) : null;
  // The recorded message order: whole project, or sliced to the focus
  // concept's own symbols when the model's question named one.
  if (kind === "sequence") {
    const node = focusId != null ? concepts?.nodes.find((n) => n.id === focusId) : undefined;
    const focusIds = node?.symbols ?? [];
    const built =
      buildProjectSequenceSpec(report, [...focusIds], node?.label ?? "", title ?? "Cross-file message order") ??
      (focusIds.length
        ? buildProjectSequenceSpec(report, [], "", title ?? "Cross-file message order")
        : null);
    return applyTitle(built);
  }
  if (kind === "c4") return applyTitle(buildC4Spec(report, stack ?? null));
  // mindmap: the concept tree is the honest fill — same nodes, tree shape.
  if (kind === "mindmap") return applyTitle(buildConceptTreeSpec(concepts, focusId));
  // trees and heatmaps are filled from the collected exports too — the model
  // only picks the kind; the nodes/tiles come from concepts.json/quality.json
  if (kind === "tree") return applyTitle(buildConceptTreeSpec(concepts, focusId));
  if (kind === "heatmap") return applyTitle(buildCoverageHeatmapSpec(quality ?? null));
  return null;
}

// ---------- the classic four: bars, donut, stat, table (also store-built) ----------

const sevOrder = ["Critical", "High", "Medium", "Low"] as const;

function findingsPerFile(report: CieReport): { name: string; value: number; loc?: ElementLoc }[] {
  const byFile = new Map<number, number>();
  for (const f of report.findings) {
    const sym = report.symbols.find((s) => s.id === f.symbol);
    if (sym) byFile.set(sym.loc.file, (byFile.get(sym.loc.file) ?? 0) + 1);
  }
  return [...byFile.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([idx, v]) => ({
      name: cut(basename(report.files[idx] ?? `File ${idx}`), 19),
      value: v,
      ...(report.files[idx] ? { loc: { file: report.files[idx] } as ElementLoc } : {}),
    }));
}

function barFor(q: string, report: CieReport): ChatChartSpec | null {
  let data: { name: string; value: number; color?: string }[] = [];
  let title = "";
  let tell = "";
  if (/\b(touch|effect|outside|database|network|world)\b/i.test(q)) {
    const tally: Record<string, number> = {};
    for (const s of report.summaries) for (const e of s.effects ?? []) tally[effectPlain(e)] = (tally[effectPlain(e)] ?? 0) + 1;
    data = Object.entries(tally)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 8)
      .map(([name, value]) => ({ name: cut(name, 19), value, color: "stone" }));
    title = "What the code touches outside itself";
    tell = "Each bar counts how many named things do this.";
  } else if (/\b(severity|urgency|urgent|sever)\b/i.test(q)) {
    data = sevOrder.map((sev) => ({
      name: severityPlain(sev).label,
      value: report.findings.filter((f) => f.severity === sev).length,
      color: sev === "Critical" ? "rose" : sev === "High" ? "rose" : sev === "Medium" ? "amber" : "stone",
    }));
    title = "Problems by urgency";
    tell = "Sorted most urgent first.";
  } else if (/\b(promise|contract|rule)\b/i.test(q)) {
    const byFile = new Map<number, number>();
    for (const c of report.contracts) {
      for (const id of c.symbols) {
        const sym = report.symbols.find((s) => s.id === id);
        if (sym && sym.kind !== "Module") byFile.set(sym.loc.file, (byFile.get(sym.loc.file) ?? 0) + 1);
      }
    }
    if (!byFile.size) return null;
    data = [...byFile.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([idx, v]) => ({ name: cut(basename(report.files[idx] ?? `File ${idx}`), 19), value: v, color: "emerald", ...(report.files[idx] ? { loc: { file: report.files[idx] } as ElementLoc } : {}) }));
    title = "Promises per file";
    tell = "How many promise-carrying named things live in each file.";
  } else {
    if (!report.findings.length) return null;
    // "compare"/"most" with no project noun — let the AI handle it instead.
    if (!PROJECT_CONTEXT.test(q)) return null;
    const rows = findingsPerFile(report);
    const max = Math.max(...rows.map((r) => r.value));
    data = rows.map((r) => ({ ...r, color: r.value === max ? "rose" : r.value >= max / 2 ? "amber" : "stone" }));
    title = "Problems per file";
    tell = "Bars sorted worst first — start at the top.";
  }
  if (!data.length) return null;
  const s = spec("hbar", title, tell);
  s.data = data;
  s.lens = true;
  s.source = "report.json";
  return s;
}

function barAnswer(built: ChatChartSpec, report: CieReport): string {
  const rows = built.data ?? [];
  const top = rows[0];
  const total = report.findings.length;
  const scope =
    built.title === "Problems per file"
      ? ` That's all ${total} problem${total === 1 ? "" : "s"} in the report, counted per file.`
      : "";
  return (
    `Here are the bars, straight from the report${top ? ` — ${top.name} is at the top with ${top.value}` : ""}. ` +
    "Nothing added, nothing estimated." +
    scope +
    ' Hover a bar to magnify it, or press "Show this chart big on the canvas".'
  );
}

function donutFor(q: string, report: CieReport, viewspec: CieViewSpec | null): ChatChartSpec | null {
  let data: { name: string; value: number; color: string }[] = [];
  let title = "";
  let center = "in total";
  if (/\b(sure|certain|trust|guess|proven)\b/i.test(q) && viewspec) {
    const tally = { Fact: 0, Inference: 0, Hypothesis: 0 };
    for (const e of viewspec.elements) {
      if (e.displayMode === "Fact") tally.Fact++;
      else if (e.displayMode === "Inference") tally.Inference++;
      else tally.Hypothesis++;
    }
    data = [
      { name: certaintyPlain("Fact").label, value: tally.Fact, color: "emerald" },
      { name: certaintyPlain("Inference").label, value: tally.Inference, color: "amber" },
      { name: certaintyPlain("Hypothesis").label, value: tally.Hypothesis, color: "stone" },
    ];
    title = "How sure CIE is about the map";
    center = "map boxes";
  } else if (/\b(promise|contract|written|who wrote)\b/i.test(q)) {
    const explicit = report.contracts.filter((c) => c.authority === "Explicit").length;
    const guessed = report.contracts.length - explicit;
    data = [
      { name: authorityPlain("Explicit").label, value: explicit, color: "emerald" },
      { name: authorityPlain("InferredCandidate").label, value: guessed, color: "amber" },
    ];
    title = "Who wrote the promises";
    center = "promises";
  } else {
    if (!report.findings.length) return null;
    if (!PROJECT_CONTEXT.test(q)) return null;
    data = sevOrder.map((sev) => ({
      name: severityPlain(sev).label,
      value: report.findings.filter((f) => f.severity === sev).length,
      color: sev === "Critical" || sev === "High" ? "rose" : sev === "Medium" ? "amber" : "stone",
    }));
    title = "All problems, by urgency";
    center = "problems";
  }
  if (!data.some((d) => d.value > 0)) return null;
  const s = spec("donut", title, "The whole circle is everything found; each slice is one part.");
  s.data = data;
  s.center = center;
  s.source = "report.json" + (viewspec ? " + viewspec.json" : "");
  return s;
}

function donutAnswer(built: ChatChartSpec, report: CieReport): string {
  const rows = (built.data ?? []).filter((d) => d.value > 0);
  const list = rows.map((d) => `${d.name}: ${d.value}`).join(", ");
  return (
    `Here's the split, straight from the report — ${list}. ` +
    "Amber parts are CIE's good guesses: worth checking, not certain." +
    (report.findings.length ? "" : "")
  );
}

function statFor(q: string, report: CieReport): DrawResult | null {
  const stat = (value: number, name: string, tell: string, unit?: string): DrawResult => {
    const s = spec("stat", "The number, straight from the report", tell);
    s.data = [{ name, value }];
    if (unit) s.unit = unit;
    s.source = "report.json";
    return { answer: `Straight from the report: ${value}${unit ?? ""} ${name.toLowerCase()}. Nothing added.`, chart: s };
  };

  if (/\b(secret|password|key|credential)\b/i.test(q)) {
    const n = report.findings.filter((f) => f.kind === "HardcodedSecret").length;
    if (n === 0) {
      const zero = stat(0, "secrets found in the code", "An honest zero — CIE looked and found none.");
      return {
        answer: "Good news: the report found no secrets or passwords written in the code. That's an honest zero — checked, not guessed.",
        chart: zero.chart,
      };
    }
    return stat(n, "secrets written in the code", "These should be moved somewhere safe.");
  }
  if (/\b(problem|finding|issue|bug|wrong)\b/i.test(q)) {
    const fixSoon = report.findings.filter((f) => f.severity === "Critical").length;
    return stat(report.findings.length, "problems in the report", fixSoon ? `${fixSoon} of them should be fixed soon.` : "None are emergencies.");
  }
  if (/\b(promise|contract|rule)\b/i.test(q)) {
    const guessed = report.contracts.filter((c) => c.authority !== "Explicit").length;
    return stat(report.contracts.length, "promises the code must keep", guessed ? `${guessed} of them are CIE's guesses — worth checking.` : "All written by the developers.");
  }
  if (/\b(file)\b/i.test(q)) {
    return stat(report.coverage.files_analyzed, `of ${report.coverage.files_total} files were checked`, report.coverage.complete ? "Everything was checked." : "Some parts were skipped — see Can I trust this?");
  }
  if (/\b(named thing|function|symbol|method|class)\b/i.test(q)) {
    return stat(report.coverage.symbols_analyzed, `of ${report.coverage.symbols_total} named things were checked`, "Named things are functions, classes and similar.");
  }
  return null;
}

function tableFor(q: string, report: CieReport): DrawResult | null {
  if (/\b(promise|contract|rule)\b/i.test(q)) {
    if (!report.contracts.length) return null;
    const s = spec("table", "Every promise in the report", "Green words = written by you; amber = CIE's guess.");
    s.columns = ["Promise", "Who wrote it", "Kept by"];
    s.rows = report.contracts.slice(0, 8).map((c) => {
      const who = authorityPlain(c.authority).label;
      const keepers = c.symbols
        .map((id) => report.symbols.find((x) => x.id === id)?.name)
        .filter(Boolean)
        .slice(0, 2)
        .join(", ");
      return [cut(obligationTitle(c.obligation), 34), who, keepers + (c.symbols.length > 2 ? ` +${c.symbols.length - 2}` : "")];
    });
    s.source = "report.json";
    return { answer: `Here are all ${report.contracts.length} promises the report holds. "CIE's guess" means worth checking — not certain.`, chart: s };
  }
  if (!report.findings.length) return null;
  if (!PROJECT_CONTEXT.test(q)) return null;
  const s = spec("table", "Things to look at", "Sorted most urgent first.");
  const sorted = [...report.findings].sort((a, b) => sevOrder.indexOf(a.severity as never) - sevOrder.indexOf(b.severity as never));
  s.columns = ["Where", "Problem", "Urgency"];
  s.rows = sorted.slice(0, 8).map((f) => {
    const sym = report.symbols.find((x) => x.id === f.symbol);
    const file = sym ? basename(report.files[sym.loc.file] ?? "") : "";
    return [cut(`${file}${f.evidence_lines?.[0] ? ` · line ${f.evidence_lines[0]}` : ""}`, 24), cut(findingTitle(f), 30), severityPlain(f.severity).label];
  });
  s.source = "report.json";
  const fixSoon = report.findings.filter((f) => f.severity === "Critical").length;
  return {
    answer:
      `Here are the things to look at — all ${report.findings.length} of them, most urgent first` +
      (fixSoon ? ` (${fixSoon} should be fixed soon)` : "") +
      ". Drawn from the report, nothing added.",
    chart: s,
  };
}
