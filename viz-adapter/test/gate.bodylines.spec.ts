/**
 * RFC-0024 patch 3 — the two additive gate checks
 * (gate.bodylines.spec.ts). `fields.bodyLines[]` narration is
 * evidence-gated (every line carries its own EvidenceRef); `fields.
 * alsoEvidence[]` second locations must be well-formed (paired popups
 * never draw a blank column). Codes are shared with the Rust mirror.
 */

import { describe, expect, it } from 'vitest';
import { gate, gateFields, VizError } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

function clone(spec: ViewSpec): ViewSpec {
  return JSON.parse(JSON.stringify(spec)) as ViewSpec;
}

const GOOD_REF = { file: 'svc/PaymentService.java', start: 120, end: 180 };

function elWithFields(id: string, fields: Record<string, unknown>) {
  // A minimal fully-attested Fact element from the real fixture, fields swapped.
  const base = JSON.parse(JSON.stringify((architecture as unknown as ViewSpec).elements[0])) as Record<string, unknown>;
  base.id = id;
  base.fields = fields;
  return base;
}

describe('gateFields — RFC-0024 patch 3 checks', () => {
  it('accepts evidence-gated bodyLines (component-board narration)', () => {
    expect(() =>
      gateFields({
        id: 'board-1',
        fields: {
          bodyLines: [
            { text: 'holds the ledger', line: 12, evidence: GOOD_REF },
            { text: 'writes on authorize', line: 40, evidence: { file: 'svc/Ledger.java', start: 8, end: 9 } },
          ],
        },
      }),
    ).not.toThrow();
  });

  it('throws BODY_LINE_WITHOUT_EVIDENCE for a line without evidence', () => {
    try {
      gateFields({
        id: 'board-2',
        fields: { bodyLines: [{ text: 'no evidence attached', line: 3 }] },
      });
      expect.unreachable();
    } catch (e) {
      expect((e as VizError).code).toBe('BODY_LINE_WITHOUT_EVIDENCE');
      expect((e as VizError).elementId).toBe('board-2');
    }
  });

  it('throws BODY_LINE_WITHOUT_EVIDENCE for a line with a MALFORMED ref (no file)', () => {
    expect(() =>
      gateFields({ id: 'board-3', fields: { bodyLines: [{ text: 'x', line: 1, evidence: { file: '', start: 0, end: 1 } }] } }),
    ).toThrowError(/BODY_LINE_WITHOUT_EVIDENCE/);
  });

  it('throws BODY_LINE_WITHOUT_EVIDENCE for a line with an inverted byte range', () => {
    expect(() =>
      gateFields({ id: 'board-4', fields: { bodyLines: [{ text: 'x', line: 1, evidence: { file: 'a.py', start: 9, end: 2 } }] } }),
    ).toThrowError(/BODY_LINE_WITHOUT_EVIDENCE/);
  });

  it('throws BODY_LINE_WITHOUT_EVIDENCE when bodyLines is not even an array', () => {
    expect(() => gateFields({ id: 'board-5', fields: { bodyLines: 'holds the ledger' } })).toThrowError(
      /BODY_LINE_WITHOUT_EVIDENCE/,
    );
  });

  it('accepts well-formed alsoEvidence (paired popups ride on it)', () => {
    expect(() =>
      gateFields({
        id: 'drift-1',
        fields: { alsoEvidence: [GOOD_REF, { file: 'docs/adr-007.md', start: 210, end: 260 }] },
      }),
    ).not.toThrow();
  });

  it('throws ALSO_EVIDENCE_MALFORMED for a ref without a file', () => {
    try {
      gateFields({ id: 'drift-2', fields: { alsoEvidence: [{ file: '', start: 1, end: 2 }] } });
      expect.unreachable();
    } catch (e) {
      expect((e as VizError).code).toBe('ALSO_EVIDENCE_MALFORMED');
      expect((e as VizError).elementId).toBe('drift-2');
    }
  });

  it('throws ALSO_EVIDENCE_MALFORMED when alsoEvidence is not an array', () => {
    expect(() => gateFields({ id: 'drift-3', fields: { alsoEvidence: GOOD_REF } })).toThrowError(
      /ALSO_EVIDENCE_MALFORMED/,
    );
  });

  it('elements WITHOUT bodyLines/alsoEvidence are untouched (additive only)', () => {
    expect(() => gateFields({ id: 'plain-1', fields: { anyBag: 'value' } })).not.toThrow();
  });
});

describe('gate(spec) runs the patch-3 checks — no bypass', () => {
  it('rejects a spec whose board card carries unevidenced body lines', () => {
    const s = clone(architecture as unknown as ViewSpec);
    s.elements.push(elWithFields('board-bad', { bodyLines: [{ text: 'fabricated narration' }] }) as never);
    expect(() => gate(s)).toThrowError(/BODY_LINE_WITHOUT_EVIDENCE/);
  });

  it('rejects a spec with a malformed alsoEvidence ref', () => {
    const s = clone(architecture as unknown as ViewSpec);
    s.elements.push(elWithFields('drift-bad', { alsoEvidence: [{ start: 5 }] }) as never);
    expect(() => gate(s)).toThrowError(/ALSO_EVIDENCE_MALFORMED/);
  });

  it('accepts a spec with fully-evidenced bodyLines + alsoEvidence', () => {
    const s = clone(architecture as unknown as ViewSpec);
    s.elements.push(
      elWithFields('board-good', {
        bodyLines: [{ text: 'holds the ledger', line: 12, evidence: GOOD_REF }],
        alsoEvidence: [{ file: 'docs/adr-007.md', start: 210, end: 260 }],
      }) as never,
    );
    expect(() => gate(s)).not.toThrow();
  });
});
