"use client";

// Renders the chart the AI attached to an answer.
//
// Story lineage (github.com/arunsoman/cie viz-adapter/src/interaction.ts):
//   US-020 — chart picked from the CIE chart catalog; engine + layout are
//            disclosed on the chart footnote, not guessed silently.
//   US-023 — hover-expansion fisheye lens: magnify locally; the base chart
//            (total height, row order, value axis) never mutates; the data
//            shown is the same spec-derived data with its source stated.
//   US-030 — the rendering strategy is stated (footnote), not vibes.
//   US-140 — every pointer action has a keyboard equivalent (arrows move the
//            lens centre, Escape releases it).
//   US-142 — never colour alone: every bar/row keeps its text label + value.

import { useEffect, useId, useRef, useState } from "react";
import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import { BarChart3, MessageCircleQuestion, MoveVertical } from "lucide-react";
import type { ChatChartSpec, UmlNode } from "@/lib/chat-chart";
import { useCie } from "@/lib/cie-store";
import { askAboutChartElement } from "@/lib/quality";

const HEX: Record<string, string> = {
  rose: "#e11d48",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
};
const hex = (c?: string) => HEX[c ?? ""] ?? "#78716c";

/**
 * Click-to-chat: tapping a chart element starts a conversation about it.
 * The question carries the chart title + the element's name and value, so
 * the local quality/story answerers (or the AI, as fallback) can ground the
 * reply in the collected data. Nothing is invented by the chart itself.
 */
function useAsk() {
  const setPendingAsk = useCie((s) => s.setPendingAsk);
  return (spec: ChatChartSpec, el: { name: string; value?: number | string }) =>
    setPendingAsk(askAboutChartElement(spec.title, el));
}

const ASK_HINT = "tap any element to ask CIE about it";

const TOOLTIP_STYLE = {
  borderRadius: 10,
  border: "1px solid #e7e5e4",
  boxShadow: "0 4px 16px rgba(0,0,0,0.06)",
  fontSize: 12.5,
  padding: "8px 12px",
  background: "#ffffff",
} as const;

// ---------- footnote: disclosed engine + layout + source (US-020/023/030) ----------

function ChartFootnote({ spec }: { spec: ChatChartSpec }) {
  return (
    <p className="mt-2 pt-2 border-t border-stone-200/70 text-[10px] text-stone-400 flex items-center gap-1.5 flex-wrap">
      <BarChart3 className="h-3 w-3" />
      <span>
        {spec.catalog ? `catalog ${spec.catalog} · ` : ""}
        {spec.engine} · {spec.layout}
        {spec.kind === "hbar" && spec.lens !== false ? " · fisheye lens: hover or use ↑↓" : ""}
        {(spec.kind === "uml" || spec.kind === "flow") ? " · hover a box to light up its connections" : ""}
        {spec.source ? ` · built from ${spec.source}` : " · drawn from your loaded report"}
      </span>
    </p>
  );
}

// ---------- hbar with hover-expansion fisheye lens (US-023) ----------

const LENS_MAG = 1.7; // peak magnification at the lens centre
const LENS_SIGMA = 46; // falloff radius in px (Gaussian)

/**
 * 1-D fisheye over row heights: every row's magnification is
 *   m(y) = 1 + (MAG-1)·exp(-(Δy/σ)²)
 * heights are then rescaled to keep the chart's total height CONSTANT, so
 * the base geometry outside the lens is untouched — only the local
 * neighbourhood expands. The value axis (bar widths) never changes.
 */
function fisheyeMagnifications(centres: number[], focusY: number | null): number[] {
  if (focusY == null) return centres.map(() => 1);
  return centres.map((cy) => {
    const d = (cy - focusY) / LENS_SIGMA;
    return 1 + (LENS_MAG - 1) * Math.exp(-d * d);
  });
}

