// Chart-in-chat: the AI may attach a complementary chart to its answer.
// The chart menu is a chat-feasible slice of the CIE chart catalog
// (viz-adapter/src/catalog.ts, US-020): each kind carries the engine and
// layout it will actually be rendered with, so the answer discloses HOW it
// was drawn, not just what it says. Bars use a hover-expansion fisheye lens
// modelled on US-023: magnify locally, never mutate the base chart, state
// the source.

export type ChatChartKind = "hbar" | "donut" | "stat" | "table" | "uml" | "flow" | "tree" | "heatmap" | "sequence" | "c4";

/**
 * Where a chart element lives in the source code — the link that used to be
 * thrown away between the engine's exports and the chart. Two honest forms:
 *  - `line` (± `endLine`): a real line number, when the export carried one
 *    flat (quality.json findings, complexity, evidence_lines).
 *  - `byteStart`/`byteEnd`: byte offsets from report.json's symbols[].loc —
 *    resolved to a line only when the file's content is actually available
 *    (bundled demo sources or a consented local folder). Until then the
 *    panel shows the file; it never guesses a line number.
 * Optional everywhere: charts built from aggregates (counts, per-day sums)
 * genuinely have no location and must stay honest about that.
 */
export interface ElementLoc {
  file: string;
  line?: number;
  endLine?: number;
  byteStart?: number;
  byteEnd?: number;
}

export interface ChatChartDataPoint {
  name: string;
  value: number;
  color?: string; // rose | amber | emerald | stone (never colour alone: labels always present)
  loc?: ElementLoc; // where this bar/point lives in the source (optional, honest)
  // True on the one bar/point a scoped question was about ("the file you
  // asked about") — rendered as an outline, never colour alone.
  highlight?: boolean;
}

// One box in a diagram (UML-style or call flow). Diagram geometry is NEVER
// taken from the model: boxes and arrows are built locally from the loaded
// store (src/lib/draw-anything.ts), so every label traces back to indexed
// data. The model may only pick the KIND and write the title/tell.
export interface UmlNode {
  id: string;
  title: string;
  kind: string; // domain | module | usecase | rule | invariant | function | step
  lines?: string[]; // compartment lines (what it does / its members)
  more?: number; // "…and n more" when lines were capped
  certainty?: "Fact" | "Inference" | "Hypothesis";
  loc?: ElementLoc; // the file (and offsets) this box was built from
  selected?: boolean; // the element the dashboard's Selection Context points at — drawn with a ring, not a guess
}

export interface UmlLink {
  from: string;
  to: string;
  label?: string; // plain-words relationship, e.g. "talks to"
  certainty?: "Fact" | "Inference" | "Hypothesis";
  dashed?: boolean; // soft relations (guesses, loose coupling)
}

// One node of a concept-hierarchy tree (kind "tree"). Levels mirror the
// engine's concept tree (Domain → Module → UseCase/Rule); built locally from
// concepts.json — the model never invents hierarchy nodes.
export interface TreeNode {
  id: string;
  label: string;
  kind: string; // Domain | Module | UseCase | Rule ...
  parent: string | null;
  note?: string; // one plain-words line, e.g. behaviour count
  certainty?: "Fact" | "Inference" | "Hypothesis";
  // True on the node the current question was about — drawn with a dark
  // outline (never colour alone), same rule as HeatTile/ChatChartDataPoint.
  highlight?: boolean;
}

// One tile of a heatmap (kind "heatmap") — e.g. per-file test coverage.
export interface HeatTile {
  name: string;
  value: number; // 0..100 — always shown as text too (never colour alone)
  label?: string; // extra line under the tile, e.g. "9 of 12 lines"
  loc?: ElementLoc; // the file this tile measures
  // True on the tile the current question was about — drawn with a dark
  // outline (never colour alone) plus a legend note under the chart.
  highlight?: boolean;
}

export interface ChatChartSpec {
  kind: ChatChartKind;
  catalog?: string; // e.g. "G5" — the catalog family the choice is based on
  engine?: string; // custom-svg | recharts | card | grid
  layout?: string; // fisheye-rows | donut+legend | hero-number | scroll-rows | uml-boxes | call-flow
  title?: string;
  tell?: string; // one-line "what this chart tells you"
  lens?: boolean; // fisheye lens (hbar only)
  data?: ChatChartDataPoint[];
  columns?: string[];
  rows?: (string | number)[][];
  rowLoc?: (ElementLoc | null)[]; // per-row source location, aligned with rows (tables)
  center?: string; // donut centre caption
  unit?: string;
  nodes?: UmlNode[]; // diagrams only — filled from the store, never from the model
  links?: UmlLink[];
  tree?: TreeNode[]; // kind "tree" — built from concepts.json
  tiles?: HeatTile[]; // kind "heatmap" — per-file measured values
  participants?: string[]; // kind "sequence" — the lanes (files), in first-appearance order
  msgs?: SequenceMsg[]; // kind "sequence" — the arrows (calls), with each step's state
  source?: string; // honesty note: which export the diagram was built from
}

