"use client";

// components/cie/atlas-view.tsx — the multi-view dashboard ("Atlas").
//
// One Selection Context (lib/atlas.ts, kept in cie-store), four synchronized
// views, an always-honest evidence story:
//
//   Containers  — C4 L2: the repo's own top-level grouping, calls across
//   Components  — C4 L3: modules of the scoped container, then its symbols
//   Sequence    — time-ordered calls for the selected flow or symbol
//   Data flow   — recorded reads/writes against stores the code names
//
// Every picture is built from the loaded engine exports through the same
// ChatChart engines the chat uses (DiagramSVG / SequenceSVG) — no second
// rendering stack, no invented geometry. A click anywhere lands in the
// Selection Context; the other views re-derive from it. Elements the engine
// could not pin down stay visible but marked (dashed / "inferred"), and the
// Facts-only switch filters the whole dashboard at once.

import { useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ChevronRight, Home, Layers, Route, Share2, Boxes, MousePointerClick, ListTree } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import {
  breadcrumbOf,
  buildComponentSpec,
  buildDataFlowSpec,
  containerOfPath,
  symbolsOfModule,
  symbolByQualName,
  type AtlasRef,
} from "@/lib/atlas";
import { buildC4Spec, computeTopFlows, traceFlow, buildSequenceSpec, type MasteryRow } from "@/lib/mastery";
import { ChatChart, type DiagramSelection } from "@/components/cie/chat-chart";
import type { ChatChartSpec } from "@/lib/chat-chart";
import type { SymbolInfo } from "@/lib/cie-types";

const VIEWS = [
  { id: "containers", label: "Containers", icon: Boxes },
  { id: "components", label: "Components", icon: Layers },
  { id: "sequence", label: "Sequence", icon: Route },
  { id: "dataflow", label: "Data flow", icon: Share2 },
] as const;

