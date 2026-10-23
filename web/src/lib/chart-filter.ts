// XFILTER — cross-chart slice & dice (linked views).
//
// The user ask: "when I click on one chart the other charts also change —
// the rest of the filters show only the filtered version." One chart's
// bucket becomes a FILTER that every sibling chart, every row list and the
// map re-render from — the drilldown pattern of menu-drilldown.tsx, lifted
// from one dashboard to the whole workspace.
//
// This module is the PURE half: the filter shape, the toggle/replace rule,
// which row universes each dimension applies to, the per-universe
// predicates, the chart row builders (single source of truth for the
// emitters' keys AND the receivers' re-aggregation — the two can never
// drift apart), and the map's spec slicing. No React, no store, no DOM —
// the node suite (web/test/xfilter.test.cjs) runs it directly on the real
// demo artifacts.
//
// Semantics (decided up front, mirroring the atlasSelection precedent):
//   ONE active filter at a time — a second click on the same bucket
//   releases it, a click on any other bucket replaces it. Stacking across
//   dimensions was considered and rejected: the single-slot precedent
//   keeps the disclosure one line and the release affordance obvious.
//
// The cross-filter matrix is honest about what each dimension CAN filter:
//   severity/kind  → findings (the severity, kind and file charts + Fixes)
//   file           → findings + summaries + contracts + map elements +
//                    canvas diagrams (every universe carries a file)
//   effect         → summaries (the effects chart)
//   authority      → contracts (the promises chart + Promises)
//   certainty      → map elements + canvas diagrams (displayMode)
// A dimension that a universe does not carry leaves that universe's
// charts untouched — never a filtered-looking chart with hidden rows.

import type {
  CieReport,
  CieViewSpec,
  VizElement,
  Finding,
  SummaryInfo,
  Contract,
} from "./cie-types";
import { findingTitle, severityPlain, effectPlain, certaintyPlain, authorityPlain } from "./plain-words";

// ---------------------------------------------------------------- types ---

export type ChartDimension =
  | "severity"
  | "kind"
  | "file"
  | "effect"
  | "authority"
  | "certainty";

/** The active cross-chart filter: which axis, which raw value, and where
 *  it came from (provenance for the banner and the release affordance). */
export interface ChartFilter {
  dimension: ChartDimension;
  /** The stable raw value the predicate matches (severity string, raw kind,
   *  full path, raw effect id, authority pseudo-bucket, displayMode). */
  value: string;
  /** Plain-words label for chips, banners and disclosures. */
  label: string;
  /** The emitting chart's id (origin), e.g. "kind". */
  origin: string;
  /** The emitting chart's plain title, e.g. "What kinds of problems?". */
  originLabel: string;
}

/** The row universes the filter can act on. "canvas" is the ViewSpec-backed
 *  chat-chart surface (viz-adapter elements carry evidence.file and
 *  displayMode, so only file/certainty apply there). */
export type RowUniverse = "findings" | "summaries" | "contracts" | "elements" | "canvas";

// --------------------------------------------------- toggle / replace ----

/** One slot, toggle-release, replace-on-different (module header explains
 *  why). Pure so the suite pins it and the store just delegates. */
export function nextChartFilter(
  current: ChartFilter | null,
  next: ChartFilter | null,
): ChartFilter | null {
  if (!next) return null;
  if (current && current.dimension === next.dimension && current.value === next.value) {
    return null; // second click on the same bucket = release
  }
  return next;
}

// ------------------------------------------------------ applicability ----

const MATRIX: Record<ChartDimension, RowUniverse[]> = {
  severity: ["findings"],
  kind: ["findings"],
  file: ["findings", "summaries", "contracts", "elements", "canvas"],
  effect: ["summaries"],
  authority: ["contracts"],
  certainty: ["elements", "canvas"],
};

/** Does the active filter constrain this universe's rows at all? A chart
 *  whose rows don't carry the dimension renders its full data, unchanged. */
export function filterApplies(filter: ChartFilter | null, universe: RowUniverse): boolean {
  return !!filter && MATRIX[filter.dimension].includes(universe);
}

/** What the filtered things are called, per dimension (banner text). */
export function filterNoun(dimension: ChartDimension): string {
  switch (dimension) {
    case "severity":
    case "kind":
    case "file":
      return "problems";
    case "effect":
      return "side effects";
    case "authority":
      return "promises";
    case "certainty":
      return "map things";
  }
}

/** Where the filtered rows are listed in full, per dimension. */
export function filterDestination(dimension: ChartDimension): "fixes" | "does" | "promises" | "map" {
  switch (dimension) {
    case "severity":
    case "kind":
    case "file":
      return "fixes";
    case "effect":
      return "does";
    case "authority":
      return "promises";
    case "certainty":
      return "map";
  }
}

/** "12 of 40" — the surviving-count disclosure every receiver prints. */
export function countText(surviving: number, total: number): string {
  return `${surviving} of ${total}`;
}

