/**
 * VIZ-03 — renderHash changes across layout swaps (hash.render.spec.ts):
 * the render-artifact key binds {semanticHash, layoutHint, preferences,
 * engineId}; the producer's stored value is the engine-neutral baseline.
 */

import { describe, expect, it } from 'vitest';
import { recomputeRenderHash, verifyRenderHash } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import cieSample from '../fixtures/specs/cie-sample.json';

const spec = cieSample as unknown as ViewSpec;

describe('VIZ-03: renderHash is the artifact key', () => {
  it('the shipped producer seal is the engine-neutral baseline', () => {
    expect(verifyRenderHash(spec, '')).toBe(true);
    expect(verifyRenderHash(spec, 'g6')).toBe(false);
  });

  it('layout swap → new render artifact, same semantic content', () => {
    const swapped = JSON.parse(JSON.stringify(spec)) as ViewSpec;
    swapped.layoutHint = { strategy: 'radial' };
    expect(recomputeRenderHash(swapped, 'g6')).not.toBe(recomputeRenderHash(spec, 'g6'));
    expect(swapped.elements.map((e) => e.id)).toEqual(spec.elements.map((e) => e.id));
  });

  it('engine swap → different artifact for the same spec', () => {
    const ids = ['g6', 'jointjs', 'reactflow', 'mermaid'];
    const hashes = new Set(ids.map((id) => recomputeRenderHash(spec, id)));
    expect(hashes.size).toBe(ids.length);
  });

  it('preferences change → different artifact', () => {
    const withFisheye = JSON.parse(JSON.stringify(spec)) as ViewSpec;
    withFisheye.preferences = { fisheye: { d: 2.5 } };
    expect(recomputeRenderHash(withFisheye, 'g6')).not.toBe(recomputeRenderHash(spec, 'g6'));
  });

  it('deterministic: same inputs → same digest', () => {
    expect(recomputeRenderHash(spec, 'g6')).toBe(recomputeRenderHash(spec, 'g6'));
  });
});
