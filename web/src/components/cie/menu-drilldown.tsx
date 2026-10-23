"use client";
import { repositoryFetch } from "@/lib/repository-fetch";

// The one-tap menu's interactive drilldown (menu-tab.tsx renders it when a
// question is picked): instead of one static picture, each pre-canned
// question opens the small dashboard its plan describes
// (lib/canned-drilldowns.ts):
//
//   ┌ header: ← All questions · the question · selection chips ───────────┐
//   │ [ chart 1 — axis A ]             [ chart 2 — axis B ]               │
//   │ [ matching rows list ]           [ viewer (empty until a pick) ]    │
//   └──────────────────────────────────────────────────────────────────────┘
//
// Everything is linked through the plan's two axes: picking a bucket on one
// chart re-aggregates the other, filters the rows, and — whenever the
// surviving rows share one file — opens that file in the viewer. Clicking a
// selected bucket again releases it; "Other" buckets merge the tail honestly
// and are never selectable. Reused infra: loadDemoSources + sourceCache (the
// source panel), openSourceViewer (the full panel), askAboutChartElement +
// snippetAround (quality.ts), and ChatChart's interactive canvas for the
// diagram-backed questions.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bar,
  BarChart,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  ArrowLeft,
  FileCode2,
  GitCommitHorizontal,
  MessageCircleQuestion,
  RotateCcw,
  ShieldCheck,
  SquareCheck,
} from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useCie } from "@/lib/cie-store";
import {
  drillFor,
  type Drill,
  type DrillLoc,
  type DrillMeta,
  type DrillRow,
  type RowsDrill,
} from "@/lib/canned-drilldowns";
import { snippetAround } from "@/lib/quality";
import { buildCannedAsk } from "@/lib/canned-drilldowns";
import { severityPlain } from "@/lib/plain-words";
import type { CannedChart } from "@/lib/canned-charts";
import type { ChatChartSpec } from "@/lib/chat-chart";
import { ChatChart } from "@/components/cie/chat-chart";
import { loadDemoSources } from "@/components/cie/source-panel";

function cut(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}
const SEV_ORDER = ["Critical", "High", "Medium", "Low"];

// ------------------------------------------------------------- palettes ---

const C = {
  rose: "#e11d48",
  roseSoft: "#fb7185",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
  stoneLight: "#a8a29e",
};
const PIE_CYCLE = [C.emerald, C.amber, C.roseSoft, C.stone, "#0f766e", C.rose, C.stoneLight];

// Never colour alone: every slice/bar keeps a text label; the palette only
// reinforces known bucket semantics, and unknown buckets cycle neutrally.
const KNOWN_COLORS: Record<string, string> = {
  // urgency (plain labels)
  "Fix soon": C.rose,
  "Please look": C.amber,
  "Worth checking": C.amber,
  "Good to know": C.stone,
  // promises
  "Written by you": C.emerald,
  "CIE's guess": C.amber,
  // certainty
  Proven: C.emerald,
  "Good guess": C.amber,
  "Not sure yet": C.stone,
  // clean-files
  "Completely clean": C.emerald,
  "Has problems": C.amber,
  // runtime coverage buckets
  "well covered (70%+)": C.emerald,
  "partly covered": C.amber,
  "never even imported": C.roseSoft,
  // drift buckets
  "in step": C.emerald,
  "a little behind": C.amber,
  "far behind (7d+)": C.rose,
  "no test file": C.stone,
  fresh: C.emerald,
  drifting: C.amber,
  "way behind (30d+)": C.rose,
};

/** The honest red of the maximum, then amber, then neutral — same rule as
 *  the canned charts; never the only carrier of meaning. */
function bucketColor(label: string, i: number, max: number, value: number, selected: boolean, pie: boolean): string {
  if (selected) return C.emerald;
  if (KNOWN_COLORS[label]) return KNOWN_COLORS[label];
  if (pie) return PIE_CYCLE[i % PIE_CYCLE.length];
  if (value === max) return C.rose;
  if (value >= max / 2) return C.amber;
  return PIE_CYCLE[i % PIE_CYCLE.length];
}

// ------------------------------------------------------- aggregation core ---

type AxisSel = { a: string | null; b: string | null };

interface Bucket {
  key: string; // the stable selection key (keyA / keyB)
  label: string; // the display label (a / b)
  count: number; // row count, or the sum when rows carry a value
  rows: number; // raw row count, for honesty sublabels
}

const OTHER = "Other";

/** Top buckets of `rows` along one axis of the plan; the tail merges into an
 *  honest "Other" slice so neither chart ever lies about its size. */
function aggregate(rows: DrillRow[], axis: "A" | "B", numeric: boolean, top = 7): Bucket[] {
  const map = new Map<string, Bucket>();
  for (const r of rows) {
    const key = axis === "A" ? r.keyA : r.keyB;
    const cur = map.get(key) ?? { key, label: axis === "A" ? r.a : r.b, count: 0, rows: 0 };
    cur.count += r.value ?? 1;
    cur.rows += 1;
    map.set(key, cur);
  }
  const sorted = [...map.values()].sort((x, y) => y.count - x.count || x.key.localeCompare(y.key));
  if (sorted.length <= top) return sorted;
  const head = sorted.slice(0, top);
  head.push({
    key: OTHER,
    label: `${OTHER} (${sorted.length - top} more)`,
    count: sorted.slice(top).reduce((a, b) => a + b.count, 0),
    rows: sorted.slice(top).reduce((a, b) => a + b.rows, 0),
  });
  return head;
}

