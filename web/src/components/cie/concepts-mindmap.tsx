"use client";

// Concepts mind map — click the root to open its concepts, click a concept
// to open ITS concepts, one level at a time (G6 + @antv/hierarchy
// `compactBox`, the same host-side tree pre-layout g6-engine.ts uses for
// every tree-shaped ViewSpec). The flat "Concept nodes" table elsewhere on
// this page stays the complete, searchable fallback — this view is a
// navigation aid over the same concepts.json tree, not a new data source.

import { useEffect, useMemo, useRef, useState } from "react";
import type { Graph as G6Graph, IElementEvent } from "@antv/g6";
import { compactBox } from "@antv/hierarchy";
import { ChevronRight } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import type { ConceptNode } from "@/lib/cie-types";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

const KIND_COLOR: Record<string, string> = {
  Domain: "#8957e5",
  Subdomain: "#8957e5",
  Container: "#1783ff",
  Service: "#1783ff",
  UseCase: "#2ea043",
  Aggregate: "#d29922",
  Entity: "#d29922",
  Rule: "#cf222e",
  Invariant: "#cf222e",
  Module: "#57606a",
  Value: "#57606a",
};

const MAX_CHILDREN_SHOWN = 60;

interface HData {
  id: string;
  children: HData[];
}

function boxWidth(label: string): number {
  return Math.max(90, Math.min(260, label.length * 6.4 + 28));
}

