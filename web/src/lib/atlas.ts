// lib/atlas.ts — the multi-view dashboard (the "atlas" stage view).
//
// Selection Context + the four synchronized views, built the same way every
// other page is built: pure functions of the loaded engine exports
// (report.json, concepts.json), rendered through the existing ChatChart
// engines (DiagramSVG / SequenceSVG). Nothing here invents geometry, and
// nothing here re-implements the call graph — mastery.ts's one builder is
// reused so both pages can never disagree about who-calls-whom.
//
// PRD mapping (Interactive Multi-View Dashboard):
//   Containers view  = C4 L2 (reuse mastery.ts buildC4Spec — same grouping)
//   Components view  = C4 L3 (modules of the scoped container, then symbols)
//   Sequence view    = traceFlow + buildSequenceSpec (already store-built)
//   Data flow view   = stores + reads/writes edges from summaries (new builder)
// Every box/arrow keeps its evidence (certainty + loc) and stays clickable
// into the source panel through the same useAsk path the chat charts use.

import type { CieReport, ConceptTree, SymbolInfo } from "./cie-types";
import type { ChatChartSpec, ElementLoc, UmlLink, UmlNode } from "./chat-chart";
import { engineFor, layoutFor } from "./chat-chart";
import { buildCallGraph, fileOf, type CallGraph } from "./mastery";

// ---------------------------------------------------------------------------
// Selection Context — the single object every atlas view reads and writes
// ---------------------------------------------------------------------------

export type AtlasScope = "system" | "container" | "module" | "symbol";

/** What the user last clicked. `id` is the same string the views key on:
 *  container = top-level folder name (or "(project root)"), module = file
 *  path, symbol = qual_name, store = file path of a data store. */
export interface AtlasRef {
  kind: "container" | "module" | "symbol" | "store" | "external" | "edge";
  id: string;
  label: string;
  loc?: ElementLoc;
}

export interface AtlasSelection {
  primary: AtlasRef | null;
  scope: AtlasScope;
}

export const ATLAS_RESET: AtlasSelection = { primary: null, scope: "system" };

/** Same grouping buildC4Spec uses — the two views must never disagree. */
export function containerOfPath(f: string): string {
  const i = f.indexOf("/");
  return i < 0 ? "(project root)" : f.slice(0, i);
}

export function scopeForRef(ref: AtlasRef): AtlasScope {
  switch (ref.kind) {
    case "container": return "container";
    case "module": return "module";
    case "store": return "module"; // a store scopes the module list around its readers/writers
    case "symbol": return "symbol";
    default: return "system";
  }
}

/** Breadcrumb trail: System › container › module › symbol(). Clicking any
 *  crumb resets the scope to that level (PRD §7). */
export function breadcrumbOf(sel: AtlasSelection): { scope: AtlasScope; ref: AtlasRef | null }[] {
  const trail: { scope: AtlasScope; ref: AtlasRef | null }[] = [{ scope: "system", ref: null }];
  const p = sel.primary;
  if (!p) return trail;
  const file = p.kind === "module" || p.kind === "store" ? p.id : p.loc?.file;
  if (p.kind === "container") {
    trail.push({ scope: "container", ref: p });
  } else if (file) {
    trail.push({ scope: "container", ref: { kind: "container", id: containerOfPath(file), label: containerOfPath(file) } });
  }
  if ((p.kind === "module" || p.kind === "store") && sel.scope !== "container") {
    trail.push({ scope: "module", ref: p });
  }
  if (p.kind === "symbol") {
    trail.push({ scope: "symbol", ref: p });
  }
  return trail;
}

// ---------------------------------------------------------------------------
// The call graph — one builder, shared with the mastery journey
// ---------------------------------------------------------------------------

let cachedGraph: { report: CieReport; g: CallGraph } | null = null;

export function graphOf(report: CieReport): CallGraph {
  if (!cachedGraph || cachedGraph.report !== report) cachedGraph = { report, g: buildCallGraph(report) };
  return cachedGraph.g;
}

// ---------------------------------------------------------------------------
// Components view (C4 L3) — two zoom levels off the same builder
// ---------------------------------------------------------------------------

/** Symbols that live in one file, in engine order. */
export function symbolsOfModule(report: CieReport, file: string): SymbolInfo[] {
  const idx = report.files.indexOf(file);
  if (idx < 0) return [];
  return report.symbols.filter((s) => s.loc.file === idx);
}

const MAX_MODULE_BOXES = 12;
const MAX_SYMBOL_LINES = 4;

/** One zoom level: boxes for the modules (files) of `container` (null =
 *  whole system), arrows for calls that cross module boundaries. Click a
 *  box → the view re-scopes to that module's symbols (same builder, level 2). */
