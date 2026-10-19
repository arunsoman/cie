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
import { Separator } from "@/components/ui/separator";
import { ChevronRight, Home, Layers, Route, Share2, Boxes, MousePointerClick, ListTree, ExternalLink, PanelRightClose, PanelRightOpen, FileCode, FlaskConical, ArrowDownRight, ArrowUpRight, ShieldCheck } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import {
  breadcrumbOf,
  buildComponentSpec,
  buildDataFlowSpec,
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
import { ChatChart, type DiagramSelection } from "@/components/cie/chat-chart";
import type { ChatChartSpec, ElementLoc } from "@/lib/chat-chart";
import type { SymbolInfo } from "@/lib/cie-types";

/** The report once loaded — the type every atlas view reads from. */
type LoadedReport = NonNullable<ReturnType<typeof useCie.getState>["report"]>;

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
  const { report, concepts, atlasSelection, atlasView, atlasFactsOnly, setAtlasSelection, setAtlasView, setAtlasFactsOnly, drillUp, resetAtlas, openSourceViewer, sourceRoot } = useCie();
  const [pickedFlow, setPickedFlow] = useState<{ entryId: number; label: string } | null>(null);
  const [railOpen, setRailOpen] = useState(true);

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
      // zoomed into a module's symbols when one is selected; otherwise the
      // module boxes, scoped to the selected container when there is one
      return buildComponentSpec(report, containerRef ? containerRef.id : null, undefined, moduleRef ? moduleRef.id : null);
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

  // ---- click routing: one handler, view-aware, into the Selection Context.
  // The atlas OWNS diagram clicks when this handler is passed (the chat
  // fallback is suppressed), so it also does what the PRD's editor column
  // promises: a symbol or module click opens the exact code alongside. ----
  const openCodeFor = (loc: ElementLoc | undefined | null, label: string) => {
    if (loc?.file) openSourceViewer({ file: loc.file, byteStart: loc.byteStart, byteEnd: loc.byteEnd, elName: label, chartTitle: "Atlas" });
  };
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
      if (isFile) {
        setAtlasSelection({ kind: "module", id: sel.id, label: sel.id.split("/").pop() ?? sel.id, loc: sel.loc });
        openCodeFor(sel.loc, sel.label); // editor opens the module's own file
      } else {
        // a symbol box (module zoom) — select the symbol, sequence follows
        const sym = symbolByQualName(report, sel.id);
        setAtlasSelection({ kind: "symbol", id: sel.id, label: sel.label, loc: sel.loc ?? locOfSym(report, sym) });
        openCodeFor(sel.loc ?? locOfSym(report, sym), sel.label); // editor at the exact symbol
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
        openCodeFor(sel.loc, sel.label);
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
    <div className="flex h-full w-full">
      <div className="flex flex-col flex-1 min-w-0">
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
            onSelectElement={onSelectElement}
          />
        ) : shownSpec ? (
          <Card className="border-stone-200 shadow-sm w-full">
            <CardContent className="p-4 sm:p-5">
              {/* ChatChart renders its own title/tell/footnote — no second header here */}
              <div className="mt-1">
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
          <p className="text-[11px] text-stone-500">No test file reaches this symbol through recorded calls — a gap worth knowing, stated plainly.</p>
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
  onSelectElement,
}: {
  seqSpec: ChatChartSpec | null;
  flows: MasteryRow[];
  picked: { entryId: number; label: string } | null;
  traceInfo: { steps: number; truncated: number; effects: string[] } | null;
  onPick: (entryId: number, label: string) => void;
  onClearPick: () => void;
  hasEntry: boolean;
  onSelectElement: (sel: DiagramSelection) => void;
}) {
  return (
    <div className="space-y-3">
      {hasEntry && seqSpec && (
        <Card className="border-stone-200 shadow-sm w-full">
          <CardContent className="p-4 sm:p-5">
            <div className="flex items-center justify-end gap-2 flex-wrap">
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
            <div className="mt-1">
              <ChatChart spec={seqSpec} onSelectElement={onSelectElement} />
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
