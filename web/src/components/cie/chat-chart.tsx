"use client";
import { DiagramNavigation } from "./diagram-navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  SelectionConductor,
  defaultRegistry,
  planSheet,
  MermaidAdapter,
} from "viz-adapter";
import type { RenderHandle } from "viz-adapter";
import type { ChatChartSpec, ElementLoc } from "@/lib/chat-chart";
import {
  chartView,
  CHAT_CATALOG,
  type ChartElement,
} from "@/lib/viz/chart-spec";
import { sliceViewSpec } from "@/lib/viz/slice";
import { filterApplies } from "@/lib/chart-filter";
import { useCie } from "@/lib/cie-store";
import { askAboutChartElement } from "@/lib/quality";

const conductor = new SelectionConductor();
export interface DiagramSelection {
  symbolId?:number;
  kind: "node" | "link" | "msg";
  id: string;
  label: string;
  loc?: ElementLoc;
  from?: string;
  to?: string;
}
export type SelectHandler = (selection: DiagramSelection) => void;
export const ASK_HINT = "Select an element to add it to your answer context";
export const KIND_PLAIN: Record<string, string> = {
  domain: "The big picture",
  module: "A code file",
  function: "A named thing",
  step: "A step",
};
export function useAsk() {
  const { addChip, openSourceViewer, setPendingAsk, report } = useCie();
  return useCallback(
    (
      spec: ChatChartSpec,
      el: { name: string; value?: number | string; loc?: ElementLoc },
    ) => {
      addChip({
        id: `${spec.kind}:${el.loc?.file ?? ""}:${el.name}`,
        label: el.name,
        kind: spec.kind,
        revision: report?.rev ?? 0,
        loc: el.loc,
        files: el.loc?.file ? [el.loc.file] : [],
      });
      if (el.loc?.file)
        openSourceViewer({
          ...el.loc,
          elName: el.name,
          elValue: el.value,
          chartTitle: spec.title,
        });
      else setPendingAsk(askAboutChartElement(spec.title, el));
    },
    [addChip, openSourceViewer, setPendingAsk, report?.rev],
  );
}
/** Every chart uses the same gated canvas host. The text outline remains
 * authoritative, including aggregate rows with no recorded source span.
 * `big` is the big-quiet-canvas version: a chart the chat sent to the
 * stage gets room to breathe — a 107-node call flow squeezed into the
 * 300px card host reads as "nothing was drawn". */