export function buildComponentSpec(
  report: CieReport,
  container: string | null,
  title?: string
): ChatChartSpec | null {
  if (!report?.files?.length) return null;
  const g = graphOf(report);

  // level 2 — one module zoomed: its symbols + calls between them
  if (container) {
    const syms = symbolsOfModule(report, container);
    if (!syms.length) return null;
    const MAX_SYMBOL_BOXES = 10;
    const shown = syms.slice(0, MAX_SYMBOL_BOXES);
    const more = syms.length - shown.length;
    const nodes: UmlNode[] = shown.map((s) => {
      const sum = g.sumBySym.get(Number(s.id));
      const lines: string[] = [];
      const fx = [...new Set([...(sum?.reads ?? []), ...(sum?.writes ?? [])])].slice(0, 2);
      if (fx.length) lines.push(`${sum?.writes?.length ? "writes" : "reads"} ${fx.join(", ")}`);
      const eff = [...new Set(sum?.effects ?? [])].slice(0, 2);
      if (eff.length) lines.push(`touches: ${eff.join(", ")}`);
      if (!lines.length) lines.push(sum ? (sum.complete ? "fully read by the engine" : "partly read — summary incomplete") : "no records kept");
      return {
        id: s.qual_name || s.name,
        title: `${s.name}()`,
        kind: s.kind === "Class" ? "class" : "function",
        lines,
        certainty: "Fact" as const,
        loc: { file: container, byteStart: s.loc.start, byteEnd: s.loc.end },
      };
    });
    if (more > 0) {
      nodes.push({
        id: "…more-symbols",
        title: `…and ${more} more named thing(s)`,
        kind: "module",
        lines: ["smaller — grouped out of this picture, not hidden"],
        certainty: "Inference",
      });
    }
    const ids = new Set(nodes.map((n) => n.id));
    const links: UmlLink[] = [];
    for (const s of shown) {
      for (const c of g.calleesOf.get(Number(s.id)) ?? []) {
        const callee = g.byId.get(c);
        if (!callee || callee.loc.file !== s.loc.file) continue; // cross-module calls live on the level-1 picture
        const to = callee.qual_name || callee.name;
        if (!ids.has(to) || to === (s.qual_name || s.name)) continue;
        links.push({ from: s.qual_name || s.name, to, label: "calls", certainty: "Fact" });
      }
    }
    const s2 = specOf("uml", title ?? `Inside ${container.split("/").pop() ?? container} — its named things and their calls`);
    s2.nodes = nodes;
    s2.links = links;
    s2.source = "report.json — this file's own symbols and recorded calls";
    return s2;
  }

  // level 1 — modules across the (scoped) system
  const files = report.files;
  const symCount = new Map<string, number>();
  const firstSym = new Map<string, SymbolInfo>();
  const byModule = new Map<string, SymbolInfo[]>();
  for (const s of report.symbols) {
    const f = fileOf(report, s);
    if (!f) continue;
    byModule.set(f, [...(byModule.get(f) ?? []), s]);
    symCount.set(f, (symCount.get(f) ?? 0) + 1);
    if (!firstSym.has(f)) firstSym.set(f, s);
  }
  const modules = files.filter((f) => symCount.has(f));
  const ordered = modules
    .slice()
    .sort((a, b) => (symCount.get(b) ?? 0) - (symCount.get(a) ?? 0) || a.localeCompare(b));
  const shown = ordered.slice(0, MAX_MODULE_BOXES);
  const rest = ordered.length - shown.length;

  const nodes: UmlNode[] = shown.map((f) => {
    const syms = byModule.get(f) ?? [];
    const lines = syms.slice(0, MAX_SYMBOL_LINES).map((s) => `${s.name}()`);
    const moreSyms = syms.length - lines.length;
    return {
      id: f,
      title: f.split("/").pop() ?? f,
      kind: "module",
      lines,
      more: moreSyms > 0 ? moreSyms : undefined,
      certainty: "Fact" as const,
      loc: { file: f, byteStart: firstSym.get(f)?.loc.start },
    };
  });
  if (rest > 0) {
    nodes.push({
      id: "…other-modules",
      title: `…and ${rest} more module(s)`,
      kind: "module",
      lines: ["smaller — grouped out of this picture, not hidden"],
      certainty: "Inference",
    });
  }
  const ids = new Set(nodes.map((n) => n.id));
  const fileOfSym = (id: number): string => fileOf(report, g.byId.get(id));
  const links: UmlLink[] = [];
  const seenEdge = new Set<string>();
  for (const s of report.symbols) {
    const from = fileOfSym(Number(s.id));
    if (!ids.has(from)) continue;
    for (const c of g.calleesOf.get(Number(s.id)) ?? []) {
      const to = fileOfSym(c);
      if (!to || to === from || !ids.has(to)) continue;
      const key = `${from}->${to}`;
      if (seenEdge.has(key)) continue;
      seenEdge.add(key);
      links.push({ from, to, label: "calls into", certainty: "Fact" });
    }
  }
  const s1 = specOf("uml", title ?? "Components — each file as a box, calls as arrows");
  s1.nodes = nodes;
  s1.links = links;
  s1.source = "report.json — the file table, named things, and recorded calls";
  return s1;
}

