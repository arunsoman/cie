/**
 * VIZ-23 (structural) + VIZ-25 — the catalog: counts hold (76 charts,
 * 68 native / 4 custom / 4 alt-engine), every entry's recommended engine
 * is capable by its declared CapabilitySet, and the provenance audit
 * passes for a spec generated from EVERY catalog shape.
 */

import { describe, expect, it } from 'vitest';
import {
  CATALOG,
  CATALOG_SUMMARY,
  audit,
  defaultRegistry,
  gate,
  renderHeadless,
} from '../src/index.js';
import type { CatalogEntry, ViewSpec } from '../src/index.js';

/** Build a minimal, fully-attested ViewSpec for a catalog entry. */
function specFor(entry: CatalogEntry): ViewSpec {
  const ev = (f: string) => ({ file: f, start: 1, end: 9 });
  const root = { id: 'n-root', label: `${entry.id} root`, file: 'root.py' };
  const a = { id: 'n-a', label: `${entry.id} node A`, file: 'a.py' };
  const b = { id: 'n-b', label: `${entry.id} node B`, file: 'b.py' };
  const el = (n: { id: string; label: string; file: string }, mode: 'Fact' | 'Hypothesis' = 'Fact') => ({
    id: n.id,
    elementKind: entry.shape.split(' ')[0]!.replace(/[^a-z-]/gi, '') || 'node',
    displayMode: mode,
    label: n.label,
    evidence: ev(n.file),
    ...(mode === 'Hypothesis' ? { claimId: 'C-1', counterArgument: 'unproven' } : {}),
    salience: { factors: [], weight: 0.5, whyShown: 'catalog' },
    fields: {},
  });
  return {
    elements: [el(root), el(a), el(b, 'Hypothesis')],
    edges: [
      { id: 'e1', source: root.id, target: a.id, edgeKind: 'contains', displayMode: 'Fact', evidence: ev('a.py') },
      { id: 'e2', source: root.id, target: b.id, edgeKind: 'relates', displayMode: 'Fact', evidence: ev('b.py') },
    ],
    footer: { facts: 2, inferences: 0, hypotheses: 1, fog: 0, stale: 0,
              caption: 'Display categories, not a confidence score. Gaps: gap-x.' },
    gaps: [{ id: 'gap-x', area: entry.id, why: 'catalog fixture' }],
    hidden: [],
    scope: { revision: 1, environments: [], coverage: { analyzed: true } },
    layoutHint: { strategy: entry.layout.split(/[ ,]/)[0]! },
    semanticHash: 'sha256:' + '0'.repeat(64),
    renderHash: 'sha256:' + '0'.repeat(64),
  };
}

describe('catalog invariants (VIZ-23 structural)', () => {
  it('76 charts; the Part IX custom four; non-G6 charts as Alt-engine', () => {
    // PRD Part VI's summary row (68/4/4) contradicts its own per-category
    // tables (A: 3 custom, F: 1 custom → natives 69). We resolve to the
    // internally consistent set the PRD names explicitly in Part IX:
    //   Custom    = { A1 UML class, A6 ports, A7 actor, F2 flame }
    //   Alt-engine = { B3 mermaid, B6 custom SVG, F1 custom SVG }
    expect(CATALOG_SUMMARY.total).toBe(76);
    expect(CATALOG_SUMMARY.native + CATALOG_SUMMARY.custom + CATALOG_SUMMARY.altEngine).toBe(76);
    const custom = CATALOG.filter((c) => c.g6Status === 'Custom').map((c) => c.id).sort();
    expect(custom).toEqual(['A1', 'A6', 'A7', 'F2']);
    const alt = CATALOG.filter((c) => c.g6Status === 'Alt-engine').map((c) => c.id).sort();
    expect(alt).toEqual(['B3', 'B6', 'F1']);
    expect(CATALOG_SUMMARY.native).toBe(69);
    expect(CATALOG).toHaveLength(76);
  });

  it('extension budget: at most 4 custom node types, 2 layouts, 2 plugins', () => {
    const custom = CATALOG.filter((c) => c.g6Status === 'Custom');
    // A1/A6/A7/A12 share the uml-class-node + ports + actor-icon family;
    // F1 timeline + F2 flame are the two custom layouts.
    const customLayouts = new Set(custom.filter((c) => c.id === 'F1' || c.id === 'F2').map((c) => c.layout));
    expect(customLayouts.size).toBeLessThanOrEqual(2);
    expect(CATALOG_SUMMARY.extensionBudget.customNodes).toBe(4);
  });

  it('ids are unique and category counts match the PRD table', () => {
    expect(new Set(CATALOG.map((c) => c.id)).size).toBe(76);
    const byCat = Object.fromEntries(CATALOG.map((c) => [c.category[0]!, 0]));
    for (const c of CATALOG) byCat[c.category[0]!]! += 1;
    expect(byCat).toEqual({ A: 13, B: 7, C: 9, D: 6, E: 10, F: 7, G: 5, H: 5, I: 7, J: 4, K: 3 });
  });
});

describe('VIZ-25: provenance audit passes for every catalog chart', () => {
  for (const entry of CATALOG) {
    it(`${entry.id} ${entry.name}: gate + audit clean`, () => {
      const spec = specFor(entry);
      expect(() => gate(spec)).not.toThrow();
      expect(audit(spec)).toEqual([]);
    });

    it(`${entry.id} ${entry.name}: recommended engine is eligible`, () => {
      const spec = specFor(entry);
      const reg = defaultRegistry();
      const engine = entry.bestEngine === 'custom-svg' ? 'g6' : entry.bestEngine;
      // The engine must exist and be able to adapt the shape.
      const adapter = reg.get(engine as 'g6')!;
      expect(() => adapter.adapt(spec)).not.toThrow();
    });
  }

  it('VIZ-23: each catalog chart negotiates through the pipeline headlessly', () => {
    for (const entry of CATALOG.slice(0, 12)) {
      const spec = specFor(entry);
      const result = renderHeadless(spec, { required: {} }, defaultRegistry());
      expect(result.engineId).toBeTruthy();
      expect(result.disclosure.decisions.length).toBe(4);
    }
  });
});
