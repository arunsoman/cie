/**
 * VIZ-19 — the text outline is authoritative when fidelity is degraded:
 * generated from the VIEWSPEC (never from the rendered output), complete
 * (every element and edge), and epistemically worded.
 */

import { describe, expect, it } from 'vitest';
import { MermaidAdapter, outlineSummary, renderOutline, textOutline } from '../src/index.js';
import type { ViewSpec } from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;

describe('VIZ-19: the outline is the degraded-fidelity authority', () => {
  it('derived from the ViewSpec only', () => {
    const entries = textOutline(spec);
    expect(entries).toHaveLength(spec.elements.length + spec.edges.length);
  });

  it('every element carries its display mode IN WORDS', () => {
    for (const el of spec.elements) {
      const entry = textOutline(spec).find((x) => x.id === el.id)!;
      expect(entry.text).toMatch(
        /Fact, statically proven|Inference, derived from cited evidence|Hypothesis, cannot be proven statically|Fog, not determinable from static analysis/,
      );
    }
  });

  it('summary states what it is, how it is known, how many links', () => {
    const summary = outlineSummary(spec);
    expect(summary).toMatch(new RegExp(`\\b${spec.elements.length} elements\\b`));
    expect(summary).toMatch(new RegExp(`\\b${spec.edges.length} relations\\b`));
    expect(summary).toMatch(/facts, .* hypotheses/);
    expect(summary).toContain('Display categories, not a confidence score.');
  });

  it('on a Degraded engine (mermaid), the outline is authoritative', () => {
    const adapter = new MermaidAdapter();
    const report = adapter.audit(spec);
    expect(report.encodingFidelity).toBe('Degraded');
    // The authoritative fallback exists and is complete — regardless of
    // what the engine could draw.
    const outline = renderOutline(spec);
    for (const el of spec.elements) expect(outline).toContain(el.label);
  });
});
