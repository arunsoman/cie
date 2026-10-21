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

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import { BarChart3, MessageCircleQuestion, MoveVertical } from "lucide-react";
import type { ChatChartSpec, UmlNode, ElementLoc } from "@/lib/chat-chart";
import type { ViewSpec } from "viz-adapter";
import { SelectionConductor, recomputeSemanticHash, recomputeRenderHash } from "viz-adapter";
import { sliceViewSpec } from "@/lib/viz/slice";
import { renderViaNegotiator } from "@/lib/viz/g6-engine";
import { useCie } from "@/lib/cie-store";
import { askAboutChartElement } from "@/lib/quality";
import { ConceptTreeG6 } from "./concept-tree-g6";

const conductor = new SelectionConductor();

const HEX: Record<string, string> = {
  rose: "#e11d48",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
};
const hex = (c?: string) => HEX[c ?? ""] ?? "#78716c";

/**
 * Click-to-chat + click-to-source: tapping a chart element with a known
 * source location opens the source panel (the code, right there, with an
 * "Ask CIE about this" button inside). Elements without a location —
 * aggregates, per-day sums, story records — keep the original behavior:
 * they hand a grounded question straight to the chat. Nothing is invented
 * by the chart itself.
 */
export function useAsk() {
  const setPendingAsk = useCie((s) => s.setPendingAsk);
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  const add = useCie(s => s.addChip);
  const revision = useCie(s => s.report?.rev ?? 0);
  return useCallback((spec: ChatChartSpec, el: { name: string; value?: number | string; loc?: ElementLoc }) => {
    add({id:`${spec.kind}:${el.loc?.file ?? ""}:${el.name}`,label:el.name,kind:spec.kind,revision,loc:el.loc,files:el.loc?.file ? [el.loc.file] : []});
    if (el.loc?.file) {
      openSourceViewer({
        file: el.loc.file,
        line: el.loc.line,
        endLine: el.loc.endLine,
        byteStart: el.loc.byteStart,
        byteEnd: el.loc.byteEnd,
        elName: el.name,
        elValue: el.value,
        chartTitle: spec.title,
      });
      return;
    }
    setPendingAsk(askAboutChartElement(spec.title, el));
  }, [add, revision, openSourceViewer, setPendingAsk]);
}

