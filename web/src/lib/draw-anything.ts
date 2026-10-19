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
  type UmlNode,
  type UmlLink,
} from "./chat-chart";
import { buildConceptTreeSpec, buildCoverageHeatmapSpec } from "./quality";
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

  // Structure diagrams (UML-style) — the most asked-for shape.
  if (UML_WORDS.test(q)) {
    const built = buildUmlSpec(report, viewspec, concepts);
    if (built) return { answer: umlAnswer(built), chart: built };
  }

  // Flow (who calls whom).
  if (FLOW_WORDS.test(q)) {
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

  // Nothing matched — let the AI answer from the digest as before.
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
  title = "UML-style map of your project"
): ChatChartSpec | null {
  if (!report || !viewspec?.elements?.length) return null;

  const MAX_NODES = 26;
  const els = viewspec.elements.slice(0, MAX_NODES);

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
  focusId?: number | null
): ChatChartSpec | null {
  if (kind === "uml") {
    const built = buildUmlSpec(report, viewspec, concepts);
    if (built) {
      if (title) built.title = title;
      if (tell) built.tell = tell;
    }
    return built;
  }
  if (kind === "flow") {
    const built = buildFlowSpec(report);
    if (built) {
      if (title) built.title = title;
      if (tell) built.tell = tell;
    }
    return built;
  }
  // trees and heatmaps are filled from the collected exports too — the model
  // only picks the kind; the nodes/tiles come from concepts.json/quality.json
  if (kind === "tree") {
    const built = buildConceptTreeSpec(concepts, focusId);
    if (built) {
      if (title) built.title = title;
      if (tell) built.tell = tell;
    }
    return built;
  }
  if (kind === "heatmap") {
    const built = buildCoverageHeatmapSpec(quality ?? null);
    if (built) {
      if (title) built.title = title;
      if (tell) built.tell = tell;
    }
    return built;
  }
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
