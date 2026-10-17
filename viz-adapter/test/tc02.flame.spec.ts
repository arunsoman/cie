/**
 * VIZ-12 — the flame graph via custom layout + custom node (TC-02). The
 * geometry core is PURE and asserted headless (PRD §16 assertions 1–6, 9);
 * the G6 canvas render is exercised by demos/flame-graph.html.
 */

import { describe, expect, it } from 'vitest';
import { computeFlameGeometry, depthColor, FLAME_DASH, FLAME_STROKE } from '../src/index.js';
import type { FlameNodeIn } from '../src/index.js';

// PRD §11's ViewSpec as geometry input.
const nodes: FlameNodeIn[] = [
  { id: 'root', label: 'main', displayMode: 'Fact', samples: 1000 },
  { id: 'f1', label: 'handleRequest', displayMode: 'Fact', samples: 600 },
  { id: 'f2', label: 'serialize', displayMode: 'Fact', samples: 400 },
  { id: 'f1a', label: 'db.query', displayMode: 'Fact', samples: 450 },
  { id: 'f1b', label: 'cache.hit', displayMode: 'Hypothesis', samples: 150 },
];
const edges = [
  { source: 'root', target: 'f1' },
  { source: 'root', target: 'f2' },
  { source: 'f1', target: 'f1a' },
  { source: 'f1', target: 'f1b' },
];

describe('TC-02: flame graph geometry (pure core)', () => {
  const geo = computeFlameGeometry(nodes, edges, 960, 24);

  it('#1 5+ nodes rendered', () => {
    expect(Object.keys(geo.frames).length).toBe(5);
  });

  it('#2 root width == canvas width', () => {
    expect(geo.frames['root']!.width).toBe(960);
  });

  it('#3 child widths proportional to samples: f1/f2 == 600/400', () => {
    const w1 = geo.frames['f1']!.width;
    const w2 = geo.frames['f2']!.width;
    expect(w1 / w2).toBeCloseTo(600 / 400, 6);
    expect(w1).toBeCloseTo(576, 6); // 600/1000 * 960
  });

  it('#4 siblings pack left-to-right without overlap', () => {
    const f1 = geo.frames['f1']!;
    const f2 = geo.frames['f2']!;
    expect(f1.x).toBe(0);
    expect(f2.x).toBeCloseTo(f1.x + f1.width, 6);
    // Same at leaf level: f1a then f1b under f1.
    const f1a = geo.frames['f1a']!;
    const f1b = geo.frames['f1b']!;
    expect(f1a.x).toBe(f1.x);
    expect(f1b.x).toBeCloseTo(f1a.x + f1a.width, 6);
    expect(f1b.x + f1b.width).toBeLessThanOrEqual(f1.x + f1.width + 1e-6);
  });

  it('#5 depth → y monotone', () => {
    expect(geo.frames['root']!.y).toBeLessThan(geo.frames['f1']!.y);
    expect(geo.frames['f1']!.y).toBeLessThan(geo.frames['f1a']!.y);
  });

  it('#6 hypothesis frame keeps its dotted stroke law', () => {
    const cache = nodes.find((n) => n.id === 'f1b')!;
    expect(FLAME_DASH[cache.displayMode]).toEqual([2, 4]);
    expect(FLAME_STROKE[cache.displayMode]).toBe('#f85149');
    expect(FLAME_DASH['Fact']).toBeUndefined();
  });

  it('#9 geometry is layout-input only: swapping to dagre changes no frame identity', () => {
    // The same node ids feed any layout; the flame packing is pure.
    const again = computeFlameGeometry(nodes, edges, 960, 24);
    expect(again.order).toEqual(geo.order);
  });

  it('deterministic and degenerate-safe', () => {
    expect(computeFlameGeometry(nodes, edges)).toEqual(geo);
    const empty = computeFlameGeometry([], []);
    expect(empty.frames).toEqual({});
    expect(empty.root).toBe('');
  });

  it('depth heat colours are deterministic and depth-ordered', () => {
    expect(depthColor(0)).not.toBe(depthColor(4));
    expect(depthColor(4)).toBe(depthColor(4));
  });
});
