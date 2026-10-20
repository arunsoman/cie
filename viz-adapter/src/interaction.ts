/**
 * Interaction layer — host-side closes for the CIE-US stories that name
 * the canvas (PRD Part IV/V). Every function here is pure and testable;
 * the browser shell wires them to real events. Honesty rules carry over:
 * proposals are never applied silently, quality is measured not asserted,
 * and every pointer interaction has a keyboard equivalent (US-140) with
 * the outline as the authoritative non-canvas surface (US-141).
 *
 * Closes the host halves of:
 *   US-022 — level-of-detail + settle detection (zoom without losing
 *            readability)
 *   US-023 — hover-expansion lens over groups (magnify locally without
 *            disturbing the map)
 *   US-028 — gesture → visual proposal (proposed change stays proposed)
 *   US-030 — layout quality reporting (disclosed strategy + measured
 *            overlap/crossings, not vibes)
 *   US-140 — keyboard parity map (every pointer action has a key)
 *   US-141 — announcements + roving order over the text outline
 *   US-142 — never-colour-alone audit (shape/pattern/label redundancy)
 *   US-143 — theme tokens (semantic roles, no raw colours in the contract)
 */

import type {
  DisplayMode,
  LayoutHint,
  ViewSpec,
} from './types';

// ── US-022: level of detail + settle ────────────────────────────────────

/** LOD tiers: what the canvas should render at a given zoom. */
export type LodTier = 'labels-only' | 'clustered' | 'full';

export interface LodDecision {
  tier: LodTier;
  /** Why this tier (disclosed to the footer/debug line). */
  why: string;
}

/**
 * Deterministic LOD: below `smallScale` only labels; below `largeScale`
 * clustered (groups collapse to aggregates); above that, full. Readability
 * is protected by DECIDING, not by hoping the engine handles it.
 */
export function levelOfDetail(
  scale: number,
  elementCount: number,
  opts?: { smallScale?: number; largeScale?: number; fullThreshold?: number },
): LodDecision {
  const small = opts?.smallScale ?? 0.35;
  const large = opts?.largeScale ?? 1.6;
  const fullThreshold = opts?.fullThreshold ?? 400;
  if (scale < small) {
    return { tier: 'labels-only', why: `scale ${scale} < ${small}: only labels stay readable` };
  }
  if (scale < large && elementCount > fullThreshold) {
    return {
      tier: 'clustered',
      why: `scale ${scale} in [${small}, ${large}) with ${elementCount} elements: clusters render, members collapse`,
    };
  }
  return { tier: 'full', why: `scale ${scale} with ${elementCount} elements: full fidelity` };
}

/** A settle detector: pan/zoom bursts collapse into one settle event. */
export class SettleDetector {
  private lastEventAt = 0;
  private settledAt = 0;
  constructor(private readonly quietMs: number = 120) {}

  /** Feed one interaction timestamp; returns true when the stream has
   *  been quiet for `quietMs` (the view may re-layout). */
  feed(nowMs: number): boolean {
    if (nowMs - this.lastEventAt >= this.quietMs && nowMs !== this.lastEventAt) {
      this.settledAt = nowMs;
      return true;
    }
    this.lastEventAt = nowMs;
    return false;
  }

  get lastSettledAt(): number {
    return this.settledAt;
  }
}

// ── US-023: hover-expansion lens ────────────────────────────────────────

export interface LensData {
  groupId: string;
  aggregateLabel: string;
  /** Members the lens expands (from the spec's groups + elements). */
  members: { id: string; label: string; displayMode: DisplayMode }[];
  /** Honest limitation: expansion data is spec-derived. */
  source: 'spec-groups';
}

/**
 * The hover-expansion lens: given the group under the pointer, assemble
 * what the lens shows — the aggregate and its members — WITHOUT
 * mutating the map (the lens overlays; the base map stays put).
 */
export function expansionLens(spec: ViewSpec, groupId: string): LensData | null {
  const group = spec.groups?.find((g) => g.id === groupId);
  if (!group) return null;
  const members = spec.elements
    .filter((e) => e.parentId === groupId)
    .map((e) => ({ id: e.id, label: e.label, displayMode: e.displayMode }));
  return {
    groupId: group.id,
    aggregateLabel: group.label,
    members,
    source: 'spec-groups',
  };
}

// ── US-028: gesture → proposal ──────────────────────────────────────────

export type GestureKind = 'drag-node' | 'connect' | 'delete-node';

export interface Gesture {
  kind: GestureKind;
  /** Element the gesture started on. */
  targetId: string;
  /** Drop target / connect-to id (connect gesture). */
  toId?: string;
  /** Proposed position for drag (canvas coords). */
  x?: number;
  y?: number;
}

export interface Proposal {
  gesture: GestureKind;
  /** Human summary of the PROPOSED change. */
  summary: string;
  /** The spec fields the proposal would touch (never applied directly). */
  touches: string[];
  /** Consequences the engine already computed (impact/deletion refs). */
  engineConsequences: string[];
  /** Permanent label: a proposal is not an applied change. */
  status: 'proposed';
}

