// RFC-0024 stage 2 — the five PersonaPresets (charter §3, ux-spec §14).
//
// One frame, one engine, one honesty contract — personas differ only in
// the ChartPlan: which panels, in what order, at what default scope, with
// which default preferences. The agent persona is deliberately ABSENT
// (ux-spec §9: an agent consumes the same plans as structured claims over
// MCP; the menu never offers a screen it cannot render).

export type PersonaId = "fresher" | "reviewer" | "architect" | "auditor" | "consultant";

/** Panel ids — the Atlas panel vocabulary that exists today
 *  (atlas-view.tsx PANELS); a preset never names a panel the frame
 *  cannot draw. planOrder is the R3 grid reading order; the R4 slot
 *  (stage 4) sits FIRST when it is active — that position is owned by
 *  G5, not by the presets. */
export type AtlasPanelId =
  | "containers"
  | "appentry"
  | "svcentry"
  | "components"
  | "sequence"
  | "dataflow"
  | "flow";

export interface PersonaPreset {
  id: PersonaId;
  label: string; // the menu label (ux-spec §14.1)
  purpose: string; // the one-line "what this view is for" under the label
  defaultFactsOnly: boolean;
  hero: AtlasPanelId; // R1
  planOrder: AtlasPanelId[]; // R3, reading order
  defaultScope: "system" | "default-container";
  /** charter §6/§10: consultant behaves as Reviewer with a change context. */
  behavesAsReviewerWithChangeContext?: boolean;
}

export const PERSONA_PRESETS: Record<PersonaId, PersonaPreset> = {
  fresher: {
    id: "fresher",
    label: "Onboarding (Fresher)",
    purpose: "orientation before detail — the shape of the whole, one container at a time",
    defaultFactsOnly: false,
    hero: "containers",
    // charter §4.2: containers → entry points/routes → components → flow → dataflow
    planOrder: ["containers", "appentry", "svcentry", "components", "flow", "dataflow"],
    defaultScope: "default-container",
  },
  reviewer: {
    id: "reviewer",
    label: "Reviewer",
    purpose: "judge the recorded: entry points, the flow they drive, and every write",
    defaultFactsOnly: true,
    hero: "svcentry",
    // ux-spec §6 planOrder maps onto today's panels: entry/routes, flow, dataflow (writes-first), components
    planOrder: ["svcentry", "appentry", "flow", "dataflow", "components", "sequence"],
    defaultScope: "system",
  },
  architect: {
    id: "architect",
    label: "Architect / Maintainer",
    purpose: "whole-system structure with inferred structure named honestly (full detail)",
    defaultFactsOnly: false,
    hero: "containers",
    planOrder: ["containers", "components", "flow", "dataflow", "sequence"],
    defaultScope: "system",
  },
  auditor: {
    id: "auditor",
    label: "Auditor / Security",
    purpose: "repeat nothing merely inferred — routes, externals and cross-boundary writes",
    defaultFactsOnly: true,
    hero: "svcentry",
    // ux-spec §8: system-wide routes, externals, findings, cross-boundary writes
    planOrder: ["svcentry", "dataflow", "flow", "components", "containers"],
    defaultScope: "system",
  },
  consultant: {
    id: "consultant",
    label: "Consultant (foreign repo)",
    purpose: "auto-focus the busiest container, then walk it like a fresher",
    defaultFactsOnly: false,
    hero: "containers",
    planOrder: ["containers", "appentry", "components", "flow", "dataflow"],
    defaultScope: "default-container",
    behavesAsReviewerWithChangeContext: true,
  },
};

export const PERSONA_ORDER: PersonaId[] = ["fresher", "reviewer", "architect", "auditor", "consultant"];

/** §14.2-4 — per-persona layout memory (sessionStorage): each preset
 *  remembers its own last scope + facts-only override, so switching
 *  Fresher↔Reviewer restores each persona's last state instead of
 *  clobbering it. Browser-only. */
export interface PersonaMemory {
  factsOnlyOverride: boolean | null; // null = follow the preset default
  factsOnlyValue: boolean; // last effective value (restored on switch-back)
}

const KEY = (id: PersonaId) => `cie.atlas.persona.${id}`;

export function loadPersonaMemory(id: PersonaId): PersonaMemory | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.sessionStorage.getItem(KEY(id));
    return raw ? (JSON.parse(raw) as PersonaMemory) : null;
  } catch {
    return null;
  }
}

export function savePersonaMemory(id: PersonaId, mem: PersonaMemory): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(KEY(id), JSON.stringify(mem));
  } catch {
    // storage full/absent — memory is a nicety, never a correctness need
  }
}
