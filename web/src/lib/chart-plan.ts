// RFC-0024 stage 2 — planSheet (web/src/lib/chart-plan.ts).
//
// The pure panel planner: the Atlas frame's R1/R3 contents derive from
// `planSheet(...)` instead of hand-code (ux-spec §14.2-1: "switch =
// re-plan, not re-page"). The producer stays interaction-blind (VIZ-A-01
// extended: CIE never knows what a click is) — the plan owns actions.
//
// Actions are the drill/filter matrix (ux-spec §3/§8, mirrored from the
// catalog's per-kind matrix): `drill` = a click may change scope,
// `filter` = a click only selects. Popups/chips/ask are universal
// choreography verbs and are NOT per-panel data.

import type { AtlasPanelId, PersonaId, PersonaPreset } from "./atlas-presets";
import { PERSONA_PRESETS } from "./atlas-presets";

export type ChartAction = "drill" | "filter";

export interface PlanRow {
  panel: AtlasPanelId;
  detail: "summary" | "full";
  actions: ChartAction[];
  /** Why this panel is here for THIS persona (charter §4–§10, one line). */
  why: string;
}

export interface AtlasPlan {
  preset: PersonaPreset;
  hero: PlanRow;
  rows: PlanRow[]; // R3 grid, reading order (R4 slot inserts at stage 4)
  factsOnly: boolean; // the effective default (user override applied by the caller)
  defaultScope: "system" | "default-container";
}

/** Per-panel interaction class — mirrors the catalog's category defaults:
 *  structural/architectural boxes drill; entry tables and rows filter
 *  (a row is a fact, not a scope). */
const PANEL_ACTIONS: Record<AtlasPanelId, ChartAction[]> = {
  containers: ["drill", "filter"],
  appentry: ["filter"],
  svcentry: ["filter"],
  components: ["drill", "filter"],
  hierarchy: ["drill", "filter"],
  sequence: ["filter"],
  dataflow: ["drill", "filter"],
  flow: ["filter"],
};

const PANEL_WHY: Record<PersonaId, Partial<Record<AtlasPanelId, string>>> = {
  fresher: {
    containers: "the shape of the whole; externals honestly drawn (charter §4.2)",
    appentry: "where execution begins — the first click targets",
    svcentry: "where each service begins",
    components: "parts inside one app, each naming its file modules",
    hierarchy: "the idea tree underneath — click a concept to focus every panel on it",
    flow: "calls, with recorded-evidence chips",
    dataflow: "code ↔ data stores",
  },
  reviewer: {
    svcentry: "judge the recorded surface first (ux-spec §6)",
    appentry: "system entries the change could break",
    flow: "the call chain the entries drive",
    dataflow: "writes first — what the change touches on disk",
    components: "the parts the changed files live in",
    hierarchy: "where the changed concept sits in the whole",
    sequence: "the order the calls happen in",
  },
  architect: {
    containers: "whole-system structure (charter §6: inferred structure is the value)",
    components: "the component tier, full detail",
    hierarchy: "the full concept DAG — Domain down to Rule/Invariant",
    flow: "call structure with inference named",
    dataflow: "data structure with inference named",
    sequence: "behaviour ordering across the system",
  },
  auditor: {
    svcentry: "the system-wide route table, no container filter (ux-spec §8)",
    dataflow: "cross-boundary writes",
    flow: "the paths writes travel",
    components: "what the routes expose",
    hierarchy: "the Rule/Invariant concepts a route's code sits under",
    containers: "trust-boundary shape: containers + externals",
  },
  consultant: {
    containers: "auto-focus the busiest container by file count (ux-spec §10)",
    appentry: "where this foreign repo begins",
    components: "the focused container's parts",
    hierarchy: "the focused container's idea tree",
    flow: "the calls that hold it together",
    dataflow: "the data it reads and writes",
  },
};

/** The pure planner. `factsOnlyOverride` carries the user's manual toggle
 *  (§14.2-3: a toggled switch beats a preset default until the preset is
 *  switched twice-through-system); `detail` escalates to "full" when the
 *  preset wants expanded caps (architect — charter §6). */
export function planSheet(
  presetId: PersonaId,
  opts: { factsOnlyOverride?: boolean | null; detail?: "summary" | "full"; viewport?: { width: number; height: number } } = {},
): AtlasPlan {
  const preset = PERSONA_PRESETS[presetId];
  const detail: "summary" | "full" = opts.detail ?? (presetId === "architect" ? "full" : "summary");
  const factsOnly = opts.factsOnlyOverride ?? preset.defaultFactsOnly;
  const why = PANEL_WHY[presetId];
  const row = (panel: AtlasPanelId): PlanRow => ({
    panel,
    detail,
    actions: PANEL_ACTIONS[panel],
    why: why[panel] ?? "",
  });
  const [heroId, ...rest] = preset.planOrder;
  return {
    preset,
    hero: row(heroId),
    rows: rest.map(row),
    factsOnly,
    defaultScope: preset.defaultScope,
  };
}