function rowMatches(r: DrillRow, sel: AxisSel): boolean {
  if (sel.a && r.keyA !== sel.a) return false;
  if (sel.b && r.keyB !== sel.b) return false;
  return true;
}

const fmt = (n: number, unit?: string) =>
  unit === "ms"
    ? `${Math.round(n * 10) / 10} ms`
    : unit
      ? `${Math.round(n)} ${unit}`
      : String(Math.round(n));

// ------------------------------------------------------------ the charts ---

const TOOLTIP_STYLE = {
  borderRadius: 10,
  border: "1px solid #e7e5e4",
  boxShadow: "0 4px 16px rgba(0,0,0,0.06)",
  fontSize: 12.5,
  padding: "8px 12px",
  background: "#ffffff",
} as const;

/** Y-axis tick that bolds the selected label (which also gets a dark bar
 *  outline + names itself in the chip row — never colour alone). */
function BarTick(props: any) {
  const label = String(props.payload?.value ?? "");
  const is = label === props.selected;
  return (
    <text x={props.x} y={props.y} textAnchor="end" fontSize={11.5} fontWeight={is ? 700 : 400} fill={is ? "#065f46" : "#57534e"}>
      {label}
    </text>
  );
}

function LinkedBar({
  data,
  selected,
  onPick,
  unit,
  height = 250,
}: {
  data: Bucket[];
  selected: string | null;
  onPick: (key: string) => void;
  unit?: string;
  height?: number;
}) {
  const max = Math.max(1, ...data.map((d) => d.count));
  const shaped = data.map((d) => ({ name: d.key === OTHER ? d.label : cut(d.label, 24), count: d.count, raw: d.key }));
  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={shaped} layout="vertical" margin={{ top: 4, right: 24, bottom: 2, left: 0 }}>
          <XAxis type="number" allowDecimals={!unit} tick={{ fontSize: 11, fill: "#78716c" }} axisLine={false} tickLine={false} />
          <YAxis
            type="category"
            dataKey="name"
            width={158}
            interval={0}
            tickMargin={0}
            tick={<BarTick selected={selected} />}
            axisLine={false}
            tickLine={false}
          />
          <Tooltip
            cursor={{ fill: "rgba(0,0,0,0.04)" }}
            contentStyle={TOOLTIP_STYLE}
            formatter={(value: number | string) => [fmt(Number(value), unit), unit ? "Sum" : "Count"]}
          />
          <Bar dataKey="count" radius={[0, 4, 4, 0]} barSize={data.length > 5 ? 15 : 18} isAnimationActive={false}>
            {shaped.map((d, i) => {
              const is = d.raw === selected;
              return (
                <Cell
                  key={i}
                  fill={d.raw === OTHER ? C.stoneLight : bucketColor(d.raw, i, max, d.count, is, false)}
                  stroke={is ? "#065f46" : undefined}
                  strokeWidth={is ? 2 : 0}
                  opacity={d.raw === OTHER ? 0.5 : 1}
                  style={{ cursor: d.raw === OTHER ? "default" : "pointer" }}
                  onClick={() => d.raw !== OTHER && onPick(d.raw)}
                />
              );
            })}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

function LinkedPie({
  data,
  selected,
  onPick,
  unit,
  center,
}: {
  data: Bucket[];
  selected: string | null;
  onPick: (key: string) => void;
  unit?: string;
  center?: { top: string; bottom: string };
}) {
  const pieClick = (entry: any) => {
    const name = String(entry?.key ?? entry?.name ?? entry?.payload?.key ?? "");
    if (name && name !== OTHER) onPick(name);
  };
  return (
    <div className="flex flex-col items-center gap-2 sm:flex-row sm:gap-5">
      <div className="relative h-[190px] w-[190px] shrink-0">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={data}
              dataKey="count"
              nameKey="key"
              innerRadius={56}
              outerRadius={84}
              paddingAngle={2}
              strokeWidth={2}
              stroke="#ffffff"
              style={{ cursor: "pointer" }}
              onClick={pieClick}
            >
              {data.map((d, i) => {
                const is = d.key === selected;
                return (
                  <Cell
                    key={i}
                    fill={d.key === OTHER ? C.stoneLight : bucketColor(d.key, i, 1, d.count, is, true)}
                    stroke={is ? "#065f46" : "#ffffff"}
                    strokeWidth={is ? 2.5 : 2}
                    opacity={d.key === OTHER ? 0.5 : 1}
                  />
                );
              })}
            </Pie>
            <Tooltip
              contentStyle={TOOLTIP_STYLE}
              formatter={(value: number | string, name) => {
                const rows = data.find((d) => d.key === String(name))?.rows ?? 0;
                return [fmt(Number(value), unit), `${name} · ${rows} ${rows === 1 ? "row" : "rows"}`];
              }}
            />
          </PieChart>
        </ResponsiveContainer>
        {center && (
          <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
            <span className="text-2xl font-bold text-stone-900">{center.top}</span>
            <span className="max-w-[170px] text-center text-[10.5px] leading-tight text-stone-500">{center.bottom}</span>
          </div>
        )}
      </div>
      <ul className="w-full min-w-0 space-y-1 text-[12.5px]">
        {data.map((d) => {
          const is = d.key === selected;
          return (
            <li key={d.key} className="flex items-center gap-2">
              <span
                className={`h-2.5 w-2.5 shrink-0 rounded-full ${d.key === OTHER ? "opacity-40" : ""}`}
                style={{ background: d.key === OTHER ? C.stoneLight : bucketColor(d.key, 0, 1, 0, is, true) }}
                aria-hidden
              />
              <button
                type="button"
                onClick={() => onPick(d.key)}
                className={`min-w-0 flex-1 truncate text-left hover:underline ${
                  d.key === OTHER ? "cursor-default text-stone-400" : is ? "font-semibold text-emerald-900" : "text-stone-700"
                }`}
                title={d.key === OTHER ? "Several at once — pick a named slice to filter" : `Filter by ${d.label}`}
              >
                {cut(d.label, 34)}
              </button>
              <span className="ml-auto shrink-0 font-semibold tabular-nums text-stone-900">{fmt(d.count, unit)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

// --------------------------------------------------------- the row list ---

const ROWS_SHOWN = 40;

function SeverityChip({ severity }: { severity?: string }) {
  if (!severity) return null;
  const p = severityPlain(severity);
  const tone =
    p.tone === "rose"
      ? "bg-rose-50 text-rose-800 border-rose-200"
      : p.tone === "amber"
        ? "bg-amber-50 text-amber-800 border-amber-200"
        : "bg-stone-50 text-stone-600 border-stone-200";
  return (
    <Badge variant="outline" className={`border text-[10px] font-semibold ${tone}`}>
      {p.label}
    </Badge>
  );
}

function shortKey(k: string): string {
  const s = k.split("/").pop() ?? k;
  return s.length > 26 ? s.slice(0, 25) + "…" : s;
}

function RowList({
  rows,
  focusedId,
  onFocus,
  meta,
}: {
  rows: DrillRow[];
  focusedId: string | null;
  onFocus: (id: string) => void;
  meta: DrillMeta;
}) {
  if (!rows.length)
    return (
      <div className="flex h-full min-h-[220px] items-center justify-center rounded-lg border border-dashed border-stone-200 bg-stone-50 px-6 text-center text-sm text-stone-400">
        No {meta.rowName}s match this filter — clear a chip above to widen it.
      </div>
    );
  const shown = rows.slice(0, ROWS_SHOWN);
  return (
    <div className="space-y-2">
      <p className="text-xs font-semibold text-stone-500">
        {meta.listTitle} · {rows.length} {rows.length === 1 ? meta.rowName : `${meta.rowName}s`}
        {rows.length > ROWS_SHOWN ? ` — showing the first ${ROWS_SHOWN}` : ""}
      </p>
      <ul className="max-h-[430px] space-y-1.5 overflow-auto pr-1">
        {shown.map((r) => {
          const isF = focusedId === r.id;
          return (
            <li key={r.id}>
              <button
                type="button"
                onClick={() => onFocus(r.id)}
                aria-pressed={isF}
                className={`w-full rounded-lg border px-3 py-2 text-left transition-colors ${
                  isF ? "border-emerald-400 bg-emerald-50 shadow-sm" : "border-stone-200 bg-white hover:border-emerald-200 hover:bg-emerald-50/40"
                }`}
              >
                <span className="flex items-start justify-between gap-2">
                  <span className={`min-w-0 flex-1 truncate text-[13px] ${isF ? "font-semibold text-emerald-900" : "font-medium text-stone-900"}`}>
                    {r.title}
                  </span>
                  <span className="flex shrink-0 items-center gap-1.5">
                    {r.extra && r.extra.length < 14 && (
                      <Badge variant="outline" className="border font-mono text-[10px] text-stone-500">
                        {r.extra}
                      </Badge>
                    )}
                    <SeverityChip severity={r.severity} />
                  </span>
                </span>
                <span className="mt-0.5 block text-[11.5px] text-stone-500 line-clamp-2">
                  {r.extra && r.extra.length >= 14 ? `${r.extra} — ` : ""}
                  {r.detail}
                </span>
                <span className="mt-1 flex flex-wrap items-center gap-1 text-[10px] text-stone-400">
                  <span className="rounded bg-stone-100 px-1.5 py-0.5 font-mono">{shortKey(r.a)}</span>
                  <span aria-hidden>·</span>
                  <span className="max-w-[160px] truncate">{r.b}</span>
                  {r.loc?.line != null && (
                    <>
                      <span aria-hidden>·</span>
                      <span className="font-mono">line {r.loc.line}</span>
                    </>
                  )}
                  {r.loc2 && (
                    <>
                      <span aria-hidden>·</span>
                      <span className="font-mono">↔ {shortKey(r.loc2.file)}{r.loc2.line != null ? `:${r.loc2.line}` : ""}</span>
                    </>
                  )}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      {rows.length > ROWS_SHOWN && (
        <p className="px-1 text-[11px] text-stone-400">
          …and {rows.length - ROWS_SHOWN} more — narrow the filter with the charts to see them.
        </p>
      )}
    </div>
  );
}

// ------------------------------------------------------------- the viewer ---

const VIEW_WINDOW = 30; // lines shown on each side of the anchor line

/** Every evidence line the currently matching rows recorded in ONE file —
 *  either side (loc/loc2) counts, so the flipped twin highlights too. */
function marksFor(rows: DrillRow[], file: string): Set<number> {
  const out = new Set<number>();
  for (const r of rows) {
    for (const l of [r.loc, r.loc2]) {
      if (!l || l.file !== file) continue;
      const ms = r.marks ?? (l.line != null ? [l.line] : []);
      for (const n of ms) out.add(n);
    }
  }
  return out;
}

/** UTF-8 byte offset → 1-based line — the client twin of the engine's
 *  FileTable::line_of, computed from real content, never guessed. */
function byteToLine(content: string, byte: number): number {
  const enc = new TextEncoder();
  const lines = content.split("\n");
  let acc = 0;
  for (let i = 0; i < lines.length; i++) {
    const span = enc.encode(lines[i]).length + 1; // +1 for the newline
    if (byte < acc + span) return i + 1;
    acc += span;
  }
  return lines.length;
}

type ViewerState =
  | { kind: "loading" }
  | { kind: "ready"; content: string; from: string }
  | { kind: "gone"; reason: string };

const CODE_LANG: Record<string, string> = {
  py: "python", sql: "sql", sh: "bash", bash: "bash", js: "javascript", jsx: "javascript",
  ts: "typescript", tsx: "typescript", kt: "kotlin", lua: "lua", rs: "rust", java: "java",
  go: "go", c: "c", h: "c", cpp: "cpp", hpp: "cpp", cs: "csharp", php: "php", rb: "ruby",
  md: "markdown", json: "json", toml: "ini", yml: "yaml", yaml: "yaml",
};

/** The drill's embedded code viewer. Empty (with per-question guidance)
 *  until a selection gives it a location; windowed around the anchor line;
 *  every matching row's evidence line is marked; the focused line is ringed.
 *  Loading reuses the source panel's paths: consented folder → /api/fs/file,
 *  demo → the bundled source bundle, uploads/clones → the honest reason. */
function DrillViewer({
  loc,
  other,
  rows,
  meta,
  focused,
  onAsk,
  onOpenFull,
}: {
  loc: DrillLoc | null;
  other: DrillLoc | null; // the paired twin (duplication B, doc→code…)
  rows: DrillRow[]; // rows matching the current filters — the marks source
  meta: DrillMeta;
  focused: DrillRow | null;
  onAsk: (loc: DrillLoc | null, title: string, row: DrillRow | null) => void;
  onOpenFull: (loc: DrillLoc) => void;
}) {
  const { sourceRoot: liveSourceRoot, source, demoKind, sourceCache, cacheSource } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;
  const [state, setState] = useState<ViewerState>({ kind: "loading" });
  const [flipped, setFlipped] = useState(false);
  const [resolvedNote, setResolvedNote] = useState<string | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

  const eff = flipped && other ? other : loc;
  const file = eff?.file ?? null;

  // reset to "loading" the render-phase way whenever the target file changes
  const [renderedFile, setRenderedFile] = useState<string | null>(file);
  if (file !== renderedFile) {
    setRenderedFile(file);
    setState({ kind: "loading" });
    setResolvedNote(null);
  }

  // A cache hit is DERIVED, not effect-state: any file read once this session
  // (by the drill viewer or by the full source panel) renders instantly,
  // with no state churn. The effect below only fetches uncached files.
  const cached = file ? sourceCache[file] ?? null : null;
  const readyContent = state.kind === "ready" ? state.content : cached;
  const fromNote =
    cached != null && state.kind !== "ready"
      ? "already read once this session"
      : state.kind === "ready"
        ? state.from
        : "";
  const ready = readyContent != null;
  // uploads / URL clones have no source on disk — said plainly, derived
  const unavailableNote: string | null =
    file == null || ready
      ? null
      : state.kind === "gone"
        ? state.reason
        : sourceRoot == null && source !== "demo"
          ? "The source files aren't available for this project — only the indexed facts are loaded. Connect a folder on this computer and the code appears here."
          : null; // still loading — the body says so

  // anchor line: a recorded flat line, else the byte offset resolved from
  // real content — never a guessed number.
  const line = useMemo(() => {
    if (!eff) return null;
    if (eff.line != null && eff.line >= 1) return eff.line;
    if (eff.byteStart != null && readyContent != null) return byteToLine(readyContent, eff.byteStart);
    return null;
  }, [eff, readyContent]);

  // scroll the anchor line into view once the content renders
  useEffect(() => {
    if (line == null || !ready) return;
    const t = setTimeout(() => {
      bodyRef.current?.querySelector(`[data-line="${line}"]`)?.scrollIntoView({ block: "center", behavior: "smooth" });
    }, 60);
    return () => clearTimeout(t);
  }, [line, ready, file]);

  useEffect(() => {
    if (!file) return;
    // Already in hand? Nothing to fetch — the content is derived from the
    // cache below. This guard also terminates the dep loop (a completed
    // fetch writes the cache, which re-runs this effect once more; without
    // the guard it would refetch forever).
    if (sourceCache[file] != null) return;
    let dead = false;
    if (sourceRoot) {
      repositoryFetch("/api/fs/file", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ root: sourceRoot, path: file, all: true }),
      })
        .then((r) => r.json().catch(() => ({})))
        .then((d: { content?: string; error?: string }) => {
          if (dead) return;
          if (d.content != null) {
            cacheSource(file, d.content);
            setState({ kind: "ready", content: d.content, from: `captured source from the loaded repository revision` });
          } else setState({ kind: "gone", reason: d.error ?? "This file couldn't be read from your project folder." });
        })
        .catch(() => !dead && setState({ kind: "gone", reason: "The connection to your project folder dropped. Please try again." }));
    } else if (source === "demo") {
      void loadDemoSources(demoKind ?? "polyglot").then((files) => {
        if (dead) return;
        // Sample bundles store full repo paths (db/migrations/001_init.sql)
        // while some exports record short names (001_init.sql). When the
        // direct key misses and exactly ONE bundled path ends with the
        // recorded name, read that one — and say so. More than one match:
        // say that too, never pick for the user.
        const direct = files[file] != null ? file : null;
        if (!direct) setResolvedNote(null);
        const twins = direct ? [] : Object.keys(files).filter((k) => k.endsWith(`/${file}`));
        const key = direct ?? (twins.length === 1 ? twins[0] : null);
        const content = key != null ? files[key] : undefined;
        if (content != null) {
          if (key !== file) setResolvedNote(`recorded as “${file}” — read from ${key}`);
          cacheSource(file, content);
          setState({ kind: "ready", content, from: "bundled with the sample project" });
        } else
          setState({
            kind: "gone",
            reason:
              twins.length > 1
                ? `The sample bundles ${twins.length} files ending in “${file}” — the recorded facts don't say which one, so nothing is opened.`
                : "That file isn't part of the bundled sample sources — the matching list stays complete without it.",
          });
      });
    }
    // uploads / URL-cloned projects: no source on disk — that case is DERIVED
    // below (unavailableNote), so the effect only handles real fetch paths.
    return () => {
      dead = true;
    };
  }, [file, sourceRoot, source, demoKind, sourceCache, cacheSource]);

  // ---------- nothing selected: the question's own honest empty state -----
  if (!file) {
    return (
      <div className="flex h-full min-h-[300px] flex-col items-center justify-center gap-2.5 rounded-lg border border-dashed border-stone-200 bg-stone-50 px-8 text-center">
        <FileCode2 className="h-6 w-6" aria-hidden />
        <p className="text-sm leading-relaxed text-stone-400">{meta.viewer.empty}</p>
        {meta.viewer.kind === "code" && (
          <p className="max-w-xs text-[10.5px] text-stone-400">
            The viewer reads files the same way the source panel does — your consented folder, or the sample&apos;s bundled sources — and marks the recorded lines.
          </p>
        )}
      </div>
    );
  }

  const marks = ready ? marksFor(rows, file!) : new Set<number>();
  const all = ready ? readyContent!.split("\n") : [];
  const lo = line != null ? Math.max(1, line - VIEW_WINDOW) : 1;
  const hi = line != null ? Math.min(all.length, line + VIEW_WINDOW) : Math.min(all.length, VIEW_WINDOW * 2);
  const pathLine = `${file}${line != null ? `:${line}` : ""}`;

  return (
    <div className="flex h-full min-h-[300px] flex-col overflow-hidden rounded-lg border border-stone-200 bg-white">
      {/* header: file(:line) + the paired-side toggle */}
      <div className="flex items-center gap-2 border-b border-stone-100 bg-stone-50/70 px-3 py-2">
        <FileCode2 className="h-3.5 w-3.5 shrink-0 text-emerald-700" />
        <span className="truncate font-mono text-[11.5px] font-semibold text-stone-800" title={pathLine}>
          {pathLine}
        </span>
        {other && (
          <button
            type="button"
            onClick={() => setFlipped(!flipped)}
            className={`ml-auto shrink-0 rounded border px-2 py-0.5 text-[10px] font-semibold transition-colors ${
              flipped
                ? "border-emerald-300 bg-emerald-50 text-emerald-800"
                : "border-stone-200 bg-white text-stone-600 hover:border-emerald-300 hover:text-emerald-800"
            }`}
          >
            {flipped ? "← back to the first side" : "show the twin →"}
          </button>
        )}
      </div>
      {focused && ready && (
        <p className="truncate border-b border-stone-50 bg-white px-3 py-1.5 text-[11px] text-stone-500">
          <span className="font-semibold text-stone-700">{focused.title}</span> — {focused.b}
        </p>
      )}

      {/* body */}
      <div ref={bodyRef} className="min-h-0 flex-1 overflow-auto bg-[#fcfcfc]">
        {unavailableNote ? (
          <p className="p-4 text-xs leading-relaxed text-stone-500">{unavailableNote}</p>
        ) : !ready ? (
          <p className="p-4 text-xs text-stone-400" role="status">
            Reading {shortKey(file!)}…
          </p>
        ) : null}
        {ready &&
          all.slice(lo - 1, hi).map((text, i) => {
            const no = lo + i;
            const isFocus = no === line;
            const isMark = marks.has(no);
            return (
              <div
                key={no}
                data-line={no}
                title={`line ${no}`}
                className={`flex items-start gap-2 px-2 font-mono text-[10.5px] leading-[1.6] break-all whitespace-pre-wrap ${
                  isFocus ? "bg-amber-50 outline outline-1 outline-amber-300" : isMark ? "bg-rose-50" : "hover:bg-stone-50"
                }`}
              >
                <span className={`w-9 shrink-0 select-none text-right ${isFocus ? "font-bold text-amber-700" : isMark ? "text-rose-700" : "text-stone-300"}`}>
                  {no}
                </span>
                <span className={`min-w-0 flex-1 ${isFocus || isMark ? "text-stone-800" : "text-stone-500"}`}>{text || " "}</span>
              </div>
            );
          })}
      </div>

      {/* footer: the per-question caption + honesty + actions */}
      <div className="flex flex-wrap items-center gap-1.5 border-t border-stone-100 bg-white px-3 py-1.5">
        <span className="min-w-0 flex-1 truncate text-[10px] text-stone-400">
          {meta.viewer.caption}
          {resolvedNote ? <span className="ml-1 text-stone-500">· {resolvedNote}</span> : null}
        </span>
        {ready && (
          <span className="shrink-0 text-[10px] text-stone-400">
            {fromNote}
            {hi < all.length || lo > 1 ? ` · windowed (file has ${all.length} lines)` : ""}
          </span>
        )}
        <button
          type="button"
          onClick={() => void navigator.clipboard?.writeText(pathLine)}
          className="shrink-0 rounded px-1.5 py-0.5 font-mono text-[10px] text-stone-400 hover:bg-stone-100"
          title={`copy ${pathLine}`}
        >
          {shortKey(pathLine)}
        </button>
        <button
          type="button"
          onClick={() => eff && onOpenFull(eff)}
          className="shrink-0 rounded bg-stone-100 px-2 py-0.5 text-[10px] font-medium text-stone-600 hover:bg-stone-200"
        >
          Full source panel
        </button>
        <button
          type="button"
          onClick={() => {
            if (!eff) return;
            const target = focused ?? rows.find((r) => (r.loc ?? r.loc2)?.file === eff.file) ?? null;
            onAsk(eff, target?.title ?? shortKey(eff.file), target);
          }}
          className="shrink-0 rounded border border-emerald-200 bg-emerald-50 px-2 py-0.5 text-[10px] font-semibold text-emerald-900 hover:bg-emerald-100"
        >
          <MessageCircleQuestion className="-mt-0.5 mr-1 inline h-3 w-3" />
          Ask CIE — what, impact, fix
        </button>
      </div>
    </div>
  );
}

/** The "commit viewer" — fix-commit rows aren't source lines, so this
 *  question's viewer is a commit card instead (honest, still interactive). */
function CommitCard({ row, meta, onAsk }: { row: DrillRow; meta: DrillMeta; onAsk: (loc: DrillLoc | null, title: string, row: DrillRow | null) => void }) {
  return (
    <div className="flex h-full min-h-[300px] flex-col justify-center rounded-lg border border-stone-200 bg-white p-5">
      <div className="flex items-center gap-2 text-emerald-800">
        <GitCommitHorizontal className="h-4 w-4" aria-hidden />
        <span className="font-mono text-xs font-bold">{row.extra ?? "commit"}</span>
        <button
          type="button"
          onClick={() => void navigator.clipboard?.writeText(row.extra ?? "")}
          className="rounded px-1.5 py-0.5 text-[10px] text-stone-400 hover:bg-stone-100"
        >
          copy sha
        </button>
      </div>
      <p className="mt-2 text-sm font-semibold leading-snug text-stone-900">{row.title}</p>
      <p className="mt-1 text-xs leading-relaxed text-stone-500">{row.detail}</p>
      <p className="mt-3 text-[10.5px] text-stone-400">{meta.viewer.caption}</p>
      <div className="mt-3 flex items-center gap-2">
        <button
          type="button"
          onClick={() => onAsk(null, row.title, row)}
          className="inline-flex items-center gap-1 rounded border border-emerald-200 bg-emerald-50 px-2 py-0.5 text-[10px] font-semibold text-emerald-900 hover:bg-emerald-100"
        >
          <MessageCircleQuestion className="h-3 w-3" /> Ask CIE about this commit
        </button>
      </div>
    </div>
  );
}

/** The "none" viewer — this question's numbers come from a live probe, not a
 *  source line; the card says so and the matching rows stay fully listed. */
function NoViewer({ meta, count, label }: { meta: DrillMeta; count: number; label: string }) {
  return (
    <div className="flex h-full min-h-[300px] flex-col items-center justify-center gap-2.5 rounded-lg border border-stone-200 bg-stone-50/60 px-8 text-center">
      <ShieldCheck className="h-6 w-6 text-emerald-700" aria-hidden />
      <p className="text-sm leading-relaxed text-stone-600">{meta.viewer.empty}</p>
      <p className="text-[11px] text-stone-500">
        {count} {count === 1 ? meta.rowName : `${meta.rowName}s`} match the current filter{label ? ` (${label})` : ""}.
      </p>
      <p className="text-[10.5px] text-stone-400">{meta.viewer.caption}</p>
    </div>
  );
}

// ------------------------------------------------------ the drilldown shell ---

function SelChip({ label, onClear }: { label: string; onClear: () => void }) {
  return (
    <button
      type="button"
      onClick={onClear}
      title="Click to clear this filter"
      className="inline-flex items-center gap-1 rounded-full border border-emerald-300 bg-emerald-50 px-2.5 py-1 text-[11px] font-semibold text-emerald-900 hover:bg-emerald-100"
    >
      <SquareCheck className="h-3 w-3" aria-hidden />
      {label}
      <span aria-hidden className="text-emerald-700">
        ✕
      </span>
    </button>
  );
}

function DrillHeader({
  question,
  hint,
  onBack,
  note,
  chips,
}: {
  question: string;
  hint: string;
  onBack: () => void;
  note?: string;
  chips?: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-stone-200 bg-stone-50/60 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-1.5 rounded-lg border border-stone-300 bg-white px-3 py-1.5 text-xs font-semibold text-stone-700 transition-colors hover:border-emerald-300 hover:text-emerald-800"
        >
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
          All questions
        </button>
      </div>
      <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 className="text-base font-bold text-stone-900">{question}</h2>
        <span className="text-xs text-stone-500">{hint}</span>
      </div>
      {note && <p className="mt-1 text-xs leading-relaxed text-stone-500">{note}</p>}
      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">{chips}</div>
    </div>
  );
}

function ChartCardFor({ title, tell, children }: { title: string; tell: string; children: React.ReactNode }) {
  return (
    <Card className="border-stone-200">
      <div className="p-4">
        <p className="text-[15px] font-semibold leading-snug text-stone-900">{title}</p>
        <p className="text-xs text-stone-500">{tell}</p>
        <div className="pt-2">{children}</div>
      </div>
    </Card>
  );
}

// ------------------------------------------------------------ the drill ---

/** The dispatcher — one hook to plan the question, then hand the render to
 *  one of the two leaf views (hooks stay unconditional in each). */
export function QuestionDrilldown({ chart, onBack }: { chart: CannedChart; onBack: () => void }) {
  const { report, viewspec, concepts, deps, runtime, quality } = useCie();

  const drill: Drill = useMemo(
    () => drillFor(chart.id, { report, viewspec, concepts, deps, runtime, quality }),
    [chart.id, report, viewspec, concepts, deps, runtime, quality]
  );

  if (drill.kind === "chart") return <ChartDrillView chart={chart} note={drill.note} onBack={onBack} />;
  return <RowsDrillView drill={drill} chart={chart} onBack={onBack} />;
}

function ChartDrillView({ chart, note, onBack }: { chart: CannedChart; note: string; onBack: () => void }) {
  return (
    <div className="space-y-4">
      <DrillHeader question={chart.question} hint={chart.hint} onBack={onBack} note={note} />
      {chart.spec ? (
        <Card className="border-stone-200">
          <CardContent className="p-5">
            <ChatChart spec={chart.spec} />
          </CardContent>
        </Card>
      ) : (
        <div className="flex h-[220px] items-center justify-center rounded-lg border border-dashed border-stone-200 bg-stone-50 px-6 text-center text-sm text-stone-400">
          Not enough data in this project to draw “{chart.question}” — CIE won&apos;t guess.
        </div>
      )}
    </div>
  );
}

function RowsDrillView({ drill, chart, onBack }: { drill: RowsDrill; chart: CannedChart; onBack: () => void }) {
  const { sourceCache, setPendingAsk, openSourceViewer } = useCie();
  const { rows, meta } = drill;
  const numeric = !!meta.valueUnit;

  const [sel, setSel] = useState<AxisSel>({ a: null, b: null });
  const [focusId, setFocusId] = useState<string | null>(null);

  const pick = useCallback((axis: "a" | "b", key: string) => {
    setSel((s) => {
      if (key === OTHER) return { ...s, [axis]: null };
      return { ...s, [axis]: s[axis] === key ? null : key };
    });
  }, []);

  // The "Ask CIE" canned prompt: problem → impact → fix, with the recorded
  // facts attached (the engine's message + remediation, severity, file:line,
  // provenance, and the verbatim code when the file is in hand).
  const askRow = useCallback(
    (loc: DrillLoc | null, title: string, row?: DrillRow | null) => {
      const snippet = loc?.file && sourceCache[loc.file] ? snippetAround(sourceCache[loc.file], loc.line ?? null) : undefined;
      setPendingAsk(
        buildCannedAsk(chart.question, row ?? { title }, loc, snippet, drill.meta.source)
      );
    },
    [sourceCache, setPendingAsk, chart.question, drill]
  );

  const openFull = useCallback(
    (loc: DrillLoc) => {
      const focused = drill.rows.find((r) => r.id === focusId);
      openSourceViewer({
        file: loc.file,
        line: loc.line,
        endLine: loc.endLine,
        byteStart: loc.byteStart,
        byteEnd: loc.byteEnd,
        elName: focused?.title ?? shortKey(loc.file),
        chartTitle: chart.question,
      });
    },
    [drill.rows, focusId, openSourceViewer, chart.question]
  );

  const matching = useMemo(() => rows.filter((r) => rowMatches(r, sel)), [rows, sel]);
  const rowsForA = useMemo(() => (sel.b ? rows.filter((r) => r.keyB === sel.b) : rows), [rows, sel.b]);
  const rowsForB = useMemo(() => (sel.a ? rows.filter((r) => r.keyA === sel.a) : rows), [rows, sel.a]);
  const aggA = useMemo(() => aggregate(rowsForA, "A", numeric), [rowsForA, numeric]);
  const aggB = useMemo(() => aggregate(rowsForB, "B", numeric), [rowsForB, numeric]);

  // list order: urgency first when severities exist (then by recorded line);
  // value rows by their value; everything else keeps the export's own order.
  const ordered = useMemo(() => {
    if (matching.some((r) => r.severity))
      return [...matching].sort(
        (x, y) =>
          SEV_ORDER.indexOf(x.severity ?? "Low") - SEV_ORDER.indexOf(y.severity ?? "Low") ||
          (x.loc?.line ?? Number.MAX_SAFE_INTEGER) - (y.loc?.line ?? Number.MAX_SAFE_INTEGER)
      );
    if (numeric) return [...matching].sort((x, y) => (y.value ?? 0) - (x.value ?? 0));
    return matching;
  }, [matching, numeric]);

  const focused = focusId ? matching.find((r) => r.id === focusId) ?? null : null;

  // the viewer's anchor: the focused row's own location; otherwise, when
  // every surviving row shares one file, that file — anchored at its first
  // recorded line. Otherwise the viewer stays at its empty state.
  const viewerLoc = useMemo<DrillLoc | null>(() => {
    if (focused) return focused.loc ?? null;
    // common file check uses the PRIMARY locations; the paired twins
    // (duplicated block on the other side, doc↔code pairs) are reached
    // through the flip toggle, not the auto-anchor.
    const files = new Map<string, DrillLoc>();
    for (const r of matching) if (r.loc?.file) files.set(r.loc.file, r.loc);
    if (files.size !== 1) return null;
    const [only] = [...files.values()];
    return only;
  }, [focused, matching]);
  const viewerOther = useMemo<DrillLoc | null>(() => {
    if (focused?.loc2) return focused.loc2;
    if (viewerLoc) {
      const withTwin = matching.find((r) => r.loc2 && (r.loc?.file === viewerLoc.file || r.loc2.file === viewerLoc.file));
      if (withTwin?.loc2 && withTwin.loc2.file !== viewerLoc.file) return withTwin.loc2;
    }
    return null;
  }, [focused, matching, viewerLoc]);

  const total = rows.reduce((a, r) => a + (r.value ?? 1), 0);
  const shownCount = matching.reduce((a, r) => a + (r.value ?? 1), 0);
  const unit = meta.valueUnit ? fmt(0, meta.valueUnit).slice(1).replace(/^\d+(\.\d+)?\s?/, "") : "";

  const clearAll = () => {
    setSel({ a: null, b: null });
    setFocusId(null);
  };

  const chips = (
    <>
      {sel.a && (
        <SelChip
          label={`${meta.aName}: ${shortKey(aggA.find((b) => b.key === sel.a)?.label ?? sel.a)}`}
          onClear={() => pick("a", sel.a!)}
        />
      )}
      {sel.b && (
        <SelChip
          label={`${meta.bName}: ${shortKey(aggB.find((b) => b.key === sel.b)?.label ?? sel.b)}`}
          onClear={() => pick("b", sel.b!)}
        />
      )}
      {(sel.a || sel.b) && (
        <>
          <span className="rounded-full bg-stone-100 px-2.5 py-1 text-[11px] tabular-nums text-stone-500">
            {fmt(shownCount, meta.valueUnit)} of {fmt(total, meta.valueUnit)}
          </span>
          <button
            type="button"
            onClick={clearAll}
            className="inline-flex items-center gap-1 rounded-full border border-stone-300 bg-white px-2.5 py-1 text-[11px] text-stone-600 hover:border-rose-300 hover:text-rose-700"
          >
            <RotateCcw className="h-3 w-3" aria-hidden /> Clear
          </button>
        </>
      )}
    </>
  );

  return (
    <div className="space-y-4">
      <DrillHeader question={chart.question} hint={chart.hint} onBack={onBack} note={meta.blurb} chips={chips} />

      {/* the two linked charts */}
      <div className={`grid items-start gap-4 ${meta.kindB === "none" ? "md:grid-cols-1" : "md:grid-cols-2"}`}>
        <ChartCardFor title={meta.axisA.title + (sel.b ? ` — only “${shortKey(sel.b)}”` : "")} tell={meta.axisA.tell}>
          {meta.kindA === "bar" ? (
            <LinkedBar data={aggA} selected={sel.a} onPick={(k) => pick("a", k)} unit={meta.valueUnit} />
          ) : (
            <LinkedPie
              data={aggA}
              selected={sel.a}
              onPick={(k) => pick("a", k)}
              unit={meta.valueUnit}
              center={{
                top: fmt(sel.b ? shownCount : total, meta.valueUnit),
                bottom: sel.b ? `only “${shortKey(sel.b)}”` : `${numeric ? fmt(total, meta.valueUnit) : rows.length} in this project`,
              }}
            />
          )}
        </ChartCardFor>
        {meta.kindB !== "none" && (
          <ChartCardFor title={meta.axisB.title + (sel.a ? ` — inside “${shortKey(sel.a)}”` : "")} tell={meta.axisB.tell}>
            {meta.kindB === "bar" ? (
              <LinkedBar data={aggB} selected={sel.b} onPick={(k) => pick("b", k)} unit={meta.valueUnit} />
            ) : (
              <LinkedPie
                data={aggB}
                selected={sel.b}
                onPick={(k) => pick("b", k)}
                unit={meta.valueUnit}
                center={{ top: fmt(shownCount, meta.valueUnit), bottom: sel.b ? meta.bName.toLowerCase() : `${rowsForB.length} for all` }}
              />
            )}
          </ChartCardFor>
        )}
      </div>

      {/* matching rows + the per-question viewer */}
      <div className="grid items-stretch gap-4 lg:grid-cols-[minmax(300px,440px)_1fr]">
        <Card className="border-stone-200">
          <CardContent className="p-4">
            <RowList rows={ordered} focusedId={focusId} onFocus={(id) => setFocusId(id === focusId ? null : id)} meta={meta} />
          </CardContent>
        </Card>
        <Card className="border-stone-200">
          <CardContent className="h-full p-4">
            {meta.viewer.kind === "commit" ? (
              focused ? (
                <CommitCard row={focused} meta={meta} onAsk={askRow} />
              ) : (
                <div className="flex h-full min-h-[300px] items-center justify-center rounded-lg border border-dashed border-stone-200 bg-stone-50 p-6 text-center">
                  <p className="text-sm text-stone-400">{meta.viewer.empty}</p>
                </div>
              )
            ) : meta.viewer.kind === "none" ? (
              <NoViewer meta={meta} count={matching.length} label={unit} />
            ) : (
              <DrillViewer
                key={`${viewerLoc?.file ?? "none"}:${focusId ?? ""}`}
                loc={viewerLoc}
                other={viewerOther}
                rows={matching}
                meta={meta}
                focused={focused}
                onAsk={askRow}
                onOpenFull={openFull}
              />
            )}
          </CardContent>
        </Card>
      </div>

      {/* the honesty footer — where these rows come from */}
      <p className="px-1 text-[11px] text-stone-400">
        {meta.source ?? "drawn from the loaded project exports"} · nothing is invented: an empty dataset renders an honest empty state
      </p>
    </div>
  );
}

// keep the ChatChartSpec type observable for callers of DrilldownChart
export type { ChatChartSpec };