/**
 * Provenance audit (PRD Part II adapter `audit()` + Part VII VIZ-25) —
 * post-gate invariants, runnable in CI for every fixture and every
 * catalog chart.
 *
 *   H-01  edges reference only spec elements
 *   H-02  edge evidence grounds the relation (hierarchy edges cite the
 *         child's own span; other edges cite an endpoint's file)
 *   H-03  displayMode is the only Fact channel
 *   H-04  every Hypothesis has a counter-argument
 *   H-05  every gap is reachable from the footer caption
 */

import type { ViewSpec } from './types';
import {recordedEvidenceValid} from './gate';

export interface AuditFinding {
  rule: string;
  detail: string;
}

const HIERARCHY_KINDS = new Set([
  'package_containment',
  'cluster_cohesion',
  'fca_subsumption',
  'call_ownership',
  'contains',
]);

export function audit(spec: ViewSpec): AuditFinding[] {
  const out: AuditFinding[] = [];
  const byId = new Map(spec.elements.map((e) => [e.id, e]));

  for (const e of spec.edges) {
    if (!byId.has(e.source) || !byId.has(e.target)) {
      out.push({ rule: 'VIZ-H-01', detail: `edge ${e.id} references an element not in the spec` });
    }
  }

  for (const e of spec.edges) {
    if(recordedEvidenceValid(e.evidence))continue;
    if (!e.evidence || !e.evidence.file) {
      out.push({ rule: 'VIZ-H-02', detail: `edge ${e.id} carries no evidence` });
      continue;
    }
    if (HIERARCHY_KINDS.has(e.edgeKind)) {
      const target = byId.get(e.target);
      if (target && target.evidence.file !== e.evidence.file) {
        out.push({
          rule: 'VIZ-H-02',
          detail: `hierarchy edge ${e.id} must cite the child's span`,
        });
      }
    } else {
      const src = byId.get(e.source);
      const tgt = byId.get(e.target);
      const ok =
        (src && src.evidence.file === e.evidence.file) ||
        (tgt && tgt.evidence.file === e.evidence.file);
      if (!ok) {
        out.push({
          rule: 'VIZ-H-02',
          detail: `edge ${e.id} evidence ${e.evidence.file} grounds neither endpoint`,
        });
      }
    }
  }

  for (const el of spec.elements) {
    if (el.displayMode === 'Fact' && el.claimId !== undefined && el.claimId.length === 0) {
      out.push({ rule: 'VIZ-H-03', detail: `element ${el.id} Fact with empty claim` });
    }
  }

  for (const el of spec.elements) {
    if (el.displayMode === 'Hypothesis' && !el.counterArgument) {
      out.push({ rule: 'VIZ-H-04', detail: `hypothesis ${el.id} without counter-argument` });
    }
  }

  for (const g of spec.gaps) {
    if (!spec.footer.caption.includes(g.id)) {
      out.push({ rule: 'VIZ-H-05', detail: `gap ${g.id} not reachable from the footer` });
    }
  }

  return out;
}
