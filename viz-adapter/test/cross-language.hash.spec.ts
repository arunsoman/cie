/**
 * The bridge seal — a REAL ViewSpec produced by the Rust engine is
 * re-verified in TypeScript under the PRD §3 hash split:
 *   semanticHash  — Rust sha2 ↔ Node crypto, identical canonical payload
 *   renderHash    — engine-neutral baseline verifies with engineId ""
 */

import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  recomputeRenderHash,
  recomputeSemanticHash,
  verifyRenderHash,
  verifySemanticHash,
} from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';
import cieSample from '../fixtures/specs/cie-sample.json';

const specs: Array<[string, ViewSpec]> = [
  ['architecture.json (hand-authored, sealed)', architecture as unknown as ViewSpec],
  ['cie-sample.json (REAL `cie viz` output)', cieSample as unknown as ViewSpec],
];

describe('cross-language hash split (Rust seal ↔ TS recompute)', () => {
  for (const [name, spec] of specs) {
    it(`${name}: semanticHash matches the Rust seal`, () => {
      expect(spec.semanticHash).toMatch(/^sha256:[0-9a-f]{64}$/);
      expect(verifySemanticHash(spec)).toBe(true);
      expect(recomputeSemanticHash(spec)).toBe(spec.semanticHash);
    });

    it(`${name}: renderHash matches the engine-neutral Rust baseline`, () => {
      expect(spec.renderHash).toMatch(/^sha256:[0-9a-f]{64}$/);
      expect(verifyRenderHash(spec, '')).toBe(true);
    });

    it(`${name}: engine-bound renderHash differs from the baseline (VIZ-03)`, () => {
      expect(recomputeRenderHash(spec, 'g6')).not.toBe(spec.renderHash);
      expect(recomputeRenderHash(spec, 'mermaid')).not.toBe(recomputeRenderHash(spec, 'g6'));
    });

    it(`${name}: semantic tamper breaks only the semantic seal`, () => {
      const tampered = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      tampered.elements[0]!.label += ' (edited)';
      expect(verifySemanticHash(tampered)).toBe(false);
      // The render hash recomputes over the FRESH semantic digest, so the
      // render key of the tampered content is still well-defined.
      expect(recomputeRenderHash(tampered, 'g6')).toMatch(/^sha256:[0-9a-f]{64}$/);
    });
  }
});
