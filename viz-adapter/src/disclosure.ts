/**
 * UI disclosure payloads (PRD Part VII VIZ-28/29/30) — every override is
 * surfaced as a NAMED decision with a reason. Pure functions; the UI
 * renders these payloads verbatim.
 */

import type { EncodingFidelity, LayoutOutcome } from './types.js';
import type { NegotiationDisclosure } from './negotiator.js';

/** VIZ-28 — the engine banner. */
export interface EngineBanner {
  kind: 'engine-decision';
  headline: string;
  chips: Array<{ engineId: string; eligible: boolean; reason?: string }>;
}

export function engineBanner(d: NegotiationDisclosure): EngineBanner {
  return {
    kind: 'engine-decision',
    headline: d.headline,
    chips: d.decisions.map((x) => ({
      engineId: x.engineId,
      eligible: x.eligible,
      reason: x.eligible ? undefined : x.reasons.join('; '),
    })),
  };
}

/** VIZ-29 — the layout chip: Exact / Substituted / Unavailable. */
export interface LayoutChip {
  kind: 'layout-chip';
  text: string;
  fidelity: LayoutOutcome['fidelity'];
}

export function layoutChip(l: LayoutOutcome): LayoutChip {
  if (l.requested === undefined) {
    return { kind: 'layout-chip', text: `Layout: ${l.actual} (auto)`, fidelity: l.fidelity };
  }
  if (l.fidelity === 'Exact') {
    return { kind: 'layout-chip', text: `Layout: ${l.actual}`, fidelity: 'Exact' };
  }
  if (l.fidelity === 'Substituted') {
    return {
      kind: 'layout-chip',
      text: `Layout: ${l.actual} (substituted from ${l.requested})`,
      fidelity: 'Substituted',
    };
  }
  return {
    kind: 'layout-chip',
    text: `Layout: ${l.requested} unavailable — rendered as ${l.actual}`,
    fidelity: 'Unavailable',
  };
}

/** VIZ-30 — the fidelity banner; shown iff NOT Full (never silent). */
export interface FidelityBanner {
  kind: 'fidelity-banner';
  visible: boolean;
  text?: string;
  authoritative: boolean;
}

export function fidelityBanner(f: EncodingFidelity): FidelityBanner {
  if (f === 'Full') {
    return { kind: 'fidelity-banner', visible: false, authoritative: false };
  }
  return {
    kind: 'fidelity-banner',
    visible: true,
    text:
      f === 'Degraded'
        ? 'Epistemic encoding degraded on this engine — the text outline is authoritative.'
        : 'Epistemic encoding unsupported on this engine — the text outline is authoritative.',
    authoritative: true,
  };
}
