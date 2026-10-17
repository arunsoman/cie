/**
 * VIZ-10 — CIE output contains no engine id (schema.no-engine.spec.ts):
 * the wire form of a produced spec must not name any engine, chart type
 * or renderer. Mirrors the Rust-side test in cie-viz/src/produce.rs.
 */

import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import type { ViewSpec } from '../src/index.js';
import cieSample from '../fixtures/specs/cie-sample.json';

const spec = cieSample as unknown as ViewSpec;

describe('VIZ-10: no engine id anywhere in the CIE output', () => {
  it('the REAL produced fixture contains no engine/chart key', () => {
    const wire = JSON.stringify(spec);
    for (const banned of ['engineId', 'engine', 'chartType', 'ViewKind', 'renderer']) {
      expect(wire.includes(banned)).toBe(false);
    }
  });

  it('every wire key is a ViewSpec contract key (both ends)', () => {
    const allowed = new Set([
      'elements', 'edges', 'groups', 'footer', 'gaps', 'hidden', 'scope',
      'layoutHint', 'preferences', 'semanticHash', 'renderHash',
    ]);
    for (const k of Object.keys(spec)) {
      expect(allowed.has(k)).toBe(true);
    }
    // And the two hashes are the PRD §3 split — nothing else.
    expect(spec.semanticHash).toMatch(/^sha256:/);
    expect(spec.renderHash).toMatch(/^sha256:/);
  });

  it('the ViewSpec TYPE declares no engine field', () => {
    const typesSrc = readFileSync(new URL('../src/types.ts', import.meta.url), 'utf8');
    const block = typesSrc.slice(
      typesSrc.indexOf('export interface ViewSpec'),
      typesSrc.indexOf('export interface', typesSrc.indexOf('export interface ViewSpec') + 1),
    );
    // Strip comments — the law is about FIELDS, not prose.
    const code = block.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
    expect(code).not.toMatch(/engine|chart|ViewKind|renderer/i);
  });
});
