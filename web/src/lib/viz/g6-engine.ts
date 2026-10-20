"use client";

// RFC-0024 stage 1 — the G6 HOST RUNTIME (web/src/lib/viz/g6-engine.ts).
//
// The page bundles @antv/g6; the viz-adapter package stays headless-pure.
// This module is the HostRenderRuntime: the negotiator runs the usual
// negotiate → gate → adapt → configure sequence, then hands the
// ADAPTER-PREPARED options here and the host owns pixels + events.
//
// Generalizes the G6 + compactBox wiring proven in concept-tree-g6.tsx
// (the RFC-0024 §6 "pattern generalized first" step):
//   - tree strategies (compact-box / mindmap / dendrogram / indented)
//     pre-layout client-side via @antv/hierarchy, then G6 only renders —
//     the same math that fixed the concept tree's geometry;
//   - every other strategy goes to G6's own layout registry untouched;
//   - pan/zoom/drag behaviors, autoFit, autoResize;
//   - every element click fires the patch-1 selection surface
//     (setSelected(ids, 'user') — loop-guarded, echo-suppressed);
//   - canvas export helper for the print/export persona stories.

import { Graph, type IElementEvent } from "@antv/g6";
import { compactBox } from "@antv/hierarchy";
import {
  defaultRegistry,
  renderHosted,
  recomputeRenderHash,
} from "viz-adapter";
import type {
  HostRenderRuntime,
  LayoutHint,
  LayoutOutcome,
  RenderHandle,
  RenderOptions,
  SelectionOrigin,
  SelectionListener,
  ViewSpec,
} from "viz-adapter";

/** Tree strategies the host pre-layouts with @antv/hierarchy (G6 renders
 *  the result; the strategy name is preserved in the disclosed outcome). */
export const TREE_LAYOUTS = new Set(["compact-box", "mindmap", "dendrogram", "indented"]);

const STROKE: Record<string, string> = {
  Fact: "#2ea043",
  Inference: "#d29922",
  Hypothesis: "#f85148",
  Fog: "#8b949e",
};
const DASH: Record<string, number[] | undefined> = {
  Fact: undefined,
  Inference: [6, 4],
  Hypothesis: [2, 4],
  Fog: [1, 3],
};

const boxWidth = (label: string) => Math.max(76, Math.min(220, label.length * 6.2 + 26));

interface HData {
  id: string;
  children: HData[];
}

/** Tree pre-layout: parent positions centre on children, height grows
 *  with LEAF count — the concept-tree fix, generalized to any ViewSpec. */
function layoutTree(spec: ViewSpec, strategy: string): Map<string, { x: number; y: number }> {
  const byId = new Map(spec.elements.map((e) => [e.id, e]));
  const childrenOf = new Map<string, string[]>();
  const roots: string[] = [];
  for (const e of spec.elements) {
    const p = e.parentId && byId.has(e.parentId) ? e.parentId : null;
    if (p === null) roots.push(e.id);
    else childrenOf.set(p, [...(childrenOf.get(p) ?? []), e.id]);
  }
  const toH = (id: string): HData => ({ id, children: (childrenOf.get(id) ?? []).map(toH) });
  const hiRoot: HData = { id: "__root__", children: roots.map(toH) };
  const direction = strategy === "dendrogram" ? "TB" : "LR";
  const laid = compactBox(hiRoot, {
    direction: direction as "LR" | "TB",
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getId: (d: any) => (d as HData).id,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getWidth: (d: any) => ((d as HData).id === "__root__" ? 1 : boxWidth(byId.get((d as HData).id)?.label ?? "")),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getHeight: () => 30,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getHGap: () => 48,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getVGap: () => 10,
  });
  const pos = new Map<string, { x: number; y: number }>();
  laid.eachNode((hn) => {
    if (hn.id === "__root__") return;
    pos.set(hn.id, { x: hn.x + hn.width / 2, y: hn.y + hn.height / 2 });
  });
  return pos;
}

/** The epistemic canvas: host-render an already-gated, adapter-prepared
 *  ViewSpec. Returns a RenderHandle with the patch-1 selection surface. */