function FisheyeHBar({ spec }: { spec: ChatChartSpec }) {
  const rows = (spec.data ?? []).map((d) => ({ ...d, color: hex(d.color) }));
  const ask = useAsk();
  const W = 460;
  const H = Math.max(150, rows.length * 34 + 10);
  const labelW = 150;
  const valueW = 44;
  const barX = labelW + 8;
  const barMax = W - barX - valueW - 8;

  const [focusY, setFocusY] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  const maxValue = Math.max(...rows.map((r) => r.value), 1);

  // Base (unfisheye'd) row geometry.
  const rowH = H / rows.length;
  const baseCentres = rows.map((_, i) => i * rowH + rowH / 2);

  const mags = fisheyeMagnifications(baseCentres, focusY);

  // Rescale heights so Σheights stays H (base chart keeps its footprint).
  const sumM = mags.reduce((a, b) => a + b, 0);
  const heights = mags.map((m) => (m / sumM) * H);

  // Recomputed centres (cumulative) — far rows stay at their base positions.
  const centres: number[] = [];
  let acc = 0;
  for (let i = 0; i < rows.length; i++) {
    centres.push(acc + heights[i] / 2);
    acc += heights[i];
  }

  const pointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const y = ((e.clientY - rect.top) / rect.height) * H;
    setFocusY(y);
  };
  const pointerLeave = () => setFocusY(null);

  // US-140: keyboard parity — arrows move the lens, Escape releases it.
  const keyDown = (e: React.KeyboardEvent) => {
    const idx = focusY == null ? -1 : Math.min(rows.length - 1, Math.max(0, Math.floor(focusY / rowH)));
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = e.key === "ArrowDown" ? Math.min(rows.length - 1, idx + 1) : Math.max(0, idx < 0 ? 0 : idx - 1);
      setFocusY(baseCentres[next]);
    } else if (e.key === "Home") {
      e.preventDefault();
      setFocusY(baseCentres[0]);
    } else if (e.key === "End") {
      e.preventDefault();
      setFocusY(baseCentres[rows.length - 1]);
    } else if (e.key === "Escape") {
      setFocusY(null);
    }
  };

  const lensActive = focusY != null;

  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <svg
        ref={svgRef}
        viewBox={`0 0 ${W} ${H}`}
        width="100%"
        height={H}
        role="img"
        aria-label={`Bar chart, ${rows.length} rows: ${rows.map((r) => `${r.name} ${r.value}`).join(", ")}`}
        tabIndex={0}
        onPointerMove={pointerMove}
        onPointerLeave={pointerLeave}
        onKeyDown={(e) => {
          keyDown(e);
          if (e.key === "Enter" || e.key === " ") {
            const idx = focusY == null ? -1 : Math.min(rows.length - 1, Math.max(0, Math.floor(focusY / rowH)));
            if (idx >= 0) {
              e.preventDefault();
              ask(spec, { name: rows[idx].name, value: rows[idx].value });
            }
          }
        }}
        className="rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-emerald-600/40"
      >
        {/* rows — order and value axis never change; only local magnification.
            Geometry transitions (90ms) smooth pointer/keyboard lens moves. */}
        {rows.map((r, i) => {
          const h = Math.max(6, heights[i] - 3);
          const cy = centres[i];
          const w = Math.max(3, (r.value / maxValue) * barMax);
          // US-142: label + value always visible; the lens only changes size.
          const labelSize = 11 + 2.5 * (mags[i] - 1);
          return (
            <g
              key={i}
              onClick={() => ask(spec, { name: r.name, value: r.value })}
              className="cursor-pointer"
            >
              <title>{`${r.name}: ${r.value} — ${ASK_HINT}`}</title>
              <text
                x={labelW}
                y={cy + labelSize * 0.35}
                textAnchor="end"
                fontSize={labelSize}
                fill={mags[i] > 1.25 ? "#1c1917" : "#57534e"}
                fontWeight={mags[i] > 1.25 ? 600 : 400}
                style={{ transition: "fill 120ms" }}
              >
                {r.name.length > 19 ? r.name.slice(0, 18) + "…" : r.name}
              </text>
              <rect x={barX} y={cy - h / 2} width={barMax} height={h} rx={4} fill="#f5f5f4" opacity={lensActive ? 0.5 : 1} style={{ transition: "y 90ms ease-out, height 90ms ease-out" }} />
              <rect x={barX} y={cy - h / 2} width={w} height={h} rx={4} fill={r.color} style={{ transition: "y 90ms ease-out, height 90ms ease-out, fill 120ms" }} />
              <text
                x={barX + w + 6}
                y={cy + 4}
                fontSize={labelSize}
                fill={mags[i] > 1.25 ? "#1c1917" : "#78716c"}
                fontWeight={mags[i] > 1.25 ? 700 : 500}
                style={{ transition: "y 90ms ease-out, fill 120ms" }}
              >
                {r.value}
              </text>
            </g>
          );
        })}
      </svg>
      {rows.length >= 4 && (
        <p className="text-[10px] text-stone-400 flex items-center gap-1 -mt-0.5">
          <MoveVertical className="h-3 w-3" /> hover a row — or use ↑ ↓ keys — to magnify that neighbourhood
        </p>
      )}
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-0.5">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT}
      </p>
    </div>
  );
}

