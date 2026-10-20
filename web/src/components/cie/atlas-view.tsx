"use client";

// components/cie/atlas-view.tsx — the dashboard ("Atlas") — ONE screen, a
// grid of charts, one Selection Context.
//
// Every picture lives on the same screen at once, laid out as a grid:
//
//   Containers  — C4 L2: the repo's own top-level grouping (the input panel)
//   Components  — C4 L3: modules of the scoped container, then its symbols
//   Sequence    — time-ordered calls for the selected flow or symbol
//   Data flow   — recorded reads/writes against stores the code names
//   Coverage / Complexity / Churn / Hot spots — measured quality panels
//
// Click a container in the Containers graph and every other panel re-derives
// from that container alone — components, flows, stores, coverage tiles,
// complexity, churn and hot-spot rankings all scope at once; the clicked box
// gets a ring so the filter is visible, not implied. Click again (or a
// breadcrumb) to release. Each panel can be expanded to fill the grid alone.
// Elements the engine could not pin down stay visible but marked
// (dashed / "inferred"), and the Facts-only switch filters the whole
// dashboard at once.

import { CrossServiceEvidence } from "./rfc0020-evidence";
import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { ChevronRight, Home, Layers, Route, Share2, Boxes, Workflow, MousePointerClick, FileCode, FlaskConical, ArrowDownRight, ArrowUpRight, ShieldCheck, Maximize2, Minimize2, Gauge, Flame, GitCommitHorizontal, ExternalLink, PanelRightClose, PanelRightOpen } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import {
  breadcrumbOf,
  buildComponentSpec,
  buildDataFlowSpec,
  buildFlowSpec,
  calleesOfSym,
  callersOf,
  containerOfPath,
  symbolsOfModule,
  symbolByQualName,
  testsReaching,
  type AtlasRef,
  type RelativeSymbol,
} from "@/lib/atlas";
import { buildC4Spec, computeTopFlows, traceFlow, buildSequenceSpec, type MasteryRow } from "@/lib/mastery";
import { buildComplexitySpec, buildCoverageHeatmapSpec, buildHotspotSpec, buildPerFileChurnSpec } from "@/lib/quality";
import { ChatChart, type DiagramSelection } from "@/components/cie/chat-chart";
import type { ChatChartSpec, ElementLoc } from "@/lib/chat-chart";
import type { SymbolInfo } from "@/lib/cie-types";

/** The report once loaded — the type every atlas view reads from. */
type LoadedReport = NonNullable<ReturnType<typeof useCie.getState>["report"]>;

const PANELS = [
  { id: "containers", label: "Containers", icon: Boxes },
  { id: "components", label: "Components", icon: Layers },
  { id: "sequence", label: "Sequence", icon: Route },
  { id: "dataflow", label: "Data flow", icon: Share2 },
  { id: "flow", label: "Flow", icon: Workflow },
] as const;

type PanelId = (typeof PANELS)[number]["id"];

/** Facts-only: drop every element the engine marked as a guess. Store-built
 *  specs carry Fact/Inference/Hypothesis on each node and link — the filter
 *  simply honours them; nothing is re-classified to look better. */
function filterFacts(spec: ChatChartSpec): ChatChartSpec {
  const nodes = (spec.nodes ?? []).filter((n) => (n.certainty ?? "Fact") === "Fact");
  const links = (spec.links ?? []).filter((l) => (l.certainty ?? "Fact") === "Fact");
  const keep = new Set(nodes.map((n) => n.id));
  return { ...spec, nodes, links: links.filter((l) => keep.has(l.from) && keep.has(l.to)) };
}

