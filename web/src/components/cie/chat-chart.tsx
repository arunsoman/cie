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

import { useEffect, useRef, useState } from "react";
import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import { BarChart3, MoveVertical } from "lucide-react";
import type { ChatChartSpec } from "@/lib/chat-chart";

const HEX: Record<string, string> = {
  rose: "#e11d48",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
};
const hex = (c?: string) => HEX[c ?? ""] ?? "#78716c";

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
        {" "}· drawn from your loaded report
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
        onKeyDown={keyDown}
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
            <g key={i}>
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
    </div>
  );
}

// ---------- donut (recharts) ----------

function ChatDonut({ spec }: { spec: ChatChartSpec }) {
  const rows = (spec.data ?? []).map((d) => ({ ...d, color: hex(d.color) }));
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
                  <Cell key={i} fill={d.color} />
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
            <li key={d.name} className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full shrink-0" style={{ background: d.color }} aria-hidden />
              <span className="text-stone-700 flex-1 min-w-0 truncate">{d.name}</span>
              <span className="font-semibold text-stone-900 tabular-nums">{d.value}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

// ---------- stat (hero number) ----------

function ChatStat({ spec }: { spec: ChatChartSpec }) {
  const d = spec.data?.[0];
  if (!d) return null;
  return (
    <div className="mt-2">
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-1">{spec.title}</p>}
      <div className="rounded-xl bg-emerald-50 border border-emerald-100 px-4 py-3 flex items-center gap-3">
        <span className="text-3xl font-bold text-emerald-800 tabular-nums">
          {d.value}
          {spec.unit && <span className="text-base font-semibold text-emerald-700 ml-0.5">{spec.unit}</span>}
        </span>
        <span className="text-sm text-emerald-900 leading-snug">{d.name}</span>
      </div>
      {spec.tell && <p className="text-xs text-stone-500 mt-1.5">{spec.tell}</p>}
    </div>
  );
}

// ---------- table (scroll rows) ----------

function ChatTable({ spec }: { spec: ChatChartSpec }) {
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
              <tr key={i} className={i % 2 ? "bg-stone-50/50" : ""}>
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
    </div>
  );
}

// ---------- dispatcher ----------

export function ChatChart({ spec }: { spec: ChatChartSpec }) {
  if (spec.kind === "table") return <div className="mt-1"><ChatTable spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "donut") return <div className="mt-1"><ChatDonut spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "stat") return <div className="mt-1"><ChatStat spec={spec} /><ChartFootnote spec={spec} /></div>;
  return <div className="mt-1"><FisheyeHBar spec={spec} /><ChartFootnote spec={spec} /></div>;
}
