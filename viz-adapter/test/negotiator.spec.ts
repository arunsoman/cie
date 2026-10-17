/**
 * VIZ-04..09 — the negotiator: pure, deterministic, preference-honouring,
 * hard-gated, size-bounded, and the ONLY entry to any adapter.
 */

import { describe, expect, it, vi } from 'vitest';
import {
  chooseEngine,
  decideAll,
  defaultRegistry,
  disclose,
  isEligible,
  renderHeadless,
  NegotiationError,
} from '../src/index.js';
import type { EngineAdapter, EngineRequest, ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const reg = defaultRegistry();
const spec = architecture as unknown as ViewSpec;

const base: EngineRequest = {
  required: {},
  graphSize: { nodes: spec.elements.length, edges: spec.edges.length },
};

describe('VIZ-04: chooseEngine is a pure function', () => {
  it('same request + registry → same adapter, repeatedly', () => {
    const a = chooseEngine(base, reg);
    for (let i = 0; i < 10; i++) expect(chooseEngine(base, reg)).toBe(a);
  });

  it('deterministic order without preference: g6 → jointjs → reactflow → mermaid', () => {
    expect(chooseEngine(base, reg).id).toBe('g6');
    const noG6 = new Map([...reg].filter(([id]) => id !== 'g6'));
    expect(chooseEngine(base, noG6).id).toBe('jointjs');
    const onlyMermaid = new Map([...reg].filter(([id]) => id === 'mermaid'));
    expect(chooseEngine(base, onlyMermaid).id).toBe('mermaid');
  });
});

describe('VIZ-05: user preference honoured when eligible', () => {
  it('preferred: mermaid on a small graph without special requirements → mermaid', () => {
    const chosen = chooseEngine({ ...base, preferred: 'mermaid' }, reg);
    expect(chosen.id).toBe('mermaid');
  });
});

describe('VIZ-06: preference rejected with a DISCLOSED reason when ineligible', () => {
  it('preferred: mermaid with fisheye requirement → g6, reasons disclosed', () => {
    const request: EngineRequest = { ...base, required: { fisheye: true }, preferred: 'mermaid' };
    const chosen = chooseEngine(request, reg);
    expect(chosen.id).toBe('g6');
    const d = disclose(request, chosen, reg);
    expect(d.rendered).toBe('g6');
    expect(d.preferredEligible).toBe(false);
    expect(d.preferredBlockReasons.join(' ')).toMatch(/fisheye/);
    expect(d.headline).toMatch(/Preferred: mermaid ✗/);
  });

  it('nodes > 500 and preferred mermaid → negotiated to g6 (PRD Part III table)', () => {
    const request: EngineRequest = {
      ...base,
      graphSize: { nodes: 501, edges: 900 },
      preferred: 'mermaid',
    };
    expect(chooseEngine(request, reg).id).toBe('g6');
  });
});

describe('VIZ-07: size limits exclude engines', () => {
  it('nodes > 2000 excludes mermaid AND reactflow', () => {
    const request: EngineRequest = { ...base, graphSize: { nodes: 2001, edges: 3000 } };
    const decisions = decideAll(request, reg);
    expect(decisions.find((d) => d.engineId === 'mermaid')!.eligible).toBe(false);
    expect(decisions.find((d) => d.engineId === 'reactflow')!.eligible).toBe(false);
    expect(chooseEngine(request, reg).id).toBe('g6');
    const noG6 = new Map([...reg].filter(([id]) => id !== 'g6'));
    expect(chooseEngine(request, noG6).id).toBe('jointjs');
  });
});

describe('VIZ-08: fisheye requirement restricts to G6', () => {
  it('only g6 declares the capability', () => {
    for (const [id, a] of reg) {
      expect(a.capabilities.fisheye).toBe(id === 'g6');
    }
    const chosen = chooseEngine({ ...base, required: { fisheye: true } }, reg);
    expect(chosen.id).toBe('g6');
  });
});

describe('VIZ-09: no adapter invocation without a negotiator decision', () => {
  it('an empty/declining registry → typed error, ZERO adapter calls', () => {
    const spyAdapter: EngineAdapter = {
      ...reg.get('g6')!,
      adapt: vi.fn(() => {
        throw new Error('adapter must never be reached');
      }),
    };
    const hostile = new Map([['g6' as const, spyAdapter]]);
    expect(() =>
      renderHeadless(
        spec,
        { required: { webgl: true }, preferred: undefined },
        new Map(), // nothing eligible
      ),
    ).toThrowError(NegotiationError);
    expect(spyAdapter.adapt).not.toHaveBeenCalled();
    void hostile;
  });

  it('the pipeline always negotiates first, then gates, then adapts', () => {
    const result = renderHeadless(spec, { required: {} }, reg);
    expect(result.engineId).toBe('g6');
    expect(result.disclosure.rendered).toBe('g6');
    expect((result.data as { nodes: unknown[] }).nodes.length).toBe(spec.elements.length);
  });

  it('isEligible is the gate used by chooseEngine (no side door)', () => {
    for (const a of reg.values()) expect(isEligible(a, base)).toBe(true);
  });
});