// ---------------------------------------------------------------------------
// Data flow view — stores + reads/writes from the summaries (engine facts)
// ---------------------------------------------------------------------------

const MAX_STORES = 6;
const MAX_FLOW_MODULES = 8;

/** Data stores = the files whose recorded reads/writes/effect names mention
 *  them (e.g. `db` written by payments, `fs` read by the probe) plus real
 *  .sql files (schema stores). Every store and every arrow traces back to a
 *  summary record — absence stays absent. */
export function buildDataFlowSpec(report: CieReport, container: string | null, title?: string): ChatChartSpec | null {
  if (!report?.summaries?.length) return null;
  const g = graphOf(report);

  const inScope = (file: string): boolean => (container ? containerOfPath(file) === container : true);

  // stores: .sql files in scope + summary-named stores that real code touches
  const storeHits = new Map<string, { readers: Set<string>; writers: Set<string> }>();
  for (const f of report.files) {
    if (f.endsWith(".sql")) storeHits.set(f, { readers: new Set(), writers: new Set() });
  }
  const namedStores = new Map<string, { readers: Set<string>; writers: Set<string> }>();
  for (const sum of report.summaries) {
    const sym = g.byId.get(Number(sum.sym));
    const file = fileOf(report, sym);
    if (!file || !inScope(file)) continue;
    for (const r of sum.reads ?? []) {
      const s = namedStores.get(r) ?? { readers: new Set<string>(), writers: new Set<string>() };
      s.readers.add(file);
      namedStores.set(r, s);
    }
    for (const w of sum.writes ?? []) {
      const s = namedStores.get(w) ?? { readers: new Set<string>(), writers: new Set<string>() };
      s.writers.add(file);
      namedStores.set(w, s);
    }
  }

  const nodes: UmlNode[] = [];
  const links: UmlLink[] = [];
  const modulesShown = new Set<string>();

  // real schema files first — they are stores the repo itself declares
  const sqlStores = [...storeHits.keys()].filter(inScope).slice(0, MAX_STORES);
  for (const f of sqlStores) {
    nodes.push({
      id: f,
      title: f.split("/").pop() ?? f,
      kind: "store",
      lines: ["schema file — a data store the repo itself declares"],
      certainty: "Fact",
      loc: { file: f },
    });
  }
  // summary-named stores (like `db`, `fs`) — the names appear in the code's
  // recorded reads/writes; drawn only for names real in-scope code touches
  const namedShown = [...namedStores.entries()]
    .sort((a, b) => b[1].readers.size + b[1].writers.size - (a[1].readers.size + a[1].writers.size))
    .slice(0, MAX_STORES);
  for (const [name, hit] of namedShown) {
    if (!hit.readers.size && !hit.writers.size) continue;
    nodes.push({
      id: `store:${name}`,
      title: name,
      kind: "store",
      lines: [`${hit.writers.size} writer(s) · ${hit.readers.size} reader(s)`, "as named in the code's own records"],
      certainty: "Fact",
    });
    for (const w of hit.writers) {
      if (!modulesShown.has(w) && inScope(w)) modulesShown.add(w);
      links.push({ from: w, to: `store:${name}`, label: "writes", certainty: "Fact" });
    }
    for (const r of hit.readers) {
      if (!modulesShown.has(r) && inScope(r)) modulesShown.add(r);
      links.push({ from: `store:${name}`, to: r, label: "feeds", certainty: "Fact" });
    }
  }
  for (const f of sqlStores) {
    for (const sum of report.summaries) {
      const sym = g.byId.get(Number(sum.sym));
      const file = fileOf(report, sym);
      if (!file || file === f) continue;
      const touches = [...(sum.reads ?? []), ...(sum.writes ?? [])].some((n) => f.includes(n) || (n === "db" && f.endsWith(".sql")));
      if (!touches) continue;
      const writes = (sum.writes ?? []).some((n) => n === "db" || f.includes(n));
      links.push({ from: writes ? file : f, to: writes ? f : file, label: writes ? "writes" : "reads", certainty: "Fact" });
      modulesShown.add(file);
    }
  }

  // module boxes for every file an edge mentions
  for (const f of [...modulesShown].slice(0, MAX_FLOW_MODULES)) {
    const syms = symbolsOfModule(report, f);
    nodes.push({
      id: f,
      title: f.split("/").pop() ?? f,
      kind: "module",
      lines: [`${syms.length} named thing(s)`, containerOfPath(f)],
      certainty: "Fact",
      loc: { file: f, byteStart: syms[0]?.loc.start },
    });
  }
  // unresolved names the code reaches for — honest fog, never a fake node
  const unresolved = new Map<string, number>();
  for (const sum of report.summaries) {
    const sym = g.byId.get(Number(sum.sym));
    const file = fileOf(report, sym);
    if (!file || !inScope(file)) continue;
    for (const u of sum.unresolved_callees ?? []) unresolved.set(u, (unresolved.get(u) ?? 0) + 1);
  }
  const topUnresolved = [...unresolved.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3);
  if (topUnresolved.length) {
    nodes.push({
      id: "(unresolved)",
      title: "names the engine couldn't resolve",
      kind: "external",
      lines: topUnresolved.map(([n, c]) => `${n} ×${c}`),
      certainty: "Inference",
    });
    for (const sum of report.summaries) {
      const sym = g.byId.get(Number(sum.sym));
      const file = fileOf(report, sym);
      if (!file || !inScope(file) || !modulesShown.has(file)) continue;
      for (const u of sum.unresolved_callees ?? []) {
        if (!topUnresolved.some(([n]) => n === u)) continue;
        links.push({ from: file, to: "(unresolved)", label: "reaches for", certainty: "Inference", dashed: true });
        break;
      }
    }
  }

  if (!nodes.length) return null;
  const s = specOf("uml", title ?? (container ? `Data flow inside ${container}` : "Data flow — who reads, who writes"));
  s.nodes = nodes;
  s.links = links;
  s.tell = "Boxes are the code and the data stores it names; arrows are recorded reads and writes. Dashed = names the engine couldn't resolve to real code.";
  s.source = "report.json — each symbol's recorded reads, writes, and unresolved names";
  return s;
}

