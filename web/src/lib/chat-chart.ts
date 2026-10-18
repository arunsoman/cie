// Chart-in-chat: the AI may attach a complementary chart to its answer.
// The chart menu is a chat-feasible slice of the CIE chart catalog
// (viz-adapter/src/catalog.ts, US-020): each kind carries the engine and
// layout it will actually be rendered with, so the answer discloses HOW it
// was drawn, not just what it says. Bars use a hover-expansion fisheye lens
// modelled on US-023: magnify locally, never mutate the base chart, state
// the source.

export type ChatChartKind = "hbar" | "donut" | "stat" | "table";

export interface ChatChartDataPoint {
  name: string;
  value: number;
  color?: string; // rose | amber | emerald | stone (never colour alone: labels always present)
}

export interface ChatChartSpec {
  kind: ChatChartKind;
  catalog?: string; // e.g. "G5" — the catalog family the choice is based on
  engine?: string; // custom-svg | recharts | card | grid
  layout?: string; // fisheye-rows | donut+legend | hero-number | scroll-rows
  title?: string;
  tell?: string; // one-line "what this chart tells you"
  lens?: boolean; // fisheye lens (hbar only)
  data?: ChatChartDataPoint[];
  columns?: string[];
  rows?: (string | number)[][];
  center?: string; // donut centre caption
  unit?: string;
}

export interface ParsedReply {
  answer: string;
  chart?: ChatChartSpec;
}

const KINDS: ChatChartKind[] = ["hbar", "donut", "stat", "table"];
const COLORS = new Set(["rose", "amber", "emerald", "stone"]);

/** Engine + layout per kind — deterministic, so a hallucinated pair gets corrected. */
export function engineFor(kind: ChatChartKind): string {
  switch (kind) {
    case "hbar": return "custom-svg";
    case "donut": return "recharts";
    case "stat": return "card";
    case "table": return "grid";
  }
}
export function layoutFor(kind: ChatChartKind): string {
  switch (kind) {
    case "hbar": return "fisheye-rows";
    case "donut": return "donut+legend";
    case "stat": return "hero-number";
    case "table": return "scroll-rows";
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
            /* fall through to plain-text answer */
          }
          break;
        }
      }
    }
  }
  return { answer: text };
}
