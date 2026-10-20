/**
 * UI disclosure payloads (PRD Part VII VIZ-28/29/30) — every override is
 * surfaced as a NAMED decision with a reason. Pure functions; the UI
 * renders these payloads verbatim.
 */

import type { EncodingFidelity, HiddenCandidate, LayoutOutcome } from './types';
import type { NegotiationDisclosure } from './negotiator';

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

/**
 * RFC-0024 patch 2 (G1) — the disclosure-ledger rows. The grouped-out box
 * (`…and N more`) is never a scope: it opens a popup listing ITS rows, one
 * per HiddenCandidate, each row acting as its own element. `label`/`kind`
 * (patch 2) become the row text; absence falls back to `id`/`unknown` and
 * the row DISCLOSES the fallback (honest fallback, never a blank row).
 */
export interface HiddenRow {
  id: string;
  label: string;
  kind: string;
  reason: string;
  salienceWeight: number;
  /** true when label/kind fell back (no patch-2 data from the producer). */
  fallback: boolean;
}

export function hiddenRows(hidden: HiddenCandidate[]): HiddenRow[] {
  return hidden.map((h) => ({
    id: h.id,
    label: h.label ?? h.id,
    kind: h.kind ?? 'unknown',
    reason: h.reason,
    salienceWeight: h.salienceWeight,
    fallback: h.label === undefined || h.kind === undefined,
  }));
}
