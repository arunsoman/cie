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
import { buildCallGraph, fileOf, type CallGraph, FULL_VIEW_CAP, type Detail } from "./mastery";

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

/** Same grouping buildC4Spec uses — the two views must never disagree.
 *  RFC-0019 §6.5: when the stack artifact names the real apps, a file's
 *  container is the app dir that claims it (longest prefix first; a root
 *  app keeps the leftovers) — not the first path segment. Without apps the
 *  first-segment heuristic stands, exactly as before. */
export function containerOfPath(f: string, apps?: { dir: string }[] | null): string {
  if (apps && apps.length) {
    const dirs = apps.map((a) => a.dir).sort((a, b) => b.length - a.length);
    for (const d of dirs) {
      if (d && (f === d || f.startsWith(d + "/"))) return d;
    }
    if (apps.some((a) => !a.dir)) return "(project root)";
  }
  const i = f.indexOf("/");
  return i < 0 ? "(project root)" : f.slice(0, i);
}

/** RFC-0019 §6.5: the container's COMPONENT tier, read straight from the
 *  concept DAG — Service / Subdomain nodes under the Container node whose
 *  label is the container. Each carries its own certainty (recorded at 1.0,
 *  inference below) and its file modules. Null when the DAG has no C4
 *  tiers (old artifacts, or a container the stack never claimed) — callers
 *  fall back to the plain module list. */
