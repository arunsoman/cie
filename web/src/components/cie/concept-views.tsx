"use client";

// Usecase-first (PRD v2.0 Phase C) — the stories the engine induced, and
// the two diagrams each story can draw.
//
// C1: ConceptDiagramCanvas renders a B3 ViewSpec (conceptToFlowchartSpec /
// conceptToSequenceSpec output) through the same negotiation every other
// canvas view uses — negotiator picks the engine, the conductor wires
// selection, a click opens the source panel at exactly the cited bytes. A
// spec the gate refused is a NAMED result: the clause prints, nothing
// renders. C3: UsecasesView reads the engine's own usecase_view projection
// (A5, precomputed on the artifact — the web layer re-derives nothing) and
// offers each story's flowchart/sequence.
//
// Honesty laws carried over: membership is 100% the engine's recorded
// extent (never labels); no call edges is a stated gap, not an invented
// picture; a projection-less artifact (analyzed before A5) says so and
// still offers the diagrams, which need only (concept, report).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  SelectionConductor,
  defaultRegistry,
  planSheet,
  MermaidAdapter,
} from "viz-adapter";
import type { RenderHandle, ViewSpec } from "viz-adapter";
import { useCie } from "@/lib/cie-store";
import { certaintyPlain } from "@/lib/plain-words";
import {
  conceptToFlowchartSpec,
  conceptToSequenceSpec,
  type ConceptDiagramKind,
} from "@/lib/viz/concept-diagrams";
import type { CieReport, ConceptTree } from "@/lib/cie-types";
import { Network, Split, Workflow, ArrowUpRight } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const conductor = new SelectionConductor();

/** Everything the canvas needs to render one concept diagram — the mapped
 *  ViewSpec plus the words that say what it is. Null when the loaded
 *  artifacts cannot produce it (no concepts, no report, unknown id). */
export interface ConceptDiagram {
  conceptId: number;
  kind: ConceptDiagramKind;
  title: string;
  hint: string;
  view: ViewSpec | null; // null = the gate refused; violations say why
  violations: string[];
}

/** Derive one concept diagram from the loaded artifacts. Pure: same inputs
 *  → same ViewSpec (the mappers are byte-deterministic, pinned in the B3
 *  suite). The kind picks the mapper; the words name the story. */
export function conceptDiagramFor(
  conceptId: number,
  kind: ConceptDiagramKind,
  concepts: ConceptTree | null,
  report: CieReport | null,
): ConceptDiagram | null {
  if (!concepts || !report) return null;
  const concept = concepts.nodes.find((n) => n.id === conceptId);
  if (!concept) return null;
  const { view, violations } =
    kind === "flowchart"
      ? conceptToFlowchartSpec(concept, report)
      : conceptToSequenceSpec(concept, report);
  const label = concept.label;
  return {
    conceptId,
    kind,
    title: kind === "flowchart" ? `Flowchart — ${label}` : `Sequence — ${label}`,
    hint:
      kind === "flowchart"
        ? `The usecase "${label}" as a call graph — nodes are its recorded members, edges the calls the engine resolved between them.`
        : `The story "${label}" as cross-class messages, ordered from its entry points outward.`,
    view,
    violations,
  };
}

/** The gated canvas for one concept diagram — the concept twin of the
 *  named-diagram ArtifactCanvas, pointed at a ViewSpec the B3 mapper
 *  already gated. Exported `ConceptOnCanvas` is the chat-opened big
 *  version; `UsecasesView` embeds the section-sized one. */
