/**
 * The epistemic gate (PRD Part II §5) — the ONLY honesty enforcement
 * point. Every adapter's `adapt` calls it; there is no render path that
 * bypasses it. The Rust producer runs the identical clause set with the
 * same stable codes.
 */

import type { ViewSpec } from './types.js';
import { VizError } from './errors.js';

/** Salience-bounded cap of 2 000 elements per view (PRD VIZ-I-06). */
export const ELEMENT_CAP = 2_000;

export function gate(spec: ViewSpec): void {
  const seen = new Set<string>();
  for (const el of spec.elements) {
    if (seen.has(el.id)) {
      throw new VizError('DUPLICATE_ELEMENT_ID', el.id);
    }
    seen.add(el.id);

    if (!el.evidence || !el.evidence.file) {
      throw new VizError('ELEMENT_WITHOUT_EVIDENCE', el.id);
    }
    if (el.displayMode !== 'Fact' && !el.claimId) {
      throw new VizError('NON_FACT_WITHOUT_CLAIM', el.id);
    }
    if (el.displayMode === 'Hypothesis' && !el.counterArgument) {
      throw new VizError('HYPOTHESIS_WITHOUT_COUNTER', el.id);
    }
    if (el.displayMode === 'Fog' && !el.fogReason) {
      throw new VizError('FOG_WITHOUT_REASON', el.id);
    }
  }

  if (spec.elements.length > ELEMENT_CAP) {
    throw new VizError('ELEMENT_CAP_EXCEEDED', `* (${spec.elements.length} elements)`);
  }

  for (const e of spec.edges) {
    if (!e.evidence || !e.evidence.file) {
      throw new VizError('EDGE_WITHOUT_EVIDENCE', e.id);
    }
  }
}
