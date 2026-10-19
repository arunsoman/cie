"use client";

// The Map: renders the CIE ViewSpec as a friendly layered picture.
// No external graph library — the spec already tells us the hierarchy
// (domain → modules → use-cases → rules), so we lay it out in bands.

import { useMemo } from "react";
import type { CieViewSpec, VizElement } from "@/lib/cie-types";
import { certaintyPlain, elementKindPlain } from "@/lib/plain-words";

interface Props {
  spec: CieViewSpec;
  selected: VizElement | null;
  onSelect: (e: VizElement) => void;
}

interface LaidNode {
  el: VizElement;
  x: number;
  y: number;
  w: number;
  h: number;
}

const VB_W_MIN = 940;
const VB_W_MAX = 7200; // past this, more width stops helping legibility — let height grow instead
const AVG_NODE_W = 170; // rough node width incl. the 14px gap, for the width estimate below
const TARGET_ASPECT = 0.6; // aimed-for height:width of the finished canvas (landscape)
const MARGIN = 30;
const BAND_GAP = 64;
const ROW_H = 66;
const NODE_H = 44;

// Style per certainty level
function style(mode: string) {
  switch (mode) {
    case "Fact":
      return { fill: "#ecfdf5", stroke: "#059669", text: "#065f46", dash: undefined };
    case "Inference":
      return { fill: "#fffbeb", stroke: "#d97706", text: "#78350f", dash: "5 3" };
    default:
      return { fill: "#f5f5f4", stroke: "#78716c", text: "#44403c", dash: "2 3" };
  }
}

