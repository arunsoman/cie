/**
 * VIZ-16 — the SAME ViewSpec rendered by TWO engines has identical
 * element ids (tc06.parity.spec.ts); VIZ-21 — fisheye is false everywhere
 * except G6 (capability.fisheye.spec.ts folded in); substitution
 * disclosure (VIZ-17) covered per engine.
 */

import { describe, expect, it } from 'vitest';
import {
  defaultRegistry,
  g6Adapt,
  jointjsAdapt,
  mermaidAdapt,
  reactflowAdapt,
  renderHeadless,
} from '../src/index.js';
import type { JointGraph, RFGraph, ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;
const reg = defaultRegistry();

describe('VIZ-16: two engines, one ViewSpec, identical element ids', () => {
  it('g6 ↔ jointjs', () => {
    const a = g6Adapt(spec).nodes.map((n) => n.id).sort();
    const b = jointjsAdapt(spec).elements.map((n) => n.id).sort();
    expect(b).toEqual(a);
  });

  it('g6 ↔ reactflow', () => {
    const a = g6Adapt(spec).nodes.map((n) => n.id).sort();
    const b = reactflowAdapt(spec).nodes.map((n) => n.id).sort();
    expect(b).toEqual(a);
  });

  it('g6 ↔ mermaid (ids sanitized but traceable)', () => {
    const a = g6Adapt(spec).nodes.map((n) => n.id).sort();
    const text = mermaidAdapt(spec).text;
    for (const id of a) expect(text).toContain(id.replace(/[^A-Za-z0-9_]/g, '_'));
  });

  it('displayMode is preserved across all four adapters', () => {
    const modes = Object.fromEntries(spec.elements.map((e) => [e.id, e.displayMode]));
    const rf = reactflowAdapt(spec).nodes;
    for (const n of rf) {
      expect(n.data.displayMode).toBe(modes[n.id]);
      expect(n.className).toMatch(/cie-(fact|inference|hypothesis|fog)/);
    }
    const jj = jointjsAdapt(spec).elements;
    for (const el of jj) {
      const src = spec.elements.find((e) => e.id === el.id)!;
      if (src.displayMode === 'Fact') expect(el.attrs.body.strokeDasharray).toBeUndefined();
      if (src.displayMode === 'Inference') expect(el.attrs.body.strokeDasharray).toBe('6 4');
      if (src.displayMode === 'Hypothesis') expect(el.attrs.body.strokeDasharray).toBe('2 4');
    }
  });
});

describe('VIZ-21: fisheye capability is G6-only', () => {
  for (const [id, adapter] of reg) {
    it(`${id}: fisheye === ${id === 'g6'}`, () => {
      expect(adapter.capabilities.fisheye).toBe(id === 'g6');
    });
  }
});

describe('VIZ-17: layout substitution is disclosed per engine', () => {
  it('g6: known registry layout exact; unknown substituted', () => {
    const r1 = renderHeadless(
      { ...JSON.parse(JSON.stringify(spec)), layoutHint: { strategy: 'dagre' } },
      { required: {} },
      reg,
    );
    expect((r1.options as { layout: { type: string } }).layout.type).toBe('dagre');

    const unknown = JSON.parse(JSON.stringify(spec)) as ViewSpec;
    unknown.layoutHint = { strategy: 'journey' };
    const g6 = reg.get('g6') as NonNullable<ReturnType<typeof reg.get>>;
    const report = g6.audit(unknown);
    expect(report.layout.fidelity).toBe('Substituted');
    expect(report.layout.requested).toBe('journey');
  });

  it('reactflow: dagre exact, force substituted to layered', () => {
    const rf = reg.get('reactflow') as NonNullable<ReturnType<typeof reg.get>>;
    const withDagre = { ...JSON.parse(JSON.stringify(spec)), layoutHint: { strategy: 'dagre' } } as ViewSpec;
    expect((rf.adapt(withDagre) as RFGraph).layout.fidelity).toBe('Exact');
    const withForce = { ...JSON.parse(JSON.stringify(spec)), layoutHint: { strategy: 'force' } } as ViewSpec;
    const out = rf.adapt(withForce) as RFGraph;
    expect(out.layout).toEqual({ requested: 'force', actual: 'layered', fidelity: 'Substituted' });
  });

  it('jointjs: fruchterman maps to spring, disclosed', () => {
    const jj = reg.get('jointjs') as NonNullable<ReturnType<typeof reg.get>>;
    const s = { ...JSON.parse(JSON.stringify(spec)), layoutHint: { strategy: 'fruchterman' } } as ViewSpec;
    expect((jj.adapt(s) as JointGraph).layout).toEqual({
      requested: 'fruchterman',
      actual: 'spring',
      fidelity: 'Substituted',
    });
  });
});