export const ASK_HINT = "tap any element to ask CIE about it";

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
        {spec.kind === "uml" || spec.kind === "flow" || spec.kind === "c4" ? " · hover a box to light up its connections" : ""}
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
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <svg
        ref={svgRef}
        viewBox={`0 0 ${W} ${H}`}
        className="rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-emerald-600/40 w-full block"
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
              onClick={() => ask(spec, { name: r.name, value: r.value, loc: r.loc })}
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
              <rect
                x={barX}
                y={cy - h / 2}
                width={w}
                height={h}
                rx={4}
                fill={r.color}
                stroke={r.highlight ? "#1c1917" : "none"}
                strokeWidth={r.highlight ? 1.8 : 0}
                style={{ transition: "y 90ms ease-out, height 90ms ease-out, fill 120ms" }}
              />
              <text
                x={barX + w + 6}
                y={cy + 4}
                fontSize={labelSize}
                fontWeight={r.highlight || mags[i] > 1.25 ? 700 : 500}
                fill={r.highlight ? "#1c1917" : mags[i] > 1.25 ? "#1c1917" : "#78716c"}
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
        {rows.some((r) => r.highlight) && <span className="text-stone-600 font-medium"> · dark outline = the file you asked about</span>}
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
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1">{spec.tell}</p>}
      <div className="flex items-center gap-3">
        <div className="relative h-[120px] w-[120px] shrink-0">
          <ResponsiveContainer width="100%" height="100%">
            <PieChart>
              <Pie data={rows} dataKey="value" nameKey="name" innerRadius={36} outerRadius={55} paddingAngle={2} strokeWidth={2} stroke="#fff">
                {rows.map((d, i) => (
                  <Cell key={i} fill={d.color} className="cursor-pointer" onClick={() => ask(spec, { name: d.name, value: d.value, loc: d.loc })} />
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
              onClick={() => ask(spec, { name: d.name, value: d.value, loc: d.loc })}
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
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-1 break-words">{spec.title}</p>}
      <button
        type="button"
        onClick={() => ask(spec, { name: d.name, value: d.value, loc: d.loc })}
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
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
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
                onClick={() => ask(spec, { name: String(r[0]), loc: spec.rowLoc?.[i] ?? undefined })}
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

export const KIND_PLAIN: Record<string, string> = {
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


// ---------- mindmap (custom-svg radial — one chosen concept at the centre,
// every descendant radiating outward by distance). Same TreeNode data and
// source as the plain hierarchy tree (kind "tree") — different geometry:
// this one answers "what's under X", the other "where does X sit".


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
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full block" role="img" aria-label={`Heat map of ${tiles.length} files: ${tiles.map((t) => `${t.name} ${t.value}%`).join(", ")}`}>
        {tiles.map((t, i) => {
          const x = GAP + (i % perRow) * (TW + GAP);
          const y = GAP + Math.floor(i / perRow) * (TH + GAP);
          return (
            <g key={`${t.name}-${i}`} onClick={() => ask(spec, { name: t.name, value: `${t.value}%`, loc: t.loc })} className="cursor-pointer">
              <title>{`${t.name}: ${t.value}% (${t.label ?? ""}) — ${ASK_HINT}`}</title>
              {/* highlight = the tile this question was about: dark outline, never colour alone */}
              <rect x={x} y={y} width={TW} height={TH} rx={8} fill={tileColor(t.value)} stroke={t.highlight ? "#1c1917" : tileStroke(t.value)} strokeWidth={t.highlight ? 2.5 : 1.2} />
              <text x={x + 10} y={y + 20} fontSize={11} fontWeight={t.highlight ? 700 : 600} fill="#1c1917">
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
        {tiles.some((t) => t.highlight) && <span className="text-stone-600 font-medium"> · dark outline = the file you asked about</span>}
      </p>
    </div>
  );
}

// ---------- sequence lanes (trace steps; clickable into the source) ----------

const SEQ_W = 960;
const SEQ_MARGIN = 22;
const LANE_LABEL_H = 30;
const MSG_ROW_H = 46;

/** What a diagram click reports upward when a host (the atlas dashboard)
 *  wants the SAME click to also drive its Selection Context. Optional and
 *  unused by the chat — every existing call site behaves exactly as before. */
export interface DiagramSelection {
  kind: "node" | "link" | "msg";
  id: string;
  label: string;
  loc?: ElementLoc;
  from?: string;
  to?: string;
}

export type SelectHandler = (sel: DiagramSelection) => void;

function SequenceSVG({ spec, onSelectElement }: { spec: ChatChartSpec; onSelectElement?: SelectHandler }) {
  const ask = useAsk();
  const uid = useId().replace(/[:]/g, "");
  const parts = spec.participants ?? [];
  const msgs = spec.msgs ?? [];
  const laneX = (name: string): number => {
    const i = parts.indexOf(name);
    return SEQ_MARGIN + 50 + ((SEQ_W - 2 * SEQ_MARGIN - 100) * Math.max(0, i)) / Math.max(1, parts.length - 1);
  };
  return (
    <svg role="img" aria-label={spec.title ?? "Sequence diagram"} viewBox={`0 0 ${SEQ_W} ${SEQ_MARGIN + LANE_LABEL_H + msgs.length * MSG_ROW_H + 14}`} width="100%">
      {/* lifelines */}
      {parts.map((p) => (
        <line key={`ll-${p}`} x1={laneX(p)} y1={SEQ_MARGIN + LANE_LABEL_H - 4} x2={laneX(p)} y2={SEQ_MARGIN + LANE_LABEL_H + msgs.length * MSG_ROW_H - 4} stroke="#e7e5e4" strokeWidth={2} />
      ))}
      {/* participant chips */}
      {parts.map((p, i) => (
        <g key={`p-${p}-${i}`}>
          <rect x={laneX(p) - 62} y={SEQ_MARGIN} width={124} height={LANE_LABEL_H - 10} rx={7} fill="#047857" />
          <text x={laneX(p)} y={SEQ_MARGIN + 18} textAnchor="middle" fontSize={11.5} fill="#fff" fontWeight={700}>
            {p.length > 20 ? p.slice(0, 19) + "…" : p}
          </text>
        </g>
      ))}
      {/* messages */}
      {msgs.map((m, i) => {
        const x1 = laneX(m.from);
        const x2 = laneX(m.to);
        const y = SEQ_MARGIN + LANE_LABEL_H + i * MSG_ROW_H + 10;
        const you = m.status === "you";
        const color = you ? "#059669" : m.status === "inferred" ? "#d97706" : "#57534e";
        const dashed = m.status !== "confirmed";
        return (
          <g
            key={`m-${m.label}-${i}`}
            className={m.loc?.file ? "cursor-pointer" : ""}
            onClick={() => {
              if (onSelectElement) {
                onSelectElement({ kind: "msg", id: m.label, label: m.label, loc: m.loc, from: m.from, to: m.to });
                if (m.loc) ask(spec, { name: m.label.replace(/\(\)$/, ""), loc: m.loc }); // arrows always open the real code
                return;
              }
              if (m.loc) ask(spec, { name: m.label.replace(/\(\)$/, ""), loc: m.loc });
            }}
          >
            <text x={(x1 + x2) / 2} y={y - 5} textAnchor="middle" fontSize={11} fontWeight={600} fill={color}>
              {m.label}
              {you ? " · YOU CONFIRMED" : m.status === "inferred" ? " · inferred" : ""}
            </text>
            {m.sub && (
              <text x={(x1 + x2) / 2} y={y + 20} textAnchor="middle" fontSize={10} fill="#78716c">
                {m.sub.length > 64 ? m.sub.slice(0, 63) + "…" : m.sub}
              </text>
            )}
            <line
              x1={x1} y1={y} x2={x2} y2={y}
              stroke={color} strokeWidth={2}
              strokeDasharray={dashed ? "5 3" : undefined}
              markerEnd={`url(#${uid}-seq-arrow)`}
              markerStart={"url(#noop)"}
            />
          </g>
        );
      })}
      <defs>
        <marker id={`${uid}-seq-arrow`} viewBox="0 0 10 10" refX={9} refY={5} markerWidth={7} markerHeight={7} orient="auto-start-reverse">
          <path d="M0,0L10,5L0,10z" fill="#57534e" />
        </marker>
      </defs>
    </svg>
  );}

// ---------- the G6 canvas path (RFC-0024 stage 1) ----------

/**
 * ChatChartSpec → gate-clean ViewSpec: the client-side conversion keeps
 * the producer's discipline — a box without a recorded location is a
 * hidden[] ledger row (G1), never an unevidenced element (the gate would
 * refuse it anyway); every narration line rides the box's recorded span;
 * edges are grounded in their source box's span (VIZ-H-02). Sealed with
 * the adapter's own hash functions so the negotiator path is byte-honest.
 */
function toViewSpec(spec: ChatChartSpec): { view: ViewSpec; metaById: Map<string, UmlNode> } {
  const nodes = spec.nodes ?? [];
  const links = spec.links ?? [];
  const metaById = new Map<string, UmlNode>();
  const elements: ViewSpec["elements"] = [];
  const hidden: ViewSpec["hidden"] = [];

  const evOf = (n: UmlNode): ViewSpec["elements"][number]["evidence"] | null => {
    if (!n.loc?.file) return null;
    return {
      file: n.loc.file,
      start: n.loc.byteStart ?? 0,
      end: n.loc.byteEnd ?? Math.max(n.loc.byteStart ?? 0, 1),
    };
  };

  for (const n of nodes) {
    const ev = evOf(n);
    if (!ev) {
      // G1: the grouped-out box never pretends to be an element — it is a
      // ledger row with patch-2 label/kind, clickable in the disclosure popup.
      hidden.push({
        id: n.id,
        reason: "no recorded source location for this box",
        salienceWeight: 0.4,
        label: n.title,
        kind: n.kind,
      });
      continue;
    }
    metaById.set(n.id, n);
    const fields: Record<string, unknown> = { kind: n.kind };
    if (n.lines?.length) {
      // Patch-3 gate rule: narration lines are evidence-gated — each line
      // cites the box's own recorded span (grounded, never fabricated).
      fields.bodyLines = n.lines.map((text) => ({
        text,
        line: n.loc?.line ?? 0,
        evidence: ev,
      }));
    }
    if (n.more) fields.more = n.more;
    elements.push({
      id: n.id,
      elementKind: n.kind,
      displayMode: "Fact",
      label: n.title,
      evidence: ev,
      salience: { factors: ["STORE_RECORD"], weight: n.selected ? 1 : 0.6, whyShown: "box built from the loaded store" },
      fields,
      parentId: undefined,
    });
  }

  const edges: ViewSpec["edges"] = [];
  links.forEach((l, i) => {
    const src = metaById.get(l.from);
    const dst = metaById.get(l.to);
    if (!src || !dst) return; // an edge into a hidden box is not drawn
    edges.push({
      id: `l-${i}`,
      source: l.from,
      target: l.to,
      edgeKind: l.label ?? "relates",
      displayMode: "Fact",
      evidence: {
        file: src.loc!.file,
        start: src.loc!.byteStart ?? 0,
        end: src.loc!.byteEnd ?? Math.max(src.loc!.byteStart ?? 0, 1),
      },
    });
  });

  const facts = elements.length;
  const view: ViewSpec = {
    elements,
    edges,
    footer: { facts, inferences: 0, hypotheses: 0, fog: 0, stale: 0, caption: hidden.length ? `${hidden.length} box(es) grouped out — open the ledger.` : "Display categories, not a confidence score." },
    gaps: hidden.length ? [{ id: "gap-grouped-boxes", area: "boxes", why: `${hidden.length} box(es) without a recorded location are listed in hidden[]` }] : [],
    hidden,
    scope: { revision: 0, environments: [], coverage: { analyzed: true } },
    layoutHint: { strategy: "dagre" },
    semanticHash: "",
    renderHash: "",
  };
  view.semanticHash = recomputeSemanticHash(view);
  view.renderHash = recomputeRenderHash(view, "");
  return { view, metaById };
}

/** The canvas body for uml/flow/c4: negotiator → gate → G6 host. Click
 *  fires the patch-1 selection surface and keeps today's ask/source
 *  behavior (the popup replaces jump-to-source at stage 3). */
function DiagramG6({ spec, onSelectElement }: { spec: ChatChartSpec; onSelectElement?: SelectHandler }) {
  const ask = useAsk();
  const hostRef = useRef<HTMLDivElement>(null);
  const [note, setNote] = useState<string>("negotiating engine…");

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const converted = toViewSpec(spec);
    const metaById = converted.metaById;
    const sliced = sliceViewSpec(converted.view);
    if (!sliced.spec) { setNote(`gate declined: ${sliced.violations.join("; ")}`); return; }
    const view = sliced.spec;
    if (!view.elements.length) return;
    let disposed = false;
    let offSelection: (() => void) | null = null;
    let handle: Awaited<ReturnType<typeof renderViaNegotiator>>["handle"] | null = null;

    void (async () => {
      try {
        const result = await renderViaNegotiator(host, view, {});
        if (disposed) {
          void result.handle.dispose();
          return;
        }
        handle = result.handle;
        setNote(`${result.disclosure.headline} · ${result.handle.layout.actual}`);
        offSelection = conductor.attach(result.handle, view.elements.map(e => e.id), (ids) => {
          const id = ids[ids.length - 1];
          if (!id) return;
          const n = metaById.get(id);
          if (!n) return;
          const state = useCie.getState();
          state.addChip({id:n.id,label:n.title,kind:n.kind,revision:state.report?.rev ?? 0,loc:n.loc,files:n.loc?.file ? [n.loc.file] : []});
          if (onSelectElement) onSelectElement({kind:"node",id:n.id,label:n.title,loc:n.loc});
          else ask(spec, { name: n.title, loc: n.loc });
        });
      } catch (err) {
        setNote(`engine declined: ${err instanceof Error ? err.message : String(err)}`);
      }
    })();

    return () => {
      disposed = true;
      offSelection?.();
      void handle?.dispose();
    };
  }, [spec, ask, onSelectElement]);

  const nodes = spec.nodes ?? [];
  return (
    <div>
      {spec.title && <p className="text-sm font-semibold text-stone-900 mb-0.5 break-words">{spec.title}</p>}
      {spec.tell && <p className="text-xs text-stone-500 mb-1.5">{spec.tell}</p>}
      <div ref={hostRef} className="w-full min-h-[240px] rounded-lg border border-stone-200/70" role="img" aria-label={`${spec.kind} diagram, ${nodes.length} boxes`} />
      <p className="text-[10px] text-stone-400 mt-1">G6 canvas · {note} · drag to pan, scroll to zoom</p>
    </div>
  );
}

/** RFC-0024 stage 6 — the SVG bypass is GONE: uml/flow/c4 draw through
 *  the negotiator → gate → G6 host, exactly one click path
 *  (verbs → conductor → engines → onSelection). The old ids live on as
 *  atlasSelection kinds, not as painters. (The SVG parity-harness toggle
 *  that RFC-0023 stage 1 added retired with the bypass itself.) */
function DiagramCanvas({ spec, onSelectElement }: { spec: ChatChartSpec; onSelectElement?: SelectHandler }) {
  return <DiagramG6 spec={spec} onSelectElement={onSelectElement} />;
}

// ---------- dispatcher ----------

export function ChatChart({ spec, onSelectElement }: { spec: ChatChartSpec; onSelectElement?: SelectHandler }) {
  if (spec.kind === "table") return <div className="mt-1"><ChatTable spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "donut") return <div className="mt-1"><ChatDonut spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "stat") return <div className="mt-1"><ChatStat spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "uml" || spec.kind === "flow" || spec.kind === "c4") return <div className="mt-1"><DiagramCanvas spec={spec} onSelectElement={onSelectElement} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "tree") return <div className="mt-1"><ConceptTreeG6 spec={spec} /><ChartFootnote spec={spec} /></div>;
  // RFC-0024 stage 6: the mindmap kind routes through the G6 tree renderer —
  // MindmapSVG (the SVG bypass) is deleted from the tree.
  if (spec.kind === "mindmap") return <div className="mt-1"><ConceptTreeG6 spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "heatmap") return <div className="mt-1"><Heatmap spec={spec} /><ChartFootnote spec={spec} /></div>;
  if (spec.kind === "sequence") return <div className="mt-1"><SequenceSVG spec={spec} onSelectElement={onSelectElement} /><p className="text-[10px] text-stone-400 flex items-center gap-1 mt-1"><MessageCircleQuestion className="h-3 w-3" /> {ASK_HINT} for an arrow — each one stands for real, recorded code · dashed = inferred · dashed emerald + YOU CONFIRMED = your own check</p><ChartFootnote spec={spec} /></div>;
  return <div className="mt-1"><FisheyeHBar spec={spec} /><ChartFootnote spec={spec} /></div>;
}