function truncate(s: string, n: number) {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

export default function MapView({ spec, selected, onSelect }: Props) {
  const { nodes, bands, vbW, vbH } = useMemo(() => {
    // 0. Canvas width scales with element count instead of staying fixed.
    // A fixed 940px column was fine for a few dozen bubbles, but wrapped a
    // large real project (thousands of elements) into a single file-cabinet
    // of rows — tens of thousands of pixels tall and only 940 wide, a 20:1+
    // aspect ratio no zoom level could show usefully. Solving
    // height ≈ (N·AVG_NODE_W/vbW)·ROW_H for the width that keeps
    // height/width ≈ TARGET_ASPECT gives vbW ≈ √(N·AVG_NODE_W·ROW_H / TARGET_ASPECT),
    // so the map stays a landscape rectangle a "Fit" can actually fit,
    // whatever the project's size.
    const totalEls = spec.elements.length;
    const estimatedW = Math.sqrt((totalEls * AVG_NODE_W * ROW_H) / TARGET_ASPECT);
    const vbW = Math.round(Math.min(VB_W_MAX, Math.max(VB_W_MIN, estimatedW)) / 20) * 20;

    // 1. Group elements into bands (top → bottom)
    const bandOrder: { kinds: string[]; name: string }[] = [
      { kinds: ["domain"], name: "The big picture" },
      { kinds: ["module"], name: "Code files" },
      { kinds: ["usecase"], name: "What it does" },
      { kinds: ["rule", "invariant"], name: "Rules it follows" },
    ];
    const groups = bandOrder.map((b) => ({
      name: b.name,
      els: spec.elements.filter((e) => b.kinds.includes(e.elementKind)),
    }));
    // any element kinds not covered → extra band at the end
    const known = new Set(bandOrder.flatMap((b) => b.kinds));
    const rest = spec.elements.filter((e) => !known.has(e.elementKind));
    if (rest.length) groups.push({ name: "More", els: rest });

    // 2. Flow layout: rows inside each band, bands stacked vertically
    const laid: LaidNode[] = [];
    const bandMeta: { y: number; h: number; name: string }[] = [];
    let y = MARGIN + 18;
    for (const g of groups) {
      if (!g.els.length) continue;
      const bandTop = y;
      // split into rows by width budget
      const widths = g.els.map((el) => Math.min(210, Math.max(104, el.label.length * 7.2 + 30)));
      const rows: number[][] = [[]];
      let rowW = 0;
      g.els.forEach((_, i) => {
        const w = widths[i];
        if (rowW + w + 14 > vbW - 2 * MARGIN && rows[rows.length - 1].length) {
          rows.push([]);
          rowW = 0;
        }
        rows[rows.length - 1].push(i);
        rowW += w + 14;
      });
      rows.forEach((row) => {
        const totalW = row.reduce((s, i) => s + widths[i] + 14, 0) - 14;
        let x = (vbW - totalW) / 2;
        row.forEach((i) => {
          laid.push({ el: g.els[i], x, y, w: widths[i], h: NODE_H });
          x += widths[i] + 14;
        });
        y += ROW_H;
      });
      bandMeta.push({ y: bandTop, h: y - bandTop, name: g.name });
      y += BAND_GAP;
    }
    const height = Math.max(y - BAND_GAP + 30, 420);
    return { nodes: laid, bands: bandMeta, vbW, vbH: height };
  }, [spec]);

  const byId = useMemo(() => {
    const m = new Map<string, LaidNode>();
    nodes.forEach((n) => m.set(n.el.id, n));
    return m;
  }, [nodes]);

  const edgePath = (from: LaidNode, to: LaidNode) => {
    const down = to.y >= from.y;
    const x1 = from.x + from.w / 2;
    const y1 = down ? from.y + from.h : from.y;
    const x2 = to.x + to.w / 2;
    const y2 = down ? to.y : to.y + to.h;
    const dy = Math.max(28, Math.abs(y2 - y1) / 2);
    return `M ${x1} ${y1} C ${x1} ${y1 + (down ? dy : -dy)}, ${x2} ${y2 + (down ? -dy : dy)}, ${x2} ${y2}`;
  };

  return (
    <div className="w-full">
      {/* Legend */}
      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 mb-3 text-sm text-stone-600">
        <span className="flex items-center gap-1.5">
          <span className="inline-block w-4 h-4 rounded border-2" style={{ background: "#ecfdf5", borderColor: "#059669" }} />
          Proven — checked directly
        </span>
        <span className="flex items-center gap-1.5">
          <span className="inline-block w-4 h-4 rounded border-2 border-dashed" style={{ background: "#fffbeb", borderColor: "#d97706" }} />
          Good guess — CIE worked it out
        </span>
        <span className="flex items-center gap-1.5">
          <span className="inline-block w-4 h-4 rounded border-2 border-dotted" style={{ background: "#f5f5f4", borderColor: "#78716c" }} />
          Not sure yet
        </span>
        <span className="text-stone-400">· Tap a bubble to see it in plain words</span>
      </div>

      {/* No inner scrolling here on purpose: the stage camera owns pan and
          zoom for this view, so the map can overflow and be zoomed to fit. */}
      <div className="rounded-xl border border-stone-200 bg-white">
        <svg
          viewBox={`0 0 ${vbW} ${vbH}`}
          className="w-full min-w-[680px] block"
          role="img"
          aria-label="Map of how the code is organised"
        >
          <defs>
            <marker id="arr-fact" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 0 L 10 5 L 0 10 z" fill="#059669" />
            </marker>
            <marker id="arr-inf" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 0 L 10 5 L 0 10 z" fill="#d97706" />
            </marker>
          </defs>

          {/* band labels */}
          {bands.map((b, i) => (
            <text key={i} x={MARGIN} y={b.y + 2} fontSize="12" fill="#a8a29e" fontWeight="600" letterSpacing="0.08em">
              {b.name.toUpperCase()}
            </text>
          ))}

          {/* edges first (under nodes) */}
          {spec.edges.map((e) => {
            const a = byId.get(e.source);
            const b = byId.get(e.target);
            if (!a || !b) return null;
            const fact = e.displayMode === "Fact";
            const color = fact ? "#059669" : "#d97706";
            const dim = selected && e.source !== selected.id && e.target !== selected.id;
            return (
              <path
                key={e.id}
                d={edgePath(a, b)}
                fill="none"
                stroke={color}
                strokeWidth={selected && (e.source === selected.id || e.target === selected.id) ? 2.4 : 1.3}
                strokeDasharray={fact ? undefined : "5 4"}
                opacity={dim ? 0.18 : 0.65}
                markerEnd={fact ? "url(#arr-fact)" : "url(#arr-inf)"}
              />
            );
          })}

          {/* nodes — each carries its plain-words facts as data attributes,
              so the fisheye lens (and any test) can read what it magnifies */}
          {nodes.map((n) => {
            const s = style(n.el.displayMode);
            const isSel = selected?.id === n.el.id;
            const cx = n.x + n.w / 2;
            return (
              <g
                key={n.el.id}
                onClick={() => onSelect(n.el)}
                className="map-node cursor-pointer"
                role="button"
                aria-label={`${elementKindPlain(n.el.elementKind)}: ${n.el.label}`}
                data-node-id={n.el.id}
                data-label={n.el.label}
                data-kind={elementKindPlain(n.el.elementKind)}
                data-mode={n.el.displayMode}
                data-file={n.el.evidence?.file ?? ""}
              >
                <title>{`${n.el.label} — ${certaintyPlain(n.el.displayMode).label}. ${elementKindPlain(n.el.elementKind)}`}</title>
                <rect
                  x={n.x}
                  y={n.y}
                  width={n.w}
                  height={n.h}
                  rx={12}
                  fill={s.fill}
                  stroke={isSel ? "#1c1917" : s.stroke}
                  strokeWidth={isSel ? 3 : 1.6}
                  strokeDasharray={s.dash}
                />
                <circle cx={n.x + 16} cy={n.y + n.h / 2} r={4.5} fill={s.stroke} />
                <text
                  x={cx + 5}
                  y={n.y + n.h / 2 + 4.5}
                  textAnchor="middle"
                  fontSize="12.5"
                  fontWeight="600"
                  fill={s.text}
                >
                  {truncate(n.el.label, Math.floor(n.w / 7))}
                </text>
              </g>
            );
          })}
        </svg>
      </div>

      {spec.footer && (
        <p className="mt-2 text-xs text-stone-400">
          CIE's honesty note: {spec.footer.inferences} good guesses, {spec.footer.facts} proven facts, {spec.footer.hypotheses} not-sure-yet. {spec.footer.caption}
        </p>
      )}
    </div>
  );
}