export function dagComponentsOf(
  concepts: ConceptTree | null | undefined,
  container: string
): { id: number; label: string; kind: string; confidence: number; provenance: string; modules: string[] }[] | null {
  const nodes = concepts?.nodes;
  if (!nodes?.length) return null;
  const cont = nodes.find((n) => n.kind === "Container" && (n.label === container || containerOfPath(n.label) === container));
  if (!cont) return null;
  const comps = nodes.filter((n) => n.parent === cont.id && (n.kind === "Service" || n.kind === "Subdomain"));
  if (!comps.length) return null;
  return comps.map((c) => ({
    id: c.id,
    label: c.label,
    kind: c.kind,
    confidence: c.confidence ?? 1,
    provenance: String(c.provenance ?? ""),
    modules: nodes.filter((m) => m.parent === c.id && m.kind === "Module").map((m) => m.label),
  }));
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
 *  crumb resets the scope to that level (PRD §7). `apps` makes the
 *  container level agree with the stack's real apps when present. */
export function breadcrumbOf(sel: AtlasSelection, apps?: { dir: string }[] | null): { scope: AtlasScope; ref: AtlasRef | null }[] {
  const trail: { scope: AtlasScope; ref: AtlasRef | null }[] = [{ scope: "system", ref: null }];
  const p = sel.primary;
  if (!p) return trail;
  const file = p.kind === "module" || p.kind === "store" ? p.id : p.loc?.file;
  if (p.kind === "container") {
    trail.push({ scope: "container", ref: p });
  } else if (file) {
    trail.push({ scope: "container", ref: { kind: "container", id: containerOfPath(file, apps), label: containerOfPath(file, apps) } });
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

/** Components view, two zoom levels off one builder:
 *  - `module` (a real file path) set → that file's symbols + internal calls.
 *  - else → module boxes across the system, scoped to `container` (a
 *    top-level folder name, or "(project root)") when given.
 *  Returns null only when there is genuinely nothing to draw — said plainly
 *  by the caller, never papered over with an invented picture. */
/** Test files verify components — they are not components. The old fallback
 *  drew one box per class FILE, so test classes posed as components on any
 *  repo with tests (the concept DAG's module tier never had this problem:
 *  the engine's walk skips the test dirs it finds). Same shapes the engines
 *  use: test-dir paths, py tests (test_ prefix, ..._test suffix), js/ts
 *  .test/.spec, Java/Kotlin FooTest/FooTests/FooIT. */
function looksLikeTest(f: string): boolean {
  const p = f.toLowerCase();
  if (/(?:^|\/)(tests?|__tests__|selftest)(?:\/|$)/.test(p)) return true;
  const base = p.split("/").pop() ?? "";
  return /(^test_|_test\.|\.test\.|\.spec\.)/.test(base) ||
    /^(?:test|tests|it|spec|specs)\.(?:java|kt|rs|ts|tsx|js|go|py)$/.test(base) ||
    /(?:test|tests|it|spec|specs)\.(?:java|kt)$/.test(base) ||
    /_test\.rs$/.test(base);
}

export function buildComponentSpec(
  report: CieReport,
  container: string | null,
  title?: string,
  module?: string | null,
  concepts?: ConceptTree | null,
  apps?: { dir: string }[] | null,
  detail: Detail = "summary"
): ChatChartSpec | null {
  if (!report?.files?.length) return null;
  const g = graphOf(report);

  // level 2 — one module zoomed: its symbols + calls between them
  if (module && report.files.includes(module)) {
    const containerArg = module;
    const syms = symbolsOfModule(report, containerArg);
    if (!syms.length) return null;
    const MAX_SYMBOL_BOXES = detail === "full" ? FULL_VIEW_CAP : 10;
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
        loc: { file: module, byteStart: s.loc.start, byteEnd: s.loc.end },
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
    const s2 = specOf("uml", title ?? `Inside ${containerArg.split("/").pop() ?? containerArg} — its named things and their calls`);
    s2.nodes = nodes;
    s2.links = links;
    s2.source = "report.json — this file's own symbols and recorded calls";
    return s2;
  }

  // level 1, system scope — when the concept DAG carries the component tier
  // (RFC-0019 stage 4), the system picture draws THE CONCEPTS, not files:
  // one box per Service/Subdomain across the whole system, each naming its
  // container and its own file modules. Class/function names belong to the
  // level-2 zoom (inside one module), not here. Without the tier (no
  // concepts, no Service/Subdomain under any container) the file fallback
  // below stands — retitled honestly, it is a MODULES picture.
  if (!container && concepts) {
    const all = concepts.nodes;
    const contLabel = new Map<number, string>();
    for (const n of all) if (n.kind === "Container") contLabel.set(n.id, n.label);
    const tier = all
      .filter((n) => (n.kind === "Service" || n.kind === "Subdomain") && n.parent != null && contLabel.has(n.parent))
      .sort((a, b) => (contLabel.get(a.parent!) ?? "").localeCompare(contLabel.get(b.parent!) ?? "") || a.label.localeCompare(b.label));
    if (tier.length) {
      // full detail (the expanded panel) lists EVERY component — the 12-box
      // summary cap is the dashboard card's; only the renderer guard stays
      const MAX_COMPS = detail === "full" ? FULL_VIEW_CAP : 12;
      const shown = tier.slice(0, MAX_COMPS);
      const restT = tier.length - shown.length;
      const LINE_CAP = detail === "full" ? 100 : 3;
      const nodeOf = (c: (typeof tier)[number]) => {
        const cont = contLabel.get(c.parent!) ?? "?";
        const mods = all.filter((m) => m.parent === c.id && m.kind === "Module").map((m) => m.label);
        const first = mods[0];
        const lines = [
          `in ${cont}`, `${c.kind.toLowerCase()} · ${(c.confidence ?? 1) >= 1 ? "recorded evidence" : "inferred from names/structure"}`,
          ...mods.slice(0, LINE_CAP).map((m) => m.split("/").pop() ?? m),
        ];
        const moreM = mods.length - LINE_CAP;
        if (moreM > 0) lines.push(`…and ${moreM} more module(s)`);
        return {
          id: first ?? `component:${c.id}`,
          title: c.label,
          kind: "module" as const,
          lines,
          certainty: ((c.confidence ?? 1) >= 1 ? "Fact" : "Inference") as "Fact" | "Inference",
          loc: first ? { file: first } : undefined,
        };
      };
      const nodes: UmlNode[] = shown.map(nodeOf);
      if (restT > 0) {
        nodes.push({
          id: "…other-components",
          title: `…and ${restT} more component(s)`,
          kind: "module",
          lines: ["smaller — grouped out of this picture, not hidden"],
          certainty: "Inference",
        });
      }
      // recorded calls BETWEEN components, wherever they land (cross-module,
      // even cross-container): symbol's owning file → its component's first
      // module (the same ids the boxes key on)
      const ids = new Set(nodes.map((n) => n.id));
      const compOfFile = new Map<string, string>();
      for (const c of tier) {
        const mods = all.filter((m) => m.parent === c.id && m.kind === "Module").map((m) => m.label);
        for (const m of mods) compOfFile.set(m, mods[0] ?? `component:${c.id}`);
      }
      const links: UmlLink[] = [];
      const seen = new Set<string>();
      for (const s of report.symbols) {
        const fromFile = fileOf(report, s);
        const from = fromFile ? compOfFile.get(fromFile) : undefined;
        if (!from || !ids.has(from)) continue;
        for (const callee of g.calleesOf.get(Number(s.id)) ?? []) {
          const toFile = fileOf(report, g.byId.get(callee));
          const to = toFile ? compOfFile.get(toFile) : undefined;
          if (!to || to === from || !ids.has(to)) continue;
          const key = `${from}->${to}`;
          if (seen.has(key)) continue;
          seen.add(key);
          links.push({ from, to, label: "calls into", certainty: "Fact" });
        }
      }
      const s0 = specOf("uml", title ?? "Components — the concept DAG's component tier, over the whole system");
      s0.nodes = nodes;
      s0.links = links;
      s0.source = "concepts.json — every container's Service/Subdomain tier; report.json for the calls";
      if (detail === "full") {
        s0.tell = restT > 0
          ? `${tier.length} component(s) in this tier — the renderer guard keeps this canvas usable; the first ${shown.length} are on it, grouped not hidden`
          : `${tier.length} component(s) drawn — the whole tier; nothing grouped out`;
      }
      return s0;
    }
  }

  // level 1 — modules across the (scoped) system
  const inContainer = (f: string): boolean =>
    container ? containerOfPath(f, apps) === container : true;

  // RFC-0019 §6.5: when the concept DAG carries this container's component
  // tier, the boxes ARE the components — role-labelled, certainty-chipped,
  // each naming its own file modules. Clicking a component drills into its
  // first module (the same module routing the grid already runs). Without
  // DAG tiers the plain module list below draws — no project regresses.
  if (container) {
    const comps = dagComponentsOf(concepts, container);
    if (comps && comps.length) {
      // full detail (the expanded panel) lists EVERY component of this
      // container and names every module inside each box
      const MAX_COMPS = detail === "full" ? FULL_VIEW_CAP : 12;
      const shown = comps.slice(0, MAX_COMPS);
      const restComps = comps.length - shown.length;
      const LINE_CAP = detail === "full" ? 100 : 3;
      const nodes: UmlNode[] = shown.map((c) => {
        const first = c.modules[0];
        const lines: string[] = [
          `${c.kind.toLowerCase()} · ${c.confidence >= 1 ? "recorded evidence" : "inferred from names/structure"}`,
          ...c.modules.slice(0, LINE_CAP).map((m) => m.split("/").pop() ?? m),
        ];
        const moreM = c.modules.length - LINE_CAP;
        if (moreM > 0) lines.push(`…and ${moreM} more module(s)`);
        return {
          id: first ?? `component:${c.id}`,
          title: c.label,
          kind: "module",
          lines,
          certainty: (c.confidence >= 1 ? "Fact" : "Inference") as "Fact" | "Inference",
          loc: first ? { file: first } : undefined,
        };
      });
      if (restComps > 0) {
        nodes.push({
          id: "…other-components",
          title: `…and ${restComps} more component(s)`,
          kind: "module",
          lines: ["smaller — grouped out of this picture, not hidden"],
          certainty: "Inference",
        });
      }
      // cross-component calls inside this container, from the same records
      const ids = new Set(nodes.map((n) => n.id));
      const compOfFile = new Map<string, string>();
      for (const c of shown) for (const m of c.modules) compOfFile.set(m, c.modules[0] ?? `component:${c.id}`);
      const g2 = graphOf(report);
      const links: UmlLink[] = [];
      const seen = new Set<string>();
      for (const s of report.symbols) {
        const fromFile = fileOf(report, s);
        const from = fromFile ? compOfFile.get(fromFile) : undefined;
        if (!from || !ids.has(from)) continue;
        for (const callee of g2.calleesOf.get(Number(s.id)) ?? []) {
          const toFile = fileOf(report, g2.byId.get(callee));
          const to = toFile ? compOfFile.get(toFile) : undefined;
          if (!to || to === from || !ids.has(to)) continue;
          const key = `${from}->${to}`;
          if (seen.has(key)) continue;
          seen.add(key);
          links.push({ from, to, label: "calls into", certainty: "Fact" });
        }
      }
      const s1 = specOf("uml", title ?? `Components of ${container} — the concept DAG's component tier`);
      s1.nodes = nodes;
      s1.links = links;
      s1.source = "concepts.json — Service/Subdomain nodes under this container, built from the persisted stack facts; report.json for the calls";
      if (detail === "full") {
        s1.tell = restComps > 0
          ? `${comps.length} component(s) under this container — the renderer guard keeps this canvas usable; the first ${shown.length} are on it, grouped not hidden`
          : `${comps.length} component(s) drawn — everything under this container; nothing grouped out`;
      }
      return s1;
    }
  }

  const files = report.files.filter(inContainer);
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
  const testFiles = files.filter((f) => symCount.has(f) && looksLikeTest(f));
  const modules = files.filter((f) => symCount.has(f) && !looksLikeTest(f));
  const ordered = modules
    .slice()
    .sort((a, b) => (symCount.get(b) ?? 0) - (symCount.get(a) ?? 0) || a.localeCompare(b));
  if (!ordered.length && !testFiles.length) return null; // nothing indexed in this scope — the caller says so honestly
  if (!ordered.length && testFiles.length) {
    // scope holds only tests — say so, never draw test files as components
    const sOnly = specOf("uml", title ?? "Components");
    sOnly.nodes = [{
      id: "no-production-modules",
      title: "no production modules in this scope",
      kind: "module",
      lines: ["only test file(s) are indexed here"],
      certainty: "Fact" as const,
    }];
    sOnly.source = "report.json — the file table";
    sOnly.tell = `${testFiles.length} test file(s) indexed — tests verify components; they are not components.`;
    return sOnly;
  }
  // full detail (the expanded panel) lists EVERY module file in this scope
  // and every named thing inside each box — only the renderer guard keeps
  // the honest grouping beyond 400 boxes
  const shown = ordered.slice(0, detail === "full" ? FULL_VIEW_CAP : MAX_MODULE_BOXES);
  const rest = ordered.length - shown.length;
  const lineCap = detail === "full" ? 100 : MAX_SYMBOL_LINES;

  const nodes: UmlNode[] = shown.map((f) => {
    const syms = byModule.get(f) ?? [];
    const lines = syms.slice(0, lineCap).map((s) => `${s.name}()`);
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
  const s1 = specOf("uml", title ?? "Modules — each file as a box, calls as arrows (no concept tier for this scope)");
  s1.nodes = nodes;
  s1.links = links;
  s1.source = "report.json — the file table, named things, and recorded calls";
  if (detail === "full" && rest === 0) {
    s1.tell = `${ordered.length} module(s) drawn — the whole scope; nothing grouped out`;
  }
  if (testFiles.length) {
    s1.tell =
      `${testFiles.length} test file(s) sit out of this picture on purpose — tests verify components; they are not components. ` +
      `Their code stays fully readable in Master and the source panel.`;
  }
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
export function buildDataFlowSpec(report: CieReport, container: string | null, title?: string, apps?: { dir: string }[] | null): ChatChartSpec | null {
  if (!report?.summaries?.length) return null;
  const g = graphOf(report);

  const inScope = (file: string): boolean => (container ? containerOfPath(file, apps) === container : true);

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
      lines: [`${syms.length} named thing(s)`, containerOfPath(f, apps)],
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
// Flow view — the flow chart that keeps adapting to the selection's level.
//
//   system    → containers as a flow (who hands work to whom)
//   container → the components of that container, entry components on top
//   module    → inside one component: its functions, entry → effects
//   symbol    → that one function, step by step (reads → calls → effects → end)
//
// The engine records no line-by-line CFG, so the flow is built ONLY from
// what it does record — the shared call graph (who calls whom, in recorded
// order) and each symbol's behaviour summary (reads, writes, effects,
// exceptions, loops, failure paths). Every box traces back to a record, and
// the tell says plainly that the order is the recorded order, never a traced
// execution. Same one call-graph builder as everywhere else — never a
// second implementation to drift.
// ---------------------------------------------------------------------------

export type FlowLevel = "system" | "container" | "module" | "symbol";

export interface FlowBuild {
  level: FlowLevel;
  /** one-line description of what this level draws — the panel badge */
  label: string;
  spec: ChatChartSpec | null;
}

const MAX_FLOW_NODES = 15;
const MAX_FLOW_STEPS = 6;

const FLOW_HONESTY =
  "Built from the engine's records: arrows are recorded calls, boxes carry each step's recorded reads, writes and effects. The order is the order they were recorded — not a traced run. Click any box to re-level this flow.";

export function buildFlowSpec(report: CieReport, sel: AtlasSelection, apps?: { dir: string }[] | null): FlowBuild {
  const ref = sel.primary;
  if (ref?.kind === "symbol") return flowOfFunction(report, ref);
  if (ref?.kind === "module") return flowOfComponent(report, ref.id);
  if (ref?.kind === "container") return flowOfContainer(report, ref.id, apps);
  return flowOfSystem(report, ref, apps);
}

/** level 0 — the whole system: containers as a flow. Also the honest fallback
 *  for selections that have no call flow of their own (a store, an edge). */
function flowOfSystem(report: CieReport, ref: AtlasRef | null, apps?: { dir: string }[] | null): FlowBuild {
  const note =
    ref?.kind === "store"
      ? "A store has no call flow of its own — the system-level flow is shown. Click a component or a function to zoom in."
      : ref?.kind === "external" || ref?.kind === "edge"
        ? "That element has no call flow of its own — the system-level flow is shown."
        : null;
  const g = graphOf(report);
  const symCount = new Map<string, number>();
  const fileCount = new Map<string, number>();
  for (const s of report.symbols) {
    const f = fileOf(report, s);
    if (!f) continue;
    const c = containerOfPath(f, apps);
    symCount.set(c, (symCount.get(c) ?? 0) + 1);
    fileCount.set(c, (fileCount.get(c) ?? 0) + 1);
  }
  const ordered = [...symCount.keys()].sort((a, b) => (symCount.get(b) ?? 0) - (symCount.get(a) ?? 0) || a.localeCompare(b));
  if (!ordered.length) return { level: "system", label: "nothing indexed to flow", spec: null };
  const shown = ordered.slice(0, MAX_FLOW_NODES);
  const rest = ordered.length - shown.length;
  const nodes: UmlNode[] = shown.map((c) => ({
    id: c,
    title: c,
    kind: "module",
    lines: [`${fileCount.get(c) ?? 0} file(s) · ${symCount.get(c) ?? 0} named thing(s)`],
    certainty: "Fact" as const,
  }));
  if (rest > 0) nodes.push({ id: "…more-containers", title: `…and ${rest} more group(s)`, kind: "module", lines: ["grouped out of this picture, not hidden"], certainty: "Inference" });
  const shownSet = new Set(shown);
  const seenEdge = new Set<string>();
  const hasIncoming = new Set<string>();
  const links: UmlLink[] = [];
  for (const s of report.symbols) {
    const fromFile = fileOf(report, s);
    if (!fromFile) continue;
    const from = containerOfPath(fromFile, apps);
    if (!shownSet.has(from)) continue;
    for (const callee of g.calleesOf.get(Number(s.id)) ?? []) {
      const toFile = fileOf(report, g.byId.get(callee));
      if (!toFile) continue;
      const to = containerOfPath(toFile, apps);
      if (to === from || !shownSet.has(to)) continue;
      const key = `${from}->${to}`;
      if (seenEdge.has(key)) continue;
      seenEdge.add(key);
      hasIncoming.add(to);
      links.push({ from, to, label: "calls into", certainty: "Fact" });
    }
  }
  for (const n of nodes) {
    if (n.id.startsWith("…")) continue;
    if (!hasIncoming.has(n.id)) n.lines = [...(n.lines ?? []), "entry — nothing in the index calls into this"];
  }
  const spec = specOf("flow", "Flow — how the top-level groups hand work to each other", note ? `${note} ${FLOW_HONESTY}` : FLOW_HONESTY);
  spec.nodes = nodes;
  spec.links = links;
  spec.source = "report.json — the file table and recorded calls, grouped by the repo's own folders";
  return { level: "system", label: "the system, container by container", spec };
}

/** level 1 — one container: its components (files) as a flow, entry
 *  components on top, recorded calls as arrows. */
function flowOfContainer(report: CieReport, container: string, apps?: { dir: string }[] | null): FlowBuild {
  const g = graphOf(report);
  const inContainer = (f: string): boolean => containerOfPath(f, apps) === container;
  const files = report.files.filter((f) => report.symbols.some((s) => fileOf(report, s) === f) && inContainer(f));
  if (!files.length) return { level: "container", label: container, spec: null };
  const shown = files.slice(0, MAX_FLOW_NODES);
  const rest = files.length - shown.length;
  const shownSet = new Set(shown);
  const symCount = new Map<string, number>();
  for (const s of report.symbols) {
    const f = fileOf(report, s);
    if (f && shownSet.has(f)) symCount.set(f, (symCount.get(f) ?? 0) + 1);
  }
  const nodes: UmlNode[] = shown.map((f) => ({
    id: f,
    title: f.split("/").pop() ?? f,
    kind: "module",
    lines: [`${symCount.get(f) ?? 0} named thing(s)`, f],
    certainty: "Fact" as const,
    loc: { file: f },
  }));
  if (rest > 0) nodes.push({ id: "…more-modules", title: `…and ${rest} more component(s)`, kind: "module", lines: ["grouped out of this picture, not hidden"], certainty: "Inference" });
  const seenEdge = new Set<string>();
  const hasIncoming = new Set<string>();
  const links: UmlLink[] = [];
  for (const s of report.symbols) {
    const from = fileOf(report, s);
    if (!from || !shownSet.has(from)) continue;
    for (const callee of g.calleesOf.get(Number(s.id)) ?? []) {
      const to = fileOf(report, g.byId.get(callee));
      if (!to || to === from || !shownSet.has(to)) continue;
      const key = `${from}->${to}`;
      if (seenEdge.has(key)) continue;
      seenEdge.add(key);
      hasIncoming.add(to);
      links.push({ from, to, label: "calls into", certainty: "Fact" });
    }
  }
  for (const n of nodes) {
    if (n.id.startsWith("…")) continue;
    if (!hasIncoming.has(n.id)) n.lines = [...(n.lines ?? []), "entry — reached from outside this flow"];
  }
  const spec = specOf("flow", `Flow inside ${container} — component by component`, FLOW_HONESTY);
  spec.nodes = nodes;
  spec.links = links;
  spec.source = "report.json — recorded calls between this container's own files";
  return { level: "container", label: `components of ${container}`, spec };
}

/** level 2 — one component (a file): its functions as a flow. Entries on top,
 *  recorded internal calls as arrows; what flows in from and out to other
 *  components is drawn as its own boxes so the boundary stays visible. */
function flowOfComponent(report: CieReport, file: string): FlowBuild {
  const g = graphOf(report);
  const syms = symbolsOfModule(report, file);
  if (!syms.length) return { level: "module", label: file, spec: null };
  const shown = syms.slice(0, MAX_FLOW_NODES);
  const rest = syms.length - shown.length;
  const shownIds = new Set(shown.map((s) => Number(s.id)));

  const externalCallers = new Map<string, string[]>(); // this file's fn qual → caller quals elsewhere
  const externalCallees = new Map<string, string[]>(); // this file's fn qual → callee quals elsewhere
  for (const s of shown) {
    const id = Number(s.id);
    const qual = s.qual_name || s.name;
    for (const [from, callees] of g.calleesOf) {
      if (shownIds.has(from)) continue;
      if (!callees.includes(id)) continue;
      const caller = g.byId.get(from);
      if (!caller) continue;
      const list = externalCallers.get(qual) ?? [];
      list.push(caller.qual_name || caller.name);
      externalCallers.set(qual, list);
    }
    for (const c of g.calleesOf.get(id) ?? []) {
      if (shownIds.has(c)) continue;
      const callee = g.byId.get(c);
      if (!callee) continue;
      const list = externalCallees.get(qual) ?? [];
      list.push(callee.qual_name || callee.name);
      externalCallees.set(qual, list);
    }
  }

  const nodes: UmlNode[] = shown.map((s) => {
    const sum = g.sumBySym.get(Number(s.id));
    const qual = s.qual_name || s.name;
    const lines: string[] = [];
    if (externalCallers.has(qual)) lines.push(`called from other components`);
    if ((g.fanIn.get(Number(s.id)) ?? 0) === 0) lines.push("entry — nothing in the index calls this");
    if (sum?.writes?.length) lines.push(`writes ${sum.writes.slice(0, 1).join(", ")}`);
    else if (sum?.effects?.length) lines.push(`touches: ${sum.effects.slice(0, 2).join(", ")}`);
    if (!lines.length) lines.push("local computation");
    return {
      id: qual,
      title: `${s.name}()`,
      kind: "function",
      lines,
      certainty: "Fact" as const,
      loc: { file, byteStart: s.loc.start, byteEnd: s.loc.end },
    };
  });
  const inIds = new Set(nodes.map((n) => n.id));
  if (externalCallers.size) {
    const names = [...new Set([...externalCallers.values()].flat())].slice(0, 3);
    nodes.unshift({
      id: "fl-callers-elsewhere",
      title: "called from other components",
      kind: "module",
      lines: names.length ? names.map((n) => `${n}()`) : ["callers the engine resolved outside this file"],
      more: [...new Set([...externalCallers.values()].flat())].length - names.length || undefined,
      certainty: "Fact",
    });
  }
  if (rest > 0) nodes.push({ id: "…more-symbols", title: `…and ${rest} more named thing(s)`, kind: "module", lines: ["grouped out of this picture, not hidden"], certainty: "Inference" });
  if (externalCallees.size) {
    const names = [...new Set([...externalCallees.values()].flat())].slice(0, 3);
    nodes.push({
      id: "fl-out-elsewhere",
      title: "calls into other components",
      kind: "module",
      lines: names.length ? names.map((n) => `${n}()`) : ["callees the engine resolved outside this file"],
      more: [...new Set([...externalCallees.values()].flat())].length - names.length || undefined,
      certainty: "Fact",
    });
  }

  const links: UmlLink[] = [];
  const seenEdge = new Set<string>();
  for (const s of shown) {
    const from = s.qual_name || s.name;
    if (!inIds.has(from)) continue;
    for (const c of g.calleesOf.get(Number(s.id)) ?? []) {
      const callee = g.byId.get(c);
      if (!callee || callee.loc.file !== s.loc.file) continue;
      const to = callee.qual_name || callee.name;
      if (!inIds.has(to) || to === from) continue;
      const key = `${from}->${to}`;
      if (seenEdge.has(key)) continue;
      seenEdge.add(key);
      links.push({ from, to, label: "calls", certainty: "Fact" });
    }
  }
  for (const [qual] of externalCallers) {
    if (inIds.has(qual)) links.push({ from: "fl-callers-elsewhere", to: qual, label: "calls", certainty: "Fact" });
  }
  for (const [qual] of externalCallees) {
    if (inIds.has(qual)) links.push({ from: qual, to: "fl-out-elsewhere", label: "calls", certainty: "Fact" });
  }

  const spec = specOf("flow", `Flow inside ${file.split("/").pop() ?? file} — function by function`, FLOW_HONESTY);
  spec.nodes = nodes;
  spec.links = links;
  spec.source = "report.json — this file's own symbols, their recorded calls, and who calls them from outside";
  return { level: "module", label: `inside ${file.split("/").pop() ?? file}`, spec };
}

/** level 3 — one function, step by step: start → what it reads → each
 *  recorded callee (in recorded order) → what it writes and touches → end,
 *  with the failure path and loops drawn when the summary records them.
 *  The engine does not trace executions — the tell says so, every box still
 *  opens the exact code. */
function flowOfFunction(report: CieReport, ref: AtlasRef): FlowBuild {
  const sym = symbolByQualName(report, ref.id);
  const label = sym ? `${sym.name}()` : ref.label;
  if (!sym) return { level: "symbol", label, spec: null };
  const g = graphOf(report);
  const id = Number(sym.id);
  const sum = g.sumBySym.get(id);
  const file = fileOf(report, sym);
  const loc: ElementLoc | undefined = file ? { file, byteStart: sym.loc.start, byteEnd: sym.loc.end } : undefined;
  const qual = sym.qual_name || sym.name;

  const nodes: UmlNode[] = [
    { id: "fl-start", title: "start", kind: "step", lines: [`${qual}() begins`], certainty: "Fact", loc },
  ];
  const reads = [...new Set(sum?.reads ?? [])];
  if (reads.length) {
    nodes.push({
      id: "fl-reads",
      title: "reads",
      kind: "step",
      lines: reads.slice(0, 3),
      more: reads.length - 3 || undefined,
      certainty: "Fact",
      loc,
    });
  }
  // each recorded callee, in recorded order — the flow's main path
  const stepIds: string[] = [];
  const seenStep = new Set<string>();
  for (const c of g.calleesOf.get(id) ?? []) {
    if (stepIds.length >= MAX_FLOW_STEPS) break;
    const callee = g.byId.get(c);
    if (!callee) continue;
    const cqual = callee.qual_name || callee.name;
    if (seenStep.has(cqual)) continue;
    seenStep.add(cqual);
    stepIds.push(cqual);
    const csum = g.sumBySym.get(c);
    const cfile = fileOf(report, callee);
    const lines: string[] = [];
    if (cfile) lines.push(cfile);
    if (csum?.writes?.length) lines.push(`writes ${csum.writes.slice(0, 1).join(", ")}`);
    else if (csum?.effects?.length) lines.push(`touches: ${csum.effects.slice(0, 2).join(", ")}`);
    if (!csum) lines.push("no records kept for this step");
    nodes.push({
      id: cqual,
      title: `${callee.name}()`,
      kind: "function",
      lines: lines.slice(0, 3),
      certainty: "Fact",
      loc: cfile ? { file: cfile, byteStart: callee.loc.start, byteEnd: callee.loc.end } : undefined,
    });
  }
  const truncatedSteps = Math.max(0, (g.calleesOf.get(id) ?? []).length - stepIds.length);
  if (truncatedSteps > 0) {
    nodes.push({ id: "fl-more-steps", title: `…and ${truncatedSteps} more call(s)`, kind: "module", lines: ["smaller — grouped out of this picture, not hidden"], certainty: "Inference" });
  }
  const unresolved = [...new Set(sum?.unresolved_callees ?? [])];
  if (unresolved.length) {
    nodes.push({
      id: "fl-unresolved",
      title: "names not resolved",
      kind: "external",
      lines: unresolved.slice(0, 3).map((n) => `${n} — the engine couldn't map it to a symbol`),
      more: unresolved.length - 3 || undefined,
      certainty: "Inference",
    });
  }
  const writes = [...new Set(sum?.writes ?? [])];
  const effects = [...new Set((sum?.effects ?? []).map((e) => e))].slice(0, 2);
  if (writes.length || effects.length) {
    nodes.push({
      id: "fl-effects",
      title: "writes & effects",
      kind: "step",
      lines: [...writes.slice(0, 2).map((w) => `writes ${w}`), ...effects],
      certainty: "Fact",
      loc,
    });
  }
  const exceptions = [...new Set(sum?.exceptions ?? [])];
  if (sum?.has_failure_path || exceptions.length) {
    nodes.push({
      id: "fl-fail",
      title: "failure path",
      kind: "step",
      lines: exceptions.length ? exceptions.slice(0, 3) : ["the summary records a failure path"],
      certainty: "Fact",
      loc,
    });
  }
  if (sum?.has_loops) {
    nodes.push({
      id: "fl-loop",
      title: "loops",
      kind: "step",
      lines: ["the summary records a loop — the body can repeat"],
      certainty: "Fact",
      loc,
    });
  }
  nodes.push({
    id: "fl-end",
    title: "end",
    kind: "step",
    lines: [sum ? (sum.complete ? "everything the summary records is drawn" : "summary incomplete — some behaviour stayed unrecorded") : "no behaviour summary was recorded for this symbol"],
    certainty: sum ? (sum.complete ? "Fact" : "Inference") : "Inference",
    loc,
  });

  // the main chain, in recorded order
  const chain: string[] = ["fl-start"];
  if (reads.length) chain.push("fl-reads");
  chain.push(...stepIds);
  if (writes.length || effects.length) chain.push("fl-effects");
  chain.push("fl-end");
  const links: UmlLink[] = [];
  for (let i = 0; i < chain.length - 1; i++) links.push({ from: chain[i], to: chain[i + 1], label: "then", certainty: "Fact" });
  if (truncatedSteps > 0 && stepIds.length) links.push({ from: stepIds[stepIds.length - 1], to: "fl-more-steps", label: "then", certainty: "Inference", dashed: true });
  else if (truncatedSteps > 0) links.push({ from: "fl-start", to: "fl-more-steps", label: "then", certainty: "Inference", dashed: true });
  const branchFrom = chain[chain.length - 2] ?? "fl-start";
  if (unresolved.length) links.push({ from: branchFrom, to: "fl-unresolved", label: "also calls", certainty: "Inference", dashed: true });
  if (sum?.has_failure_path || exceptions.length) {
    links.push({ from: branchFrom, to: "fl-fail", label: "when it fails", certainty: "Fact", dashed: true });
    links.push({ from: "fl-fail", to: "fl-end", label: "then", certainty: "Fact", dashed: true });
  }
  if (sum?.has_loops) links.push({ from: "fl-start", to: "fl-loop", label: "can repeat", certainty: "Inference", dashed: true });

  const spec = specOf("flow", `${qual}() — what happens, step by step`, FLOW_HONESTY);
  spec.nodes = nodes;
  spec.links = links;
  spec.source = "report.json — this symbol's behaviour summary and its recorded callees, in recorded order";
  return { level: "symbol", label: `${qual}() step by step`, spec };
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
  const obligations=report.contracts.filter(c=>c.symbols.includes(symbolId));
  if(obligations.length && obligations.every(c=>c.verified_by!==undefined)) {
    const ids=[...new Set(obligations.flatMap(c=>c.verified_by||[]))];
    return ids.flatMap(id=>{const s=report.symbols.find(s=>s.id===id);return s?[{id,qualName:s.qual_name||s.name,file:fileOf(report,s),byteStart:s.loc.start,byteEnd:s.loc.end}]:[];});
  }
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