/**
 * Map a canvas gesture to a PROPOSAL. The proposal is the unit the
 * product's "express a proposed change visually" flow sends to the
 * engine for consequence computation (impact/deletion) — the gesture
 * itself never mutates the spec.
 */
export function gestureToProposal(g: Gesture, spec: ViewSpec): Proposal | null {
  const exists = spec.elements.some((e) => e.id === g.targetId);
  if (!exists) return null;
  switch (g.kind) {
    case 'drag-node':
      return {
        gesture: 'drag-node',
        summary: `move ${g.targetId} to (${g.x ?? '?'}, ${g.y ?? '?'})`,
        touches: ['layout'],
        engineConsequences: [],
        status: 'proposed',
      };
    case 'connect': {
      const toExists = g.toId ? spec.elements.some((e) => e.id === g.toId) : false;
      if (!toExists) return null;
      return {
        gesture: 'connect',
        summary: `connect ${g.targetId} → ${g.toId}`,
        touches: ['edges'],
        engineConsequences: ['impact of the new relation is an engine query (cli:impact)'],
        status: 'proposed',
      };
    }
    case 'delete-node':
      return {
        gesture: 'delete-node',
        summary: `remove ${g.targetId}`,
        touches: ['elements', 'edges'],
        engineConsequences: [
          'deletion counterfactual is engine work (deletion oracle, US-099/US-058)',
        ],
        status: 'proposed',
      };
  }
}

// ── US-030: layout quality reporting ────────────────────────────────────

export interface Position {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayoutQuality {
  strategy: string;
  /** Disclosed strategy (VIZ-D-03): what was requested vs what ran. */
  requested: string | undefined;
  actual: string;
  /** Measured overlaps (node rectangles intersecting). */
  overlaps: number;
  /** Measured edge crossings (segment pairs intersecting). */
  crossings: number;
  /** The honest one-liner for the quality badge. */
  summary: string;
}

function rectsOverlap(a: Position, b: Position): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

function segmentsIntersect(
  p1: { x: number; y: number },
  p2: { x: number; y: number },
  p3: { x: number; y: number },
  p4: { x: number; y: number },
): boolean {
  const d = (px: number, py: number, qx: number, qy: number, rx: number, ry: number) =>
    (qx - px) * (ry - py) - (qy - py) * (rx - px);
  const d1 = d(p3.x, p3.y, p4.x, p4.y, p1.x, p1.y);
  const d2 = d(p3.x, p3.y, p4.x, p4.y, p2.x, p2.y);
  const d3 = d(p1.x, p1.y, p2.x, p2.y, p3.x, p3.y);
  const d4 = d(p1.x, p1.y, p2.x, p2.y, p4.x, p4.y);
  return ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0));
}

/**
 * Report layout quality from MEASURED geometry: node overlaps and edge
 * crossings over the actual positions. The strategy name is disclosed
 * (requested vs actual — VIZ-D-03), never asserted as "good".
 */
export function layoutQuality(
  spec: ViewSpec,
  positions: Map<string, Position>,
  nodeSize: { w: number; h: number },
  requested: string | undefined,
  actual: string,
): LayoutQuality {
  let overlaps = 0;
  const els = spec.elements.filter((e) => positions.has(e.id));
  for (let i = 0; i < els.length; i++) {
    for (let j = i + 1; j < els.length; j++) {
      const a = positions.get(els[i].id)!;
      const b = positions.get(els[j].id)!;
      if (rectsOverlap(a, b)) overlaps++;
    }
  }
  let crossings = 0;
  const center = (id: string) => {
    const p = positions.get(id)!;
    return { x: p.x + nodeSize.w / 2, y: p.y + nodeSize.h / 2 };
  };
  const segs = spec.edges
    .filter((e) => positions.has(e.source) && positions.has(e.target))
    .map((e) => ({ id: e.id, a: center(e.source), b: center(e.target) }));
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      // skip pairs sharing an endpoint (they "cross" at the node only)
      if (segs[i].a === segs[j].a || segs[i].b === segs[j].b || segs[i].a === segs[j].b || segs[i].b === segs[j].a) {
        continue;
      }
      if (segmentsIntersect(segs[i].a, segs[i].b, segs[j].a, segs[j].b)) crossings++;
    }
  }
  return {
    strategy: actual,
    requested,
    actual,
    overlaps,
    crossings,
    summary: `layout ${actual}${requested && requested !== actual ? ` (requested ${requested})` : ''}: ${overlaps} overlaps, ${crossings} crossings`,
  };
}

// ── US-140: keyboard parity ─────────────────────────────────────────────

export interface KeyBinding {
  pointer: GestureKind | 'select' | 'zoom-in' | 'zoom-out' | 'reset';
  keys: string[];
  /** What the action does (announcement text). */
  action: string;
}

/**
 * The keyboard parity contract: every pointer interaction has at least
 * one keyboard path. `verifyParity` fails when a pointer action is
 * missing — the map is a contract, not documentation.
 */