// ---------------------------------------------------------- predicates ---

/** A finding's file, via its symbol — null when the export didn't record
 *  the symbol (the file chart then honestly drops the row, as before). */
export function fileOfSymbol(report: CieReport, symId: number): string | null {
  const sym = report.symbols.find((s) => s.id === symId);
  if (!sym) return null;
  return report.files[sym.loc.file] ?? null;
}

export function findingMatches(report: CieReport, f: Finding, filter: ChartFilter): boolean {
  switch (filter.dimension) {
    case "severity":
      return f.severity === filter.value;
    case "kind":
      return f.kind === filter.value;
    case "file":
      return fileOfSymbol(report, f.symbol) === filter.value;
    default:
      return true;
  }
}

export function summaryMatches(report: CieReport, s: SummaryInfo, filter: ChartFilter): boolean {
  switch (filter.dimension) {
    case "file":
      return fileOfSymbol(report, s.sym) === filter.value;
    case "effect":
      return (s.effects ?? []).includes(filter.value);
    default:
      return true;
  }
}

/** The promises donut buckets authority into two pseudo-values: everything
 *  written by you (Explicit) vs everything CIE guessed (the rest) — the
 *  same buckets the chart draws, so the click and the filter agree. */
export const AUTHORITY_EXPLICIT = "explicit";
export const AUTHORITY_INFERRED = "inferred";

export function contractMatches(report: CieReport, c: Contract, filter: ChartFilter): boolean {
  switch (filter.dimension) {
    case "authority":
      return filter.value === AUTHORITY_EXPLICIT
        ? c.authority === "Explicit"
        : c.authority !== "Explicit";
    case "file":
      return c.symbols.some((id) => fileOfSymbol(report, id) === filter.value);
    default:
      return true;
  }
}

export function elementMatches(el: Pick<VizElement, "displayMode" | "evidence">, filter: ChartFilter): boolean {
  switch (filter.dimension) {
    case "certainty":
      return el.displayMode === filter.value;
    case "file":
      return el.evidence?.file === filter.value;
    default:
      return true;
  }
}

// ------------------------------------------------- filtered row lists ----

/** The findings universe under the active filter (full list untouched when
 *  the dimension doesn't apply). */
export function filteredFindings(report: CieReport, filter: ChartFilter | null): Finding[] {
  if (!filter || !filterApplies(filter, "findings")) return report.findings;
  return report.findings.filter((f) => findingMatches(report, f, filter));
}

export function filteredSummaries(report: CieReport, filter: ChartFilter | null): SummaryInfo[] {
  if (!filter || !filterApplies(filter, "summaries")) return report.summaries;
  return report.summaries.filter((s) => summaryMatches(report, s, filter));
}

export function filteredContracts(report: CieReport, filter: ChartFilter | null): Contract[] {
  if (!filter || !filterApplies(filter, "contracts")) return report.contracts;
  return report.contracts.filter((c) => contractMatches(report, c, filter));
}

// ------------------------------------------------------ row builders -----
// Single source of truth for the canned charts: the SAME functions build
// the chart the user clicks and re-aggregate the sibling charts under the
// filter, so an emitter's key can never disagree with a receiver's
// predicate. Shapes carry BOTH the display fields (name/full/count/color)
// and the raw `value`/`key` the filter slot stores.

export const CHART_COLORS = {
  rose: "#e11d48",
  roseSoft: "#fb7185",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
  stoneLight: "#a8a29e",
} as const;

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

/** One horizontal-bar row: display label + full label + count + the raw
 *  filter value the click emits. */
export interface ChartRow {
  name: string;
  full: string;
  count: number;
  color: string;
  value: string;
}

/** One donut row: `key` is the raw filter value. */
export interface DonutRow {
  name: string;
  value: number;
  color: string;
  key: string;
}

const SEV_ORDER = ["Critical", "High", "Medium", "Low"] as const;

export function severityRows(findings: Finding[]): ChartRow[] {
  return SEV_ORDER.map((sev) => {
    const count = findings.filter((f) => f.severity === sev).length;
    const p = severityPlain(sev);
    return {
      name: p.label,
      full: `${p.label} (${sev}) — ${p.hint}`,
      count,
      color:
        sev === "Critical"
          ? CHART_COLORS.rose
          : sev === "High"
            ? CHART_COLORS.roseSoft
            : sev === "Medium"
              ? CHART_COLORS.amber
              : CHART_COLORS.stoneLight,
      value: sev,
    };
  });
}

export function kindRows(findings: Finding[]): ChartRow[] {
  const byKind = new Map<string, number>();
  for (const f of findings) byKind.set(f.kind, (byKind.get(f.kind) ?? 0) + 1);
  const fake = (kind: string) =>
    ({ kind, id: "", severity: "Low", symbol: 0, symbol_name: "", message: "", evidence_lines: [], remediation: "" }) as Finding;
  return [...byKind.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([kind, count]) => ({
      name: truncate(findingTitle(fake(kind)), 24),
      full: findingTitle(fake(kind)),
      count,
      color: CHART_COLORS.emerald,
      value: kind, // the RAW kind — the stable key the predicate matches
    }));
}