export function ConceptsMindMap() {
  const { concepts, report } = useCie();
  const host = useRef<HTMLDivElement>(null);
  const [trail, setTrail] = useState<number[]>([]);
  const [detailId, setDetailId] = useState<number | null>(null);

  const byId = useMemo(
    () => new Map((concepts?.nodes ?? []).map((n) => [n.id, n])),
    [concepts],
  );
  const root = useMemo(
    () => (concepts?.nodes ?? []).find((n) => n.parent === null) ?? null,
    [concepts],
  );

  useEffect(() => {
    if (root && trail.length === 0) setTrail([root.id]);
  }, [root, trail.length]);

  const focusId = trail[trail.length - 1] ?? root?.id ?? null;
  const focusNode = focusId != null ? byId.get(focusId) ?? null : null;

  const symbolNames = useMemo(() => {
    const m = new Map<number, string>();
    for (const s of report?.symbols ?? []) m.set(s.id, s.qual_name);
    return m;
  }, [report]);

  const allChildren = useMemo(
    () =>
      (focusNode?.children ?? [])
        .map((id) => byId.get(id))
        .filter((n): n is ConceptNode => !!n),
    [focusNode, byId],
  );
  const children = useMemo(
    () =>
      [...allChildren]
        .sort((a, b) => b.children.length - a.children.length || b.symbols.length - a.symbols.length)
        .slice(0, MAX_CHILDREN_SHOWN),
    [allChildren],
  );
  const hiddenCount = allChildren.length - children.length;

  useEffect(() => {
    const container = host.current;
    if (!container || !focusNode) return;
    let cancelled = false;
    let graph: G6Graph | undefined;
    void (async () => {
      const { Graph } = await import("@antv/g6");
      if (cancelled) return;

      const hiRoot: HData = {
        id: String(focusNode.id),
        children: children.map((c) => ({ id: String(c.id), children: [] })),
      };
      const laid = compactBox(hiRoot, {
        direction: "LR" as const,
        getId: (d: unknown) => (d as HData).id,
        getWidth: (d: unknown) =>
          (d as HData).id === String(focusNode.id)
            ? boxWidth(focusNode.label) + 10
            : boxWidth(byId.get(Number((d as HData).id))?.label ?? ""),
        getHeight: () => 32,
        getHGap: () => 80,
        getVGap: () => 12,
      });
      const pos = new Map<string, { x: number; y: number }>();
      laid.eachNode((hn: { id: string; x: number; y: number; width: number; height: number }) => {
        pos.set(hn.id, { x: hn.x + hn.width / 2, y: hn.y + hn.height / 2 });
      });

      const allNodes = [focusNode, ...children];
      const nodes = allNodes.map((n) => {
        const p = pos.get(String(n.id)) ?? { x: 0, y: 0 };
        const isFocus = n.id === focusNode.id;
        const w = boxWidth(n.label) + (isFocus ? 10 : 0);
        return {
          id: String(n.id),
          style: {
            x: p.x,
            y: p.y,
            size: [w, isFocus ? 40 : 32] as [number, number],
            labelText: n.label,
            labelFontSize: isFocus ? 12 : 10.5,
            labelFontWeight: isFocus ? 700 : 500,
            labelPlacement: "center" as const,
            labelWordWrap: true,
            labelWordWrapWidth: w - 10,
            labelMaxLines: 2,
            radius: 8,
            fill: `${KIND_COLOR[n.kind] ?? "#57606a"}${isFocus ? "33" : "1a"}`,
            stroke: KIND_COLOR[n.kind] ?? "#57606a",
            lineWidth: isFocus ? 2.2 : 1.3,
            cursor: "pointer" as const,
          },
        };
      });
      const edges = children.map((c) => ({
        id: `e:${focusNode.id}:${c.id}`,
        source: String(focusNode.id),
        target: String(c.id),
        style: { stroke: "#9aa39c", lineWidth: 1.2, endArrow: false },
      }));

      const height = Math.max(320, Math.min(1400, 70 + children.length * 42));
      container.style.height = `${height}px`;

      graph = new Graph({
        container,
        autoResize: true,
        autoFit: "view",
        padding: 24,
        data: { nodes, edges },
        node: { type: "rect" },
        edge: { type: "cubic-horizontal" },
        behaviors: ["zoom-canvas", "drag-canvas"],
      });
      graph.on("node:click", (e: IElementEvent) => {
        const id = Number(e.target?.id);
        if (!Number.isFinite(id) || id === focusNode.id) return;
        const node = byId.get(id);
        if (!node) return;
        if (node.children.length > 0) setTrail((t) => [...t, id]);
        else setDetailId(id);
      });
      await graph.render();
    })();
    return () => {
      cancelled = true;
      void graph?.destroy();
    };
  }, [focusNode, children, byId]);

  if (!concepts || !root) {
    return <p className="text-xs text-stone-500 px-1">No concept tree loaded for this project.</p>;
  }

  const detailNode = detailId != null ? byId.get(detailId) ?? null : null;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-1 text-xs flex-wrap px-1">
        {trail.map((id, i) => {
          const n = byId.get(id);
          if (!n) return null;
          return (
            <span key={id} className="flex items-center gap-1">
              {i > 0 && <ChevronRight className="h-3 w-3 text-stone-400" aria-hidden />}
              <button
                type="button"
                className={
                  i === trail.length - 1
                    ? "font-semibold text-stone-900"
                    : "text-emerald-700 hover:underline"
                }
                onClick={() => setTrail(trail.slice(0, i + 1))}
              >
                {n.label} <span className="text-stone-400">({n.kind})</span>
              </button>
            </span>
          );
        })}
      </div>
      <div
        ref={host}
        className="w-full rounded-lg border border-stone-200 bg-stone-50"
        role="img"
        aria-label="Concept mind map — click a node with children to drill in"
      />
      {focusNode && (
        <p className="text-[11px] text-stone-500 px-1">
          {allChildren.length} child concept{allChildren.length === 1 ? "" : "s"} ·{" "}
          {focusNode.symbols.length} symbol{focusNode.symbols.length === 1 ? "" : "s"} · confidence{" "}
          {(focusNode.confidence ?? 1).toFixed(2)}
          {hiddenCount > 0 &&
            ` — showing the ${MAX_CHILDREN_SHOWN} busiest of ${allChildren.length}; use the table below to search the rest.`}
        </p>
      )}
      {detailNode && (
        <Card id="concept-detail">
          <CardContent className="p-3 space-y-1.5 text-xs">
            <div className="flex items-center gap-2">
              <span className="font-semibold text-sm text-stone-900">{detailNode.label}</span>
              <Badge variant="outline" className="text-[10px]">
                {detailNode.kind}
              </Badge>
              {detailNode.confidence != null && (
                <span className="text-stone-400">confidence {detailNode.confidence.toFixed(2)}</span>
              )}
            </div>
            {detailNode.symbols.length > 0 && (
              <div>
                <span className="text-stone-500">Symbols: </span>
                {detailNode.symbols.map((sid) => symbolNames.get(sid) ?? `#${sid}`).join(", ")}
              </div>
            )}
            {detailNode.behaviors.length > 0 && (
              <div>
                <span className="text-stone-500">Behaviors: </span>
                {detailNode.behaviors.map((b) => b.kind).join(", ")}
              </div>
            )}
            {detailNode.rules && detailNode.rules.length > 0 && (
              <div>
                <span className="text-stone-500">Rules: </span>
                {detailNode.rules
                  .map((r) => `${r.kind ?? "?"}${r.status ? ` (${r.status})` : ""}`)
                  .join(", ")}
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
