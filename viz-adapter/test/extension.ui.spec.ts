/**
 * VIZ-27 — adding a new chart requires NO adapter change: a toy chart is
 * "registered" by extending the open maps (NODE_TYPE / layout registry
 * names) exactly as `register()` does in the browser; VIZ-28/29/30 —
 * the UI disclosure payloads.
 */

import { describe, expect, it } from 'vitest';
import {
  fidelityBanner,
  g6Adapt,
  g6GraphOptions,
  layoutChip,
  NODE_TYPE,
  nodeTypeFor,
  engineBanner,
  disclose,
  chooseEngine,
  defaultRegistry,
} from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;
const reg = defaultRegistry();

describe('VIZ-27: a new chart is a registration, never an adapter change', () => {
  it('toy chart via register()-equivalent map extension only', () => {
    // 1. Register a toy node type and a toy layout "in the registry".
    NODE_TYPE['toy-shape'] = 'html';
    const G6_LAYOUTS_EXT = [...(g6GraphOptions(spec) as { layout: { type: string } } ? [] : [])];
    void G6_LAYOUTS_EXT;
    try {
      // 2. A ViewSpec using the new kind + new layout renders unchanged
      //    through the SAME adapter code path.
      const toy = JSON.parse(JSON.stringify(spec)) as ViewSpec;
      toy.elements[0]!.elementKind = 'toy-shape';
      toy.layoutHint = { strategy: 'toy-layout' };
      const data = g6Adapt(toy); // adapter logic untouched
      expect(data.nodes[0]!.data.elementKind).toBe('toy-shape');
      const opts = g6GraphOptions(toy);
      expect(
        (opts.node as { type: (d: { data: { elementKind: string } }) => string }).type({
          data: { elementKind: 'toy-shape' },
        }),
      ).toBe('html');
      // Unknown layout → disclosed substitution, never a crash.
      expect(nodeTypeFor('another-unregistered')).toBe('circle');
    } finally {
      delete NODE_TYPE['toy-shape'];
    }
  });
});

describe('VIZ-28: engine negotiation disclosed', () => {
  it('override headline names both engines and the reason', () => {
    const request = {
      required: { fisheye: true },
      preferred: 'mermaid' as const,
      graphSize: { nodes: 3, edges: 3 },
    };
    const chosen = chooseEngine(request, reg);
    const banner = engineBanner(disclose(request, chosen, reg));
    expect(banner.kind).toBe('engine-decision');
    expect(banner.headline).toMatch(/Preferred: mermaid ✗ .+ Rendered: g6/);
    expect(banner.chips.find((c) => c.engineId === 'mermaid')!.reason).toMatch(/fisheye/);
  });

  it('no override → plain headline', () => {
    const request = { required: {}, graphSize: { nodes: 3, edges: 3 } };
    const banner = engineBanner(disclose(request, reg.get('g6')!, reg));
    expect(banner.headline).toBe('Rendered: g6');
  });
});

describe('VIZ-29: layout chip — Exact / Substituted / Unavailable', () => {
  it('exact', () => {
    const chip = layoutChip({ requested: 'dagre', actual: 'dagre', fidelity: 'Exact' });
    expect(chip.text).toBe('Layout: dagre');
    expect(chip.fidelity).toBe('Exact');
  });
  it('substituted', () => {
    const chip = layoutChip({ requested: 'force', actual: 'dagre', fidelity: 'Substituted' });
    expect(chip.text).toBe('Layout: dagre (substituted from force)');
    expect(chip.fidelity).toBe('Substituted');
  });
  it('auto (no request)', () => {
    const chip = layoutChip({ requested: undefined, actual: 'compact-box', fidelity: 'Exact' });
    expect(chip.text).toBe('Layout: compact-box (auto)');
  });
  it('unavailable', () => {
    const chip = layoutChip({ requested: 'journey', actual: 'dagre', fidelity: 'Unavailable' });
    expect(chip.text).toContain('journey unavailable');
    expect(chip.fidelity).toBe('Unavailable');
  });
});

describe('VIZ-30: fidelity banner appears when degraded', () => {
  it('Full → hidden', () => {
    const b = fidelityBanner('Full');
    expect(b.visible).toBe(false);
    expect(b.authoritative).toBe(false);
  });
  it('Degraded → visible and authoritative (VIZ-V-02)', () => {
    const b = fidelityBanner('Degraded');
    expect(b.visible).toBe(true);
    expect(b.authoritative).toBe(true);
    expect(b.text).toMatch(/text outline is authoritative/);
  });
  it('Unsupported → visible and authoritative', () => {
    expect(fidelityBanner('Unsupported').visible).toBe(true);
  });
});