// ---------- mastery journey card vocabulary (lib/mastery.ts builds them;
// the chat renders them — never the model, never invented) ----------

/** One row of an entry-points / flows card. */
export interface MasteryRow {
  idx: number;
  title: string;
  detail: string;
  file?: string;
  byteStart?: number;
  byteEnd?: number;
  /** CONFIRMED = the engine fully read this symbol; INFERRED = concluded, not read */
  status: "confirmed" | "inferred";
  /** one-line honesty reason attached to an INFERRED tag */
  why?: string;
  /** trace target for flow candidates */
  entryId?: number;
}

/** One step of a trace — the engine's own record for that symbol. */
export interface TraceStep {
  idx: number;
  caller?: string;
  name: string;
  qualName: string;
  file: string;
  byteStart?: number;
  byteEnd?: number;
  reads: string[];
  writes: string[];
  effects: string[];
  exceptions: string[];
  /** no summary recorded for this symbol at all */
  noSummary: boolean;
  /** the engine's summary recorded AND marked complete */
  complete: boolean;
}

/** One arrow of a sequence diagram. */
export interface SequenceMsg {
  from: string;
  to: string;
  label: string;
  sub?: string;
  /** the strongest state the step holds at draw time — dashed emerald = you */
  status: "you" | "confirmed" | "inferred";
  /** the real code this arrow stands for (panel opens exactly here) */
  loc?: ElementLoc;
}

/** The interactive card a mastery fast path may attach to a chat answer:
 *  entry points, flow candidates, the trace (with the verification loop),
 *  or the "5 things" draft. Built in lib/mastery.ts from the loaded
 *  report — the model has no say over a single field of it. */
export interface MasteryCardData {
  kind: "entrypoints" | "flows" | "trace" | "fivethings";
  title: string;
  entryRows?: MasteryRow[];
  steps?: TraceStep[];
  /** trace honesty: symbols the walk reached but capped out of the draw */
  truncated?: number;
  entryQual?: string;
  entryId?: number;
  items?: string[];
  note?: string;
  traceKey?: string;
}

export interface ParsedReply {
  answer: string;
  chart?: ChatChartSpec;
}

const KINDS: ChatChartKind[] = ["hbar", "donut", "stat", "table", "uml", "flow", "tree", "heatmap"];
const COLORS = new Set(["rose", "amber", "emerald", "stone"]);

/** Engine + layout per kind — deterministic, so a hallucinated pair gets corrected. */
export function engineFor(kind: ChatChartKind): string {
  switch (kind) {
    case "hbar": return "custom-svg";
    case "donut": return "recharts";
    case "stat": return "card";
    case "table": return "grid";
    case "uml": return "custom-svg";
    case "flow": return "custom-svg";
    case "tree": return "custom-svg";
    case "heatmap": return "custom-svg";
    case "sequence": return "custom-svg";
    case "c4": return "custom-svg";
  }
}
export function layoutFor(kind: ChatChartKind): string {
  switch (kind) {
    case "hbar": return "fisheye-rows";
    case "donut": return "donut+legend";
    case "stat": return "hero-number";
    case "table": return "scroll-rows";
    case "uml": return "uml-boxes";
    case "flow": return "call-flow";
    case "tree": return "concept-tree";
    case "heatmap": return "coverage-grid";
    case "sequence": return "sequence-lanes";
    case "c4": return "uml-boxes";
  }
}

