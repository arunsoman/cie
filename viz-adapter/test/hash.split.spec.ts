/**
 * VIZ-02 — semanticHash is unchanged across layout AND engine swaps
 * (hash.split.spec.ts). Also: the hidden[] ledger is outside the domain
 * (PRD §3's enumerated fields), preferences never touch it.
 */

import { describe, expect, it } from 'vitest';
import { recomputeSemanticHash, verifySemanticHash } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';
import cieSample from '../fixtures/specs/cie-sample.json';

const specs: Array<[string, ViewSpec]> = [
  ['architecture.json', architecture as unknown as ViewSpec],
  ['cie-sample.json (REAL `cie viz` output)', cieSample as unknown as ViewSpec],
];

describe('VIZ-02: semanticHash survives every swap', () => {
  for (const [name, spec] of specs) {
    it(`${name}: shipped seal verifies (VIZ-A-08 recompute)`, () => {
      expect(spec.semanticHash).toMatch(/^sha256:[0-9a-f]{64}$/);
      expect(verifySemanticHash(spec)).toBe(true);
      expect(recomputeSemanticHash(spec)).toBe(spec.semanticHash);
    });

    it(`${name}: layout swap`, () => {
      const s = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      s.layoutHint = { strategy: 'fruchterman' };
      expect(recomputeSemanticHash(s)).toBe(spec.semanticHash);
    });

    it(`${name}: engine swap (renderHash field ignored — it is not semantic)`, () => {
      const s = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      s.renderHash = 'sha256:whatever-the-adapter-wrote';
      expect(recomputeSemanticHash(s)).toBe(spec.semanticHash);
    });

    it(`${name}: preferences variant`, () => {
      const s = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      s.preferences = { fisheye: { d: 3 }, renderer: 'svg' };
      expect(recomputeSemanticHash(s)).toBe(spec.semanticHash);
    });

    it(`${name}: hidden[] ledger mutation (PRD §3: hidden is outside)`, () => {
      const s = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      s.hidden.push({ id: 'cX', reason: 'test', salienceWeight: 0.1 });
      expect(recomputeSemanticHash(s)).toBe(spec.semanticHash);
    });

    it(`${name}: semantic tamper breaks the seal`, () => {
      const s = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      s.elements[0]!.label += ' (edited)';
      expect(verifySemanticHash(s)).toBe(false);
    });
  }
});
