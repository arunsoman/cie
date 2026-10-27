"use client";

// Named diagrams (PRD v2.0 §25) — the polyglot bundle, rendered.
//
// Every artifact in the bundle (C4, state machines, workflows, impact
// views) is a §25 ViewSpec that the engine already validated clause-for-
// clause against the gate. lib/viz/polyglot-spec.ts maps each one onto the
// adapter's ViewSpec shape (salience/fields defaults, edge ids, footer,
// gaps, hashes) and re-runs the REAL gate; this component renders the
// mapped spec through the same negotiation every other canvas view uses —
// negotiator picks the engine, the conductor wires selection, and a click
// opens the source panel at exactly the cited bytes. A spec the gate
// refused is a NAMED result: the stable codes print, nothing renders.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  SelectionConductor,
  defaultRegistry,
  planSheet,
  MermaidAdapter,
} from "viz-adapter";
import type { RenderHandle } from "viz-adapter";
import type { PolyglotArtifact } from "@/lib/viz/polyglot-spec";
import { polyglotArtifacts } from "@/lib/viz/polyglot-spec";
import { useCie } from "@/lib/cie-store";
import { certaintyPlain } from "@/lib/plain-words";
import { Network, Workflow, GitBranch, Radio } from "lucide-react";

const conductor = new SelectionConductor();

const KIND_ICON: Record<PolyglotArtifact["kind"], typeof Network> = {
  c4: Network,
  workflows: Workflow,
  "state-machine": GitBranch,
  impact: Radio,
};

const KIND_PLAIN: Record<PolyglotArtifact["kind"], string> = {
  c4: "architecture",
  workflows: "flow",
  "state-machine": "machine",
  impact: "impact",
};

export function PolyglotView() {
  const { polyglotSpecs, report } = useCie();
  const [selected, setSelected] = useState<string | null>(null);

  const artifacts = useMemo(
    () => (polyglotSpecs ? polyglotArtifacts(polyglotSpecs, { revision: report?.rev ?? 0 }) : []),
    [polyglotSpecs, report?.rev],
  );

  // Keep pointing at a real artifact across project switches.
  const current = artifacts.find((a) => a.name === selected) ?? artifacts[0] ?? null;
  useEffect(() => {
    if (selected && !artifacts.some((a) => a.name === selected)) setSelected(null);
  }, [artifacts, selected]);

  if (!polyglotSpecs || artifacts.length === 0) {
    return (
      <div className="max-w-2xl">
        <h2 className="text-lg font-semibold text-stone-900">No named diagrams for this project</h2>
        <p className="mt-2 text-sm text-stone-600 leading-relaxed">
          Named diagrams come from the polyglot pass: the C4 containers and components, the state
          machines and workflows the engine can actually evidence, and impact views for duplicate
          families — each box citing the file and bytes that earned it.
        </p>
        <p className="mt-2 text-sm text-stone-600 leading-relaxed">
          Connect a repo (the &ldquo;Naming components and flows&rdquo; step runs the pass), or load
          the five-service sample, which ships a bundle. What the engine can&rsquo;t evidence, it
          doesn&rsquo;t draw — an honest nothing instead of a guessed picture.
        </p>
      </div>
    );
  }

  const kindCounts = artifacts.reduce<Record<string, number>>((acc, a) => {
    acc[a.kind] = (acc[a.kind] ?? 0) + 1;
    return acc;
  }, {});

  return (
    <div className="space-y-4" data-view="polyglot">
      {/* the honest headline: what the pass named, what it counted */}
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <h2 className="text-lg font-semibold text-stone-900">Named diagrams</h2>
        <p className="text-xs text-stone-500">
          {artifacts.length} artifact{artifacts.length === 1 ? "" : "s"}:{" "}
          {kindCounts.c4 ?? 0} architecture · {kindCounts.workflows ?? 0} flow ·{" "}
          {kindCounts["state-machine"] ?? 0} machine{kindCounts["state-machine"] === 1 ? "" : "s"} ·{" "}
          {kindCounts.impact ?? 0} impact
          {polyglotSpecs["impact-centers-truncated"] ? " (impact capped at 50 — the rest stay counted, not shown)" : ""}
        </p>
      </div>

      {/* artifact picker — grouped in the producer's stable order */}
      <div className="flex flex-wrap gap-1.5" role="tablist" aria-label="Named diagram picker">
        {artifacts.map((a) => {
          const Icon = KIND_ICON[a.kind];
          const active = current?.name === a.name;
          return (
            <button
              key={a.name}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => setSelected(a.name)}
              className={`h-8 rounded-lg px-2.5 text-xs inline-flex items-center gap-1.5 border transition-colors ${
                active
                  ? "bg-emerald-700 text-white border-emerald-700"
                  : a.view
                    ? "bg-white text-stone-700 border-stone-200 hover:border-emerald-300 hover:bg-emerald-50"
                    : "bg-stone-50 text-stone-400 border-stone-200"
              }`}
              title={a.hint}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
              <span className="max-w-[260px] truncate">{a.label}</span>
              <span className={`text-[10px] ${active ? "text-emerald-100" : "text-stone-400"}`}>
                {KIND_PLAIN[a.kind]}
              </span>
            </button>
          );
        })}
      </div>

      {current && <ArtifactCanvas artifact={current} />}
    </div>
  );
}

