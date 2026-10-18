/**
 * Interaction layer tests (CIE-US host closes: US-022, US-023, US-028,
 * US-030, US-140, US-141, US-142, US-143).
 */
import { describe, expect, it } from 'vitest';
import type { ViewSpec } from '../src/types.js';
import {
  MODE_CHANNELS,
  SettleDetector,
  THEME_TOKENS,
  announce,
  colourAloneAudit,
  expansionLens,
  gestureToProposal,
  keyboardMap,
  layoutQuality,
  levelOfDetail,
  rovingOrder,
  verifyParity,
  verifyTheme,
} from '../src/interaction.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;

describe('US-022 level of detail + settle', () => {
  it('decides tiers deterministically with a disclosed why', () => {
    expect(levelOfDetail(0.2, 50).tier).toBe('labels-only');
    expect(levelOfDetail(1.0, 900).tier).toBe('clustered');
    expect(levelOfDetail(2.0, 900).tier).toBe('full');
    expect(levelOfDetail(1.0, 10).tier).toBe('full');
    const d = levelOfDetail(0.2, 50);
    expect(d.why).toContain('labels');
  });

  it('settle detector collapses bursts and reports quiet', () => {
    const s = new SettleDetector(100);
    expect(s.feed(0)).toBe(false);
    expect(s.feed(30)).toBe(false);
    expect(s.feed(60)).toBe(false);
    expect(s.feed(200)).toBe(true); // 140ms quiet → settled
    expect(s.lastSettledAt).toBe(200);
  });
});

describe('US-023 hover-expansion lens', () => {
  it('returns aggregate + members from spec groups without mutating', () => {
    const lens = expansionLens(spec, 'no-such-group');
    expect(lens).toBeNull();
    // architecture.json has no groups: lens is honest about that
  });

  it('expands a grouped spec', () => {
    const grouped: ViewSpec = {
      ...spec,
      groups: [{ id: 'g1', label: 'Checkout cluster', kind: 'cluster' }],
      elements: spec.elements.map((e, i) =>
        i === 0 ? { ...e, parentId: 'g1' } : e,
      ),
    };
    const lens = expansionLens(grouped, 'g1');
    expect(lens).not.toBeNull();
    expect(lens!.aggregateLabel).toBe('Checkout cluster');
    expect(lens!.members).toHaveLength(1);
    expect(lens!.members[0].displayMode).toBe('Fact');
    expect(lens!.source).toBe('spec-groups');
  });
});

describe('US-028 gesture → proposal', () => {
  it('proposals are proposed, never applied', () => {
    const p = gestureToProposal(
      { kind: 'delete-node', targetId: spec.elements[0].id },
      spec,
    );
    expect(p).not.toBeNull();
    expect(p!.status).toBe('proposed');
    expect(p!.engineConsequences.length).toBeGreaterThan(0);
    expect(p!.summary).toContain(spec.elements[0].id);
  });

  it('connect requires both endpoints; unknown targets refuse', () => {
    const ok = gestureToProposal(
      { kind: 'connect', targetId: spec.elements[0].id, toId: spec.elements[1].id },
      spec,
    );
    expect(ok!.summary).toContain('→');
    expect(gestureToProposal({ kind: 'connect', targetId: 'x', toId: 'y' }, spec)).toBeNull();
    expect(gestureToProposal({ kind: 'delete-node', targetId: 'ghost' }, spec)).toBeNull();
  });
});