export function ChatChart({
  spec,
  onSelectElement,
  big = false,
}: {
  spec: ChatChartSpec;
  onSelectElement?: SelectHandler;
  big?: boolean;
}) {
  const {
    report,
    concepts,
    viewspec,
    atlasFactsOnly,
    chatContext,
    addChip,
    openSourceViewer,
    chartFilter,
  } = useCie();
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<RenderHandle | null>(null);
  const [note, setNote] = useState("Negotiating canvas…");
  const [failure, setFailure] = useState(false);
  const [layout, setLayout] = useState<"auto" | "elk-layered">("auto");
  const data = useMemo(
    () => chartView(spec, report, concepts, viewspec),
    [spec, report, concepts, viewspec],
  );
  // XFILTER receiver: the charts page's file/certainty filter re-slices this
  // diagram through the same sliceViewSpec gate as the atlas scope — kept
  // elements keep their parent chains, the hidden ledger names the rest.
  // Elements only honestly carry a file and a displayMode, so those are the
  // two dimensions that can apply here; everything else leaves diagrams
  // untouched. An empty match is a NAMED refusal (never a blank panel).
  const filterKeep: "none" | Set<string> | undefined = useMemo(() => {
    if (!chartFilter || !filterApplies(chartFilter, "canvas")) return undefined;
    const ids = new Set<string>();
    for (const el of data.view.elements) {
      const hit =
        chartFilter.dimension === "file"
          ? el.evidence?.file === chartFilter.value
          : el.displayMode === chartFilter.value;
      if (hit) ids.add(el.id);
    }
    if (!ids.size) return "none";
    return ids.size < data.view.elements.length ? ids : undefined; // everything matches → no slice
  }, [chartFilter, data.view]);
  const gated = useMemo(
    () =>
      filterKeep === "none"
        ? {
            spec: null,
            violations: [
              `chart filter “${chartFilter?.label}” matches nothing on this diagram — clear it (Numbers page) to draw the whole picture`,
            ],
            dropped: data.view.elements.length,
          }
        : sliceViewSpec(
            layout === "auto"
              ? data.view
              : { ...data.view, layoutHint: { strategy: layout } },
            {
              factsOnly: onSelectElement && spec.kind!=="sequence" ? atlasFactsOnly : false,
              keepIds: filterKeep ?? undefined,
            },
          ),
    [data.view, layout, atlasFactsOnly, !!onSelectElement, filterKeep, chartFilter],
  );
  const callback = useRef(onSelectElement);
  callback.current = onSelectElement;
  const pick = useCallback(
    (entry: ChartElement) => {
      // When the host supplies onSelectElement it owns the interaction: its
      // handlers add their own canonical, richer chips (the atlas's
      // `container:` / `concept:` / `main:` / `route:` ids carry the report's
      // file lists and provenance) or deliberately open a popup instead.
      // Adding the raw-entry chip here TOO used to double-add under a
      // different id — one click, two chips. Callback-less hosts (the chat
      // answers, the one-tap menu, the big canvas) have no other chip path,
      // so the raw chip belongs to them.
      if (callback.current) {
        callback.current({
          kind:
            entry.kind === "edge"
              ? "link"
              : spec.kind === "sequence"
                ? "msg"
                : "node",
          id:
            spec.kind === "sequence"
              ? entry.label.split(": ").slice(1).join(": ")
              : entry.id,
          label: entry.label,
          loc: entry.loc,
          symbolId:entry.symbolId,
        });
        return;
      }
      const fn=entry.symbolId!=null ? report?.symbols.find(s=>Number(s.id)===entry.symbolId) : report?.symbols.find(s=>['Function','Method','Test'].includes(s.kind)&&report.files[s.loc.file]===entry.loc?.file&&s.loc.start===entry.loc?.byteStart&&s.loc.end===entry.loc?.byteEnd);
      if(fn){useCie.getState().selectFunction(Number(fn.id));return;}
      addChip({
        id: entry.id,
        label: entry.label,
        kind: entry.kind,
        revision: report?.rev ?? 0,
        loc: entry.loc,
        files: entry.files,
      });
      if (entry.loc?.file)
        openSourceViewer({
          ...entry.loc,
          elName: entry.label,
          chartTitle: spec.title,
        });
    },
    [addChip, openSourceViewer, report?.rev, spec.kind, spec.title],
  );
  useEffect(() => {
    const container = host.current,
      view = gated.spec;
    if (!container) return;
    if (!view) {
      setFailure(true);
      setNote(`Gate declined: ${gated.violations.join("; ")}`);
      return;
    }
    if (!view.elements.length) {
      setFailure(false);
      setNote(
        "Text outline only: no recorded source spans for these aggregates.",
      );
      return;
    }
    let cancelled = false,
      off: (() => void) | undefined,
      owned: RenderHandle | undefined;
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
          `${result.disclosure.headline} · layout ${owned.layout.actual} (${owned.layout.fidelity}) · ${plans.length} eligible sheet views${
            filterKeep instanceof Set
              ? ` · chart filter “${chartFilter?.label}”: ${filterKeep.size} of ${data.view.elements.length} elements kept`
              : ""
          }`,
        );
        off = conductor.attach(
          owned,
          [...view.elements, ...view.edges].map((e) => e.id),
          (ids) => {
            const entry = data.entries.find((e) => e.id === ids.at(-1));
            if (entry) pick(entry);
          },
        );
        // Merge chat-context chips with whatever the view itself flagged
        // selected (ChatNode.selected — e.g. the Atlas scope ringing the
        // container a table row resolved to). One call: this is the ONLY
        // place that sets the initial selection, so neither source can
        // silently overwrite the other.
        const flaggedSelected = view.elements
          .filter((e) => e.fields?.selected)
          .map((e) => e.id);
        owned.setSelected(
          [
            ...new Set([
              ...useCie.getState().chatContext.map((c) => c.id),
              ...flaggedSelected,
            ]),
          ],
          "programmatic",
        );
      } catch (err) {
        if (!cancelled) {
          setFailure(true);
          setNote(
            `Engine declined: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
      off?.();
      if (handle.current === owned) handle.current = null;
      void owned?.dispose();
    };
  }, [gated, data.entries, pick]);
  useEffect(() => {
    conductor.select(chatContext.map((c) => c.id));
  }, [chatContext]);
  const exportPng = async () => {
    if (!handle.current) return;
    const { exportCanvas } = await import("@/lib/viz/g6-engine");
    const url = await exportCanvas(handle.current);
    const a = document.createElement("a");
    a.href = url;
    a.download = "cie-chart.png";
    a.click();
  };
  return (
    <div className="mt-1" data-cie-chart={spec.kind}>
      {spec.title && <p className="text-sm font-semibold">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500">{spec.tell}</p>}
      {spec.kind==="sequence"&&atlasFactsOnly&&<p className="text-xs text-amber-700">Candidate and unresolved boundaries remain visible; dashed calls are not proven runtime dispatch.</p>}
      {!!gated.spec?.elements.length && (
        <div
          ref={host}
          className={`w-full rounded-lg border ${big ? "h-[640px]" : "h-[300px]"}`}
          role="img"
          aria-label={`${spec.kind} canvas`}
        />
      )}
      {!gated.spec?.elements.length && <div ref={host} />}
      {!!gated.spec?.elements.length && <DiagramNavigation handle={handle} />}
      <p
        role={failure ? "alert" : undefined}
        className="text-[10px] text-stone-500 mt-1"
      >
        {note} · catalog {spec.catalog ?? CHAT_CATALOG[spec.kind]} ·{" "}
        {spec.source ?? "loaded project exports"}
      </p>
      {!!gated.spec?.elements.length && (
        <label className="text-xs mr-2">
          Layout{" "}
          <select
            aria-label="Chart layout"
            value={layout}
            onChange={(e) =>
              setLayout(e.target.value as "auto" | "elk-layered")
            }
          >
            <option value="auto">Automatic</option>
            <option value="elk-layered">ELK layered</option>
          </select>
        </label>
      )}
      {!!gated.spec?.elements.length && (
        <button
          type="button"
          className="text-xs underline mr-2"
          onClick={() => {
            const out = new MermaidAdapter().adapt(gated.spec!);
            const url = URL.createObjectURL(
              new Blob([out.text], { type: "text/plain" }),
            );
            const a = document.createElement("a");
            a.href = url;
            a.download = "cie-chart.mmd";
            a.click();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
          }}
        >
          Export Mermaid
        </button>
      )}
      {!!gated.spec?.elements.length && !failure && (
        <button
          type="button"
          className="text-xs underline"
          onClick={() => void exportPng()}
        >
          Export PNG
        </button>
      )}
      <details
        className="text-xs mt-2"
        open={!gated.spec?.elements.length || failure}
      >
        <summary>
          Text outline · {data.entries.length} recorded elements
        </summary>
        {spec.columns && (
          <p className="font-semibold">{spec.columns.join(" · ")}</p>
        )}
        <ul aria-label="Chart elements" className="max-h-64 overflow-auto">
          {data.entries.map((e) => (
            <li key={e.id}>
              <button
                type="button"
                className="text-left p-1 hover:underline"
                onClick={() => pick(e)}
                aria-pressed={chatContext.some((c) => c.id === e.id)}
              >
                {e.label}
                {e.mode && e.mode !== "Fact" ? ` · ${e.mode}` : ""}
              </button>
              {e.lines?.map((line, i) => (
                <p key={i} className="pl-3 text-stone-500">
                  {line}
                </p>
              ))}
            </li>
          ))}
        </ul>
      </details>
      {!!gated.spec?.hidden.length && (
        <details className="text-xs mt-1">
          <summary>
            Disclosure ledger · {gated.spec.hidden.length} omitted
          </summary>
          <ul>
            {gated.spec.hidden.map((e, i) => (
              <li key={`${e.id}:${i}`}>
                {e.label ?? e.id} · {e.kind ?? "unknown"} · {e.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
