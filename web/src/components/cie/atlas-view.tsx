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

import { useMemo, useState } from "react";
import { CrossServiceEvidence } from "./rfc0020-evidence";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { ChevronRight, Home, Layers, Route, Share2, Boxes, Workflow, Rocket, Server, Network, MousePointerClick, ShieldCheck, Maximize2, Minimize2, Gauge, Flame, GitCommitHorizontal } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { planSheet } from "@/lib/chart-plan";
import { CodePopup, type PopupRequest } from "./code-popup";
import {
  breadcrumbOf,
  buildComponentSpec,
  buildDataFlowSpec,
  buildFlowSpec,
  conceptNodeFor,
  conceptResolve,
  containerOfPath,
  graphOf,
  symbolByQualName,
  type AtlasRef,
} from "@/lib/atlas";
import { buildC4Spec, computeTopFlows, traceFlow, buildSequenceSpec, type MasteryRow } from "@/lib/mastery";
import { buildComplexitySpec, buildConceptTreeSpec, buildCoverageHeatmapSpec, buildHotspotSpec, buildPerFileChurnSpec } from "@/lib/quality";
import { ChatChart, type DiagramSelection } from "@/components/cie/chat-chart";
import type { ChatChartSpec, ElementLoc } from "@/lib/chat-chart";
import type { SymbolInfo } from "@/lib/cie-types";
import type { MainSite, RouteSite } from "@/lib/stack-types";

/** The report once loaded — the type every atlas view reads from. */
type LoadedReport = NonNullable<ReturnType<typeof useCie.getState>["report"]>;

const PANELS = [
  { id: "containers", label: "Containers", icon: Boxes },
  { id: "appentry", label: "Application entry points", icon: Rocket },
  { id: "svcentry", label: "Service entry points", icon: Server },
  { id: "components", label: "Components", icon: Layers },
  { id: "hierarchy", label: "Hierarchy", icon: Network },
  { id: "sequence", label: "Sequence", icon: Route },
  { id: "dataflow", label: "Data flow", icon: Share2 },
  { id: "flow", label: "Flow", icon: Workflow },
] as const;

type PanelId = (typeof PANELS)[number]["id"];