function specOf(kind: ChatChartSpec["kind"], title: string, tell?: string): ChatChartSpec {
  const s: ChatChartSpec = { kind, engine: engineFor(kind), layout: layoutFor(kind), title };
  if (tell) s.tell = tell;
  return s;
}

// ---------------------------------------------------------------------------
// Detail rail — callers, callees, tests that reach here (PRD §9)
// ---------------------------------------------------------------------------

export interface RelativeSymbol {
  id: number;
  qualName: string;
  file: string;
  byteStart?: number;
  byteEnd?: number;
}

export function callersOf(report: CieReport, symbolId: number): RelativeSymbol[] {
  const g = graphOf(report);
  const out: RelativeSymbol[] = [];
  for (const [from, callees] of g.calleesOf) {
    if (!callees.includes(symbolId)) continue;
    const s = g.byId.get(from);
    if (!s) continue;
    out.push({ id: from, qualName: s.qual_name || s.name, file: fileOf(report, s), byteStart: s.loc.start, byteEnd: s.loc.end });
  }
  return out;
}

export function calleesOfSym(report: CieReport, symbolId: number): RelativeSymbol[] {
  const g = graphOf(report);
  const out: (RelativeSymbol | null)[] = (g.calleesOf.get(symbolId) ?? []).map((id) => {
    const s = g.byId.get(id);
    if (!s) return null;
    return { id, qualName: s.qual_name || s.name, file: fileOf(report, s), byteStart: s.loc.start, byteEnd: s.loc.end };
  });
  return out.filter((x): x is RelativeSymbol => x != null);
}

/** Test files that reach this symbol through recorded calls (PRD §9 "Tests
 *  that reach here"). File names starting with test_ or ending _test are
 *  the convention CIE treats as tests — said plainly, not guessed deeper. */
export function testsReaching(report: CieReport, symbolId: number): RelativeSymbol[] {
  const isTest = (f: string): boolean => f.startsWith("test_") || f.endsWith("_test") || f.includes("/test_") || f.includes(".test.");
  return callersOf(report, symbolId).filter((c) => isTest(c.file));
}

export function symbolByQualName(report: CieReport, qualName: string): SymbolInfo | null {
  return report.symbols.find((s) => (s.qual_name || s.name) === qualName) ?? null;
}

/** Concepts tree helper — the module node for a file, when concepts.json
 *  has one (it usually does; absence is fine, the atlas works without it). */
export function conceptNodeForFile(concepts: ConceptTree | null, file: string) {
  return concepts?.nodes.find((n) => n.label === file) ?? null;
}