function sanitizeChart(raw: unknown): ChatChartSpec | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const c = raw as Record<string, unknown>;
  const kind = String(c.kind ?? "") as ChatChartKind;
  if (!KINDS.includes(kind)) return undefined;

  const spec: ChatChartSpec = {
    kind,
    catalog: typeof c.catalog === "string" ? c.catalog.slice(0, 8) : undefined,
    engine: engineFor(kind),
    layout: layoutFor(kind),
    title: typeof c.title === "string" ? c.title.slice(0, 90) : undefined,
    tell: typeof c.tell === "string" ? c.tell.slice(0, 160) : undefined,
    lens: kind === "hbar" ? c.lens !== false : undefined,
  };

  if (kind === "table") {
    const cols = Array.isArray(c.columns) ? c.columns.filter((x) => typeof x === "string").slice(0, 4) : [];
    const rows = Array.isArray(c.rows)
      ? c.rows
          .filter((r) => Array.isArray(r))
          .slice(0, 8)
          .map((r) => (r as unknown[]).slice(0, cols.length || 4).map((v) => String(v ?? "").slice(0, 60)))
      : [];
    if (!cols.length || !rows.length) return undefined;
    spec.columns = cols;
    spec.rows = rows;
    return spec;
  }

  // Diagrams, trees and heatmaps: the model gets NO say over boxes, arrows,
  // hierarchy nodes or tiles. Whatever it sends is dropped here; the caller
  // refills them from the loaded store (src/lib/draw-anything.ts,
  // src/lib/quality.ts). Only title + tell survive.
  if (kind === "uml" || kind === "flow" || kind === "tree" || kind === "heatmap") {
    return spec;
  }

  const data = Array.isArray(c.data)
    ? (c.data as Record<string, unknown>[])
        .filter((d) => d && typeof d.name === "string" && Number.isFinite(Number(d.value)))
        .slice(0, 8)
        .map((d) => ({
          name: String(d.name).slice(0, 40),
          value: Number(d.value),
          color: typeof d.color === "string" && COLORS.has(d.color) ? d.color : undefined,
        }))
    : [];
  if (!data.length) return undefined;
  spec.data = data;
  if (kind === "stat") {
    spec.unit = typeof c.unit === "string" ? c.unit.slice(0, 24) : undefined;
  }
  if (kind === "donut") {
    spec.center = typeof c.center === "string" ? c.center.slice(0, 40) : "in total";
  }
  return spec;
}

/** True for the kinds whose boxes/arrows/lanes are built from the store. */
export function isDiagramKind(kind: ChatChartKind): boolean {
  return kind === "uml" || kind === "flow" || kind === "tree" || kind === "heatmap" || kind === "sequence" || kind === "c4";
}

/**
 * Models sometimes write a raw newline/tab inside a JSON string instead of
 * the escaped \n/\t JSON requires (very common when the "answer" field is a
 * multi-line bulleted list) — JSON.parse rejects that outright. Walk the
 * same in-string state parseChartReply already tracks and escape any raw
 * control character found inside a string literal, so a model's formatting
 * slip doesn't throw away the whole parse (and the chart with it).
 */
function escapeRawControlCharsInStrings(body: string): string {
  let out = "";
  let inStr = false;
  let esc = false;
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (esc) { out += ch; esc = false; continue; }
    if (ch === "\\") { out += ch; esc = true; continue; }
    if (ch === '"') { inStr = !inStr; out += ch; continue; }
    if (inStr && ch.charCodeAt(0) < 0x20) {
      if (ch === "\n") out += "\\n";
      else if (ch === "\r") out += "\\r";
      else if (ch === "\t") out += "\\t";
      else out += "\\u" + ch.charCodeAt(0).toString(16).padStart(4, "0");
      continue;
    }
    out += ch;
  }
  return out;
}

/**
 * The model is asked to answer with a single JSON object, but models
 * sometimes wrap it in prose or code fences. Extract tolerantly: find the
 * first balanced {...} block and parse it; fall back to treating the whole
 * text as the answer (chart-less answers are always valid).
 */
export function parseChartReply(raw: string): ParsedReply {
  const text = String(raw ?? "").trim();
  if (!text) return { answer: "CIE gave an empty answer. Please try asking again." };

  let body = text;
  const fence = text.match(/```(?:json)?\s*([\s\S]*?)```/);
  if (fence) body = fence[1].trim();

  const start = body.indexOf("{");
  if (start >= 0) {
    let depth = 0;
    let inStr = false;
    let esc = false;
    for (let i = start; i < body.length; i++) {
      const ch = body[i];
      if (esc) { esc = false; continue; }
      if (ch === "\\") { esc = true; continue; }
      if (ch === '"') inStr = !inStr;
      if (inStr) continue;
      if (ch === "{") depth++;
      else if (ch === "}") {
        depth--;
        if (depth === 0) {
          const candidate = body.slice(start, i + 1);
          try {
            const obj = JSON.parse(candidate) as Record<string, unknown>;
            if (typeof obj.answer === "string") {
              return { answer: obj.answer, chart: sanitizeChart(obj.chart) };
            }
          } catch {
            // Raw parse failed — most often an unescaped newline/tab inside
            // a string (models write these often). Retry once, sanitized,
            // before giving up and showing the raw JSON as plain text.
            try {
              const obj = JSON.parse(escapeRawControlCharsInStrings(candidate)) as Record<string, unknown>;
              if (typeof obj.answer === "string") {
                return { answer: obj.answer, chart: sanitizeChart(obj.chart) };
              }
            } catch {
              /* fall through to plain-text answer */
            }
          }
          break;
        }
      }
    }
  }
  return { answer: text };
}