export function AtlasView() {
  const { report, quality, stack, concepts, atlasSelection, atlasFactsOnly, setAtlasSelection, setAtlasFactsOnly, drillUp, resetAtlas, openSourceViewer, persona, personaDisclosure, setPendingAsk, addChip, removeChip } = useCie();
  // RFC-0024 stage 2 — the panels derive FROM the plan (§14.2-1: switch =
  // re-plan, not re-page). planSheet is pure; the grid below renders
  // plan.rows in reading order instead of a hand-coded sequence.
  const plan = useMemo(() => planSheet(persona, { factsOnlyOverride: atlasFactsOnly }), [persona, atlasFactsOnly]);
  const [pickedFlow, setPickedFlow] = useState<{ entryId: number; label: string; scope: string | null } | null>(null);
  const [expanded, setExpanded] = useState<PanelId | null>(null); // null = the whole grid

  const primary = atlasSelection.primary;
  const containerRef = primary?.kind === "container" ? primary : null;
  const moduleRef = primary?.kind === "module" ? primary : null;
  const symbolRef = primary?.kind === "symbol" ? primary : null;
  const conceptRef = primary?.kind === "concept" ? primary : null;
  // A concept rarely names a container directly — resolve its own symbol
  // set to the container(s) that hold them (see conceptResolve), most
  // populated first, so picking a concept scopes Containers/Components/Data
  // flow exactly like picking that container would. The Flow panel (and the
  // sequence trace below) stay MORE precise than this: they filter to the
  // concept's exact symbols, not its whole container.
  const conceptScope = useMemo(() => {
    if (!report || !conceptRef) return null;
    return conceptResolve(report, conceptRef, stack?.apps ?? null);
  }, [report, conceptRef, stack]);
  // A symbol picked straight off the Application/Service entry-point tables
  // (onPickMain/onPickRoute) carries no container of its own — resolve it
  // from the symbol's file the same way a concept resolves to one, so
  // picking "GwApplication" scopes Containers/Components/Data flow/the
  // entry tables to api-gw instead of leaving every other panel unscoped.
  const symbolScope = symbolRef?.loc?.file ? containerOfPath(symbolRef.loc.file, stack?.apps ?? null) : null;
  const scopeId = containerRef?.id ?? conceptScope?.containers[0] ?? symbolScope ?? null; // every panel reads this one filter
  // The concept tree node that best represents whatever IS selected, from
  // any panel — a container, a module, a symbol, or a concept itself — so
  // Hierarchy focuses in step with the rest of the atlas instead of only
  // ever driving it. Null (no focus) when nothing resolves to a concept.
  const focusConceptId = useMemo(() => {
    if (!report || !concepts) return null;
    return conceptNodeFor(concepts, primary, report);
  }, [concepts, primary, report]);

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
    // full signature: containers are the stack artifact's real apps (RFC-0019
    // §6.5), and image-only compose services draw as the C4 Context
    // externals they are — the old two-arg call silently bypassed both
    const s = buildC4Spec(report, stack, "Containers — how the repo itself is laid out");
    if (s && scopeId) {
      s.nodes = s.nodes?.map((n) => (n.id === scopeId ? { ...n, selected: true } : n));
    }
    return s;
  }, [report, stack, scopeId]);

  const compSpec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    // zoomed into a module's symbols when one is selected; otherwise the
    // concept DAG's component tier (Service/Subdomain under the selected
    // container), scoped to the stack's real apps when it names them.
    // Without a DAG for this container the files-as-modules fallback draws —
    // never per-class boxes pretending to be components.
    return buildComponentSpec(report, scopeId, undefined, moduleRef ? moduleRef.id : null, concepts, stack?.apps ?? null);
  }, [report, concepts, stack, scopeId, moduleRef]);

  // the EXPANDED panel gets "full" detail — every module/component in the
  // scope on one canvas; the small cards keep the 12-box summary. Derived
  // only while a panel is expanded (nothing computed otherwise).
  const fullC4 = useMemo<ChatChartSpec | null>(() => {
    if (!expanded || !report) return null;
    return buildC4Spec(report, stack, "Containers — how the repo itself is laid out", "full");
  }, [expanded, report, stack]);

  const fullComp = useMemo<ChatChartSpec | null>(() => {
    if (!expanded || !report) return null;
    return buildComponentSpec(report, scopeId, undefined, moduleRef ? moduleRef.id : null, concepts, stack?.apps ?? null, "full");
  }, [expanded, report, scopeId, moduleRef, concepts, stack]);

  const dataflowSpec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    return buildDataFlowSpec(report, scopeId);
  }, [report, scopeId]);

  // ---- hierarchy: the concept tree itself. Centred on whatever's selected
  // anywhere in the atlas (focusConceptId) — same "every panel re-derives
  // from the one Selection Context" rule as every other panel, just reading
  // the concept join instead of the container string. ----
  const conceptSpec = useMemo<ChatChartSpec | null>(() => {
    if (!concepts) return null;
    return buildConceptTreeSpec(concepts, focusConceptId);
  }, [concepts, focusConceptId]);

  // ---- entry-point tables — the tech-stack artifact's own mains/routes,
  // scoped to the selected container like every other panel. Neither table
  // invents anything: a row exists only because cie-stack parsed it from the
  // build files / route annotations as written. ----
  const appEntryRows: MainSite[] = useMemo(() => {
    if (!stack) return [];
    return stack.mains.filter((m) => !scopeId || containerOfPath(m.rel) === scopeId);
  }, [stack, scopeId]);
  const svcEntryRows: RouteSite[] = useMemo(() => {
    if (!stack) return [];
    return stack.routes.filter((r) => !scopeId || containerOfPath(r.rel) === scopeId);
  }, [stack, scopeId]);

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
  // A concept has no single qual_name of its own — walk its member symbols
  // (most-called-into first, the same "likely entry" heuristic computeTopFlows
  // uses elsewhere) and trace from whichever one the engine actually
  // recorded calls for, so picking a concept auto-traces ITS flow instead of
  // falling through to the scope's generic top flow.
  const conceptEntryQual = useMemo(() => {
    if (!report || !conceptRef?.symbolIds?.length) return null;
    const g = graphOf(report);
    const members = conceptRef.symbolIds
      .map((id) => report.symbols.find((s) => Number(s.id) === id))
      .filter((s): s is SymbolInfo => !!s);
    if (!members.length) return null;
    const withCalls = members.find((s) => (g.calleesOf.get(Number(s.id))?.length ?? 0) > 0) ?? members[0];
    return withCalls.qual_name || withCalls.name;
  }, [report, conceptRef]);
  const seqTrace = useMemo(() => {
    if (!report) return null;
    const qual = symbolRef?.id ?? conceptEntryQual ?? picked?.label ?? autoFlow?.label;
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
  // ---- RFC-0024 stage 3: the verb-composed click path (§3 vocabulary).
  // POPUP first (ux-spec §5 retirement: jump-to-source is REPLACED — the
  // popup is the preview, the full panel is its Enter/button path); SCOPE
  // per the plan's drill/filter row; the unresolved-names bucket stays
  // evidence, never a scope (G2); the grouped-out box opens the G1
  // disclosure popup (the ledger rows), never a scope.
  const [popup, setPopup] = useState<PopupRequest | null>(null);
  const [disclosure, setDisclosure] = useState<{ title: string; rows: NonNullable<ChatChartSpec["hidden"]> } | null>(null);
  const openPopup = (loc: ElementLoc | undefined | null, label: string, kind?: string, certainty?: PopupRequest["certainty"]) => {
    if (!loc?.file) return;
    // §2.3 — CHIP is automatic on the click that resolves an identity; the
    // popup merely displays that a chip was added and offers removal.
    const chipId = `${kind ?? "el"}:${label}:${loc.file}`;
    addChip({ id: chipId, label, kind: kind ?? "element", revision: report?.rev ?? 0, files: [loc.file], loc: { file: loc.file, byteStart: loc.byteStart, byteEnd: loc.byteEnd, line: loc.line }, sourceTitle: "Atlas" });
    setPopup({
      primary: { file: loc.file, byteStart: loc.byteStart, byteEnd: loc.byteEnd, line: loc.line },
      label,
      kind,
      certainty: certainty ?? "Fact",
      chipped: true,
      onToggleChip: () => {
        removeChip(chipId);
        setPopup(null);
      },
      onAsk: () => {
        setPopup(null);
        setPendingAsk(`what does ${label} do, and what evidence supports that?`);
      },
    });
  };
  const openLedger = (title: string, spec: ChatChartSpec | null | undefined) => {
    const rows = spec?.hidden ?? [];
    if (rows.length) setDisclosure({ title, rows });
  };
  const onContainersClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (sel.id.startsWith("…")) {
      openLedger("Grouped-out containers", shownC4);
      return; // G1: the grouped-out box is a ledger, not a scope
    }
    // the chip itself is setAtlasSelection's (one chip per selection — a
    // second addChip here used to double-chip under a different id)
    setAtlasSelection({ kind: "container", id: sel.id, label: sel.id });
  };
  const onComponentsClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (sel.id.startsWith("…")) {
      openLedger("Grouped-out modules", shownComp);
      return;
    }
    if (!report) return;
    const isFile = report.files.includes(sel.id);
    if (isFile) {
      setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
      openPopup(sel.loc, sel.label, "module"); // POPUP; SCOPE above
    } else {
      // a symbol box (module zoom) — select the symbol, sequence follows
      const sym = symbolByQualName(report, sel.id);
      setAtlasSelection({ kind: "symbol", id: sel.id, label: sel.label, loc: sel.loc ?? locOfSym(report, sym) });
      openPopup(sel.loc ?? locOfSym(report, sym), sel.label, "symbol");
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
      openPopup(sel.loc, sel.label, "module");
      return;
    }
    return; // the unresolved bucket is evidence, not a scope
  };
  // Sequence and Flow are OUTPUTS of the Selection Context, never inputs to
  // it: every other panel's click can re-scope them (via setAtlasSelection
  // above), but a click inside Sequence or Flow itself only opens the code
  // at that exact spot — it never re-levels or re-centres either chart. That
  // keeps the redraw direction one-way: tables/Containers/Components/Data
  // flow drive Sequence+Flow, not the other way round.
  const onSequenceClick = (sel: DiagramSelection) => {
    if (sel.kind !== "msg") return;
    if (!report) return;
    const qual = sel.id.replace(/\(\)$/, "");
    const sym = symbolByQualName(report, qual);
    openPopup(sel.loc ?? locOfSym(report, sym), qual, "call");
  };
  const onHierarchyClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (!report || !concepts) return;
    const node = concepts.nodes.find((n) => String(n.id) === sel.id || `c${n.id}` === sel.id);
    if (!node) return;
    const symbolIds = node.symbols ?? [];
    // Anchor the concept's loc at a symbol that actually sits in its
    // MAJORITY container (same container conceptResolve/scopeId pick) —
    // not just symbolIds[0], which can land in a different container than
    // the one every other panel scopes to and desync the breadcrumb from
    // the "every other panel shows X only" banner.
    const resolved = conceptResolve(report, { kind: "concept", id: String(node.id), label: node.label, symbolIds }, stack?.apps ?? null);
    const primaryContainer = resolved.containers[0] ?? null;
    const anchorFile = primaryContainer ? resolved.files.find((f) => containerOfPath(f, stack?.apps ?? null) === primaryContainer) : resolved.files[0];
    const anchorSym = anchorFile ? symbolIds.map((id) => report.symbols.find((s) => Number(s.id) === id)).find((s) => s && report.files[s.loc.file] === anchorFile) : null;
    const loc = anchorSym ? locOfSym(report, anchorSym) : undefined;
    setAtlasSelection({ kind: "concept", id: String(node.id), label: node.label, symbolIds, loc });
    addChip({ id: `concept:${node.id}`, label: node.label, kind: "concept", revision: report.rev ?? 0, files: loc?.file ? [loc.file] : [], loc, sourceTitle: "Atlas" });
  };
  const onFlowClick = (sel: DiagramSelection) => {
    if (sel.kind !== "node") return;
    if (!report) return;
    if (sel.id.startsWith("…") || sel.id.startsWith("fl-")) {
      openPopup(sel.loc, sel.label, "flow step");
      return;
    }
    const sym = symbolByQualName(report, sel.id);
    openPopup(sel.loc ?? (sym ? locOfSym(report, sym) : undefined), sel.label, "flow step");
  };
  // entry-point rows: always open the exact source line; when the engine's
  // own symbol index carries a matching function IN THE SAME FILE (never
  // guessed across files) also select it, so Sequence/Flow re-centre on it.
  const onPickMain = (m: MainSite) => {
    addChip({id:`main:${m.rel}:${m.line}`,label:m.name,kind:"entry",revision:report?.rev??0,loc:{file:m.rel,line:m.line},files:[m.rel]});
    openSourceViewer({ file: m.rel, line: m.line });
    if (!report) return;
    const sym = [m.name, `${m.name}.main`, "main"]
      .map((q) => symbolByQualName(report, q))
      .find((s): s is SymbolInfo => !!s && (report.files[s.loc.file] ?? "") === m.rel);
    if (sym) {
      const qual = sym.qual_name || sym.name;
      setAtlasSelection({ kind: "symbol", id: qual, label: sym.name, loc: { file: m.rel, byteStart: sym.loc.start, byteEnd: sym.loc.end } });
    }
  };
  const onPickRoute = (r: RouteSite) => {
    addChip({id:`route:${r.rel}:${r.line}`,label:r.method,kind:"route",revision:report?.rev??0,loc:{file:r.rel,line:r.line},files:[r.rel]});
    openSourceViewer({ file: r.rel, line: r.line });
    if (!report) return;
    const sym = symbolByQualName(report, `${r.owner}.${r.method}`);
    if (sym && (report.files[sym.loc.file] ?? "") === r.rel) {
      const qual = sym.qual_name || sym.name;
      setAtlasSelection({ kind: "symbol", id: qual, label: sym.name, loc: { file: r.rel, byteStart: sym.loc.start, byteEnd: sym.loc.end } });
    }
  };

  const crumbs = breadcrumbOf(atlasSelection);
  const shown = (s: ChatChartSpec | null): ChatChartSpec | null => s; // gate/conductor owns Facts-only and its ledger
  const shownC4 = shown(c4Spec);
  const shownComp = shown(compSpec);
  const shownFlow = shown(dataflowSpec);
  const shownHierarchy = shown(conceptSpec);
  const shownSeq = shown(seqSpec);
  const shownFlowChart = shown(flowBuild?.spec ?? null);
  // expanded-panel variants (full detail), same Facts-only filter as the cards
  const shownFullC4 = shown(fullC4);
  const shownFullComp = shown(fullComp);

  if (!report) {
    return (
      <div className="p-6 w-full">
        <Card><CardContent className="p-6 text-sm text-stone-500">Load a project first — the atlas reads the same exports every other page uses.</CardContent></Card>
      </div>
    );
  }

  const expandedPanel = PANELS.find((p) => p.id === expanded);
  // `full` = the expanded panel draws the FULL-detail spec (every module
  // and component, no “…and N more” grouping below the renderer guard)
  const body = (id: PanelId, full = false): React.ReactNode => {
    if (id === "containers") {
      const s = full ? (shownFullC4 ?? shownC4) : shownC4;
      return s ? <ChatChart spec={s} onSelectElement={onContainersClick} /> : <HonestEmpty scope={null} what="top-level groups to draw" />;
    }
    if (id === "appentry")
      return stack ? (
        appEntryRows.length ? <AppEntryTable rows={appEntryRows} onPick={onPickMain} /> : <HonestEmpty scope={scopeId} what="application entry point — a main()/boot class the tech-stack pass could name" />
      ) : (
        <p className="text-xs text-stone-500 py-6 text-center">No tech-stack report loaded for this project — nothing to table yet.</p>
      );
    if (id === "svcentry")
      return stack ? (
        svcEntryRows.length ? <ServiceEntryTable rows={svcEntryRows} onPick={onPickRoute} /> : <HonestEmpty scope={scopeId} what="service entry point — an HTTP route the tech-stack pass parsed from controller/router annotations" />
      ) : (
        <p className="text-xs text-stone-500 py-6 text-center">No tech-stack report loaded for this project — nothing to table yet.</p>
      );
    if (id === "components") {
      const s = full ? (shownFullComp ?? shownComp) : shownComp;
      return s ? <ChatChart spec={s} onSelectElement={onComponentsClick} /> : (
        <HonestEmpty scope={scopeId} what="modules with named things" />
      );
    }
    if (id === "dataflow")
      return shownFlow ? <ChatChart spec={shownFlow} onSelectElement={onDataflowClick} /> : (
        <HonestEmpty scope={scopeId} what="recorded reads and writes" />
      );
    if (id === "hierarchy")
      return shownHierarchy ? <ChatChart spec={shownHierarchy} onSelectElement={onHierarchyClick} /> : (
        <HonestEmpty scope={scopeId} what="a concept tree (needs concepts.json)" />
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
                scopedTo={expandedPanel.id === "containers" || expandedPanel.id === "hierarchy" ? null : scopeId}
                expanded
                onToggle={() => setExpanded(null)}
              >
                {body(expandedPanel.id, true)}
              </Panel>
            </div>
          ) : (
            <>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
              {[plan.hero, ...plan.rows].map((row) => {
                const meta = PANELS.find((p) => p.id === row.panel);
                if (!meta) return null;
                const wide = row.panel === "containers" || row.panel === "flow" || row.panel === "hierarchy";
                const panel = (
                  <Panel
                    id={meta.id}
                    icon={meta.icon}
                    label={meta.label}
                    scopeNote={
                      row.panel === "flow" && flowBuild
                        ? `level: ${flowBuild.label}`
                        : row.panel === "hierarchy"
                          ? (focusConceptId != null ? "focused on the current selection" : row.why || null)
                          : row.why || null
                    }
                    scopedTo={row.panel === "containers" || row.panel === "flow" || row.panel === "hierarchy" ? null : scopeId}
                    onToggle={() => setExpanded(meta.id)}
                  >
                    {body(meta.id)}
                  </Panel>
                );
                return wide ? (
                  <div key={row.panel} className="lg:col-span-2">{panel}</div>
                ) : (
                  <div key={row.panel}>{panel}</div>
                );
              })}
              {hasCov && (
                <QualityPanel icon={ShieldCheck} label="Test coverage" scopedTo={scopeId} spec={covSpec} scopeWhat="measured coverage rows" />
              )}
              {hasCplx && <QualityPanel icon={Gauge} label="Complexity" scopedTo={scopeId} spec={cplxSpec} scopeWhat="measured functions" />}
              {hasChurn && <QualityPanel icon={GitCommitHorizontal} label="Churn per file" scopedTo={scopeId} spec={churnSpec} scopeWhat="files with churn" />}
              {hasHot && <QualityPanel icon={Flame} label="Hot spots" scopedTo={scopeId} spec={hotSpec} scopeWhat="ranked files" />}
            </div>
            </>
          )}
          {!expandedPanel && (
            <p className="mt-3 flex items-center gap-1.5 text-[10.5px] text-stone-400">
              <MousePointerClick className="h-3 w-3" aria-hidden />
              click a container above — every other panel shows only that container; click it again to release · expand any panel with the ⤢ button
            </p>
          )}
          {personaDisclosure && (
            <p className="mt-1 text-[10.5px] text-stone-400" data-testid="persona-disclosure" role="status">
              {personaDisclosure}
            </p>
          )}
        </div>
      </div>

      {/* RFC-0024 stage 3 — the popup layer floats above all regions; it
          never claims grid space (§1). POPUP replaced jump-to-source. */}
      {popup && (
        <CodePopup
          req={popup}
          onClose={() => setPopup(null)}
          onOpenFull={(loc) => {
            setPopup(null);
            openSourceViewer({
              file: loc.file,
              byteStart: loc.byteStart,
              byteEnd: loc.byteEnd,
              line: loc.line,
              elName: popup.label,
              chartTitle: "Atlas",
            });
          }}
        />
      )}

      {/* G1 — the disclosure popup: the grouped-out box opens its ledger
          list; a row click acts as its own element (POPUP when located). */}
      {disclosure && (
        <div className="fixed inset-0 z-[80] flex items-center justify-center bg-stone-900/10 p-4" role="dialog" aria-modal="true" aria-label={disclosure.title}>
          <div className="w-full max-w-[480px] max-h-[70vh] overflow-hidden rounded-xl border border-stone-200 bg-white shadow-xl flex flex-col">
            <div className="flex items-center justify-between border-b border-stone-100 px-3 py-2">
              <p className="text-xs font-semibold text-stone-900">{disclosure.title}</p>
              <button type="button" onClick={() => setDisclosure(null)} aria-label="Close (Esc)" className="rounded p-1 text-stone-400 hover:bg-stone-100">✕</button>
            </div>
            <div className="overflow-auto p-1.5">
              {disclosure.rows.map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => {
                    setDisclosure(null);
                    if (r.loc?.file) {
                      openPopup(r.loc, r.label, r.kind, "Fact");
                    } else {
                      setPendingAsk(`what is ${r.label} in this project?`);
                    }
                  }}
                  className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left hover:bg-stone-50"
                >
                  <span className="rounded bg-stone-100 px-1 py-0.5 text-[9px] font-semibold text-stone-500">{r.kind}</span>
                  <span className="min-w-0 flex-1 truncate text-xs text-stone-800">{r.label}</span>
                  <span className="text-[10px] text-stone-400">{r.reason}</span>
                </button>
              ))}
            </div>
            <p className="border-t border-stone-100 px-3 py-1.5 text-[10px] text-stone-400">
              {disclosure.rows.length} row(s) grouped out of the summary card — grouped out of the picture, not hidden; each row acts as its own element.
            </p>
          </div>
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
    <Card className={`border-stone-200 shadow-sm ${expanded ? "" : "max-h-[520px] lg:max-h-[46vh] overflow-hidden flex flex-col"}`} data-testid={`atlas-panel-${testId ?? id}`}>
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
// Entry-point tables — the tech-stack artifact's mains/routes (PRD-adjacent:
// "where does this thing start"). Read straight from cie-stack's own export,
// never re-derived — a row here is a claim cie-stack already made elsewhere
// (the Mastery journey's mains graph / controllers table), just filtered to
// the atlas's current scope.
// ---------------------------------------------------------------------------