// ---------- donut (recharts) ----------

function ChatDonut({ spec }: { spec: ChatChartSpec }) {
  const rows = (spec.data ?? []).map((d) => ({ ...d, color: hex(d.color) }));
  const ask = useAsk();
  const total = rows.reduce((a, d) => a + d.value, 0);
  if (!total) return null;
  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1">{spec.tell}</p>}
      <div className="flex items-center gap-3">
        <div className="relative h-[120px] w-[120px] shrink-0">
          <ResponsiveContainer width="100%" height="100%">
            <PieChart>
              <Pie data={rows} dataKey="value" nameKey="name" innerRadius={36} outerRadius={55} paddingAngle={2} strokeWidth={2} stroke="#fff">
                {rows.map((d, i) => (
                  <Cell key={i} fill={d.color} className="cursor-pointer" onClick={() => ask(spec, { name: d.name, value: d.value })} />
                ))}
              </Pie>
              <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(value: number | string, name) => [`${value}`, String(name)]} />
            </PieChart>
          </ResponsiveContainer>
          <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
            <span className="text-lg font-bold text-stone-900">{total}</span>
            <span className="text-[9px] text-stone-500 text-center leading-tight px-3">{spec.center ?? "in total"}</span>
          </div>
        </div>
        <ul className="space-y-1 text-xs min-w-0">
          {rows.map((d) => (
            <li
              key={d.name}
              className="flex items-center gap-1.5 cursor-pointer rounded px-1 -mx-1 hover:bg-stone-100"
              onClick={() => ask(spec, { name: d.name, value: d.value })}
              title={`${d.name}: ${d.value} — ${ASK_HINT}`}
            >
              <span className="h-2 w-2 rounded-full shrink-0" style={{ background: d.color }} aria-hidden />
              <span className="text-stone-700 flex-1 min-w-0 truncate">{d.name}</span>
              <span className="font-semibold text-stone-900 tabular-nums">{d.value}</span>
            </li>
          ))}
        </ul>
      </div>
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT}
      </p>
    </div>
  );
}

// ---------- stat (hero number) ----------

function ChatStat({ spec }: { spec: ChatChartSpec }) {
  const d = spec.data?.[0];
  const ask = useAsk();
  if (!d) return null;
  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-1">{spec.title}</p>}
      <button
        type="button"
        onClick={() => ask(spec, { name: d.name, value: d.value })}
        title={`${d.name} — ${ASK_HINT}`}
        className="w-full text-left rounded-xl bg-emerald-50 border border-emerald-100 px-4 py-3 flex items-center gap-3 cursor-pointer hover:bg-emerald-100/70 transition-colors"
      >
        <span className="text-3xl font-bold text-emerald-800 tabular-nums">
          {d.value}
          {spec.unit && <span className="text-base font-semibold text-emerald-700 ml-0.5">{spec.unit}</span>}
        </span>
        <span className="text-sm text-emerald-900 leading-snug">{d.name}</span>
      </button>
      {spec.tell && <p className="text-xs text-stone-500 mt-1.5">{spec.tell}</p>}
    </div>
  );
}

// ---------- table (scroll rows) ----------