type AtlasViewId = (typeof VIEWS)[number]["id"];

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
  const { report, concepts, atlasSelection, atlasView, atlasFactsOnly, setAtlasSelection, setAtlasView, setAtlasFactsOnly, drillUp, resetAtlas } = useCie();
  const [pickedFlow, setPickedFlow] = useState<{ entryId: number; label: string } | null>(null);

  const primary = atlasSelection.primary;
  const containerRef = primary?.kind === "container" ? primary : null;
  const moduleRef = primary?.kind === "module" ? primary : null;
  const symbolRef = primary?.kind === "symbol" ? primary : null;
  const storeRef = primary?.kind === "store" ? primary : null;

  // ---- the four view specs, re-derived from the Selection Context ----
  const spec = useMemo<ChatChartSpec | null>(() => {
    if (!report) return null;
    if (atlasView === "containers") {
      return buildC4Spec(report, "Containers — how the repo itself is laid out");
    }
    if (atlasView === "components") {
      // scoped to a container when one is selected; zoomed into a module's
      // symbols when the selection is a module
      return buildComponentSpec(report, moduleRef ? moduleRef.id : containerRef ? containerRef.id : null);
    }
    if (atlasView === "dataflow") {
      return buildDataFlowSpec(report, containerRef ? containerRef.id : null);
    }
    return null; // sequence is built below (needs its own entry choice)
  }, [report, atlasView, containerRef, moduleRef]);

  // ---- sequence: symbol-centred trace, or the flow picker ----
  const seqTrace = useMemo(() => {
    if (!report) return null;
    const qual = symbolRef?.id ?? pickedFlow?.label;
    const entryId = qual ? (symbolByQualName(report, qual)?.id ?? pickedFlow?.entryId) : pickedFlow?.entryId;
    if (entryId == null) return null;
    const res = traceFlow(report, Number(entryId));
    if (!res) return null;
    return { res, entryLabel: qual ?? pickedFlow?.label ?? "", entryId: Number(entryId) };
  }, [report, symbolRef, pickedFlow]);

  const seqSpec = useMemo(() => {
    if (!seqTrace || !report) return null;
    return buildSequenceSpec(`Sequence — ${seqTrace.entryLabel}()`, seqTrace.res.steps, new Set<number>());
  }, [seqTrace, report]);

  // flow candidates: the engine's own entry points, scoped when a container is
  const flows: MasteryRow[] = useMemo(() => {
    if (!report) return [];
    const { rows } = computeTopFlows(report);
    const inContainer = containerRef
      ? rows.filter((r) => {
          const sym = symbolByQualName(report, r.title);
          const f = sym ? report.files[sym.loc.file] : r.file;
          return f && containerOfPath(f) === containerRef.id;
        })
      : rows;
    return inContainer.slice(0, 3);
  }, [report, containerRef]);

  // ---- click routing: one handler, view-aware, into the Selection Context ----
  const onSelectElement = (sel: DiagramSelection) => {
    if (!report) return;
    if (atlasView === "containers" && sel.kind === "node") {
      if (sel.id.startsWith("…")) return; // the grouped-out box is not a scope
      setAtlasSelection({ kind: "container", id: sel.id, label: sel.id });
      return;
    }
    if (atlasView === "components" && sel.kind === "node") {
      if (sel.id.startsWith("…")) return;
      const isFile = report.files.includes(sel.id);
      if (isFile) setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
      else {
        // a symbol box (module zoom) — select the symbol, sequence follows
        const sym = symbolByQualName(report, sel.id);
        setAtlasSelection({ kind: "symbol", id: sel.id, label: sel.label, loc: sel.loc ?? locOfSym(report, sym) });
      }
      return;
    }
    if (atlasView === "dataflow" && sel.kind === "node") {
      if (sel.id.startsWith("store:")) {
        setAtlasSelection({ kind: "store", id: sel.id, label: sel.label });
        return;
      }
      if (report.files.includes(sel.id)) {
        setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
        return;
      }
      return; // the unresolved bucket is evidence, not a scope
    }
    if (atlasView === "sequence" && sel.kind === "msg") {
      // arrow click: also select the callee so the other views re-centre
      const qual = sel.id.replace(/\(\)$/, "");
      if (symbolByQualName(report, qual)) setAtlasSelection({ kind: "symbol", id: qual, label: qual, loc: sel.loc });
    }
  };

  const crumbs = breadcrumbOf(atlasSelection);
  const shownSpec = atlasFactsOnly && spec ? filterFacts(spec) : spec;
  const shownSeq = atlasFactsOnly && seqSpec ? filterFacts(seqSpec) : seqSpec;

  if (!report) {
    return (
      <div className="p-6 w-full">
        <Card><CardContent className="p-6 text-sm text-stone-500">Load a project first — the atlas reads the same exports every other page uses.</CardContent></Card>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full w-full">
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
        {primary && (
          <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px] text-stone-500" onClick={drillUp}>
            up one level
          </Button>
        )}
        <span className="flex-1" />
        <label className="flex items-center gap-1.5 text-[11px] text-stone-500 cursor-pointer select-none">
          <Switch checked={atlasFactsOnly} onCheckedChange={setAtlasFactsOnly} aria-label="Facts only" className="scale-90" />
          Facts only
        </label>
      </div>

      {/* view tabs */}
      <div className="flex items-center gap-1 px-3 pt-2 pb-1 border-b border-stone-200/80 overflow-x-auto">
        {VIEWS.map((v) => {
          const Icon = v.icon;
          const active = atlasView === v.id;
          return (
            <button
              key={v.id}
              type="button"
              onClick={() => setAtlasView(v.id)}
              className={`h-8 rounded-lg px-3 text-[13px] font-medium inline-flex items-center gap-1.5 whitespace-nowrap transition-colors ${
                active ? "bg-emerald-50 text-emerald-900" : "text-stone-500 hover:text-stone-800 hover:bg-stone-100"
              }`}
              aria-pressed={active}
            >
              <Icon className="h-3.5 w-3.5" aria-hidden />
              {v.label}
            </button>
          );
        })}
        <span className="flex-1" />
        <span className="pr-2 text-[10px] text-stone-400 hidden sm:flex items-center gap-1">
          <MousePointerClick className="h-3 w-3" aria-hidden />
          one click re-orients every view
        </span>
      </div>

      {/* the active view */}
      <div className="flex-1 min-h-0 overflow-auto p-3 sm:p-4">
        {atlasView === "sequence" ? (
          <SequencePane
            seqSpec={shownSeq}
            flows={flows}
            picked={pickedFlow}
            traceInfo={seqTrace ? { steps: seqTrace.res.steps.length, truncated: seqTrace.res.truncated, effects: seqTrace.res.effectsSeen } : null}
            onPick={(entryId, label) => setPickedFlow({ entryId, label })}
            onClearPick={() => setPickedFlow(null)}
            hasEntry={!!(symbolRef || pickedFlow)}
          />
        ) : shownSpec ? (
          <Card className="border-stone-200 shadow-sm w-full">
            <CardContent className="p-4 sm:p-5">
              <h2 className="text-sm font-semibold text-stone-800">{shownSpec.title}</h2>
              {shownSpec.tell && <p className="text-xs text-stone-500 mt-0.5">{shownSpec.tell}</p>}
              <div className="mt-3">
                <ChatChart spec={shownSpec} onSelectElement={onSelectElement} />
              </div>
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardContent className="p-6 text-sm text-stone-500">
              {atlasView === "components" && containerRef
                ? `Nothing to draw for “${containerRef.label}” — the engine indexed no named things there. Said plainly instead of drawn anyway.`
                : "Nothing to draw here yet — this view is built from records the engine exports, and there are none for this scope."}
            </CardContent>
          </Card>
        )}

        {/* what the current selection means for the other views (PRD §6) */}
        {primary && (
          <Card className="border-stone-200 shadow-sm w-full mt-3">
            <CardContent className="p-4 text-xs text-stone-600 space-y-1">
              <p className="font-semibold text-stone-800 text-[13px] flex items-center gap-1.5">
                <ListTree className="h-3.5 w-3.5 text-emerald-700" aria-hidden />
                Because you selected {primary.label}
              </p>
              <SelectionConsequences primary={primary} moduleSymbols={moduleRef ? symbolsOfModule(report, moduleRef.id).length : null} storeRef={storeRef} />
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}

function locOfSym(report: NonNullable<ReturnType<typeof useCie.getState>["report"]>, sym: SymbolInfo | null) {
  if (!sym) return undefined;
  return { file: report.files[sym.loc.file] ?? "", byteStart: sym.loc.start, byteEnd: sym.loc.end };
}

/** The PRD's interaction model, said out loud: each click names what the
 *  other views are now showing — no hidden re-orientation. */
function SelectionConsequences({ primary, moduleSymbols, storeRef }: { primary: AtlasRef; moduleSymbols: number | null; storeRef: AtlasRef | null }) {
  const rows: string[] = [];
  if (primary.kind === "container") {
    rows.push("Components zooms into this container's own modules.");
    rows.push("Sequence offers the top entry-point flows that start here.");
    rows.push("Data flow keeps only the reads and writes this container performs.");
  } else if (primary.kind === "module") {
    rows.push(`Components shows this file's ${moduleSymbols ?? "—"} named thing(s) and their calls.`);
    rows.push("Sequence can trace any of them call by call.");
    rows.push("Data flow highlights this module's reads and writes.");
  } else if (primary.kind === "store") {
    rows.push("Data flow centres on this store — every recorded writer and reader.");
    rows.push("Components scopes to the modules that touch it.");
  } else if (primary.kind === "symbol") {
    rows.push("Sequence becomes the call walk that starts at this function.");
    rows.push("Components highlights it inside its module.");
    rows.push("Clicking the arrow again in Sequence opens the exact code.");
  }
  if (storeRef) rows.push("The store's schema file opens in the source panel from Data flow.");
  return (
    <>
      {rows.map((r, i) => (
        <p key={i} className="leading-snug">{r}</p>
      ))}
    </>
  );
}

/** Sequence pane: honest picker until something is selected. */
function SequencePane({
  seqSpec,
  flows,
  picked,
  traceInfo,
  onPick,
  onClearPick,
  hasEntry,
}: {
  seqSpec: ChatChartSpec | null;
  flows: MasteryRow[];
  picked: { entryId: number; label: string } | null;
  traceInfo: { steps: number; truncated: number; effects: string[] } | null;
  onPick: (entryId: number, label: string) => void;
  onClearPick: () => void;
  hasEntry: boolean;
}) {
  return (
    <div className="space-y-3">
      {hasEntry && seqSpec && (
        <Card className="border-stone-200 shadow-sm w-full">
          <CardContent className="p-4 sm:p-5">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <h2 className="text-sm font-semibold text-stone-800">{seqSpec.title}</h2>
              <div className="flex items-center gap-2">
                {traceInfo && (
                  <Badge variant="outline" className="text-[10px] text-stone-500 border-stone-300">
                    {traceInfo.steps} step{traceInfo.steps === 1 ? "" : "s"} drawn
                    {traceInfo.truncated > 0 ? ` · ${traceInfo.truncated} reached but capped — counted, never hidden` : ""}
                  </Badge>
                )}
                <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={onClearPick}>
                  clear
                </Button>
              </div>
            </div>
            {seqSpec.tell && <p className="text-xs text-stone-500 mt-0.5">{seqSpec.tell}</p>}
            {traceInfo && traceInfo.effects.length > 0 && (
              <p className="text-[11px] text-stone-500 mt-1">
                Effects this walk touches, as recorded: {traceInfo.effects.join(", ")}
              </p>
            )}
            <div className="mt-3">
              <ChatChart spec={seqSpec} />
            </div>
          </CardContent>
        </Card>
      )}
      {!hasEntry && (
        <Card>
          <CardContent className="p-5">
            <h2 className="text-sm font-semibold text-stone-800">Pick a flow to walk through</h2>
            <p className="text-xs text-stone-500 mt-0.5">
              Candidates ranked by how much code each start point can reach — a structural guess about importance, not a measured truth. Selecting a
              symbol anywhere in the atlas also centres this view on it.
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
                The report doesn't record who-calls-whom for this project, so there are no flow candidates to rank — said honestly rather than invented.
              </p>
            )}
          </CardContent>
        </Card>
      )}
      {hasEntry && !seqSpec && (
        <Card>
          <CardContent className="p-5 text-xs text-stone-500">
            The engine recorded no call walk for this selection — it may be a leaf, or its calls weren't resolved. Nothing is drawn rather than something
            invented.
          </CardContent>
        </Card>
      )}
    </div>
  );
}