function AppEntryTable({ rows, onPick }: { rows: MainSite[]; onPick: (m: MainSite) => void }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[12px]">
        <thead className="text-[10px] text-stone-400 text-left border-b border-stone-100">
          <tr>
            <th className="py-1.5 px-3 font-bold">Entry point</th>
            <th className="py-1.5 px-3 font-bold">Kind</th>
            <th className="py-1.5 px-3 font-bold">Lang</th>
            <th className="py-1.5 px-3 font-bold">Where</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((m, i) => (
            <tr key={`${m.rel}-${m.name}-${i}`} className="border-b border-stone-50 hover:bg-emerald-50/40 cursor-pointer" onClick={() => onPick(m)}>
              <td className="py-2 px-3 font-mono font-semibold text-stone-800">{m.name}</td>
              <td className="py-2 px-3 text-stone-600">{m.kind}</td>
              <td className="py-2 px-3 text-stone-500">{m.lang}</td>
              <td className="py-2 px-3 font-mono text-[11px] text-emerald-800">{m.rel.split("/").pop()}:{m.line}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="px-3 pb-2 pt-1 text-[10.5px] text-stone-400">
        {rows.length} entry point(s) — named in the build files or the engine's own index, never executed to confirm; click a row to open the code (and re-centre Sequence/Flow on it, when the engine can match it to a symbol).
      </p>
    </div>
  );
}

function SVC_VERB_CLASS(verb: string): string {
  switch (verb) {
    case "GET": return "bg-sky-100 text-sky-800 border border-sky-200";
    case "POST": return "bg-emerald-100 text-emerald-800 border border-emerald-200";
    case "PUT": return "bg-amber-100 text-amber-800 border border-amber-200";
    case "DELETE": return "bg-rose-100 text-rose-800 border border-rose-200";
    case "PATCH": return "bg-violet-100 text-violet-800 border border-violet-200";
    default: return "bg-stone-100 text-stone-700 border border-stone-200";
  }
}

function ServiceEntryTable({ rows, onPick }: { rows: RouteSite[]; onPick: (r: RouteSite) => void }) {
  const byOwner = new Map<string, RouteSite[]>();
  const order: string[] = [];
  for (const r of rows) {
    if (!byOwner.has(r.owner)) {
      byOwner.set(r.owner, []);
      order.push(r.owner);
    }
    byOwner.get(r.owner)!.push(r);
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[12px]">
        <thead className="text-[10px] text-stone-400 text-left border-b border-stone-100">
          <tr>
            <th className="py-1.5 px-3 font-bold">Controller / router</th>
            <th className="py-1.5 px-3 font-bold">Routes (as written)</th>
          </tr>
        </thead>
        <tbody>
          {order.map((owner) => {
            const rs = byOwner.get(owner)!;
            return (
              <tr key={owner} className="border-b border-stone-50 align-top">
                <td className="py-2 px-3 font-mono font-semibold text-stone-800 whitespace-nowrap">{owner}</td>
                <td className="py-2 px-3">
                  <span className="flex flex-wrap gap-1">
                    {rs.map((r, j) => (
                      <button
                        key={j}
                        type="button"
                        onClick={() => onPick(r)}
                        title={`${r.verb} ${r.path} → ${r.method}() (${r.rel.split("/").pop()}:${r.line})`}
                        className="inline-flex items-center gap-1 rounded border border-stone-200 bg-white px-1.5 py-0.5 hover:border-emerald-400 hover:bg-emerald-50"
                      >
                        <span className={`rounded px-1 py-0 text-[9.5px] font-bold ${SVC_VERB_CLASS(r.verb)}`}>{r.verb}</span>
                        <span className="font-mono text-[10.5px] text-stone-700">{r.path}</span>
                      </button>
                    ))}
                  </span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="px-3 pb-2 pt-1 text-[10.5px] text-stone-400">
        {order.length} controller/router(s), {rows.length} route(s) — read from route annotations as written; click a route to open its handler (and re-centre Sequence/Flow on it, when the engine can match it to a symbol).
      </p>
    </div>
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
