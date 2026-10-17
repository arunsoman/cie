/**
 * Text outline — the A11Y linearization (PRD Part IV §7 VIZ-V-02).
 * Generated from the ViewSpec, NEVER from the rendered output; it is the
 * AUTHORITATIVE rendering whenever an engine's fidelity is Degraded, and
 * the fallback when canvas rendering fails.
 */

import type { ViewSpec } from './types.js';

const WORD: Record<string, string> = {
  Fact: 'Fact, statically proven',
  Inference: 'Inference, derived from cited evidence',
  Hypothesis: 'Hypothesis, cannot be proven statically',
  Fog: 'Fog, not determinable from static analysis',
};

export interface OutlineEntry {
  kind: 'element' | 'edge';
  id: string;
  text: string;
}

/** Reading order: elements in spec order, then edges in spec order. */
export function textOutline(spec: ViewSpec): OutlineEntry[] {
  const out: OutlineEntry[] = [];
  for (const el of spec.elements) {
    const links = spec.edges.filter((e) => e.source === el.id || e.target === el.id).length;
    out.push({
      kind: 'element',
      id: el.id,
      text: `${el.label} — ${WORD[el.displayMode] ?? el.displayMode} (${links} link${links === 1 ? '' : 's'})`,
    });
  }
  for (const e of spec.edges) {
    out.push({
      kind: 'edge',
      id: e.id,
      text: `${e.source} ${e.edgeKind} ${e.target} — ${WORD[e.displayMode] ?? e.displayMode}`,
    });
  }
  return out;
}

/** Screen-reader headline: what it is, how it is known, how many links. */
export function outlineSummary(spec: ViewSpec): string {
  const f = spec.footer;
  return (
    `${spec.elements.length} elements and ${spec.edges.length} relations. ` +
    `${f.facts} facts, ${f.inferences} inferences, ${f.hypotheses} hypotheses, ${f.fog} fog. ` +
    f.caption
  );
}

export function renderOutline(spec: ViewSpec): string {
  return [outlineSummary(spec), '', ...textOutline(spec).map((e) => `  ${e.text}`)].join('\n');
}
