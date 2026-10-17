/**
 * VIZ-01 — gate() rejects unattested elements before ANY engine runs
 * (adapter.gate.spec.ts). Codes are stable across the Rust/TS bridge.
 */

import { describe, expect, it } from 'vitest';
import { gate, VizError, ELEMENT_CAP } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';
import cieSample from '../fixtures/specs/cie-sample.json';

const fixtures: Array<[string, ViewSpec]> = [
  ['architecture.json', architecture as unknown as ViewSpec],
  ['cie-sample.json (REAL `cie viz` output)', cieSample as unknown as ViewSpec],
];

function clone(spec: ViewSpec): ViewSpec {
  return JSON.parse(JSON.stringify(spec)) as ViewSpec;
}

describe('gate(spec) — VIZ-01', () => {
  for (const [name, spec] of fixtures) {
    it(`accepts fully-attested ${name}`, () => {
      expect(() => gate(spec)).not.toThrow();
    });
  }

  it('throws ELEMENT_WITHOUT_EVIDENCE for an unevidenced element', () => {
    const s = clone(architecture as unknown as ViewSpec);
    (s.elements[0] as { evidence: unknown }).evidence = undefined;
    try {
      gate(s);
      expect.unreachable();
    } catch (e) {
      expect((e as VizError).code).toBe('ELEMENT_WITHOUT_EVIDENCE');
      expect((e as VizError).elementId).toBe('checkout');
    }
  });

  it('VIZ-A-04: NON_FACT_WITHOUT_CLAIM', () => {
    const s = clone(architecture as unknown as ViewSpec);
    delete s.elements.find((e) => e.id === 'authorize')!.claimId;
    expect(() => gate(s)).toThrowError(/NON_FACT_WITHOUT_CLAIM/);
  });

  it('VIZ-A-05: HYPOTHESIS_WITHOUT_COUNTER', () => {
    const s = clone(architecture as unknown as ViewSpec);
    delete s.elements.find((e) => e.id === 'topup')!.counterArgument;
    expect(() => gate(s)).toThrowError(/HYPOTHESIS_WITHOUT_COUNTER/);
  });

  it('VIZ-A-06: FOG_WITHOUT_REASON', () => {
    const s = clone(architecture as unknown as ViewSpec);
    delete s.elements.find((e) => e.id === 'legacy')!.fogReason;
    expect(() => gate(s)).toThrowError(/FOG_WITHOUT_REASON/);
  });

  it('throws EDGE_WITHOUT_EVIDENCE for an unevidenced edge', () => {
    const s = clone(architecture as unknown as ViewSpec);
    (s.edges[0] as { evidence: unknown }).evidence = undefined;
    expect(() => gate(s)).toThrowError(/EDGE_WITHOUT_EVIDENCE/);
  });

  it(`throws ELEMENT_CAP_EXCEEDED above ${ELEMENT_CAP}`, () => {
    const s = clone(architecture as unknown as ViewSpec);
    s.elements = Array.from({ length: ELEMENT_CAP + 1 }, (_, i) => ({
      ...s.elements[0]!,
      id: `bulk-${i}`,
    }));
    expect(() => gate(s)).toThrowError(/ELEMENT_CAP_EXCEEDED/);
  });

  it('every adapter runs the same gate (no bypass path)', async () => {
    const { defaultRegistry } = await import('../src/index.js');
    const reg = defaultRegistry();
    const s = clone(architecture as unknown as ViewSpec);
    (s.elements[0] as { evidence: unknown }).evidence = undefined;
    for (const adapter of reg.values()) {
      expect(() => adapter.adapt(s)).toThrowError(VizError);
    }
  });
});