export function fileRows(findings: Finding[], report: CieReport): ChartRow[] {
  const byFile = new Map<number, number>();
  for (const f of findings) {
    const sym = report.symbols.find((s) => s.id === f.symbol);
    if (sym) byFile.set(sym.loc.file, (byFile.get(sym.loc.file) ?? 0) + 1);
  }
  return [...byFile.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([idx, count]) => {
      const path = report.files[idx] ?? `File ${idx}`;
      const base = path.split("/").pop() ?? path;
      return { name: truncate(base, 22), full: path, count, color: CHART_COLORS.amber, value: path };
    });
}

export function effectRows(summaries: SummaryInfo[]): ChartRow[] {
  const byEffect = new Map<string, number>();
  for (const s of summaries) for (const e of s.effects ?? []) byEffect.set(e, (byEffect.get(e) ?? 0) + 1);
  return [...byEffect.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([e, count]) => ({
      name: truncate(effectPlain(e), 26),
      full: effectPlain(e),
      count,
      color: CHART_COLORS.stone,
      value: e,
    }));
}

export function promiseRows(contracts: Contract[]): DonutRow[] {
  const written = contracts.filter((c) => c.authority === "Explicit").length;
  const guessed = contracts.length - written;
  return [
    { name: authorityPlain("Explicit").label, value: written, color: CHART_COLORS.emerald, key: AUTHORITY_EXPLICIT },
    { name: authorityPlain("InferredCandidate").label, value: guessed, color: CHART_COLORS.amber, key: AUTHORITY_INFERRED },
  ];
}

export function certaintyRows(viewspec: CieViewSpec | null): DonutRow[] {
  if (!viewspec) return [];
  const tally = new Map<string, number>();
  for (const e of viewspec.elements) tally.set(e.displayMode, (tally.get(e.displayMode) ?? 0) + 1);
  for (const e of viewspec.edges) tally.set(e.displayMode, (tally.get(e.displayMode) ?? 0) + 1);
  return (["Fact", "Inference", "Hypothesis"] as const)
    .map((m) => ({
      name: certaintyPlain(m).label,
      value: tally.get(m) ?? 0,
      color: m === "Fact" ? CHART_COLORS.emerald : m === "Inference" ? CHART_COLORS.amber : CHART_COLORS.stoneLight,
      key: m,
    }))
    .filter((d) => d.value > 0);
}

// --------------------------------------------------------- map slicing ---
// The Map is a receiver, not an emitter: its bubbles' clicks already own a
// binding (setMapSelected → the chat referent + source panel), so a filter
// from the charts page re-slices the map's spec instead. Same rules as the
// viz-adapter's sliceViewSpec: kept elements keep their parent chain,
// edges survive only between kept ends, the footer is recomputed from the
// FINAL element set, and the omissions are disclosed in the gaps ledger —
// never a silently smaller map.

export interface SlicedMap {
  spec: CieViewSpec;
  kept: number;
  total: number;
}

export function sliceMapSpec(spec: CieViewSpec, filter: ChartFilter): SlicedMap | null {
  if (!filterApplies(filter, "elements")) return null;
  const byId = new Map(spec.elements.map((e) => [e.id, e]));
  const kept = new Set<string>();
  const addWithParents = (id: string) => {
    if (kept.has(id)) return;
    const el = byId.get(id);
    if (!el) return;
    kept.add(id);
    if (el.parentId) addWithParents(el.parentId);
  };
  for (const el of spec.elements) {
    if (elementMatches(el, filter)) addWithParents(el.id);
  }
  const elements = spec.elements.filter((e) => kept.has(e.id));
  // Edges survive on ENDPOINTS only — the scope-slice rule of the
  // viz-adapter's sliceViewSpec. An edge's own certainty stays visible in
  // its colour/dash, and dropping a parent-link edge would strand a kept
  // child unconnected; the elements ledger discloses what the filter hid.
  const edges = spec.edges.filter((e) => kept.has(e.source) && kept.has(e.target));
  const omitted = spec.elements.length - elements.length;
  const sliced: CieViewSpec = {
    ...spec,
    elements,
    edges,
    footer: spec.footer
      ? {
          ...spec.footer,
          facts: elements.filter((e) => e.displayMode === "Fact").length,
          inferences: elements.filter((e) => e.displayMode === "Inference").length,
          hypotheses: elements.filter((e) => e.displayMode === "Hypothesis").length,
        }
      : undefined,
    gaps: [
      ...(spec.gaps ?? []),
      ...(omitted
        ? [{ id: "xfilter-omissions", area: "slice & dice", why: `${omitted} of ${spec.elements.length} map bubbles omitted by the chart filter` }]
        : []),
    ],
  };
  return { spec: sliced, kept: elements.length, total: spec.elements.length };
}
