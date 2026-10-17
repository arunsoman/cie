/**
 * Registry + negotiation (PRD Part I §2, Part II §6).
 *
 * Engine decision ladder (hard → soft):
 *   1. capability gate      — non-overridable
 *   2. user preference      — honoured when eligible
 *   3. graph size limits    — hard
 *   4. deterministic order  — g6 → jointjs → reactflow → mermaid
 *
 * `chooseEngine` is a PURE function: same (request, registry, graphSize)
 * → same adapter, always (VIZ-04). Every deviation from the user's
 * preference is DISCLOSED (VIZ-06/28) — silent degradation is forbidden.
 */

import type {
  CapabilitySet,
  EngineAdapter,
  EngineId,
  EngineRequest,
  LayoutHint,
  RenderOptions,
  RenderHandle,
  ViewSpec,
} from './types.js';
import { NegotiationError, VizError } from './errors.js';

/** PRD §6's deterministic tie-break order. */
export const ENGINE_ORDER: EngineId[] = ['g6', 'jointjs', 'reactflow', 'mermaid'];

/**
 * Capability gate + size limits + layout availability, exactly as the PRD
 * specifies. An adapter is ELIGIBLE when every required capability is
 * owned, both graph sizes fit, and the requested layout is either
 * supported natively or the engine can register custom layouts.
 */
export function isEligible(adapter: EngineAdapter, request: EngineRequest): boolean {
  for (const [cap, needed] of Object.entries(request.required)) {
    if (needed && !adapter.capabilities[cap as keyof CapabilitySet]) return false;
  }
  if (request.graphSize.nodes > adapter.limits.maxNodes) return false;
  if (request.graphSize.edges > adapter.limits.maxEdges) return false;
  if (
    request.layoutHint &&
    !adapter.limits.supportedLayouts.includes(request.layoutHint.strategy) &&
    !adapter.capabilities.customLayouts
  ) {
    return false;
  }
  return true;
}

export function chooseEngine(
  request: EngineRequest,
  registry: Map<EngineId, EngineAdapter>,
): EngineAdapter {
  const candidates = [...registry.values()].filter((a) => isEligible(a, request));
  if (candidates.length === 0) {
    throw new NegotiationError(
      request.preferred
        ? `no eligible engine for requirement ${JSON.stringify(request.required)} (preferred ${request.preferred} ineligible)`
        : `no eligible engine for requirement ${JSON.stringify(request.required)}`,
      request,
    );
  }
  if (request.preferred && candidates.some((a) => a.id === request.preferred)) {
    return candidates.find((a) => a.id === request.preferred)!;
  }
  return candidates.sort((a, b) => ENGINE_ORDER.indexOf(a.id) - ENGINE_ORDER.indexOf(b.id))[0]!;
}

/** Why wasn't this engine chosen? — the disclosure payload (VIZ-06/28). */
export interface EngineDecision {
  engineId: EngineId;
  eligible: boolean;
  reasons: string[];
}

export function decideAll(request: EngineRequest, registry: Map<EngineId, EngineAdapter>): EngineDecision[] {
  return ENGINE_ORDER.filter((id) => registry.has(id)).map((id) => {
    const a = registry.get(id)!;
    const reasons: string[] = [];
    for (const [cap, needed] of Object.entries(request.required)) {
      if (needed && !a.capabilities[cap as keyof CapabilitySet]) {
        reasons.push(`capability ${cap} unavailable`);
      }
    }
    if (request.graphSize.nodes > a.limits.maxNodes) {
      reasons.push(`${request.graphSize.nodes} nodes exceed maxNodes ${a.limits.maxNodes}`);
    }
    if (request.graphSize.edges > a.limits.maxEdges) {
      reasons.push(`${request.graphSize.edges} edges exceed maxEdges ${a.limits.maxEdges}`);
    }
    if (
      request.layoutHint &&
      !a.limits.supportedLayouts.includes(request.layoutHint.strategy) &&
      !a.capabilities.customLayouts
    ) {
      reasons.push(`layout ${request.layoutHint.strategy} unsupported and customLayouts unavailable`);
    }
    return { engineId: id, eligible: reasons.length === 0, reasons };
  });
}

/**
 * Named UI decision for the whole negotiation (VIZ-28): every override is
 * disclosed as `Preferred: X ✗ (reason) · Rendered: Y`.
 */
export interface NegotiationDisclosure {
  rendered: EngineId;
  preferred: EngineId | undefined;
  preferredEligible: boolean;
  preferredBlockReasons: string[];
  headline: string;
  decisions: EngineDecision[];
}

export function disclose(
  request: EngineRequest,
  chosen: EngineAdapter,
  registry: Map<EngineId, EngineAdapter>,
): NegotiationDisclosure {
  const decisions = decideAll(request, registry);
  const preferredDecision = decisions.find((d) => d.engineId === request.preferred);
  const preferredEligible =
    request.preferred === undefined
      ? false
      : (preferredDecision?.eligible ?? false);
  const headline =
    request.preferred && request.preferred !== chosen.id
      ? `Preferred: ${request.preferred} ✗ (${preferredDecision?.reasons.join('; ') ?? 'not registered'}) · Rendered: ${chosen.id}`
      : `Rendered: ${chosen.id}`;
  return {
    rendered: chosen.id,
    preferred: request.preferred,
    preferredEligible,
    preferredBlockReasons: preferredDecision?.reasons ?? [],
    headline,
    decisions,
  };
}

/**
 * VIZ-09: there is NO render path that bypasses the negotiator. The
 * pipeline orchestrates: choose → gate → adapt — an adapter is never
 * invoked without a negotiator decision, and a negotiation failure is a
 * typed error with zero adapter invocations.
 */
export interface HeadlessResult {
  engineId: EngineId;
  data: unknown;
  options: unknown;
  disclosure: NegotiationDisclosure;
  renderHash: string;
  spec: ViewSpec;
}

export function renderHeadless(
  spec: ViewSpec,
  request: Omit<EngineRequest, 'graphSize' | 'layoutHint'> & {
    graphSize?: { nodes: number; edges: number };
    layoutHint?: LayoutHint;
  },
  registry: Map<EngineId, EngineAdapter>,
  options: RenderOptions = {},
): HeadlessResult {
  const full: EngineRequest = {
    ...request,
    graphSize: request.graphSize ?? { nodes: spec.elements.length, edges: spec.edges.length },
    layoutHint: request.layoutHint ?? spec.layoutHint,
  };
  const adapter = chooseEngine(full, registry); // typed NegotiationError when none
  const disclosure = disclose(full, adapter, registry);
  adapter.gate(spec); // single honesty enforcement point
  const data = adapter.adapt(spec);
  const opts = adapter.configure(spec, options);
  return {
    engineId: adapter.id,
    data,
    options: opts,
    disclosure,
    renderHash: renderHashOf(spec, adapter.id),
    spec,
  };
}

/** renderHash binding for a concrete engine (PRD §3; Rust-mirrored). */
export function renderHashOf(spec: ViewSpec, engineId: string): string {
  // Imported lazily to keep this module tree-shakeable in browser builds.
  const { recomputeRenderHash } = hashModule;
  return recomputeRenderHash(spec, engineId);
}

import * as hashModule from './hash.js';