function ChatTable({ spec }: { spec: ChatChartSpec }) {
  const ask = useAsk();
  if (!spec.columns?.length || !spec.rows?.length) return null;
  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <div className="max-h-56 overflow-y-auto rounded-lg border border-stone-200">
        <table className="w-full text-xs">
          <thead className="bg-stone-50 sticky top-0">
            <tr>
              {spec.columns.map((c) => (
                <th key={c} className="text-left font-semibold text-stone-600 px-2.5 py-1.5 border-b border-stone-200">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {spec.rows.map((r, i) => (
              <tr
                key={i}
                className={`${i % 2 ? "bg-stone-50/50" : ""} cursor-pointer hover:bg-emerald-50/60`}
                onClick={() => ask(spec, { name: String(r[0]) })}
                title={`${r[0]} — ${ASK_HINT}`}
              >
                {r.map((cell, j) => (
                  <td key={j} className={`px-2.5 py-1.5 text-stone-700 ${j === 0 ? "font-medium text-stone-900" : "tabular-nums"} border-b border-stone-100`}>
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT}
      </p>
    </div>
  );
}

// ---------- UML-style diagrams (uml = structure, flow = who calls whom) ----------
//
// Boxes + arrows built with plain SVG, same visual language as the map:
// emerald = proven, amber = good guess (dashed), grey = not sure yet.
// The boxes never come from the model — draw-anything.ts fills them from the
// loaded store, so every label is traceable.
// Layout mirrors the engine's own band hint (domain → modules → use-cases →
// rules); flows layer callers above callees (longest call path).

type Cert = "Fact" | "Inference" | "Hypothesis";

const CERT_STYLE: Record<Cert, { fill: string; stroke: string; text: string; dash?: string }> = {
  Fact: { fill: "#ecfdf5", stroke: "#059669", text: "#065f46" },
  Inference: { fill: "#fffbeb", stroke: "#d97706", text: "#78350f", dash: "5 3" },
  Hypothesis: { fill: "#f5f5f4", stroke: "#78716c", text: "#44403c", dash: "2 3" },
};

const KIND_PLAIN: Record<string, string> = {
  domain: "The big picture",
  module: "A code file",
  usecase: "Something it does",
  rule: "A rule it follows",
  invariant: "Must always be true",
  function: "A named thing",
  step: "A step",
};

/** Greedy word wrap — returns at most maxLines lines, last one ellipsised. */
function wrap(s: string, maxChars: number, maxLines: number): string[] {
  const words = s.split(/\s+/).filter(Boolean);
  const out: string[] = [];
  let cur = "";
  for (const w of words) {
    if (cur && (cur + " " + w).length > maxChars) {
      out.push(cur);
      cur = w;
      if (out.length === maxLines) break;
    } else {
      cur = cur ? cur + " " + w : w;
    }
  }
  if (out.length < maxLines && cur) out.push(cur);
  if (out.length === maxLines) {
    // did we drop anything?
    const consumed = out.join(" ").length;
    if (consumed < s.length - 1) out[maxLines - 1] = out[maxLines - 1].slice(0, Math.max(0, maxChars - 1)) + "…";
  }
  return out.length ? out : [s.slice(0, maxChars - 1) + "…"];
}

interface Box {
  node: UmlNode;
  row: number;
  x: number;
  y: number;
  w: number;
  h: number;
  titleLines: string[];
}

const DG_W = 960;
const DG_MARGIN = 26;
const DG_GAP = 16;
const ROW_GAP = 34;

function DiagramSVG({ spec }: { spec: ChatChartSpec }) {
  const uid = useId().replace(/[:]/g, "");
  const [hover, setHover] = useState<string | null>(null);
  const ask = useAsk();
  const nodes = spec.nodes ?? [];
  const links = spec.links ?? [];

  // ---- rows ----
  const rows: UmlNode[][] = [];
  if (spec.kind === "flow") {
    // depth = longest call chain from a root; callers sit above callees
    const depth = new Map<string, number>();
    const calc = (id: string, seen: Set<string>): number => {
      if (depth.has(id)) return depth.get(id)!;
      if (seen.has(id)) return 0; // cycle guard
      seen.add(id);
      const callers = links.filter((l) => l.to === id).map((l) => l.from);
      const d = callers.length ? Math.max(...callers.map((c) => calc(c, seen) + 1)) : 0;
      depth.set(id, d);
      return d;
    };
    for (const n of nodes) calc(n.id, new Set());
    const byDepth = new Map<number, UmlNode[]>();
    for (const n of nodes) {
      const d = depth.get(n.id) ?? 0;
      byDepth.set(d, [...(byDepth.get(d) ?? []), n]);
    }
    [...byDepth.keys()].sort((a, b) => a - b).forEach((d) => rows.push(byDepth.get(d)!));
  } else {
    const bands: { kinds: string[] }[] = [
      { kinds: ["domain"] },
      { kinds: ["module"] },
      { kinds: ["usecase"] },
      { kinds: ["rule", "invariant"] },
    ];
    const known = new Set(bands.flatMap((b) => b.kinds));
    for (const b of bands) {
      const group = nodes.filter((n) => b.kinds.includes(n.kind));
      if (group.length) rows.push(group);
    }
    const rest = nodes.filter((n) => !known.has(n.kind));
    if (rest.length) rows.push(rest);
  }

  // ---- geometry ----
  const boxes: Box[] = [];
  let y = DG_MARGIN + 6;
  rows.forEach((row, ri) => {
    const metas = row.map((n) => {
      const isLong = n.kind === "rule" || n.kind === "invariant";
      const titleLines = isLong ? wrap(n.title, 30, 2) : n.title.length > 30 ? wrap(n.title, 28, 2) : [n.title];
      const body = n.lines ?? [];
      const bodyLines = body.map((l) => (l.length > 26 ? l.slice(0, 25) + "…" : l));
      const textW = Math.max(...titleLines.map((t) => t.length), ...bodyLines.map((t) => t.length), 8);
      const w = Math.min(240, Math.max(112, textW * 6.3 + 26));
      const h = 8 + titleLines.length * 13.5 + (bodyLines.length ? 6 + bodyLines.length * 12.5 + (n.more ? 11 : 0) : 0) + 8;
      return { node: n, titleLines, bodyLines, w, h };
    });
    // wrap wide rows
    const line: Box[] = [];
    let rowW = 0;
    const pushRow = () => {
      const total = line.reduce((s, b) => s + b.w + DG_GAP, -DG_GAP);
      let x = Math.max(DG_MARGIN, (DG_W - total) / 2);
      const baseY = y;
      let maxH = 0;
      for (const b of line) {
        b.x = x;
        b.y = baseY;
        x += b.w + DG_GAP;
        maxH = Math.max(maxH, b.h);
        boxes.push(b);
      }
      y = baseY + maxH + ROW_GAP;
    };
    for (const m of metas) {
      const b: Box = { node: m.node, row: ri, x: 0, y: 0, w: m.w, h: m.h, titleLines: m.titleLines };
      if (rowW + m.w > DG_W - 2 * DG_MARGIN && line.length) {
        pushRow();
        line.length = 0;
        rowW = 0;
      }
      line.push(b);
      rowW += m.w + DG_GAP;
    }
    if (line.length) pushRow();
  });
  const vbH = Math.max(y - ROW_GAP + DG_MARGIN, 320);
  const boxById = new Map(boxes.map((b) => [b.node.id, b]));

  // ---- arrow routing ----
  const edgePath = (a: Box, b: Box) => {
    const xc1 = a.x + a.w / 2;
    const xc2 = b.x + b.w / 2;
    if (b.row > a.row) {
      const y1 = a.y + a.h;
      const y2 = b.y;
      const dy = Math.max(24, Math.abs(y2 - y1) / 2);
      return `M ${xc1} ${y1} C ${xc1} ${y1 + dy}, ${xc2} ${y2 - dy}, ${xc2} ${y2}`;
    }
    if (b.row === a.row) {
      const leftToRight = a.x + a.w <= b.x;
      const rightToLeft = b.x + b.w <= a.x;
      if (leftToRight || rightToLeft) {
        const [s, t] = leftToRight ? [a, b] : [b, a];
        const mx = (s.x + s.w + t.x) / 2;
        return `M ${s.x + s.w} ${s.y + s.h / 2} C ${mx} ${s.y + s.h / 2}, ${mx} ${t.y + t.h / 2}, ${t.x} ${t.y + t.h / 2}`;
      }
      const y = Math.max(a.y + a.h, b.y + b.h);
      return `M ${xc1} ${a.y + a.h} C ${xc1} ${y + 22}, ${xc2} ${y + 22}, ${xc2} ${b.y + b.h}`;
    }
    // backward: loop around the side
    if (a.x >= b.x + b.w) {
      const bx = Math.min(a.x, b.x + b.w) - 44;
      return `M ${a.x} ${a.y + a.h / 2} C ${bx} ${a.y + a.h / 2}, ${bx} ${b.y + b.h / 2}, ${b.x + b.w} ${b.y + b.h / 2}`;
    }
    const bx = Math.max(a.x + a.w, b.x + b.w) + 44;
    return `M ${a.x + a.w} ${a.y + a.h / 2} C ${bx} ${a.y + a.h / 2}, ${bx} ${b.y + b.h / 2}, ${b.x + b.w} ${b.y + b.h / 2}`;
  };

  // bezier midpoint for the edge label
  const mid = (a: Box, b: Box) => {
    const d = edgePath(a, b);
    const n = d.match(/-?[\d.]+/g)?.map(Number) ?? [];
    if (n.length < 8) return null;
    const [x0, y0, x1, y1, x2, y2, x3, y3] = n;
    return {
      x: (x0 + 3 * x1 + 3 * x2 + x3) / 8,
      y: (y0 + 3 * y1 + 3 * y2 + y3) / 8,
    };
  };

  const lit = (linkFrom: string, linkTo: string) => !hover || hover === linkFrom || hover === linkTo;

  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 mb-1.5 text-[10.5px] text-stone-500">
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm border" style={{ background: CERT_STYLE.Fact.fill, borderColor: CERT_STYLE.Fact.stroke }} /> proven</span>
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm border border-dashed" style={{ background: CERT_STYLE.Inference.fill, borderColor: CERT_STYLE.Inference.stroke }} /> good guess</span>
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm border border-dotted" style={{ background: CERT_STYLE.Hypothesis.fill, borderColor: CERT_STYLE.Hypothesis.stroke }} /> not sure yet</span>
      </div>
      <div className="overflow-x-auto rounded-lg border border-stone-200 bg-white">
        <svg
          viewBox={`0 0 ${DG_W} ${vbH}`}
          className="block min-w-[620px]"
          width="100%"
          role="img"
          aria-label={`${spec.title ?? "Diagram"}: ${nodes.length} boxes, ${links.length} connections`}
        >
          <defs>
            <marker id={`${uid}-af`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
              <path d="M 0 0 L 10 5 L 0 10 z" fill={CERT_STYLE.Fact.stroke} />
            </marker>
            <marker id={`${uid}-ai`} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
              <path d="M 0 0 L 10 5 L 0 10 z" fill={CERT_STYLE.Inference.stroke} />
            </marker>
          </defs>

          {/* arrows first (under boxes) */}
          {links.map((l, i) => {
            const a = boxById.get(l.from);
            const b = boxById.get(l.to);
            if (!a || !b) return null;
            const fact = (l.certainty ?? "Fact") === "Fact";
            const color = fact ? CERT_STYLE.Fact.stroke : CERT_STYLE.Inference.stroke;
            const on = lit(l.from, l.to);
            const m = mid(a, b);
            return (
              <g key={i} opacity={on ? 0.85 : 0.12} style={{ transition: "opacity 120ms" }}>
                <path
                  d={edgePath(a, b)}
                  fill="none"
                  stroke={color}
                  strokeWidth={on && hover ? 2 : 1.2}
                  strokeDasharray={l.dashed ? "5 4" : undefined}
                  markerEnd={fact ? `url(#${uid}-af)` : `url(#${uid}-ai)`}
                />
                {l.label && m && (!hover || on) && (
                  <text
                    x={m.x}
                    y={m.y - 3}
                    textAnchor="middle"
                    fontSize="8.5"
                    fill="#78716c"
                    stroke="#ffffff"
                    strokeWidth="3"
                    paintOrder="stroke"
                  >
                    {l.label}
                  </text>
                )}
              </g>
            );
          })}

          {/* boxes — UML look: title compartment + body compartment */}
          {boxes.map((b) => {
            const cert = (b.node.certainty ?? "Inference") as Cert;
            const st = CERT_STYLE[cert];
            const isHover = hover === b.node.id;
            const body = b.node.lines ?? [];
            const bodyLines = body.map((l) => (l.length > 26 ? l.slice(0, 25) + "…" : l));
            const headH = 8 + b.titleLines.length * 13.5;
            const label = `${KIND_PLAIN[b.node.kind] ?? b.node.kind}: ${b.node.title} — ${cert === "Fact" ? "proven" : cert === "Inference" ? "good guess" : "not sure yet"}`;
            return (
              <g
                key={b.node.id}
                tabIndex={0}
                role="button"
                aria-label={label}
                onPointerEnter={() => setHover(b.node.id)}
                onPointerLeave={() => setHover(null)}
                onFocus={() => setHover(b.node.id)}
                onBlur={() => setHover(null)}
                onClick={() => ask(spec, { name: b.node.title })}
                className="outline-none cursor-pointer"
              >
                <title>{label}</title>
                <rect
                  x={b.x}
                  y={b.y}
                  width={b.w}
                  height={b.h}
                  rx={8}
                  fill={st.fill}
                  stroke={isHover ? "#1c1917" : st.stroke}
                  strokeWidth={isHover ? 2.4 : 1.4}
                  strokeDasharray={st.dash}
                  style={{ transition: "stroke-width 100ms" }}
                />
                {/* title compartment */}
                <line x1={b.x} y1={b.y + headH} x2={b.x + b.w} y2={b.y + headH} stroke={st.stroke} strokeWidth={0.8} opacity={0.55} />
                {b.titleLines.map((t, ti) => (
                  <text
                    key={ti}
                    x={b.x + b.w / 2}
                    y={b.y + 17 + ti * 13.5}
                    textAnchor="middle"
                    fontSize={b.titleLines.length > 1 ? 10.5 : 11.5}
                    fontWeight={700}
                    fill={st.text}
                  >
                    {t}
                  </text>
                ))}
                {/* body compartment */}
                {bodyLines.map((t, li) => (
                  <text key={li} x={b.x + 10} y={b.y + headH + 14 + li * 12.5} fontSize={10} fill="#57534e">
                    {t}
                  </text>
                ))}
                {b.node.more ? (
                  <text x={b.x + 10} y={b.y + headH + 14 + bodyLines.length * 12.5} fontSize={9} fontStyle="italic" fill="#a8a29e">
                    …and {b.node.more} more
                  </text>
                ) : null}
              </g>
            );
          })}
        </svg>
      </div>
      {nodes.length >= 12 && (
        <p className="text-[10px] text-stone-400 mt-1 flex items-center gap-1">
          <MoveVertical className="h-3 w-3" /> scroll sideways if the picture is wider than this box — or press “Show this chart big on the canvas”
        </p>
      )}
    </div>
  );
}

// ---------- concept hierarchy tree (custom-svg, indented levels) ----------
//
// Nodes come from concepts.json via quality.ts — the model never invents
// hierarchy nodes. Layout: an indented tree (Domain → Module → …) with elbow
// connectors; every node answers to a click (click-to-chat), like the map.

function TreeSVG({ spec }: { spec: ChatChartSpec }) {
  const ask = useAsk();
  const nodes = spec.tree ?? [];
  if (!nodes.length) return null;

  const byId = new Map(nodes.map((n) => [n.id, n]));
  const childrenOf = new Map<string | null, typeof nodes>();
  for (const n of nodes) {
    const p = n.parent && byId.has(n.parent) ? n.parent : null;
    childrenOf.set(p, [...(childrenOf.get(p) ?? []), n]);
  }
  // depth per node (cycle-guarded)
  const depth = new Map<string, number>();
  const walk = (n: (typeof nodes)[number], d: number, seen: Set<string>) => {
    if (seen.has(n.id) || seen.size > nodes.length + 2) return;
    seen.add(n.id);
    depth.set(n.id, d);
    for (const c of childrenOf.get(n.id) ?? []) walk(c, d + 1, seen);
  };
  for (const r of childrenOf.get(null) ?? []) walk(r, 0, new Set());
  for (const n of nodes) if (!depth.has(n.id)) depth.set(n.id, 0); // orphans surface at top level

  const COL_W = 200;
  const ROW_H = 40;
  const maxDepth = Math.max(0, ...[...depth.values()]);
  const W = Math.max(460, (maxDepth + 1) * COL_W + 40);
  const H = nodes.length * ROW_H + 16;

  // ordered list: parents before children, depth-first in given order
  const ordered: { n: (typeof nodes)[number]; d: number; y: number }[] = [];
  let y = 12;
  const emit = (id: string | null, seen: Set<string>) => {
    for (const c of childrenOf.get(id) ?? []) {
      if (seen.has(c.id)) continue;
      seen.add(c.id);
      const d = depth.get(c.id) ?? 0;
      ordered.push({ n: c, d, y: y + ROW_H / 2 });
      y += ROW_H;
      emit(c.id, seen);
    }
  };
  emit(null, new Set());

  const BOX_H = 28;
  const KIND_STYLE: Record<string, { fill: string; stroke: string; text: string }> = {
    Domain: { fill: "#ecfdf5", stroke: "#059669", text: "#065f46" },
    Module: { fill: "#f5f5f4", stroke: "#78716c", text: "#44403c" },
    UseCase: { fill: "#fffbeb", stroke: "#d97706", text: "#78350f" },
    Rule: { fill: "#fef2f2", stroke: "#e11d48", text: "#881337" },
  };
  const style = (k: string) => KIND_STYLE[k] ?? { fill: "#f5f5f4", stroke: "#a8a29e", text: "#44403c" };

  const posById = new Map(ordered.map((o) => [o.n.id, o]));

  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <div className="overflow-x-auto">
        <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} role="img" aria-label={`Concept hierarchy, ${nodes.length} nodes`}>
          {/* elbow connectors: parent right edge → child left edge */}
          {ordered.map(({ n, d, y: ny }) => {
            if (!n.parent) return null;
            const p = posById.get(n.parent);
            if (!p) return null;
            const x1 = 24 + p.d * COL_W + 176;
            const x2 = 24 + d * COL_W;
            const my = (p.y + ny) / 2;
            return (
              <path
                key={`e-${n.id}`}
                d={`M ${x1} ${p.y} C ${x1 + 14} ${p.y}, ${x1 + 14} ${my}, ${x2 - 6} ${my} L ${x2 - 6} ${ny}`}
                fill="none"
                stroke="#d6d3d1"
                strokeWidth={1.2}
              />
            );
          })}
          {ordered.map(({ n, d, y: ny }) => {
            const st = style(n.kind);
            const label = `${KIND_PLAIN[n.kind] ?? n.kind}: ${n.label}${n.note ? ` — ${n.note}` : ""}`;
            return (
              <g
                key={n.id}
                onClick={() => ask(spec, { name: n.label })}
                className="cursor-pointer"
                tabIndex={0}
                role="button"
                aria-label={`${label} — ${ASK_HINT}`}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    ask(spec, { name: n.label });
                  }
                }}
              >
                <title>{`${label} — ${ASK_HINT}`}</title>
                <rect
                  x={24 + d * COL_W}
                  y={ny - BOX_H / 2}
                  width={176}
                  height={BOX_H}
                  rx={7}
                  fill={st.fill}
                  stroke={st.stroke}
                  strokeWidth={1.2}
                />
                <text x={24 + d * COL_W + 10} y={ny - 6} fontSize={9} fill={st.text} fontWeight={700}>
                  {(KIND_PLAIN[n.kind] ?? n.kind).toUpperCase()}
                </text>
                <text x={24 + d * COL_W + 10} y={ny + 8} fontSize={11} fill="#1c1917" fontWeight={500}>
                  {n.label.length > 22 ? n.label.slice(0, 21) + "…" : n.label}
                  {n.note && <tspan dx={6} fontSize={9} fill="#78716c">{n.note}</tspan>}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT} · scroll sideways if the tree is wider than this box
      </p>
    </div>
  );
}

// ---------- heatmap (custom-svg grid — e.g. test coverage per file) ----------
//
// Tiles come from the measured export (quality.json) — never from the model.
// Never colour alone: every tile carries its percentage as text.

function Heatmap({ spec }: { spec: ChatChartSpec }) {
  const ask = useAsk();
  const tiles = spec.tiles ?? [];
  if (!tiles.length) return null;
  const tileColor = (v: number) => (v >= 70 ? "#d1fae5" : v >= 40 ? "#fef3c7" : v > 0 ? "#fee2e2" : "#ffe4e6");
  const tileStroke = (v: number) => (v >= 70 ? "#059669" : v >= 40 ? "#d97706" : "#e11d48");
  const TW = 148;
  const TH = 64;
  const GAP = 8;
  const perRow = Math.max(2, Math.min(4, Math.floor(480 / (TW + GAP))));
  const rows = Math.ceil(tiles.length / perRow);
  const W = perRow * (TW + GAP) + GAP;
  const H = rows * (TH + GAP) + GAP;

  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} role="img" aria-label={`Heat map of ${tiles.length} files: ${tiles.map((t) => `${t.name} ${t.value}%`).join(", ")}`}>
        {tiles.map((t, i) => {
          const x = GAP + (i % perRow) * (TW + GAP);
          const y = GAP + Math.floor(i / perRow) * (TH + GAP);
          return (
            <g key={`${t.name}-${i}`} onClick={() => ask(spec, { name: t.name, value: `${t.value}%` })} className="cursor-pointer">
              <title>{`${t.name}: ${t.value}% (${t.label ?? ""}) — ${ASK_HINT}`}</title>
              <rect x={x} y={y} width={TW} height={TH} rx={8} fill={tileColor(t.value)} stroke={tileStroke(t.value)} strokeWidth={1.2} />
              <text x={x + 10} y={y + 20} fontSize={11} fontWeight={600} fill="#1c1917">
                {t.name.length > 18 ? t.name.slice(0, 17) + "…" : t.name}
              </text>
              <text x={x + 10} y={y + 38} fontSize={15} fontWeight={700} fill={tileStroke(t.value)}>
                {t.value}%
              </text>
              {t.label && (
                <text x={x + 10} y={y + 53} fontSize={8.5} fill="#78716c">
                  {t.label.length > 22 ? t.label.slice(0, 21) + "…" : t.label}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      <p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1">
        <MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT} — green well covered · amber partial · rose never imported (percentages always shown)
      </p>
    </div>
  );
}

// ---------- dispatcher ----------

export function ChatChart({ spec }: { spec: ChatChartSpec }) {
  if (spec.kind === "table") return <div className="mt-1"><ChatTable spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "donut") return <div className="mt-1"><ChatDonut spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "stat") return <div className="mt-1"><ChatStat spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "uml" || spec.kind === "flow") return <div className="mt-1"><DiagramSVG spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "tree") return <div className="mt-1"><TreeSVG spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "heatmap") return <div className="mt-1"><Heatmap spec={spec} /><ChartFootnote spec={spec} /></div>;
  return <div className="mt-1"><FisheyeHBar spec={spec} /><ChartFootnote spec={spec} /></div>;
}