export function keyboardMap(): KeyBinding[] {
  return [
    { pointer: 'select', keys: ['Tab', 'Shift+Tab', 'Enter'], action: 'move focus / select element' },
    { pointer: 'zoom-in', keys: ['+'], action: 'zoom in' },
    { pointer: 'zoom-out', keys: ['-'], action: 'zoom out' },
    { pointer: 'reset', keys: ['0'], action: 'reset view' },
    { pointer: 'drag-node', keys: ['ArrowKeys (focused)'], action: 'move focused element' },
    { pointer: 'connect', keys: ['c'], action: 'start/complete connection from focused element' },
    { pointer: 'delete-node', keys: ['Delete'], action: 'propose removal of focused element' },
  ];
}

export function verifyParity(map: KeyBinding[]): { ok: boolean; missing: string[] } {
  const required: (KeyBinding['pointer'])[] = [
    'select', 'zoom-in', 'zoom-out', 'reset', 'drag-node', 'connect', 'delete-node',
  ];
  const have = new Set(map.map((k) => k.pointer));
  const missing = required.filter((r) => !have.has(r));
  return { ok: missing.length === 0, missing };
}

// ── US-141: announcements ───────────────────────────────────────────────

/**
 * Screen-reader announcement for the focused element: label, how it is
 * known (displayMode), and its evidence file — derived from the SPEC
 * (the outline stays authoritative; this is the live-focus variant).
 */
export function announce(spec: ViewSpec, elementId: string): string | null {
  const el = spec.elements.find((e) => e.id === elementId);
  if (!el) return null;
  const known: Record<DisplayMode, string> = {
    Fact: 'fact, statically proven',
    Inference: 'inference, derived from cited evidence',
    Hypothesis: 'hypothesis, cannot be proven statically',
    Fog: 'fog, not determinable from static analysis',
  };
  return `${el.label}, ${known[el.displayMode]}, evidence ${el.evidence.file}`;
}

/** Roving tabindex order = outline order (elements, then edges). */
export function rovingOrder(spec: ViewSpec): string[] {
  return [...spec.elements.map((e) => e.id), ...spec.edges.map((e) => e.id)];
}

// ── US-142: never colour alone ──────────────────────────────────────────

/**
 * Each displayMode carries a REQUIRED non-colour channel: a shape suffix
 * and a text prefix. Any renderer that emits colour MUST also emit these
 * (the audit fails a colour-only encoding).
 */
export const MODE_CHANNELS: Record<DisplayMode, { shape: string; prefix: string }> = {
  Fact: { shape: 'square', prefix: '[=]' },
  Inference: { shape: 'circle', prefix: '[~]' },
  Hypothesis: { shape: 'dashed-border', prefix: '[?]' },
  Fog: { shape: 'dotted-border', prefix: '[X]' },
};

/**
 * Never-colour-alone audit: every element must be distinguishable by
 * shape/prefix alone. Two elements of the SAME mode share the channels
 * (allowed); elements of DIFFERENT modes must not collapse to the same
 * shape in the render contract (they cannot — the map is injective).
 * The audit's real job: fail if a renderer registered a mode channel
 * override that collides.
 */
export function colourAloneAudit(
  overrides: Partial<Record<DisplayMode, string>> = {},
): { ok: boolean; findings: string[] } {
  const findings: string[] = [];
  const modes = Object.keys(MODE_CHANNELS) as DisplayMode[];
  const shapes = new Map<string, string>();
  for (const m of modes) {
    const shape = overrides[m] ?? MODE_CHANNELS[m].shape;
    const owner = shapes.get(shape);
    if (owner) {
      findings.push(`VIZ-V-03: modes ${owner} and ${m} collide on shape ${shape} — colour would be the only channel`);
    } else {
      shapes.set(shape, m);
    }
  }
  return { ok: findings.length === 0, findings };
}

// ── US-143: theme tokens ────────────────────────────────────────────────

/**
 * Semantic theme tokens — the contract carries ROLES, never raw colours:
 * a theme maps roles to values; the adapter never hardcodes one.
 */
export const THEME_TOKENS = [
  'surface', 'surface-raised', 'text-primary', 'text-muted',
  'stroke-fact', 'stroke-inference', 'stroke-hypothesis', 'stroke-fog',
  'focus-ring', 'selection',
] as const;

export type ThemeToken = (typeof THEME_TOKENS)[number];
export type Theme = Record<ThemeToken, string>;

/**
 * Theme-independence check: the TEXT outline + mode prefixes carry the
 * information, so ANY theme (even monochrome) renders the view honestly.
 * Verifies a theme defines every required token — no partial themes.
 */
export function verifyTheme(t: Partial<Theme>): { ok: boolean; missing: string[] } {
  const missing = THEME_TOKENS.filter((k) => !(k in t) || typeof t[k] !== 'string' || t[k] === '');
  return { ok: missing.length === 0, missing };
}
