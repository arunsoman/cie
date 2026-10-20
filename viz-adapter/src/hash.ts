/**
 * The hash split (CIE-VIZ-MULTI PRD Part I §3) — Node-side recomputation.
 *
 *   semanticHash : sha256 over {elements, edges, groups, gaps, footer,
 *                 scope} — the CACHE key; hidden[], layoutHint,
 *                 preferences and the hashes themselves are OUTSIDE.
 *   renderHash   : sha256 over {semanticHash, layoutHint, preferences,
 *                 engineId} — the RENDER-ARTIFACT key; adapters pass
 *                 their own engine id (VIZ-03), the producer's stored
 *                 value is the engine-neutral baseline (VIZ-10).
 *
 * Canonicalisation mirrors the Rust producer exactly: stable stringify
 * with recursively sorted keys, integral floats as integers.
 */

import { createHash } from 'node:crypto';
import type { ViewSpec } from './types';

/** Stable stringify: object keys sorted at every depth (arrays in order),
 *  integral floats rendered the JS way (they already are — this pass pins
 *  the contract for non-Node embedders). */
export function stableStringify(value: unknown): string {
  const walk = (v: unknown): unknown => {
    if (v === null || typeof v !== 'object') return v;
    if (Array.isArray(v)) return v.map(walk);
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(v as Record<string, unknown>).sort()) {
      out[k] = walk((v as Record<string, unknown>)[k]);
    }
    return out;
  };
  return JSON.stringify(walk(value));
}

function sha256Hex(text: string): string {
  return `sha256:${createHash('sha256').update(text).digest('hex')}`;
}

export function recomputeSemanticHash(spec: ViewSpec): string {
  const payload = {
    elements: spec.elements,
    edges: spec.edges,
    groups: spec.groups ?? null,
    footer: spec.footer,
    gaps: spec.gaps,
    scope: spec.scope,
  };
  return sha256Hex(stableStringify(payload));
}

export function recomputeRenderHash(spec: ViewSpec, engineId: string): string {
  const payload = {
    semanticHash: recomputeSemanticHash(spec),
    layoutHint: spec.layoutHint ?? null,
    preferences: spec.preferences ?? null,
    engineId,
  };
  return sha256Hex(stableStringify(payload));
}

/** The producer's engine-neutral baseline (engineId ""). */
export function verifySemanticHash(spec: ViewSpec): boolean {
  return spec.semanticHash === recomputeSemanticHash(spec);
}

export function verifyRenderHash(spec: ViewSpec, engineId: string): boolean {
  return spec.renderHash === recomputeRenderHash(spec, engineId);
}