async function renderSpecG6(
  container: HTMLElement,
  spec: ViewSpec,
  options: RenderOptions,
): Promise<RenderHandle> {
  const cfg = options as unknown as Record<string, unknown>;
  const data = cfg.data as { nodes: { id: string }[]; edges: unknown[] } | undefined;
  if (!data) throw new Error("g6 host: no adapter-prepared data in options");
  const strategy = String((cfg.layout as Record<string, unknown> | undefined)?.type ?? "dagre");

  // The patch-1 selection surface — one hub per rendering, loop-guarded.
  const listeners = new Set<SelectionListener>();
  let current: string[] = [];
  const sameSet = (a: string[], b: string[]) =>
    a.length === b.length && a.every((x, i) => x === b[i]);
  const setSelected = (ids: string[], origin: SelectionOrigin) => {
    if (sameSet(ids, current)) return; // an echo never re-fires
    current = [...ids];
    for (const l of [...listeners]) l(current, origin);
  };

  let graph: Graph;

  if (TREE_LAYOUTS.has(strategy)) {
    // Pre-layout client-side (the concept-tree math), then render.
    const pos = layoutTree(spec, strategy);
    const byId = new Map(spec.elements.map((e) => [e.id, e]));
    const nodes = spec.elements.map((e) => {
      const p = pos.get(e.id);
      const st = STROKE[e.displayMode] ?? STROKE.Fact;
      return {
        id: e.id,
        style: {
          x: p?.x ?? 0,
          y: p?.y ?? 0,
          size: [boxWidth(e.label), 30] as [number, number],
          radius: 7,
          fill: "#f5f5f4",
          stroke: st,
          lineWidth: e.displayMode === "Fact" ? 1.6 : 1.2,
          lineDash: DASH[e.displayMode],
          labelText: e.label,
          labelFontSize: 10,
          labelPlacement: "center" as const,
          labelWordWrap: true,
          labelWordWrapWidth: boxWidth(e.label) - 12,
          labelMaxLines: 2,
          labelTextOverflow: "ellipsis",
        },
        data: { label: e.label, elementKind: e.elementKind, displayMode: e.displayMode },
      };
    });
    const edges = spec.edges
      .filter((e) => byId.has(e.source) && byId.has(e.target))
      .map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        style: {
          stroke: STROKE[e.displayMode] ?? STROKE.Fact,
          lineDash: DASH[e.displayMode],
          lineWidth: 1.2,
        },
      }));
    const leafCount = spec.elements.filter((e) => !e.parentId || !byId.has(e.parentId)).length;
    container.style.height = `${Math.max(240, Math.min(560, leafCount * 38))}px`;
    graph = new Graph({
      container,
      autoResize: true,
      autoFit: "view",
      padding: 24,
      data: { nodes, edges },
      node: { type: "rect" },
      edge: { type: "cubic-horizontal" },
      behaviors: ["drag-canvas", "zoom-canvas"],
    });
  } else {
    // Adapter-prepared config passthrough (epistemic style fns intact).
    const { data: _d, ...rest } = cfg;
    void _d;
    graph = new Graph({
      container,
      autoResize: true,
      autoFit: "view",
      padding: 16,
      behaviors: ["drag-canvas", "zoom-canvas", "drag-element"],
      ...(rest as Record<string, unknown>),
    });
  }

  await graph.render();

  const onClick = (e: IElementEvent) => {
    const id = e.target?.id;
    if (id == null) return;
    setSelected([String(id)], "user");
  };
  graph.on("element:click", onClick);

  const handle: RenderHandle = {
    engineId: "g6",
    semanticHash: spec.semanticHash,
    renderHash: recomputeRenderHash(spec, "g6"),
    layout: g6LayoutOutcomeOf(spec),
    onSelection(cb) {
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
    setSelected(ids, origin) {
      setSelected(ids, origin);
    },
    async dispose() {
      graph.off("element:click", onClick);
      graph.destroy();
      graphs.delete(handle);
    },
  };

  // relayout support lives beside the graph instance (WeakMap by handle).
  graphs.set(handle, graph);
  return handle;
}

const graphs = new WeakMap<RenderHandle, Graph>();

/** Layout outcome from the adapter package (kept local: the host owns the
 *  disclosed outcome it rendered with). */
function g6LayoutOutcomeOf(spec: ViewSpec): LayoutOutcome {
  const strategy = spec.layoutHint?.strategy ?? "dagre";
  return { requested: spec.layoutHint?.strategy, actual: strategy, fidelity: "Exact" };
}

/** The HostRenderRuntime the page registers on the g6 adapter. */
export function g6HostRuntime(): HostRenderRuntime {
  return {
    async render(_engineId, container, spec, options) {
      return renderSpecG6(container, spec, options);
    },
    async relayout(_engineId, handle, hint: LayoutHint): Promise<LayoutOutcome> {
      const graph = graphs.get(handle);
      if (!graph) throw new Error("g6 host: relayout on a dead handle");
      graph.setLayout({ type: hint.strategy, ...(hint.params ?? {}) });
      await graph.render();
      const outcome: LayoutOutcome = {
        requested: hint.strategy,
        actual: hint.strategy,
        fidelity: "Exact",
      };
      return outcome;
    },
  };
}

/** One-shot hosted render: negotiate → gate → adapt → configure → the
 *  host above. */
export async function renderViaNegotiator(
  container: HTMLElement,
  spec: ViewSpec,
  options: RenderOptions = {},
) {
  return renderHosted(spec, { required: {} }, defaultRegistry(), container, options, g6HostRuntime());
}