export function ConceptDiagramCanvas({
  diagram,
  big = false,
}: {
  diagram: ConceptDiagram;
  big?: boolean;
}) {
  const { report, addChip, openSourceViewer } = useCie();
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<RenderHandle | null>(null);
  const [note, setNote] = useState("Negotiating canvas…");
  const [failure, setFailure] = useState(false);
  const [layout, setLayout] = useState<"auto" | "elk-layered">("auto");

  const view = useMemo(
    () =>
      diagram.view && layout === "auto"
        ? diagram.view
        : diagram.view
          ? { ...diagram.view, layoutHint: { strategy: layout } }
          : null,
    [diagram, layout],
  );

  // element/edge lookup for the click → source-panel path (both carry
  // byte-span evidence on the spec itself)
  const byId = useMemo(() => {
    const m = new Map<string, { label: string; file: string; start: number; end: number }>();
    const v = diagram.view;
    if (!v) return m;
    for (const el of v.elements)
      m.set(el.id, {
        label: el.label,
        file: el.evidence.file,
        start: el.evidence.start,
        end: el.evidence.end,
      });
    v.edges.forEach((ed, i) =>
      m.set(ed.id, {
        label: `${ed.source} → ${ed.target} (#${i + 1})`,
        file: ed.evidence?.file ?? "",
        start: ed.evidence?.start ?? 0,
        end: ed.evidence?.end ?? 0,
      }),
    );
    return m;
  }, [diagram]);

  const pick = useCallback(
    (id: string) => {
      const rec = byId.get(id);
      if (!rec || !rec.file) return;
      addChip({
        id,
        label: rec.label,
        kind: "concept",
        revision: report?.rev ?? 0,
        loc: { file: rec.file, byteStart: rec.start, byteEnd: rec.end },
        files: [rec.file],
      });
      openSourceViewer({
        file: rec.file,
        byteStart: rec.start,
        byteEnd: rec.end,
        elName: rec.label,
        chartTitle: diagram.title,
      });
    },
    [byId, addChip, openSourceViewer, report?.rev, diagram.title],
  );

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    if (!view) {
      setFailure(true);
      setNote(`Gate declined: ${diagram.violations.join("; ")}`);
      return;
    }
    if (!view.elements.length) {
      setFailure(false);
      setNote("Nothing nameable in this story — the honest answer for this usecase.");
      return;
    }
    let cancelled = false;
    let owned: RenderHandle | undefined;
    setFailure(false);
    void (async () => {
      try {
        const plans = planSheet(view, defaultRegistry());
        const { renderViaNegotiator } = await import("@/lib/viz/g6-engine");
        const result = await renderViaNegotiator(container, view);
        if (cancelled) {
          await result.handle.dispose();
          return;
        }
        owned = result.handle;
        handle.current = owned;
        setNote(
          `${result.disclosure.headline} · layout ${owned.layout.actual} (${owned.layout.fidelity}) · ${plans.length} eligible sheet views`,
        );
        const off = conductor.attach(
          owned,
          [...view.elements, ...view.edges].map((e) => e.id),
          (ids) => {
            const id = ids.at(-1);
            if (id) pick(id);
          },
        );
        // chips already in the context stay ringed
        owned.setSelected(
          useCie.getState().chatContext.map((c) => c.id),
          "programmatic",
        );
        return off;
      } catch (err) {
        if (!cancelled) {
          setFailure(true);
          setNote(`Engine declined: ${err instanceof Error ? err.message : String(err)}`);
        }
      }
    })();
    return () => {
      cancelled = true;
      if (handle.current === owned) handle.current = null;
      void owned?.dispose();
    };
  }, [view, pick, diagram.violations]);

  useEffect(() => {
    conductor.select(useCie.getState().chatContext.map((c) => c.id));
  }, []);

  const exportPng = async () => {
    if (!handle.current) return;
    const { exportCanvas } = await import("@/lib/viz/g6-engine");
    const url = await exportCanvas(handle.current);
    const a = document.createElement("a");
    a.href = url;
    a.download = `cie-${diagram.kind}-${diagram.conceptId}.png`;
    a.click();
  };

  const f = view?.footer;
  const gaps = diagram.view?.gaps ?? [];
  const caption = typeof f?.caption === "string" ? f.caption : "";

  return (
    <div data-concept-diagram={diagram.kind}>
      <p className="text-sm font-semibold text-stone-900">{diagram.title}</p>
      <p className="text-xs text-stone-500">{diagram.hint}</p>
      {view && !!view.elements.length && (
        <div
          ref={host}
          className={`mt-2 w-full rounded-lg border border-stone-200 bg-white ${big ? "h-[640px]" : "h-[440px]"}`}
          role="img"
          aria-label={`${diagram.title} canvas`}
        />
      )}
      {(!view || !view.elements.length) && <div ref={host} />}
      <p role={failure ? "alert" : undefined} className="text-[10px] text-stone-500 mt-1">
        {note}
      </p>

      {/* the epistemic footer — what this picture knows, and how sure it is */}
      {f && (
        <p className="text-[11px] text-stone-500 mt-1.5 inline-flex items-center gap-x-3 flex-wrap">
          <span className="font-medium text-stone-600">How sure:</span>
          <span>
            {f.facts} {certaintyPlain("Fact").label.toLowerCase()}
          </span>
          <span aria-hidden>·</span>
          <span>
            {f.inferences} {certaintyPlain("Inference").label.toLowerCase()}
          </span>
          <span aria-hidden>·</span>
          <span>
            {f.hypotheses} {certaintyPlain("Hypothesis").label.toLowerCase()}
          </span>
          {f.fog > 0 && (
            <>
              <span aria-hidden>·</span>
              <span>
                {f.fog} {certaintyPlain("Fog").label.toLowerCase()}
              </span>
            </>
          )}
        </p>
      )}
      {caption && <p className="text-[11px] text-stone-400 mt-0.5">&ldquo;{caption}&rdquo;</p>}

      {/* honesty disclosures: the no-internal-calls gap, skipped members, suppression notes */}
      {gaps.length > 0 && (
        <details className="text-xs mt-2">
          <summary>
            Disclosure · {gaps.length} counted note{gaps.length === 1 ? "" : "s"}
          </summary>
          <ul className="mt-1 space-y-1">
            {gaps.map((g) => (
              <li key={g.id} className="text-stone-600">
                {g.why}
              </li>
            ))}
          </ul>
        </details>
      )}

      {view && !!view.elements.length && !failure && (
        <p className="mt-2 space-x-3">
          <button
            type="button"
            className="text-xs underline text-stone-600 hover:text-stone-900"
            onClick={() => {
              const out = new MermaidAdapter().adapt(view);
              const url = URL.createObjectURL(new Blob([out.text], { type: "text/plain" }));
              const a = document.createElement("a");
              a.href = url;
              a.download = `cie-${diagram.kind}-${diagram.conceptId}.mmd`;
              a.click();
              setTimeout(() => URL.revokeObjectURL(url), 1000);
            }}
          >
            Export Mermaid
          </button>
          <button
            type="button"
            className="text-xs underline text-stone-600 hover:text-stone-900"
            onClick={() => void exportPng()}
          >
            Export PNG
          </button>
          <label className="text-xs text-stone-600">
            Layout{" "}
            <select
              aria-label="Diagram layout"
              value={layout}
              onChange={(e) => setLayout(e.target.value as "auto" | "elk-layered")}
            >
              <option value="auto">Automatic</option>
              <option value="elk-layered">ELK layered</option>
            </select>
          </label>
        </p>
      )}

      {/* the text outline stays authoritative — same law as every chart */}
      <details className="text-xs mt-2">
        <summary>Text outline · {view?.elements.length ?? 0} named elements</summary>
        <ul aria-label="Concept diagram elements" className="max-h-64 overflow-auto">
          {(view?.elements ?? []).map((e) => (
            <li key={e.id}>
              <button
                type="button"
                className="text-left p-1 hover:underline"
                onClick={() => pick(e.id)}
                aria-pressed={useCie.getState().chatContext.some((c) => c.id === e.id)}
              >
                {e.label}
                <span className="text-stone-400"> · {e.elementKind}</span>
                {e.fields?.entry ? <span> · entry</span> : null}
                {e.fields?.isolate ? <span> · acts alone</span> : null}
              </button>
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}

/** A usecase diagram the chat (or the section) asked for — the big, quiet
 *  version on the stage canvas. */
export function ConceptOnCanvas() {
  const { canvasConcept, concepts, report, closeCanvasChart } = useCie();
  const diagram = useMemo(
    () =>
      canvasConcept
        ? conceptDiagramFor(canvasConcept.conceptId, canvasConcept.kind, concepts, report)
        : null,
    [canvasConcept, concepts, report],
  );
  if (!canvasConcept || !diagram) return null;
  return (
    <div className="p-4 sm:p-6 w-full max-w-5xl">
      <Card className="border-stone-200 shadow-sm w-full">
        <CardContent className="p-5 w-full">
          <ConceptDiagramCanvas diagram={diagram} big />
        </CardContent>
      </Card>
      <button
        type="button"
        onClick={closeCanvasChart}
        className="mt-3 text-xs text-stone-500 hover:text-stone-800 underline"
      >
        Close this diagram and go back to Start here
      </button>
    </div>
  );
}

// ---------- C3: the usecase-first section ----------

function chainWords(chain: { id: number; label: string; kind: string }[]): string {
  if (chain.length <= 1) return "a top-level story (no broader usecase under it)";
  return `part of ${chain
    .slice(0, -1)
    .map((l) => `“${l.label}”`)
    .join(" → ")}`;
}

export function UsecasesView() {
  const { report, concepts, openConceptOnCanvas, openSourceViewer } = useCie();
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const projection = report?.usecase_view ?? null;
  const usecaseNodes = useMemo(
    () => (concepts?.nodes ?? []).filter((n) => n.kind === "UseCase"),
    [concepts],
  );

  // Picker rows: the projection when present, else the UseCase nodes —
  // both keyed by concept id, so the draw buttons work either way.
  const rows = useMemo(() => {
    if (projection && projection.length > 0) {
      return [...projection]
        .sort((a, b) => a.label.localeCompare(b.label) || a.id - b.id)
        .map((u) => ({ id: u.id, label: u.label, members: u.members.length, chain: u.chain }));
    }
    return [...usecaseNodes]
      .sort((a, b) => a.label.localeCompare(b.label) || a.id - b.id)
      .map((n) => ({ id: n.id, label: n.label, members: n.symbols.length, chain: [] }));
  }, [projection, usecaseNodes]);

  // Default to a story with a real chain (the most interesting one), else
  // the first row — deterministic, and it survives project switches via
  // the effect below.
  const defaultId = useMemo(() => {
    const withChain = rows.find((r) => r.chain.length > 1);
    return withChain?.id ?? rows[0]?.id ?? null;
  }, [rows]);
  useEffect(() => {
    if (selectedId != null && !rows.some((r) => r.id === selectedId)) setSelectedId(null);
  }, [rows, selectedId]);
  const currentId = selectedId ?? defaultId;

  const record = useMemo(
    () => (projection ?? []).find((u) => u.id === currentId) ?? null,
    [projection, currentId],
  );
  const conceptNode = useMemo(
    () => usecaseNodes.find((n) => n.id === currentId) ?? null,
    [usecaseNodes, currentId],
  );

  if (!report || !concepts) return null;

  if (rows.length === 0) {
    return (
      <div className="max-w-2xl">
        <h2 className="text-lg font-semibold text-stone-900">No usecases for this project</h2>
        <p className="mt-2 text-sm text-stone-600 leading-relaxed">
          Usecases are the stories CIE induces from your functions — groups of code that share
          recorded behaviors and effects, named by what they do. This project&rsquo;s analysis named
          none, which is the honest answer for a codebase this small (or this unlike an application).
        </p>
      </div>
    );
  }

  const current = rows.find((r) => r.id === currentId) ?? rows[0];

  return (
    <div className="space-y-4" data-view="usecases">
      {/* the honest headline: what the engine induced, and from what */}
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <h2 className="text-lg font-semibold text-stone-900">Usecases</h2>
        <p className="text-xs text-stone-500">
          {rows.length} usecase{rows.length === 1 ? "" : "s"} ·{" "}
          {projection
            ? `${projection.reduce((n, u) => n + u.members.length, 0)} member function${projection.reduce((n, u) => n + u.members.length, 0) === 1 ? "" : "s"} resolved · ${projection.filter((u) => u.chain.length > 1).length} with a broader-story chain`
            : "the usecase-first projection is not in this artifact — analyzed before CIE recorded it"}
        </p>
      </div>

      <Card className="border-stone-200">
        <CardHeader className="pb-3">
          <CardTitle className="text-base">Pick a story</CardTitle>
          <CardDescription>
            Each usecase is a group of functions the engine found doing one thing together —
            membership comes from recorded evidence, never from a name guess.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <Select
            value={currentId != null ? String(currentId) : undefined}
            onValueChange={(v) => setSelectedId(Number(v))}
          >
            <SelectTrigger className="w-full sm:w-96">
              <SelectValue placeholder="Pick a usecase" />
            </SelectTrigger>
            <SelectContent>
              {rows.map((r) => (
                <SelectItem key={r.id} value={String(r.id)}>
                  {r.label} · {r.members} member{r.members === 1 ? "" : "s"}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {record && (
            <div className="space-y-2 text-sm">
              <p className="text-stone-600">
                <ArrowUpRight className="inline h-3.5 w-3.5 text-stone-400" aria-hidden />{" "}
                {chainWords(record.chain)}
              </p>
              {record.intent.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {record.intent.map((t) => (
                    <Badge
                      key={t}
                      variant="outline"
                      className="bg-emerald-50 text-emerald-800 border-emerald-200 font-normal"
                    >
                      {t}
                    </Badge>
                  ))}
                  <span className="text-xs text-stone-400 self-center">
                    confidence {record.confidence.toFixed(2)} · {record.provenance}
                    {record.coverage_complete ? "" : " · coverage incomplete — the extent may miss members the engine could not place"}
                  </span>
                </div>
              )}
              {record.narrower.length > 0 && (
                <p className="text-xs text-stone-500">
                  Narrower stories hanging off this one:{" "}
                  {record.narrower
                    .map((id) => rows.find((r) => r.id === id)?.label)
                    .filter(Boolean)
                    .map((l, i) => (
                      <span key={i}>
                        {i > 0 ? ", " : ""}
                        <span className="text-stone-700">{l}</span>
                      </span>
                    ))}
                </p>
              )}
            </div>
          )}
          {!record && current && (
            <p className="text-xs text-stone-500">
              This artifact predates the usecase-first projection, so the chain and member table
              are not shown — the diagrams below still draw from the recorded extent.
            </p>
          )}

          {/* the two diagrams this story can draw */}
          {current && (
            <div className="flex flex-wrap gap-2 pt-1">
              <button
                type="button"
                onClick={() => openConceptOnCanvas(current.id, "flowchart")}
                className="h-8 rounded-lg px-3 text-xs inline-flex items-center gap-1.5 border border-stone-200 bg-white text-stone-700 hover:border-emerald-300 hover:bg-emerald-50 transition-colors"
              >
                <Workflow className="h-3.5 w-3.5 text-emerald-700" aria-hidden /> Draw the
                flowchart
              </button>
              <button
                type="button"
                onClick={() => openConceptOnCanvas(current.id, "sequence")}
                className="h-8 rounded-lg px-3 text-xs inline-flex items-center gap-1.5 border border-stone-200 bg-white text-stone-700 hover:border-emerald-300 hover:bg-emerald-50 transition-colors"
              >
                <Split className="h-3.5 w-3.5 text-emerald-700" aria-hidden /> Show the sequence
                diagram
              </button>
            </div>
          )}
        </CardContent>
      </Card>

      {/* the member table — the engine's resolved extent, exactly as recorded */}
      {record && record.members.length > 0 && (
        <Card className="border-stone-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base">
              Who does “{record.label}” — {record.members.length} member
              {record.members.length === 1 ? "" : "s"}
            </CardTitle>
            <CardDescription>
              Every member the engine placed in this story, with its recorded location. Tap one to
              open the code exactly there.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="divide-y divide-stone-100">
              {record.members.map((m) => (
                <li key={m.symbol}>
                  <button
                    type="button"
                    onClick={() =>
                      openSourceViewer({
                        file: m.file,
                        line: m.line,
                        elName: m.name,
                        chartTitle: `Usecase — ${record.label}`,
                      })
                    }
                    className="w-full text-left py-1.5 flex flex-wrap items-baseline gap-x-3 hover:bg-emerald-50/60 rounded px-1 -mx-1"
                  >
                    <span className="font-mono text-xs text-stone-800">{m.name}</span>
                    {m.owner && <span className="text-[10px] text-stone-400">in {m.owner}</span>}
                    <span className="text-[10px] text-stone-400 font-mono truncate">
                      {m.file}:{m.line}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
            {record.member_classes.length > 0 && (
              <div className="mt-3 flex flex-wrap gap-1.5 items-center">
                <span className="text-xs text-stone-500 inline-flex items-center gap-1">
                  <Network className="h-3.5 w-3.5 text-stone-400" aria-hidden /> cuts across:
                </span>
                {record.member_classes.map((c) => (
                  <Badge
                    key={c.id}
                    variant="outline"
                    className="bg-stone-50 text-stone-700 border-stone-200 font-normal"
                  >
                    {c.label} ({c.kind.toLowerCase()}) · {c.members.length}
                  </Badge>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* the concept-node fallback line — the extent is visible even without the projection */}
      {!record && conceptNode && (
        <p className="text-xs text-stone-500">
          The recorded extent holds {conceptNode.symbols.length} member
          {conceptNode.symbols.length === 1 ? "" : "s"} — the diagrams above draw exactly those.
        </p>
      )}
    </div>
  );
}