describe('US-030 layout quality', () => {
  it('measures overlaps and crossings from geometry and discloses strategy', () => {
    const two = spec.elements.slice(0, 2);
    const positions = new Map([
      [two[0].id, { x: 0, y: 0, w: 50, h: 30 }],
      [two[1].id, { x: 10, y: 5, w: 50, h: 30 }], // overlaps node 1
    ]);
    const q = layoutQuality(spec, positions, { w: 50, h: 30 }, 'dagre', 'dagre');
    expect(q.overlaps).toBe(1);
    expect(q.requested).toBe('dagre');
    expect(q.summary).toContain('dagre');
    // substituted strategy is disclosed as such
    const q2 = layoutQuality(spec, positions, { w: 50, h: 30 }, 'force', 'grid');
    expect(q2.summary).toContain('requested force');
    // a clean layout has zero overlaps
    const clean = new Map([
      [two[0].id, { x: 0, y: 0, w: 50, h: 30 }],
      [two[1].id, { x: 200, y: 0, w: 50, h: 30 }],
    ]);
    expect(layoutQuality(spec, clean, { w: 50, h: 30 }, undefined, 'grid').overlaps).toBe(0);
  });

  it('counts edge crossings when edges cross', () => {
    const ids = spec.elements.slice(0, 4).map((e) => e.id);
    const crossSpec: ViewSpec = {
      ...spec,
      elements: spec.elements.slice(0, 4),
      edges: [
        {
          id: 'e1',
          source: ids[0],
          target: ids[2],
          edgeKind: 'relates',
          displayMode: 'Hypothesis',
          evidence: { file: 'a.py', start: 0, end: 1 },
          counterArgument: 'no counter evidence',
        },
        {
          id: 'e2',
          source: ids[1],
          target: ids[3],
          edgeKind: 'relates',
          displayMode: 'Fact',
          evidence: { file: 'a.py', start: 0, end: 1 },
        },
      ],
    };
    const positions = new Map([
      [ids[0], { x: 0, y: 0, w: 10, h: 10 }],
      [ids[1], { x: 0, y: 100, w: 10, h: 10 }],
      [ids[2], { x: 100, y: 100, w: 10, h: 10 }],
      [ids[3], { x: 100, y: 0, w: 10, h: 10 }],
    ]);
    const q = layoutQuality(crossSpec, positions, { w: 10, h: 10 }, undefined, 'manual');
    expect(q.crossings).toBe(1);
  });
});

describe('US-140 keyboard parity', () => {
  it('every pointer action has a key path', () => {
    const v = verifyParity(keyboardMap());
    expect(v.ok).toBe(true);
    expect(v.missing).toHaveLength(0);
  });

  it('the contract fails when an action loses its keys', () => {
    const v = verifyParity(keyboardMap().filter((k) => k.pointer !== 'delete-node'));
    expect(v.ok).toBe(false);
    expect(v.missing).toContain('delete-node');
  });
});

describe('US-141 announcements', () => {
  it('announces label + how-known + evidence file', () => {
    const a = announce(spec, spec.elements[0].id);
    expect(a).toContain(spec.elements[0].label);
    expect(a).toContain('evidence');
    expect(a).toContain(spec.elements[0].evidence.file);
    expect(announce(spec, 'ghost')).toBeNull();
  });

  it('roving order covers elements then edges (outline order)', () => {
    const order = rovingOrder(spec);
    expect(order).toHaveLength(spec.elements.length + spec.edges.length);
    expect(order.slice(0, spec.elements.length)).toEqual(spec.elements.map((e) => e.id));
  });
});

describe('US-142 never colour alone', () => {
  it('each mode has a distinct non-colour channel', () => {
    const a = colourAloneAudit();
    expect(a.ok).toBe(true);
    // the channels differ pairwise
    const shapes = Object.values(MODE_CHANNELS).map((c) => c.shape);
    expect(new Set(shapes).size).toBe(shapes.length);
    const prefixes = Object.values(MODE_CHANNELS).map((c) => c.prefix);
    expect(new Set(prefixes).size).toBe(prefixes.length);
  });

  it('a colliding override is a first-class finding', () => {
    const a = colourAloneAudit({ Fog: 'square' }); // collides with Fact
    expect(a.ok).toBe(false);
    expect(a.findings[0]).toContain('VIZ-V-03');
  });
});

describe('US-143 theme tokens', () => {
  it('tokens are semantic roles; themes must be complete', () => {
    expect(THEME_TOKENS).not.toContain('#ffffff');
    const ok = verifyTheme(Object.fromEntries(THEME_TOKENS.map((t) => [t, 'var(--x)'])));
    expect(ok.ok).toBe(true);
    const bad = verifyTheme({ surface: 'white' });
    expect(bad.ok).toBe(false);
    expect(bad.missing.length).toBeGreaterThan(0);
  });
});
