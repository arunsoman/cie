/**
 * The epistemic gate (PRD Part II §5) — the ONLY honesty enforcement
 * point. Every adapter's `adapt` calls it; there is no render path that
 * bypasses it. The Rust producer runs the identical clause set with the
 * same stable codes.
 */

import type { ViewSpec, EvidenceRef } from './types';
import { VizError } from './errors';

export function recordedEvidenceValid(value:unknown):boolean {
  if(!value||typeof value!=='object')return false;const e=value as Record<string,unknown>;
  const text=(v:unknown)=>typeof v==='string'&&v.length>0;
  if(!text(e.repository)||!text(e.revision))return false;
  return e.kind==='stored-fact'?!!(text(e.artifact)&&text(e.recordId)):e.kind==='aggregate'?!!text(e.descriptor):e.kind==='runtime'?!!(text(e.runId)&&text(e.sampleId)):false;
}
export function evidenceValid(value:EvidenceRef|undefined):boolean{return !!value&&(!!value.file||recordedEvidenceValid(value));}

/** Salience-bounded cap of 2 000 elements per view (PRD VIZ-I-06). */
export const ELEMENT_CAP = 2_000;

export function gate(spec: ViewSpec): void {
  const seen = new Set<string>();
  for (const el of spec.elements) {
    if (seen.has(el.id)) {
      throw new VizError('DUPLICATE_ELEMENT_ID', el.id);
    }
    seen.add(el.id);

    if (!evidenceValid(el.evidence)) {
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
    // RFC-0024 patch 3: evidence-gated bodyLines + well-formed
    // alsoEvidence (the two additive checks run HERE — no bypass).
    gateFields(el);
  }

  if (spec.elements.length > ELEMENT_CAP) {
    throw new VizError('ELEMENT_CAP_EXCEEDED', `* (${spec.elements.length} elements)`);
  }

  for (const e of spec.edges) {
    if (!evidenceValid(e.evidence)) {
      throw new VizError('EDGE_WITHOUT_EVIDENCE', e.id);
    }
  }
}

/**
 * RFC-0024 patch 3 — the two additive gate checks (ux-spec §13.1), run by
 * every adapter alongside `gate`. Codes are stable across the Rust mirror
 * (cie-viz/src/gate.rs).
 *
 * 1. `fields.bodyLines[]` — component-board narration is evidence-gated:
 *    every line must carry a well-formed EvidenceRef (file + sane range)
 *    of its own. A narration line without evidence is a fabricated fact —
 *    rejected, never rendered.
 * 2. `fields.alsoEvidence[]` — paired-popup second locations: every ref
 *    must be well-formed. A paired popup is only drawn when BOTH refs
 *    exist (ux-spec §2.2); a malformed ref would draw a blank column.
 */

function isWellFormedRef(r: unknown): r is { file: string; start: number; end: number } {
  if (typeof r !== 'object' || r === null) return false;
  const ref = r as Record<string, unknown>;
  return (
    typeof ref.file === 'string' &&
    ref.file.length > 0 &&
    typeof ref.start === 'number' &&
    Number.isFinite(ref.start) &&
    typeof ref.end === 'number' &&
    Number.isFinite(ref.end) &&
    (ref.start as number) >= 0 &&
    (ref.end as number) >= (ref.start as number)
  );
}

export function gateFields(el: {
  id: string;
  fields?: Record<string, unknown>;
}): void {
  const bag = el.fields ?? {};
  const bodyLines = bag.bodyLines;
  if (bodyLines !== undefined) {
    if (!Array.isArray(bodyLines)) {
      throw new VizError('BODY_LINE_WITHOUT_EVIDENCE', el.id);
    }
    for (const line of bodyLines) {
      const rec = (typeof line === 'object' && line !== null ? line : {}) as Record<string, unknown>;
      if (!isWellFormedRef(rec.evidence)) {
        throw new VizError('BODY_LINE_WITHOUT_EVIDENCE', el.id);
      }
    }
  }

  const also = bag.alsoEvidence;
  if (also !== undefined) {
    if (!Array.isArray(also)) {
      throw new VizError('ALSO_EVIDENCE_MALFORMED', el.id);
    }
    for (const ref of also) {
      if (!isWellFormedRef(ref)) {
        throw new VizError('ALSO_EVIDENCE_MALFORMED', el.id);
      }
    }
  }
}
