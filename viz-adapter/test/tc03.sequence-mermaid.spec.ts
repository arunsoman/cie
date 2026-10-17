/**
 * VIZ-13 — Mermaid renders the sequence diagram (TC-03) as engine-native
 * TEXT (headless-assertable), and VIZ-18 — its fidelity is Degraded and
 * DISCLOSED. Plus flowchart classDef encoding checks.
 */

import { describe, expect, it } from 'vitest';
import { MermaidAdapter, mermaidAdapt } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';

function seqSpec(): ViewSpec {
  const ev = { file: 'a.py', start: 0, end: 1 };
  return {
    elements: [
      { id: 'user', elementKind: 'lifeline', displayMode: 'Fact', label: 'User', evidence: ev,
        salience: { factors: [], weight: 0.5, whyShown: 't' }, fields: {} },
      { id: 'api', elementKind: 'lifeline', displayMode: 'Fact', label: 'API', evidence: ev,
        salience: { factors: [], weight: 0.5, whyShown: 't' }, fields: {} },
      { id: 'db', elementKind: 'lifeline', displayMode: 'Fog', label: 'DB', claimId: 'C-1',
        fogReason: 'connection pooling unresolved', evidence: ev,
        salience: { factors: [], weight: 0.2, whyShown: 't' }, fields: {} },
    ],
    edges: [
      { id: 'm1', source: 'user', target: 'api', edgeKind: 'message', displayMode: 'Fact', evidence: ev },
      { id: 'm2', source: 'api', target: 'db', edgeKind: 'message', displayMode: 'Inference', evidence: ev },
    ],
    footer: { facts: 2, inferences: 0, hypotheses: 0, fog: 1, stale: 0,
              caption: 'Display categories, not a confidence score.' },
    gaps: [], hidden: [],
    scope: { revision: 1, environments: [], coverage: { analyzed: true } },
    semanticHash: 'sha256:' + '0'.repeat(64),
    renderHash: 'sha256:' + '0'.repeat(64),
  };
}

describe('TC-03: mermaid sequence diagram (VIZ-13)', () => {
  const out = mermaidAdapt(seqSpec());

  it('shape-selected as sequenceDiagram', () => {
    expect(out.kind).toBe('sequenceDiagram');
  });

  it('participants + ordered messages with epistemic arrows', () => {
    const lines = out.text.split('\n');
    expect(lines[0]).toBe('sequenceDiagram');
    expect(lines.filter((l) => l.startsWith('participant')).length).toBe(3);
    expect(lines).toContain('user->>api: message'); // Fact = solid arrow
    expect(lines).toContain('api->>db: message'); // Inference keeps solid msg, class-carrying elsewhere
  });

  it('VIZ-18: audit reports Degraded fidelity (disclosed, never silent)', () => {
    const report = new MermaidAdapter().audit(seqSpec(), out.layout);
    expect(report.encodingFidelity).toBe('Degraded');
  });
});

describe('mermaid flowchart encoding (VIZ-V-01)', () => {
  /** A non-sequence spec (no lifeline kind) → flowchart branch. */
  function graphSpec(): ViewSpec {
    const s = seqSpec();
    s.elements = s.elements.map((e) => ({ ...e, elementKind: 'service' }));
    s.edges = s.edges.map((e) => ({ ...e, edgeKind: 'calls' }));
    return s;
  }

  it('classDefs carry the degraded table: dashes kept, fog grey-only', () => {
    const out = mermaidAdapt(graphSpec());
    expect(out.kind).toBe('flowchart');
    expect(out.classDefs.join('\n')).toMatch(/cieInference.*dasharray:6 4/);
    expect(out.classDefs.join('\n')).toMatch(/cieHypothesis.*dasharray:2 4/);
    expect(out.classDefs.join('\n')).toMatch(/cieFog fill:#8b949e/);
    expect(out.classDefs.join('\n')).not.toMatch(/cieFog.*dasharray/); // the degradation
  });

  it('layout: dagre/elk exact, others substituted + disclosed', () => {
    const s = graphSpec();
    s.layoutHint = { strategy: 'elk' };
    expect(mermaidAdapt(s).layout).toEqual({ requested: 'elk', actual: 'elk', fidelity: 'Exact' });
    s.layoutHint = { strategy: 'force' };
    expect(mermaidAdapt(s).layout).toEqual({ requested: 'force', actual: 'dagre', fidelity: 'Substituted' });
  });

  it('gate runs inside adapt — no bypass', () => {
    const s = graphSpec();
    (s.elements[0] as { evidence: unknown }).evidence = undefined;
    expect(() => mermaidAdapt(s)).toThrowError(/ELEMENT_WITHOUT_EVIDENCE/);
  });
});
