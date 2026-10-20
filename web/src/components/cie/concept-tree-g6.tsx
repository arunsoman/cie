"use client";

// Concept hierarchy tree (kind "tree"), drawn with AntV G6.
//
// Replaces the old hand-rolled SVG whose layout gave every node — parent or
// leaf — its own fixed-height row in document order: canvas height grew
// with total node count instead of leaf count, parents never centred on
// their children, and the elbow curves fanned across unrelated rows. Here
// @antv/hierarchy's compactBox algorithm (the same tree-layout math behind
// G6's own tree-graph presets) computes positions — it sizes each subtree
// by its leaf count and centres every parent on its children's vertical
// midpoint — and G6 only renders the result plus pan/zoom/click.
//
// Nodes still come from concepts.json via quality.ts — this component only
// lays out and draws what spec.tree hands it; it never invents nodes.

import { useEffect, useRef } from "react";
import { Graph, type EdgeData, type IElementEvent, type NodeData } from "@antv/g6";
import { compactBox } from "@antv/hierarchy";
import { MessageCircleQuestion } from "lucide-react";
import type { ChatChartSpec, TreeNode } from "@/lib/chat-chart";
import { ASK_HINT, KIND_PLAIN, useAsk } from "./chat-chart";
import { useCie } from "@/lib/cie-store";

const KIND_STYLE: Record<string, { fill: string; stroke: string; text: string }> = {
  Domain: { fill: "#ecfdf5", stroke: "#059669", text: "#065f46" },
  Module: { fill: "#f5f5f4", stroke: "#78716c", text: "#44403c" },
  UseCase: { fill: "#fffbeb", stroke: "#d97706", text: "#78350f" },
  Rule: { fill: "#fef2f2", stroke: "#e11d48", text: "#881337" },
};
const styleFor = (k: string) => KIND_STYLE[k] ?? { fill: "#f5f5f4", stroke: "#a8a29e", text: "#44403c" };
const boxWidth = (label: string) => Math.max(76, Math.min(190, label.length * 6.2 + 24));

interface HData {
  id: string;
  children: HData[];
}

export function ConceptTreeG6({ spec }: { spec: ChatChartSpec }) {
  const ask = useAsk();
  const setAtlasSelection = useCie((s) => s.setAtlasSelection);
  const addChip = useCie((s) => s.addChip);
  const nodes = spec.tree ?? [];
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || !nodes.length) return;

    const byId = new Map(nodes.map((n) => [n.id, n]));
    const childrenOf = new Map<string, TreeNode[]>();
    const roots: TreeNode[] = [];
    for (const n of nodes) {
      const p = n.parent && byId.has(n.parent) ? n.parent : null;
      if (p === null) roots.push(n);
      else childrenOf.set(p, [...(childrenOf.get(p) ?? []), n]);
    }

    // compactBox lays out one tree — wrap multiple top-level ideas under an
    // invisible root so they still compact together instead of each
    // getting their own disconnected layout.
    const toHData = (n: TreeNode): HData => ({ id: n.id, children: (childrenOf.get(n.id) ?? []).map(toHData) });
    const hiRoot: HData = { id: "__root__", children: roots.map(toHData) };

    const laidOut = compactBox(hiRoot, {
      direction: "LR",
      getId: (d) => (d as HData).id,
      getWidth: (d) => ((d as HData).id === "__root__" ? 1 : boxWidth(byId.get((d as HData).id)?.label ?? "")),
      getHeight: () => 30,
      getHGap: () => 56,
      getVGap: () => 10,
    });

    const g6Nodes: NodeData[] = [];
    const g6Edges: EdgeData[] = [];
    laidOut.eachNode((hn) => {
      if (hn.id === "__root__") return;
      const n = byId.get(hn.id);
      if (!n) return;
      const st = styleFor(n.kind);
      const kindLabel = (KIND_PLAIN[n.kind.toLowerCase()] ?? n.kind).toUpperCase();
      g6Nodes.push({
        id: n.id,
        style: {
          x: hn.x + hn.width / 2,
          y: hn.y + hn.height / 2,
          size: [hn.width, 30],
          radius: 7,
          fill: st.fill,
          stroke: n.highlight ? "#1c1917" : st.stroke,
          lineWidth: n.highlight ? 2.5 : 1.2,
          labelText: `${kindLabel}  ${n.label}${n.note ? ` — ${n.note}` : ""}`,
          labelFill: st.text,
          labelFontSize: 10,
          labelFontWeight: 500,
          labelWordWrap: true,
          labelWordWrapWidth: hn.width - 12,
          labelMaxLines: 2,
          labelTextOverflow: "ellipsis",
          labelTextAlign: "center",
          labelPlacement: "center",
        },
        data: { label: n.label },
      });
      if (n.parent && byId.has(n.parent)) {
        g6Edges.push({
          id: `e-${n.id}`,
          source: n.parent,
          target: n.id,
          style: { stroke: "#d6d3d1", lineWidth: 1.2 },
        });
      }
    });

    const leafCount = nodes.filter((n) => !childrenOf.has(n.id)).length;
    host.style.height = `${Math.max(260, Math.min(560, leafCount * 38))}px`;

    const graph = new Graph({
      container: host,
      autoResize: true,
      autoFit: "view",
      padding: 24,
      data: { nodes: g6Nodes, edges: g6Edges },
      node: { type: "rect" },
      edge: { type: "cubic-horizontal" },
      behaviors: ["drag-canvas", "zoom-canvas"],
    });
    void graph.render();

    // RFC-0024 stage 4 — the chip-first upgrade of today's ask-only click
    // (ux-spec §5.2 "Tree node": SCOPE + FOCUS + CHIP). SCOPE lands in the
    // Atlas selection (module level); the chip rides along; the ask stays
    // for nodes that resolve to nothing scopeable.
    const onClick = (e: IElementEvent) => {
      const id = e.target?.id;
      if (id == null) return;
      const datum = graph.getNodeData(id as string);
      const label = (datum?.data?.label as string | undefined) ?? String(id);
      const node = nodes.find((n) => n.id === id);
      if (node) {
        setAtlasSelection({ kind: node.parent === null ? "container" : "module", id: node.label, label: node.label });
        addChip({ id: `tree:${node.id}`, label: node.label, kind: "module", sourceTitle: "Concept tree" });
      }
      ask(spec, { name: label });
    };
    const onEnter = () => { host.style.cursor = "pointer"; };
    const onLeave = () => { host.style.cursor = "default"; };
    graph.on("node:click", onClick);
    graph.on("node:pointerenter", onEnter);
    graph.on("node:pointerleave", onLeave);

    const ro = new ResizeObserver(() => {
      graph.resize();
      void graph.fitView();
    });
    ro.observe(host);

    return () => {
      ro.disconnect();
      graph.off("node:click", onClick);
      graph.off("node:pointerenter", onEnter);
      graph.off("node:pointerleave", onLeave);
      graph.destroy();
    };
  }, [nodes, ask, spec, setAtlasSelection, addChip]);

  if (!nodes.length) return null;

  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <div ref={hostRef} className="w-full rounded-lg border border-stone-200/70" role="img" aria-label={`Concept hierarchy, ${nodes.length} nodes`} />
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT} · drag to pan, scroll to zoom
        {nodes.some((n) => n.highlight) ? " · dark outline = what you asked about" : ""}
      </p>
    </div>
  );
}