export function AtlasView() {
  const { report, stack, quality, atlasSelection, atlasFactsOnly, setAtlasSelection, setAtlasFactsOnly, drillUp, resetAtlas, openSourceViewer, sourceRoot } = useCie();
  const [pickedFlow, setPickedFlow] = useState<{ entryId: number; label: string; scope: string | null } | null>(null);
  const [railOpen, setRailOpen] = useState(true);
  const [expanded, setExpanded] = useState<PanelId | null>(null); // null = the whole grid

  const primary = atlasSelection.primary;
  const containerRef = primary?.kind === "container" ? primary : null;
  const moduleRef = primary?.kind === "module" ? primary : null;
  const symbolRef = primary?.kind === "symbol" ? primary : null;
  const scopeId = containerRef?.id ?? null; // every panel reads this one filter

  // A flow pick is stamped with the scope it was made under — when the scope
  // moves the pick no longer matches and is ignored (released), so the panel
  // can never keep showing the old container's walk. No effect needed: the
  // staleness is derived, not synced.
  const picked = pickedFlow && pickedFlow.scope === scopeId ? pickedFlow : null;

  // ---- flow candidates first: the engine's own entry points, scoped when a
  // container is — the sequence panel auto-traces the top one (below). ----
  const flows: MasteryRow[] = useMemo(() => {
    if (!report) return [];
    const { rows } = computeTopFlows(report);
    const inContainer = scopeId
      ? rows.filter((r) => {
          const sym = symbolByQualName(report, r.title);
          const f = sym ? report.files[sym.loc.file] : r.file;
          return f && containerOfPath(f) === scopeId;
        })
      : rows;
    return inContainer.slice(0, 3);
  }, [report, scopeId]);

  // ---- the panel specs, re-derived from the Selection Context ----
  const c4Spec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    const s = buildC4Spec(report, "Containers — how the repo itself is laid out");
    if (s && containerRef) {
      s.nodes = s.nodes?.map((n) => (n.id === containerRef.id ? { ...n, selected: true } : n));
    }
    return s;
  }, [report, containerRef]);

  const compSpec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    // zoomed into a module's symbols when one is selected; otherwise the
    // module boxes, scoped to the selected container when there is one
    return buildComponentSpec(report, scopeId, undefined, moduleRef ? moduleRef.id : null);
  }, [report, scopeId, moduleRef]);

  const dataflowSpec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    return buildDataFlowSpec(report, scopeId);
  }, [report, scopeId]);

  // ---- flow: the chart that re-levels itself. Whatever the Selection
  // Context points at — a container, a component, a function — the flow is
  // rebuilt at THAT level: container click → its components' flow; drill
  // into a function → that function, step by step. ----
  const flowBuild = useMemo(() => {
    if (!report) return null;
    return buildFlowSpec(report, atlasSelection);
  }, [report, atlasSelection]);

  // ---- sequence: symbol-centred trace, or the top flow of the scope ----
  const autoFlow = useMemo(() => {
    if (symbolRef || picked || !scopeId) return null;
    if (!flows.length || flows[0].entryId == null) return null;
    return { entryId: Number(flows[0].entryId), label: flows[0].title };
  }, [symbolRef, picked, scopeId, flows]);
  const seqTrace = useMemo(() => {
    if (!report) return null;
    const qual = symbolRef?.id ?? picked?.label ?? autoFlow?.label;
    const entryId = qual ? (symbolByQualName(report, qual)?.id ?? picked?.entryId ?? autoFlow?.entryId) : autoFlow?.entryId;
    if (entryId == null) return null;
    const res = traceFlow(report, Number(entryId));
    if (!res) return null;
    return { res, entryLabel: qual ?? autoFlow?.label ?? "", entryId: Number(entryId) };
  }, [report, symbolRef, picked, autoFlow]);

  const seqSpec = useMemo(() => {
    if (!seqTrace || !report) return null;
    return buildSequenceSpec(`Sequence — ${seqTrace.entryLabel}()`, seqTrace.res.steps, new Set<number>());
  }, [seqTrace, report]);

  // ---- measured quality panels — scoped when a container is. The raw
  // dataset is checked first so a scope with no rows can say so honestly
  // instead of the panel quietly vanishing. ----
  const hasCov = !!quality?.coverage_heatmap?.files?.length;
  const hasCplx = !!quality?.complexity?.functions?.length;
  const hasChurn = !!quality?.churn?.sample?.per_file?.length;
  const hasHot = !!quality?.hotspots?.rows?.length;
  const covSpec = useMemo(() => (hasCov ? buildCoverageHeatmapSpec(quality, scopeId) : null), [quality, scopeId, hasCov]);
  const cplxSpec = useMemo(() => (hasCplx ? buildComplexitySpec(quality!, scopeId) : null), [quality, scopeId, hasCplx]);
  const churnSpec = useMemo(() => (hasChurn ? buildPerFileChurnSpec(quality!, scopeId) : null), [quality, scopeId, hasChurn]);
  const hotSpec = useMemo(() => (hasHot ? buildHotspotSpec(quality!, scopeId) : null), [quality, scopeId, hasHot]);

  // ---- click routing: one handler per panel (each ChatChart gets its own),
  // all landing in the same Selection Context. The atlas OWNS diagram clicks
  // when a handler is passed (the chat fallback is suppressed), so a symbol
  // or module click also opens the exact code alongside. ----
  const openCodeFor = (loc: ElementLoc | undefined | null, label: string) => {
    if (loc?.file) openSourceViewer({ file: loc.file, byteStart: loc.byteStart, byteEnd: loc.byteEnd, elName: label, chartTitle: "Atlas" });
  };
  const onContainersClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (sel.id.startsWith("…")) return; // the grouped-out box is not a scope
    setAtlasSelection({ kind: "container", id: sel.id, label: sel.id });
  };
  const onComponentsClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (sel.id.startsWith("…")) return;
    if (!report) return;
    const isFile = report.files.includes(sel.id);
    if (isFile) {
      setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
      openCodeFor(sel.loc, sel.label); // editor opens the module's own file
    } else {
      // a symbol box (module zoom) — select the symbol, sequence follows
      const sym = symbolByQualName(report, sel.id);
      setAtlasSelection({ kind: "symbol", id: sel.id, label: sel.label, loc: sel.loc ?? locOfSym(report, sym) });
      openCodeFor(sel.loc ?? locOfSym(report, sym), sel.label); // editor at the exact symbol
    }
  };
  const onDataflowClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (!report) return;
    if (sel.id.startsWith("store:")) {
      setAtlasSelection({ kind: "store", id: sel.id, label: sel.label });
      return;
    }
    if (report.files.includes(sel.id)) {
      setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
      openCodeFor(sel.loc, sel.label);
      return;
    }
    return; // the unresolved bucket is evidence, not a scope
  };
  const onSequenceClick = (sel: DiagramSelection) => {
    if (sel.kind !== "msg") return;
    if (!report) return;
    // arrow click: also select the callee so the other panels re-centre
    const qual = sel.id.replace(/\(\)$/, "");
    if (symbolByQualName(report, qual)) setAtlasSelection({ kind: "symbol", id: qual, label: qual, loc: sel.loc });
  };
  // flow clicks re-level the dashboard: a function box drills to that
  // function, a component box to that component, a container box to that
  // container. fl-* boxes are the flow's own markers (start, reads, end) —
  // they only open the code at the exact spot they stand for.
  const onFlowClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (!report) return;
    if (sel.id.startsWith("…") || sel.id.startsWith("fl-")) {
      openCodeFor(sel.loc, sel.label);
      return;
    }
    const sym = symbolByQualName(report, sel.id);
    if (sym) {
      setAtlasSelection({ kind: "symbol", id: sel.id, label: sel.label, loc: sel.loc ?? locOfSym(report, sym) });
      openCodeFor(sel.loc ?? locOfSym(report, sym), sel.label);
      return;
    }
    if (report.files.includes(sel.id)) {
      setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
      openCodeFor(sel.loc, sel.label);
      return;
    }
    const isContainer = sel.id === "(project root)" || report.files.some((f) => containerOfPath(f) === sel.id);
    if (isContainer) setAtlasSelection({ kind: "container", id: sel.id, label: sel.id });
  };

  const crumbs = breadcrumbOf(atlasSelection);
  const shown = (s: ChatChartSpec | null): ChatChartSpec | null => (atlasFactsOnly && s ? filterFacts(s) : s);
  const shownC4 = shown(c4Spec);
  const shownComp = shown(compSpec);
  const shownFlow = shown(dataflowSpec);
  const shownSeq = shown(seqSpec);
  const shownFlowChart = shown(flowBuild?.spec ?? null);

  if (!report) {
    return (
      <div className="p-6 w-full">
        <Card><CardContent className="p-6 text-sm text-stone-500">Load a project first — the atlas reads the same exports every other page uses.</CardContent></Card>
      </div>
    );
  }

  const expandedPanel = PANELS.find((p) => p.id === expanded);
  const body = (id: PanelId): React.ReactNode => {
    if (id === "containers")
      return shownC4 ? <ChatChart spec={shownC4} onSelectElement={onContainersClick} /> : <HonestEmpty scope={null} what="top-level groups to draw" />;
    if (id === "components")
      return shownComp ? <ChatChart spec={shownComp} onSelectElement={onComponentsClick} /> : (
        <HonestEmpty scope={scopeId} what="modules with named things" />
      );
    if (id === "dataflow")
      return shownFlow ? <ChatChart spec={shownFlow} onSelectElement={onDataflowClick} /> : (
        <HonestEmpty scope={scopeId} what="recorded reads and writes" />
      );
    if (id === "flow")
      return shownFlowChart ? <ChatChart spec={shownFlowChart} onSelectElement={onFlowClick} /> : (
        <HonestEmpty scope={scopeId} what="recorded calls and behaviour summaries to draw a flow from" />
      );
    return (
      <SequencePanel
        seqSpec={shownSeq}
        flows={flows}
        picked={picked}
        auto={autoFlow}
        scope={scopeId}
        traceInfo={seqTrace ? { steps: seqTrace.res.steps.length, truncated: seqTrace.res.truncated, effects: seqTrace.res.effectsSeen } : null}
        onPick={(entryId, label) => setPickedFlow({ entryId, label, scope: scopeId })}
        onClearPick={() => setPickedFlow(null)}
        hasEntry={!!(symbolRef || picked || autoFlow)}
        onSelectElement={onSequenceClick}
      />
    );
  };

  return (
    <div className="flex h-full w-full">
      <div className="flex flex-col flex-1 min-w-0">
        <CrossServiceEvidence stack={stack} scope={scopeId} onOpen={(file,line,label)=>openSourceViewer({file,line,elName:label,chartTitle:"Cross-service calls"})}/>
        {/* breadcrumb + filter bar (PRD §7) */}
        <div className="flex flex-wrap items-center gap-1.5 px-4 pt-3 pb-2 border-b border-stone-200/80 bg-white/70">
          <Home className="h-3.5 w-3.5 text-stone-400" aria-hidden />
          {crumbs.map((c, i) => (
            <span key={`${c.scope}-${i}`} className="flex items-center gap-1.5">
              {i > 0 && <ChevronRight className="h-3 w-3 text-stone-300" aria-hidden />}
              <button
                type="button"
                onClick={() => (c.ref ? setAtlasSelection(c.ref) : resetAtlas())}
                className={`rounded-md px-1.5 py-0.5 text-xs ${i === crumbs.length - 1 ? "font-semibold text-emerald-800 bg-emerald-50" : "text-stone-500 hover:bg-stone-100"}`}
              >
                {c.ref ? c.ref.label : "System"}
              </button>
            </span>
          ))}
          <span className="flex-1" />
          {scopeId && (
            <span className="rounded-md bg-emerald-50 px-1.5 py-0.5 text-[10px] font-medium text-emerald-800" data-testid="atlas-scope-note">
              every other panel shows {scopeId} only
            </span>
          )}
          {primary && (
            <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px] text-stone-500" onClick={drillUp}>
              up one level
            </Button>
          )}
          {primary && (
            <button
              type="button"
              onClick={() => setRailOpen((o) => !o)}
              className="h-6 px-1.5 rounded-md text-stone-400 hover:text-stone-700 hover:bg-stone-100 inline-flex items-center gap-1 text-[11px]"
              aria-label={railOpen ? "Hide the selection panel" : "Show the selection panel"}
            >
              {railOpen ? <PanelRightClose className="h-3.5 w-3.5" aria-hidden /> : <PanelRightOpen className="h-3.5 w-3.5" aria-hidden />}
              {railOpen ? "hide panel" : "details"}
            </button>
          )}
          <label className="flex items-center gap-1.5 text-[11px] text-stone-500 cursor-pointer select-none pr-1">
            <Switch checked={atlasFactsOnly} onCheckedChange={setAtlasFactsOnly} aria-label="Facts only" className="scale-90" />
            Facts only
          </label>
        </div>

        {/* THE GRID — every chart on one screen. Clicking a container in the
            first panel scopes all the others; a second click releases it. */}
        <div className="flex-1 min-h-0 overflow-auto p-3 sm:p-4" data-testid="atlas-grid">
          {expandedPanel ? (
            <div className="grid grid-cols-1 gap-3">
              <Panel
                id={expandedPanel.id}
                icon={expandedPanel.icon}
                label={expandedPanel.label}
                scopedTo={expandedPanel.id === "containers" ? null : scopeId}
                expanded
                onToggle={() => setExpanded(null)}
              >
                {body(expandedPanel.id)}
              </Panel>
            </div>
          ) : (
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
              <div className="lg:col-span-2">
                <Panel id="containers" icon={Boxes} label="Containers" scopedTo={null} onToggle={() => setExpanded("containers")}>
                  {body("containers")}
                </Panel>
              </div>
              <Panel id="components" icon={Layers} label="Components" scopedTo={scopeId} onToggle={() => setExpanded("components")}>
                {body("components")}
              </Panel>
              <Panel id="dataflow" icon={Share2} label="Data flow" scopedTo={scopeId} onToggle={() => setExpanded("dataflow")}>
                {body("dataflow")}
              </Panel>
              <div className="lg:col-span-2">
                <Panel
                  id="flow"
                  icon={Workflow}
                  label="Flow"
                  scopeNote={flowBuild ? `level: ${flowBuild.label}` : null}
                  scopedTo={null}
                  onToggle={() => setExpanded("flow")}
                >
                  {body("flow")}
                </Panel>
              </div>
              <div className="lg:col-span-2">
                <Panel id="sequence" icon={Route} label="Sequence" scopedTo={scopeId} onToggle={() => setExpanded("sequence")}>
                  {body("sequence")}
                </Panel>
              </div>
              {hasCov && (
                <QualityPanel icon={ShieldCheck} label="Test coverage" scopedTo={scopeId} spec={covSpec} scopeWhat="measured coverage rows" />
              )}
              {hasCplx && <QualityPanel icon={Gauge} label="Complexity" scopedTo={scopeId} spec={cplxSpec} scopeWhat="measured functions" />}
              {hasChurn && <QualityPanel icon={GitCommitHorizontal} label="Churn per file" scopedTo={scopeId} spec={churnSpec} scopeWhat="files with churn" />}
              {hasHot && <QualityPanel icon={Flame} label="Hot spots" scopedTo={scopeId} spec={hotSpec} scopeWhat="ranked files" />}
            </div>
          )}
          {!expandedPanel && (
            <p className="mt-3 flex items-center gap-1.5 text-[10.5px] text-stone-400">
              <MousePointerClick className="h-3 w-3" aria-hidden />
              click a container above — every other panel shows only that container; click it again to release · expand any panel with the ⤢ button
            </p>
          )}
        </div>
      </div>

      {/* the detail rail (PRD §5.6): evidence, callers, callees, tests, IDE links */}
      {primary && railOpen && (
        <div className="hidden md:block w-80 shrink-0 border-l border-stone-200 bg-stone-50/60">
          <ScrollArea className="h-full">
            <DetailRail
              primary={primary}
              onPickRef={(ref) => setAtlasSelection(ref)}
              onOpenCode={(loc, label) =>
                openSourceViewer({ file: loc.file, byteStart: loc.byteStart, byteEnd: loc.byteEnd, elName: label, chartTitle: "Atlas detail panel" })
              }
              sourceRoot={sourceRoot}
            />
          </ScrollArea>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Panel chrome — one chart card in the grid, with its scope said out loud
// ---------------------------------------------------------------------------

function Panel({
  id,
  icon: Icon,
  label,
  scopedTo,
  expanded,
  onToggle,
  children,
  noExpand,
  testId,
  scopeNote,
}: {
  id: PanelId | "quality";
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  scopedTo: string | null;
  expanded?: boolean;
  onToggle?: () => void;
  children: React.ReactNode;
  noExpand?: boolean; // quality panels keep their place in the grid — no fill-the-grid mode
  testId?: string;
  /** replaces the default scope caption when a panel's state isn't a container scope (the Flow panel's level) */
  scopeNote?: string | null;
}) {
  return (
    <Card className={`border-stone-200 shadow-sm ${expanded ? "" : "max-h-[520px] overflow-hidden flex flex-col"}`} data-testid={`atlas-panel-${testId ?? id}`}>
      <div className="flex items-center gap-1.5 px-4 pt-3 pb-1 shrink-0">
        <Icon className="h-3.5 w-3.5 text-emerald-700" aria-hidden />
        <span className="text-[13px] font-semibold text-stone-800">{label}</span>
        {scopedTo ? (
          <Badge variant="outline" className="text-[10px] border-emerald-300 text-emerald-800 bg-emerald-50" data-testid={`atlas-scope-${testId ?? id}`}>
            scoped: {scopedTo}
          </Badge>
        ) : scopeNote ? (
          <span className="text-[10px] text-emerald-800 bg-emerald-50 rounded-md px-1.5 py-0.5 border border-emerald-300" data-testid={`atlas-scope-${testId ?? id}`}>{scopeNote}</span>
        ) : (
          <span className="text-[10px] text-stone-400">{id === "containers" ? "click one to filter everything" : "all containers"}</span>
        )}
        <span className="flex-1" />
        {!noExpand && onToggle && (
          <button
            type="button"
            onClick={onToggle}
            className="h-6 w-6 rounded-md text-stone-400 hover:text-stone-700 hover:bg-stone-100 inline-flex items-center justify-center"
            aria-label={expanded ? `Show all panels` : `Expand ${label} to fill the grid`}
            title={expanded ? "Show all panels" : `Expand ${label}`}
          >
            {expanded ? <Minimize2 className="h-3.5 w-3.5" aria-hidden /> : <Maximize2 className="h-3.5 w-3.5" aria-hidden />}
          </button>
        )}
      </div>
      <CardContent className={`px-4 pb-4 pt-0 ${expanded ? "" : "overflow-auto min-h-0"}`}>
        <div className="mt-1">{children}</div>
      </CardContent>
    </Card>
  );
}

/** A measured quality panel: the chart when the scope has rows, an honest
 *  note when the selected container has none of what this panel measures. */
function QualityPanel({
  icon: Icon,
  label,
  scopedTo,
  spec,
  scopeWhat,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  scopedTo: string | null;
  spec: ChatChartSpec | null;
  scopeWhat: string;
}) {
  const slug = label.toLowerCase().replace(/[^a-z]+/g, "-");
  return (
    <Panel id="quality" icon={Icon} label={label} scopedTo={scopedTo} noExpand testId={slug}>
      {spec ? <ChatChart spec={spec} /> : <HonestEmpty scope={scopedTo} what={scopeWhat} />}
    </Panel>
  );
}

function HonestEmpty({ scope, what }: { scope: string | null; what: string }) {
  return (
    <p className="text-xs text-stone-500 py-6 text-center">
      {scope ? `The engine recorded no ${what} in ${scope} — nothing is drawn rather than something invented.` : `Nothing to draw here yet — this panel is built from records the engine exports, and there are none for this scope.`}
    </p>
  );
}

function locOfSym(report: LoadedReport, sym: SymbolInfo | null) {
  if (!sym) return undefined;
  return { file: report.files[sym.loc.file] ?? "", byteStart: sym.loc.start, byteEnd: sym.loc.end };
}

// ---------------------------------------------------------------------------
// Detail rail (PRD §5.6 + §9): Evidence & Provenance, Callers, Callees,
// Tests that reach here, and honest IDE deep links.
// ---------------------------------------------------------------------------

function DetailRail({
  primary,
  onPickRef,
  onOpenCode,
  sourceRoot,
}: {
  primary: AtlasRef;
  onPickRef: (ref: AtlasRef) => void;
  onOpenCode: (loc: { file: string; byteStart?: number; byteEnd?: number }, label: string) => void;
  sourceRoot: string | null;
}) {
  const report = useCie((s) => s.report);
  const [ideFor, setIdeFor] = useState<"vscode" | "cursor" | "jetbrains">("vscode");
  if (!report) return null;

  const sym = primary.kind === "symbol" ? symbolByQualName(report, primary.id) : null;
  const file = primary.kind === "module" ? primary.id : primary.kind === "symbol" ? sym?.loc.file != null ? report.files[sym.loc.file] : undefined : primary.loc?.file;
  // IDE deep link (PRD §9): vscode://file/<abs> etc. Only for local-folder
  // projects — uploads and URL clones have no path on this machine, and the
  // rail says so instead of linking to nothing. Byte offsets are resolved to
  // a line only inside the source panel, where the real content is present.
  const ideHref = sourceRoot && file ? `${ideFor === "vscode" ? "vscode://file/" : ideFor === "cursor" ? "cursor://file/" : "jetbrains://idea/open?file="}${sourceRoot}/${file}` : null;

  return (
    <div className="p-3 space-y-3 text-xs">
      <div>
        <p className="text-[10px] uppercase tracking-wide text-stone-400 font-semibold">Selected</p>
        <p className="text-[13px] font-semibold text-stone-800 break-words">{primary.label}</p>
        <p className="text-[11px] text-stone-500">
          {primary.kind === "symbol" ? "a named thing (function/class)" : primary.kind === "module" ? "a module — one file" : primary.kind === "container" ? "a container — one top-level folder" : primary.kind === "store" ? "a data store" : "an element"}
          {file ? ` · ${file}` : ""}
        </p>
        <div className="flex flex-wrap gap-1 mt-1.5">
          {file && (
            <Button variant="outline" size="sm" className="h-6 px-2 text-[11px]" onClick={() => onOpenCode({ file, byteStart: primary.loc?.byteStart, byteEnd: primary.loc?.byteEnd }, primary.label)}>
              <FileCode className="h-3 w-3 mr-1" aria-hidden /> Open the code
            </Button>
          )}
          {file && sourceRoot && ideHref && (
            <a href={ideHref} title="Open this file in your IDE" className="h-6 px-2 inline-flex items-center rounded-md border border-stone-300 bg-white text-[11px] text-stone-600 hover:bg-stone-100">
              <ExternalLink className="h-3 w-3 mr-1" aria-hidden /> {ideFor}
            </a>
          )}
        </div>
        {file && sourceRoot && (
          <p className="mt-1 flex gap-2 text-[10px] text-stone-400">
            {(["vscode", "cursor", "jetbrains"] as const).map((i) => (
              <button key={i} type="button" onClick={() => setIdeFor(i)} className={ideFor === i ? "underline text-stone-600" : "hover:text-stone-600"}>
                {i}
              </button>
            ))}
            <span>· deep link only for local folders — uploads/URL clones have no path on this machine</span>
          </p>
        )}
        {file && !sourceRoot && (
          <p className="mt-1 text-[10px] text-stone-400">IDE deep links need a local folder — this project has no readable path here, so the panel serves the code instead.</p>
        )}
      </div>

      {sym && <SymbolRelatives report={report} sym={sym} onPickRef={onPickRef} onOpenCode={onOpenCode} />}

      {primary.kind === "module" && (
        <RailSection title="What the engine recorded">
          <ul className="space-y-1">
            {symbolsOfModule(report, primary.id).slice(0, 8).map((s) => (
              <li key={s.id}>
                <button type="button" className="text-left text-emerald-800 hover:underline" onClick={() => onPickRef({ kind: "symbol", id: s.qual_name || s.name, label: s.name, loc: { file: primary.id, byteStart: s.loc.start, byteEnd: s.loc.end } })}>
                  {s.name}()
                </button>
              </li>
            ))}
          </ul>
        </RailSection>
      )}

      {primary.kind === "container" && <ContainerEvidence report={report} container={primary.id} />}

      {primary.kind === "store" && (
        <RailSection title="Why this store is here">
          <p className="text-[11px] text-stone-500">
            {primary.id.startsWith("store:")
              ? "The code's own records name it in reads/writes — it is drawn because real symbols touch it, not because a doc said so."
              : "A schema file in the repo — the store the code itself declares."}
          </p>
        </RailSection>
      )}

      <Separator />
      <EvidenceNote report={report} />
    </div>
  );
}

function RailSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="text-[10px] uppercase tracking-wide text-stone-400 font-semibold">{title}</p>
      <div className="mt-1">{children}</div>
    </div>
  );
}

