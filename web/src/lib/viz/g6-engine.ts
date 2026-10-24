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
//   - autoFit, autoResize; G6's own pan/zoom/drag BEHAVIORS are
//     deliberately off everywhere (see NAVIGATION note below);
//   - every element click fires the patch-1 selection surface
//     (setSelected(ids, 'user') — loop-guarded, echo-suppressed);
//   - canvas export helper for the print/export persona stories.

import { Graph, type IElementEvent } from "@antv/g6";
import { compactBox, mindmap, dendrogram, indented } from "@antv/hierarchy";
import {
  defaultRegistry,
  renderHosted,
  recomputeRenderHash,
  createSelectionHub,
  g6LayoutOutcome,
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
export const TREE_LAYOUTS = new Set([
  "compact-box",
  "mindmap",
  "dendrogram",
  "indented",
]);

// ── NAVIGATION IS SINGLE-LAYERED (the "charts move / zoom loses control" fix)
//
// G6's zoom-canvas/drag-canvas/drag-element behaviors attach their own wheel
// and pointer listeners to the graph's canvas. On the stage that DOUBLE-
// HANDLES every gesture: the stage camera (camera-stage.tsx) wheel-zooms the
// wrapper while G6 simultaneously zooms the inner graph at a different
// anchor — one tick left the outer camera at 162% while the graph had zoomed
// too, and a drag panned both layers at once. The toolbar's % then reads a
// number that no longer describes what the visitor sees. In document views
// the same wheel listener swallowed scrolling over a chart. So: G6 never
// owns navigation anywhere in this app. On the stage the CAMERA owns zoom
// and pan (one gesture, one layer, honest toolbar); in document views the
// page owns the wheel. Element CLICKS and the selection surface are wired
// separately below and are unaffected.
const G6_BEHAVIORS: string[] = [];

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

const boxWidth = (label: string) =>
  Math.max(76, Math.min(220, label.length * 6.2 + 26));

interface HData {
  id: string;
  children: HData[];
}

/** Tree pre-layout: parent positions centre on children, height grows
 *  with LEAF count — the concept-tree fix, generalized to any ViewSpec. */
function layoutTree(
  spec: ViewSpec,
  strategy: string,
): Map<string, { x: number; y: number }> {
  const byId = new Map(spec.elements.map((e) => [e.id, e]));
  const childrenOf = new Map<string, string[]>();
  const roots: string[] = [];
  for (const e of spec.elements) {
    const p = e.parentId && byId.has(e.parentId) ? e.parentId : null;
    if (p === null) roots.push(e.id);
    else childrenOf.set(p, [...(childrenOf.get(p) ?? []), e.id]);
  }
  const toH = (id: string): HData => ({
    id,
    children: (childrenOf.get(id) ?? []).map(toH),
  });
  const hiRoot: HData = { id: "__root__", children: roots.map(toH) };
  const direction = strategy === "dendrogram" ? "TB" : "LR";
  const layout = ({ "compact-box": compactBox, mindmap, dendrogram, indented }[
    strategy
  ] ?? compactBox) as typeof compactBox;
  const laid = layout(hiRoot, {
    direction: direction as "LR" | "TB",
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getId: (d: any) => (d as HData).id,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getWidth: (d: any) =>
      (d as HData).id === "__root__"
        ? 1
        : boxWidth(byId.get((d as HData).id)?.label ?? ""),
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
  if (pos.size !== spec.elements.length)
    throw new Error("Tree layout requires acyclic parent containment");
  return pos;
}

/** Sequence lanes — `spec.kind === "sequence"` strategy name "sequence-lanes"
 *  (chat-chart.ts's `layoutFor`), registered in viz-adapter's G6_LAYOUTS so
 *  the negotiator doesn't substitute it away, handled here the same way
 *  tree strategies are: pre-layout client-side, G6 only renders.
 *
 *  chartView()'s sequence branch (chart-spec.ts) only gives us one flat
 *  element per message, labelled `"${from} → ${to}: ${msg}"`, plus a
 *  call-order chain edge between consecutive elements — there is no
 *  participant/lane field to read. Parsing that label back into
 *  {from, to, msg} is honest (it's the exact string chartView built, from
 *  the exact `from`/`to` buildSequenceSpec already recorded) and keeps this
 *  entirely a presentation-layer change — nothing upstream moves. The
 *  call-order chain edges are NOT rendered here (they don't mean anything
 *  in lane-space); each message element becomes its own real, clickable
 *  edge between two synthetic per-row anchor nodes on its lanes instead —
 *  a self-call (from === to) gets its anchor offset sideways so the loop
 *  stays visible instead of collapsing to a point. */
function parseSequenceLabel(
  label: string,
): { from: string; to: string; msg: string } | null {
  const arrow = label.indexOf(" → ");
  if (arrow < 0) return null;
  const from = label.slice(0, arrow);
  const rest = label.slice(arrow + 3);
  const colon = rest.indexOf(": ");
  if (colon < 0) return null;
  return { from, to: rest.slice(0, colon), msg: rest.slice(colon + 2) };
}

function renderSequence(container: HTMLElement, spec: ViewSpec): Graph {
  const parsed = spec.elements.map((e) => ({ e, p: parseSequenceLabel(e.label) }));
  const lanes: string[] = [];
  for (const { p } of parsed) {
    if (!p) continue;
    if (!lanes.includes(p.from)) lanes.push(p.from);
    if (!lanes.includes(p.to)) lanes.push(p.to);
  }
  const LANE_W = 170, HEADER_Y = 30, HEADER_H = 32, ROW_H = 56, SELF_DX = 56;
  const laneX = new Map(lanes.map((l, i) => [l, 60 + i * LANE_W]));
  const footerY = HEADER_Y + HEADER_H + 40 + parsed.length * ROW_H + 20;
  container.style.height = `${Math.max(280, Math.min(900, footerY + HEADER_H + 30))}px`;

  const nodes: { id: string; style: Record<string, unknown>; data: Record<string, unknown> }[] = [];
  for (const lane of lanes) {
    const x = laneX.get(lane)!;
    const w = boxWidth(lane);
    for (const [key, y] of [["top", HEADER_Y], ["bottom", footerY]] as const) {
      nodes.push({
        id: `lane:${lane}:${key}`,
        style: { x, y: y + HEADER_H / 2, size: [w, HEADER_H], radius: 5, fill: "#eef3ef", stroke: "#3c6e5d", lineWidth: 1.4, labelText: lane, labelFontSize: 12, labelFontWeight: 500, labelPlacement: "center" as const, labelWordWrap: true, labelWordWrapWidth: w - 10 },
        data: {},
      });
    }
  }
  const edges: { id: string; source: string; target: string; style: Record<string, unknown>; data?: Record<string, unknown> }[] = [];
  for (const lane of lanes) {
    edges.push({
      id: `lifeline:${lane}`,
      source: `lane:${lane}:top`,
      target: `lane:${lane}:bottom`,
      style: { stroke: "#9aa39c", lineWidth: 1.2, lineDash: [3, 4], endArrow: false },
    });
  }
  parsed.forEach(({ e, p }, i) => {
    const y = HEADER_Y + HEADER_H + 40 + i * ROW_H;
    const self = !!p && p.from === p.to;
    const fromX = p ? laneX.get(p.from)! : 60;
    const toX = p ? (self ? fromX + SELF_DX : laneX.get(p.to)!) : 60;
    const fromId = `${e.id}::from`, toId = `${e.id}::to`;
    nodes.push({ id: fromId, style: { x: fromX, y, size: 1, opacity: 0 }, data: {} });
    nodes.push({ id: toId, style: { x: toX, y: self ? y + ROW_H * 0.55 : y, size: 1, opacity: 0 }, data: {} });
    const st = STROKE[e.displayMode] ?? STROKE.Fact;
    edges.push({
      id: e.id,
      source: fromId,
      target: toId,
      style: {
        stroke: st,
        lineDash: DASH[e.displayMode],
        lineWidth: e.displayMode === "Fact" ? 1.8 : 1.4,
        endArrow: true,
        labelText: p?.msg ?? e.label,
        labelFontSize: 12,
        labelAutoRotate: false,
        labelBackground: true,
        labelBackgroundFill: "#f7f8f4",
      },
      data: { label: p?.msg ?? e.label, displayMode: e.displayMode },
    });
  });

  return new Graph({
    container,
    autoResize: true,
    autoFit: undefined,
    padding: 20,
    data: { nodes, edges },
    node: { type: "rect" },
    edge: { type: "polyline", style: { router: { type: "orth" } } },
    behaviors: G6_BEHAVIORS,
  });
}

/** True when the spec IS a lifeline sequence — the B3 mapper's shape:
 *  every element is a `lifeline` (a class/module lane) and the edges are
 *  `message:<text>` cross-class calls in reading order. These must render
 *  as LANES, not through a force/dagre layout — a one-class story through
 *  the generic branch used to come out as one giant circle. */
function isLifelineSequence(spec: ViewSpec): boolean {
  return (
    spec.elements.length > 0 &&
    spec.elements.every((e) => e.elementKind === "lifeline")
  );
}

/** Lifeline lanes — the concept-sequence renderer. Elements are the lanes
 *  (deduped classes, entries' lanes first — the mapper's order IS the
 *  reading order), edges are `message:<text>` cross-class messages already
 *  ordered from the entry points outward (fields.depth when present, else
 *  edge order). Lane headers top and bottom, dashed lifelines between,
 *  one horizontal arrow per message at successive depths. */
function renderLifelineSequence(container: HTMLElement, spec: ViewSpec): Graph {
  const lanes = spec.elements;
  const laneIds = new Set(lanes.map((l) => l.id));
  const msgs = spec.edges
    .map((e, i) => ({
      e,
      i,
      depth: typeof e.fields?.depth === "number" ? (e.fields.depth as number) : i,
      label: e.edgeKind.startsWith("message:") ? e.edgeKind.slice("message:".length) : e.edgeKind,
    }))
    .filter((m) => laneIds.has(m.e.source) && laneIds.has(m.e.target))
    .sort((a, b) => a.depth - b.depth || a.i - b.i);

  const LANE_W = 170, HEADER_Y = 30, HEADER_H = 32, ROW_H = 56;
  const laneX = new Map(lanes.map((l, i) => [l.id, 60 + i * LANE_W]));
  const bodyTop = HEADER_Y + HEADER_H + 40;
  const footerY = bodyTop + Math.max(msgs.length, 1) * ROW_H + 20;
  container.style.height = `${Math.max(280, Math.min(900, footerY + HEADER_H + 30))}px`;

  const nodes: { id: string; style: Record<string, unknown>; data: Record<string, unknown> }[] = [];
  for (const lane of lanes) {
    const x = laneX.get(lane.id)!;
    const w = Math.min(LANE_W - 10, boxWidth(lane.label));
    for (const [key, y] of [["top", HEADER_Y], ["bottom", footerY]] as const) {
      nodes.push({
        id: `${lane.id}:${key}`,
        style: { x, y: y + HEADER_H / 2, size: [w, HEADER_H], radius: 5, fill: "#eef3ef", stroke: "#3c6e5d", lineWidth: 1.4, labelText: lane.label, labelFontSize: 12, labelFontWeight: 500, labelPlacement: "center" as const, labelWordWrap: true, labelWordWrapWidth: w - 10 },
        data: { label: lane.label },
      });
    }
  }
  const edges: { id: string; source: string; target: string; style: Record<string, unknown>; data?: Record<string, unknown> }[] = [];
  for (const lane of lanes) {
    edges.push({
      id: `lifeline:${lane.id}`,
      source: `${lane.id}:top`,
      target: `${lane.id}:bottom`,
      style: { stroke: "#9aa39c", lineWidth: 1.2, lineDash: [3, 4], endArrow: false },
    });
  }
  msgs.forEach(({ e, label }, row) => {
    const y = bodyTop + row * ROW_H;
    const fromId = `${e.id}::from`, toId = `${e.id}::to`;
    nodes.push({ id: fromId, style: { x: laneX.get(e.source)!, y, size: 1, opacity: 0 }, data: {} });
    nodes.push({ id: toId, style: { x: laneX.get(e.target)!, y, size: 1, opacity: 0 }, data: {} });
    const st = STROKE[e.displayMode] ?? STROKE.Fact;
    edges.push({
      id: e.id,
      source: fromId,
      target: toId,
      style: {
        stroke: st,
        lineDash: DASH[e.displayMode],
        lineWidth: e.displayMode === "Fact" ? 1.8 : 1.4,
        endArrow: true,
        labelText: label,
        labelFontSize: 12,
        labelAutoRotate: false,
        labelBackground: true,
        labelBackgroundFill: "#f7f8f4",
      },
      data: { label, displayMode: e.displayMode },
    });
  });

  return new Graph({
    container,
    autoResize: true,
    autoFit: undefined,
    padding: 20,
    data: { nodes, edges },
    node: { type: "rect" },
    edge: { type: "line" },
    behaviors: G6_BEHAVIORS,
  });
}

/** The epistemic canvas: host-render an already-gated, adapter-prepared
 *  ViewSpec. Returns a RenderHandle with the patch-1 selection surface. */
async function renderSpecG6(
  container: HTMLElement,
  spec: ViewSpec,
  options: RenderOptions,
): Promise<RenderHandle> {
  const cfg = options as unknown as Record<string, unknown>;
  const data = cfg.data as
    | { nodes: { id: string }[]; edges: unknown[] }
    | undefined;
  if (!data) throw new Error("g6 host: no adapter-prepared data in options");
  const strategy = String(
    (cfg.layout as Record<string, unknown> | undefined)?.type ?? "dagre",
  );

  // The patch-1 selection surface — one hub per rendering, loop-guarded.
  const hub = createSelectionHub();
  const setSelected = hub.setSelected;

  let graph: Graph;

  if (strategy === "sequence-lanes") {
    graph = renderSequence(container, spec);
  } else if (isLifelineSequence(spec)) {
    // B3 concept sequences: lifelines are the lanes, message: edges the
    // arrows — the geometry is semantic, never re-laid-out.
    graph = renderLifelineSequence(container, spec);
  } else if (TREE_LAYOUTS.has(strategy)) {
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
          labelFontSize: 12,
          labelPlacement: "center" as const,
          labelWordWrap: true,
          labelWordWrapWidth: boxWidth(e.label) - 12,
          labelMaxLines: 2,
          labelTextOverflow: "ellipsis",
        },
        data: {
          label: e.label,
          elementKind: e.elementKind,
          displayMode: e.displayMode,
        },
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
    const leafCount = spec.elements.filter(
      (e) => !e.parentId || !byId.has(e.parentId),
    ).length;
    container.style.height = `${Math.max(240, Math.min(560, leafCount * 38))}px`;
    graph = new Graph({
      container,
      autoResize: true,
      autoFit: undefined,
      padding: 24,
      data: { nodes, edges },
      node: { type: "rect" },
      edge: { type: "cubic-horizontal" },
      behaviors: G6_BEHAVIORS,
    });
  } else {
    if (strategy === "elk-layered") {
      const positions = await elkPositions(spec);
      cfg.data = {
        ...data,
        nodes: data.nodes.map((n) => ({ ...n, style: positions.get(n.id) })),
      };
      cfg.layout = undefined;
    }
    // Adapter-prepared config passthrough (epistemic style fns intact).
    const { data: _d, renderer: _renderer, ...rest } = cfg;
    void _d;
    graph = new Graph({
      container,
      autoResize: true,
      padding: 16,
      behaviors: G6_BEHAVIORS,
      ...(rest as Record<string, unknown>),
      autoFit: undefined,
      data: cfg.data as never,
    });
  }

  try {
    graph.updateNodeData(graph.getNodeData().map(n=>({id:n.id,style:{...n.style,labelFontSize:12}})));
    graph.updateEdgeData(graph.getEdgeData().map(e=>({id:e.id,style:{...e.style,labelFontSize:12}})));
    await graph.render();
    await readableViewport(graph);
  } catch (err) {
    graph.destroy();
    throw err;
  }

  const onClick = (e: IElementEvent) => {
    const id = e.target?.id;
    if (id == null) return;
    setSelected([String(id)], "user");
  };
  graph.on("node:click", onClick);
  graph.on("edge:click", onClick);
  // Custom strategies (sequence-lanes, tree) don't render every id in
  // spec.elements/spec.edges 1:1 (e.g. the "order:N" chain edges built for
  // sequence specs are deliberately skipped by renderSequence) — restrict
  // the highlight sync to ids the live graph actually has, or
  // setElementState throws "Unknown element type of id" on unmatched ones.
  const knownIds = new Set([
    ...graph.getNodeData().map((n) => n.id),
    ...graph.getEdgeData().map((e) => e.id),
  ]);
  let disposed = false;
  const offHighlight = hub.subscribe((ids) => {
    if(disposed)return;
    const states = Object.fromEntries(
      [...spec.elements, ...spec.edges]
        .filter((e) => knownIds.has(e.id))
        .map((e) => [e.id, ids.includes(e.id) ? ["selected"] : []]),
    );
    void graph.setElementState(states, false).catch(error=>{if(!disposed)console.error(error);});
  });

  const handle: RenderHandle = {
    engineId: "g6",
    semanticHash: spec.semanticHash,
    renderHash: recomputeRenderHash(spec, "g6"),
    layout: isLifelineSequence(spec)
      ? // the lanes rendered — disclose the truth instead of the adapter's
        // "dagre substituted" story for a diagram that never saw dagre
        { requested: (spec.layoutHint as { strategy?: string } | undefined)?.strategy, actual: "sequence-lanes", fidelity: "Exact" }
      : g6LayoutOutcome(spec),
    onSelection(cb) {
      return hub.subscribe(cb);
    },
    setSelected(ids, origin) {
      setSelected(
        ids.filter((id) =>
          [...spec.elements, ...spec.edges].some((el) => el.id === id),
        ),
        origin,
      );
    },
    async dispose() {
      disposed = true;
      offHighlight();
      graph.off("node:click", onClick);
      graph.off("edge:click", onClick);
      graph.destroy();
      graphs.delete(handle);
      specs.delete(handle);
    },
  };

  // relayout support lives beside the graph instance (WeakMap by handle).
  graphs.set(handle, graph);
  specs.set(handle, spec);
  return handle;
}

async function readableViewport(graph:Graph) {
  await graph.zoomTo(1,false);
  const first=graph.getNodeData().find(n=>!String(n.id).includes('::') && !String(n.id).startsWith('lane:') && !String(n.id).endsWith(':bottom')) ?? graph.getNodeData()[0];
  if(first?.id) await graph.focusElement(first.id,false);
  else await graph.fitCenter(false);
}
export async function navigateDiagram(handle:RenderHandle,action:string) {
  const graph=graphs.get(handle);if(!graph)return;
  if(action==='overview'){await graph.fitView(undefined,false);return;}
  if(action==='readable'){await readableViewport(graph);return;}
  if(action==='in'||action==='out'){await graph.zoomTo(Math.max(1,Math.min(4,graph.getZoom()*(action==='in'?1.25:0.8))),false);return;}
  const delta:Record<string,[number,number]>={left:[200,0],right:[-200,0],up:[0,160],down:[0,-160]};
  if(delta[action])await graph.translateBy(delta[action],false);
}
const graphs = new WeakMap<RenderHandle, Graph>();
const specs = new WeakMap<RenderHandle, ViewSpec>();
async function elkPositions(spec: ViewSpec) {
  const { default: ELK } = await import("elkjs/lib/elk.bundled.js");
  const out = await new ELK().layout({
    id: "root",
    layoutOptions: { "elk.algorithm": "layered", "elk.direction": "RIGHT" },
    children: spec.elements.map((e) => ({
      id: e.id,
      width: 220,
      height:
        46 +
        Math.min(
          8,
          Array.isArray(e.fields.bodyLines) ? e.fields.bodyLines.length : 0,
        ) *
          14,
    })),
    edges: spec.edges.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  });
  return new Map(
    (out.children ?? []).map((n) => [
      n.id,
      {
        x: (n.x ?? 0) + (n.width ?? 0) / 2,
        y: (n.y ?? 0) + (n.height ?? 0) / 2,
      },
    ]),
  );
}

/** The HostRenderRuntime the page registers on the g6 adapter. */
export function g6HostRuntime(): HostRenderRuntime {
  return {
    async render(_engineId, container, spec, options) {
      return renderSpecG6(container, spec, options);
    },
    async relayout(
      _engineId,
      handle,
      hint: LayoutHint,
    ): Promise<LayoutOutcome> {
      const graph = graphs.get(handle);
      if (!graph) throw new Error("g6 host: relayout on a dead handle");
      const original = specs.get(handle)!;
      if (isLifelineSequence(original)) {
        // lane geometry is semantic — a layout swap would re-dagre the
        // lanes back into floating boxes; decline honestly instead
        return { requested: hint.strategy, actual: "sequence-lanes", fidelity: "Exact" };
      }
      const next = { ...original, layoutHint: hint };
      const outcome = g6LayoutOutcome(next);
      if (
        TREE_LAYOUTS.has(outcome.actual) ||
        outcome.actual === "elk-layered"
      ) {
        const positions =
          outcome.actual === "elk-layered"
            ? await elkPositions(next)
            : layoutTree(next, outcome.actual);
        graph.setOptions({ layout: undefined });
        graph.updateNodeData(
          next.elements.map((e) => ({ id: e.id, style: positions.get(e.id) })),
        );
      } else graph.setLayout({ type: outcome.actual, ...(hint.params ?? {}) });
      await graph.render();
      await readableViewport(graph);
      handle.layout = outcome;
      handle.renderHash = recomputeRenderHash(next, "g6");
      specs.set(handle, next);
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
  return renderHosted(
    spec,
    { required: {} },
    defaultRegistry(),
    container,
    options,
    g6HostRuntime(),
  );
}

/** Canvas-only export from a live negotiated handle. */
export async function exportCanvas(handle: RenderHandle): Promise<string> {
  const graph = graphs.get(handle);
  if (!graph) throw new Error("Cannot export a disposed chart");
  return graph.toDataURL({ mode: "overall", type: "image/png" });
}