/** One artifact on the gated canvas — the same negotiation the chat chart
 *  uses, pointed at a mapped bundle spec. Exported for the stage: a named
 *  diagram the CHAT asked for renders here too, just bigger (`big`). */
export function ArtifactCanvas({ artifact, big = false }: { artifact: PolyglotArtifact; big?: boolean }) {
  const { report, addChip, openSourceViewer } = useCie();
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<RenderHandle | null>(null);
  const [note, setNote] = useState("Negotiating canvas…");
  const [failure, setFailure] = useState(false);
  const [layout, setLayout] = useState<"auto" | "elk-layered">("auto");

  const view = useMemo(
    () =>
      artifact.view && layout === "auto"
        ? artifact.view
        : artifact.view
          ? { ...artifact.view, layoutHint: { strategy: layout } }
          : null,
    [artifact, layout],
  );

  // element/edge lookup for the click → source-panel path
  const byId = useMemo(() => {
    const m = new Map<string, { label: string; file: string; start: number; end: number }>();
    for (const el of artifact.spec.elements)
      m.set(el.id, { label: el.label, file: el.evidence.file, start: el.evidence.start, end: el.evidence.end });
    for (const ed of artifact.spec.edges)
      m.set(`${ed.source}->${ed.target}:${ed.edgeKind}#${(artifact.spec.edges as unknown[]).indexOf(ed)}`, {
        label: `${ed.source} → ${ed.target}`,
        file: ed.evidence.file,
        start: ed.evidence.start,
        end: ed.evidence.end,
      });
    return m;
  }, [artifact]);

  const pick = useCallback(
    (id: string) => {
      const rec = byId.get(id);
      if (!rec) return;
      addChip({
        id,
        label: rec.label,
        kind: artifact.kind,
        revision: report?.rev ?? 0,
        loc: { file: rec.file, byteStart: rec.start, byteEnd: rec.end },
        files: rec.file ? [rec.file] : [],
      });
      if (rec.file)
        openSourceViewer({
          file: rec.file,
          byteStart: rec.start,
          byteEnd: rec.end,
          elName: rec.label,
          chartTitle: artifact.label,
        });
    },
    [byId, addChip, openSourceViewer, report?.rev, artifact.kind, artifact.label],
  );

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    if (!view) {
      setFailure(true);
      setNote(`Gate declined: ${artifact.violations.join("; ")}`);
      return;
    }
    if (!view.elements.length) {
      setFailure(false);
      setNote("Nothing nameable in this artifact — the honest answer for this project.");
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
          `layout ${owned.layout.actual} (${owned.layout.fidelity}) · ${plans.length} eligible sheet views`,
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
  }, [view, pick, artifact.violations]);

  useEffect(() => {
    conductor.select(useCie.getState().chatContext.map((c) => c.id));
  }, []);

  const exportPng = async () => {
    if (!handle.current) return;
    const { exportCanvas } = await import("@/lib/viz/g6-engine");
    const url = await exportCanvas(handle.current);
    const a = document.createElement("a");
    a.href = url;
    a.download = "cie-named-diagram.png";
    a.click();
  };

  const f = view?.footer;
  const gapCount = artifact.spec.gaps?.length ?? 0;
  const caption = typeof f?.caption === "string" ? f.caption : "";
  const counterArg = typeof artifact.spec.footer?.counterArgument === "string" ? artifact.spec.footer.counterArgument : "";
  const fogReason = typeof artifact.spec.footer?.fogReason === "string" ? artifact.spec.footer.fogReason : "";

  return (
    <div data-polyglot-artifact={artifact.name}>
      <p className="text-sm font-semibold text-stone-900">{artifact.label}</p>
      <p className="text-xs text-stone-500">{artifact.hint}</p>
      {view && !!view.elements.length && (
        <div
          ref={host}
          className={`mt-2 w-full rounded-lg border border-stone-200 bg-white ${big ? "h-[640px]" : "h-[440px]"}`}
          role="img"
          aria-label={`${artifact.label} canvas`}
        />
      )}
      {(!view || !view.elements.length) && <div ref={host} />}
      <p
        role={failure ? "alert" : undefined}
        className="text-[10px] text-stone-500 mt-1"
      >
        {note}
      </p>

      {/* the epistemic footer — what this picture knows, and how sure it is */}
      {f && (
        <p className="text-[11px] text-stone-500 mt-1.5 inline-flex items-center gap-x-3 flex-wrap">
          <span className="font-medium text-stone-600">How sure:</span>
          <span>{f.facts} {certaintyPlain("Fact").label.toLowerCase()}</span>
          <span aria-hidden>·</span>
          <span>{f.inferences} {certaintyPlain("Inference").label.toLowerCase()}</span>
          <span aria-hidden>·</span>
          <span>{f.hypotheses} {certaintyPlain("Hypothesis").label.toLowerCase()}</span>
          {f.fog > 0 && (
            <>
              <span aria-hidden>·</span>
              <span>{f.fog} {certaintyPlain("Fog").label.toLowerCase()}</span>
            </>
          )}
        </p>
      )}
      {caption && <p className="text-[11px] text-stone-400 mt-0.5">&ldquo;{caption}&rdquo;</p>}

      {/* producer disclosures: counted-not-shown gaps + the machine/flow counter-argument */}
      {(gapCount > 0 || counterArg || fogReason) && (
        <details className="text-xs mt-2">
          <summary>
            Disclosure · {gapCount} counted-not-shown note{gapCount === 1 ? "" : "s"}
          </summary>
          <ul className="mt-1 space-y-1">
            {(artifact.spec.gaps ?? []).map((g, i) => (
              <li key={i} className="text-stone-600">
                {g.reason}
              </li>
            ))}
            {counterArg && (
              <li className="text-stone-600">
                <span className="font-medium">Counter-argument:</span> {counterArg}
              </li>
            )}
            {fogReason && (
              <li className="text-stone-600">
                <span className="font-medium">Why fog:</span> {fogReason}
              </li>
            )}
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
              a.download = "cie-named-diagram.mmd";
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
        <summary>Text outline · {artifact.spec.elements.length} named elements</summary>
        <ul aria-label="Named diagram elements" className="max-h-64 overflow-auto">
          {artifact.spec.elements.map((e) => (
            <li key={e.id}>
              <button
                type="button"
                className="text-left p-1 hover:underline"
                onClick={() => pick(e.id)}
                aria-pressed={useCie.getState().chatContext.some((c) => c.id === e.id)}
              >
                {e.label}
                <span className="text-stone-400"> · {e.elementKind}</span>
                {e.displayMode !== "Fact" && <span> · {e.displayMode}</span>}
              </button>
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}