/** Callers / callees / tests that reach here — straight from the shared call
 *  graph; each row carries its Fact/Inference status and opens the code. */
function SymbolRelatives({
  report,
  sym,
  onPickRef,
  onOpenCode,
}: {
  report: LoadedReport;
  sym: SymbolInfo;
  onPickRef: (ref: AtlasRef) => void;
  onOpenCode: (loc: { file: string; byteStart?: number; byteEnd?: number }, label: string) => void;
}) {
  const id = Number(sym.id);
  const callers = callersOf(report, id);
  const callees = calleesOfSym(report, id);
  const tests = testsReaching(report, id);
  const fileOfRel = (r: RelativeSymbol) => r.file;

  const pick = (r: RelativeSymbol): AtlasRef => ({ kind: "symbol", id: r.qualName, label: r.qualName, loc: { file: r.file, byteStart: r.byteStart, byteEnd: r.byteEnd } });

  return (
    <>
      <RailSection title={`Callers (${callers.length})`}>
        {callers.length ? (
          <ul className="space-y-1">
            {callers.slice(0, 6).map((c) => (
              <li key={c.id} className="flex items-baseline justify-between gap-1">
                <button type="button" className="text-emerald-800 hover:underline text-left truncate" onClick={() => onPickRef(pick(c))}>
                  {c.qualName}
                </button>
                <button type="button" className="text-stone-400 hover:text-stone-700 shrink-0" aria-label={`Open ${c.qualName} in the source panel`} onClick={() => onOpenCode({ file: fileOfRel(c), byteStart: c.byteStart, byteEnd: c.byteEnd }, c.qualName)}>
                  <FileCode className="h-3 w-3" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[11px] text-stone-500">No recorded caller — reached from outside the indexed calls, or an entry point.</p>
        )}
      </RailSection>
      <RailSection title={`Callees (${callees.length})`}>
        {callees.length ? (
          <ul className="space-y-1">
            {callees.slice(0, 6).map((c) => (
              <li key={c.id} className="flex items-baseline justify-between gap-1">
                <button type="button" className="text-emerald-800 hover:underline text-left truncate" onClick={() => onPickRef(pick(c))}>
                  {c.qualName}
                </button>
                <button type="button" className="text-stone-400 hover:text-stone-700 shrink-0" aria-label={`Open ${c.qualName} in the source panel`} onClick={() => onOpenCode({ file: c.file, byteStart: c.byteStart, byteEnd: c.byteEnd }, c.qualName)}>
                  <FileCode className="h-3 w-3" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[11px] text-stone-500">No resolved callee recorded.</p>
        )}
      </RailSection>
      <RailSection title={`Tests that reach here (${tests.length})`}>
        <p className="text-[11px] text-stone-500">Static reachability, not executed coverage. Engine obligation bindings use a six-edge bound; older/unbound symbols use the legacy filename heuristic.</p>
        {tests.length ? (
          <ul className="space-y-1">
            {tests.slice(0, 4).map((c) => (
              <li key={c.id} className="flex items-baseline justify-between gap-1">
                <button type="button" className="text-emerald-800 hover:underline text-left truncate" onClick={() => onPickRef(pick(c))}>
                  {c.qualName}
                </button>
                <button type="button" className="text-stone-400 hover:text-stone-700 shrink-0" aria-label={`Open the test ${c.qualName}`} onClick={() => onOpenCode({ file: c.file, byteStart: c.byteStart, byteEnd: c.byteEnd }, c.qualName)}>
                  <FlaskConical className="h-3 w-3" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[11px] text-stone-500">No test reaches this symbol in the recorded graph and available depth bound.</p>
        )}
      </RailSection>
      <SummaryEvidence report={report} symId={id} />
    </>
  );
}

/** Per-symbol evidence: the engine's own summary record, never paraphrased
 *  into more certainty than it carries. */
function SummaryEvidence({ report, symId }: { report: LoadedReport; symId: number }) {
  const sum = report.summaries.find((s) => Number(s.sym) === symId);
  return (
    <RailSection title="Evidence & provenance">
      {sum ? (
        <ul className="space-y-0.5 text-[11px] text-stone-600">
          <li className="flex items-center gap-1.5">
            <ShieldCheck className={`h-3 w-3 ${sum.complete ? "text-emerald-600" : "text-amber-500"}`} aria-hidden />
            {sum.complete ? "complete — the engine fully read this symbol" : "incomplete — the engine says this summary is partial"}
          </li>
          {sum.reads.length > 0 && <li>reads: {sum.reads.slice(0, 3).join(", ")}</li>}
          {sum.writes.length > 0 && <li>writes: {sum.writes.slice(0, 3).join(", ")}</li>}
          {sum.effects.length > 0 && <li>effects: {sum.effects.slice(0, 3).join(", ")}</li>}
          {sum.unresolved_callees.length > 0 && <li className="text-amber-700">names it reaches for that the engine could not resolve: {sum.unresolved_callees.slice(0, 2).join(", ")}</li>}
        </ul>
      ) : (
        <p className="text-[11px] text-stone-500">No summary record for this symbol — shown as inferred everywhere, never as fact.</p>
      )}
    </RailSection>
  );
}

function ContainerEvidence({ report, container }: { report: LoadedReport; container: string }) {
  const files = report.files.filter((f) => containerOfPath(f) === container);
  const syms = files.length ? report.symbols.filter((s) => files.includes(report.files[s.loc.file])) : [];
  return (
    <RailSection title="What lives here">
      <ul className="space-y-0.5 text-[11px] text-stone-600">
        <li>{files.length} file(s), {syms.length} named thing(s) — counted, not guessed.</li>
        <li>The Components view zooms into exactly these files.</li>
      </ul>
    </RailSection>
  );
}

/** Project-level provenance: where every picture on this page comes from. */
function EvidenceNote({ report }: { report: LoadedReport }) {
  const ev = report.evidence;
  return (
    <RailSection title="Provenance for this whole page">
      <ul className="space-y-0.5 text-[11px] text-stone-600">
        <li>Every box and arrow is built from report.json — the engine's own export — through the same chart engines the chat uses.</li>
        <li>Unresolved calls recorded: {ev.unresolved.length}. Unsupported constructs: {ev.unsupported.length}. Both stay named, never dropped.</li>
        <li>Assumptions on record: {ev.assumptions.length ? ev.assumptions.length : "none"}. Provenance: {ev.provenance}.</li>
      </ul>
    </RailSection>
  );
}

/** The sequence panel, grid-native: when a container is scoped and nothing
 *  else is picked, the top flow of that scope is auto-traced (said out loud),
 *  and the other in-scope flows sit one chip away. With no scope the honest
 *  picker stands in — nothing is traced until something is chosen. */
function SequencePanel({
  seqSpec,
  flows,
  picked,
  auto,
  scope,
  traceInfo,
  onPick,
  onClearPick,
  hasEntry,
  onSelectElement,
}: {
  seqSpec: ChatChartSpec | null;
  flows: MasteryRow[];
  picked: { entryId: number; label: string } | null;
  auto: { entryId: number; label: string } | null;
  scope: string | null;
  traceInfo: { steps: number; truncated: number; effects: string[] } | null;
  onPick: (entryId: number, label: string) => void;
  onClearPick: () => void;
  hasEntry: boolean;
  onSelectElement: (sel: DiagramSelection) => void;
}) {
  const activeLabel = picked?.label ?? auto?.label ?? null;
  return (
    <div className="space-y-2">
      {hasEntry && seqSpec && (
        <>
          <div className="flex items-center justify-end gap-2 flex-wrap">
            {auto && !picked && scope && (
              <Badge variant="outline" className="text-[10px] border-emerald-300 bg-emerald-50 text-emerald-800" data-testid="atlas-seq-auto">
                auto-traced the top flow starting in {scope}
              </Badge>
            )}
            {traceInfo && (
              <Badge variant="outline" className="text-[10px] text-stone-500 border-stone-300">
                {traceInfo.steps} step{traceInfo.steps === 1 ? "" : "s"} drawn
                {traceInfo.truncated > 0 ? ` · ${traceInfo.truncated} reached but capped — counted, never hidden` : ""}
              </Badge>
            )}
            {picked && (
              <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={onClearPick}>
                clear pick
              </Button>
            )}
          </div>
          <ChatChart spec={seqSpec} onSelectElement={onSelectElement} />
          {flows.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-[10px] text-stone-400">flows in scope:</span>
              {flows.map((f) => (
                <button
                  key={f.title}
                  type="button"
                  onClick={() => f.entryId != null && onPick(f.entryId, f.title)}
                  className={`rounded-md border px-1.5 py-0.5 text-[10.5px] transition-colors ${
                    activeLabel === f.title
                      ? "border-emerald-300 bg-emerald-50 text-emerald-900"
                      : "border-stone-200 text-stone-500 hover:border-emerald-300 hover:bg-emerald-50/40"
                  }`}
                >
                  {f.title}
                </button>
              ))}
            </div>
          )}
        </>
      )}
      {!hasEntry && (
        <div>
          <h2 className="text-sm font-semibold text-stone-800">Pick a flow to walk through</h2>
          <p className="text-xs text-stone-500 mt-0.5">
            Candidates ranked by how much code each start point can reach — a structural guess about importance, not a measured truth. Selecting a symbol
            anywhere on this dashboard also centres this panel on it.
          </p>
          {flows.length ? (
            <ul className="mt-3 space-y-1.5">
              {flows.map((f) => (
                <li key={f.title}>
                  <button
                    type="button"
                    onClick={() => f.entryId != null && onPick(f.entryId, f.title)}
                    className="w-full text-left rounded-lg border border-stone-200 hover:border-emerald-300 hover:bg-emerald-50/40 px-3 py-2 transition-colors"
                  >
                    <span className="text-[13px] font-medium text-stone-800">{f.title}</span>
                    {f.detail && <span className="block text-[11px] text-stone-500">{f.detail}</span>}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-3 text-xs text-stone-500">
              The report doesn't record who-calls-whom{scope ? ` for ${scope}` : " for this project"}, so there are no flow candidates to rank — said
              honestly rather than invented.
            </p>
          )}
        </div>
      )}
      {hasEntry && !seqSpec && (
        <p className="text-xs text-stone-500 py-6 text-center">
          The engine recorded no call walk for this selection — it may be a leaf, or its calls weren't resolved. Nothing is drawn rather than something
          invented.
        </p>
      )}
    </div>
  );
}
