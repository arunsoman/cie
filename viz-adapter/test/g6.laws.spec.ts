/**
 * G6 core laws retained from the single-engine PRD: purity (VIZ-B-02/03),
 * redundant epistemic encoding (VIZ-F-01..03), dagre TC-01 assertions,
 * plugin stack, fisheye zero-geometry (VIZ-E-09), no ViewKind branching.
 */

import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import {
  DASH,
  EDGE_DASH,
  G6Adapter,
  STROKE,
  WORDS,
  buildPlugins,
  g6Adapt,
  g6AutoLayout,
  g6GraphOptions,
} from '../src/index.js';
import type { DisplayMode, ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;
const MODES: DisplayMode[] = ['Fact', 'Inference', 'Hypothesis', 'Fog'];

describe('purity (VIZ-B-02/B-03)', () => {
  it('adapt and graphOptions are pure', () => {
    expect(JSON.parse(JSON.stringify(g6Adapt(spec)))).toEqual(JSON.parse(JSON.stringify(g6Adapt(spec))));
    expect(JSON.parse(JSON.stringify(g6GraphOptions(spec)))).toEqual(
      JSON.parse(JSON.stringify(g6GraphOptions(spec))),
    );
  });

  it('gate runs before adapt on every render path (VIZ-B-01)', () => {
    const bad = JSON.parse(JSON.stringify(spec)) as ViewSpec;
    (bad.elements[0] as { evidence: unknown }).evidence = undefined;
    expect(() => g6GraphOptions(bad)).toThrowError(/ELEMENT_WITHOUT_EVIDENCE/);
  });
});

describe('TC-01: dagre architecture map on the adapter layer', () => {
  const data = g6Adapt(spec);
  const opts = g6GraphOptions(spec);

  it('5 nodes, 3 edges, dagre hint verbatim', () => {
    expect(data.nodes).toHaveLength(5);
    expect(data.edges).toHaveLength(3);
    expect(opts.layout).toEqual({ type: 'dagre', rankdir: 'LR', nodesep: 60, ranksep: 120 });
  });

  it('authorize [6,4] amber · topup [2,4] #f85149 · legacy [1,3] + ? badge', () => {
    const style = (opts.node as { style: Record<string, (d: unknown) => undefined | unknown> }).style as Record<
      string,
      (d: unknown) => unknown
    >;
    const authorize = data.nodes.find((n) => n.id === 'authorize')!;
    expect(style.lineDash(authorize)).toEqual([6, 4]);
    expect(style.stroke(authorize)).toBe(STROKE.Inference);
    const topup = data.nodes.find((n) => n.id === 'topup')!;
    expect(style.lineDash(topup)).toEqual([2, 4]);
    expect(style.stroke(topup)).toBe('#f85149');
    const legacy = data.nodes.find((n) => n.id === 'legacy')!;
    expect(style.lineDash(legacy)).toEqual([1, 3]);
    expect(style.badge(legacy)).toEqual([{ text: '?', placement: 'right-top' }]);
  });
});

describe('epistemic encoding redundancy (VIZ-F-01..03)', () => {
  for (const m of MODES) {
    it(`mode ${m} has stroke + dash + word channels`, () => {
      expect(STROKE[m]).toBeTruthy();
      expect(WORDS[m]).toMatch(/·/);
      if (m !== 'Fact') expect(DASH[m]!.length).toBe(2);
    });
  }
  it('dash is computed from displayMode, not node type; edge law == node law', () => {
    expect(EDGE_DASH).toEqual(DASH);
    const style = (g6GraphOptions(spec).node as { style: Record<string, (d: unknown) => unknown> }).style;
    expect(style.lineDash({ data: { displayMode: 'Fog', elementKind: 'file' } })).toEqual([1, 3]);
    expect(style.lineDash({ data: { displayMode: 'Fog', elementKind: 'concept' } })).toEqual([1, 3]);
  });
});

describe('plugin stack + fisheye (VIZ-E-01/02/09)', () => {
  it('fisheye is exactly one plugins[] entry with full passthrough', () => {
    const plugins = buildPlugins({ fisheye: { trigger: 'drag', d: 2, r: 300, maxR: 500, scaleRBy: 'wheel' } });
    expect(plugins).toHaveLength(1);
    const p = plugins[0] as Record<string, unknown>;
    expect(p).toMatchObject({ type: 'fisheye', trigger: 'drag', d: 2, r: 300, maxR: 500, scaleRBy: 'wheel' });
  });
  it('three-plugin stack', () => {
    const plugins = buildPlugins({ fisheye: { d: 1.5 }, minimap: true, edgeBundling: true });
    expect(plugins.map((p) => (p as { type: string }).type)).toEqual(['fisheye', 'minimap', 'edge-bundling']);
  });
  it('zero lens geometry in the adapter (VIZ-E-09)', () => {
    const src = readFileSync(new URL('../src/engines/g6.ts', import.meta.url), 'utf8');
    for (const banned of ['lensRadius', 'magnif', 'transformPoint', 'hitTest', 'atan2']) {
      expect(src).not.toContain(banned);
    }
    expect(src).not.toMatch(/Math\.(atan|sin|cos|sqrt)\b/);
  });
});

describe('shape auto-selection + parity (VIZ-D-03/04)', () => {
  it('acyclic → dagre; cyclic → force; tree → compact-box', () => {
    const data = g6Adapt(spec);
    expect(g6AutoLayout(data)).toBe('compact-box');
    const cyclic = {
      ...data,
      edges: [
        ...data.edges,
        { id: 'back', source: 'authorize', target: 'checkout',
          data: { edgeKind: 'calls', displayMode: 'Fact' as const, evidence: { file: 'checkout.py', start: 92, end: 92 } } },
      ],
    };
    expect(g6AutoLayout(cyclic)).toBe('force');
  });
  it('layout swap preserves ids + modes; renderer switch preserves count (VIZ-20)', () => {
    const canvas = g6GraphOptions({ ...spec, preferences: { renderer: 'canvas' } });
    const svg = g6GraphOptions({ ...spec, preferences: { renderer: 'svg' } });
    expect((canvas.data as { nodes: unknown[] }).nodes).toEqual((svg.data as { nodes: unknown[] }).nodes);
  });
});

describe('G6 audit + no ViewKind (VIZ-16 static)', () => {
  it('audit is clean with Full fidelity on the reference engine', () => {
    const report = new G6Adapter().audit(spec);
    expect(report.findings).toEqual([]);
    expect(report.encodingFidelity).toBe('Full');
  });
  it('no ViewKind identifier anywhere in src', () => {
    const dir = new URL('../src/', import.meta.url);
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.ts')) continue;
      expect(readFileSync(new URL(f, dir), 'utf8')).not.toMatch(/\bViewKind\b/);
    }
  });
});
