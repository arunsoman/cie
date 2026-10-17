/**
 * Mermaid engine adapter (PRD Part III/IV) — text-first rendering with
 * DEGRADED epistemic encoding (VIZ-V-01): Hypothesis/Inference keep their
 * classDef dashes, Fog degrades to grey fill only. The text outline is
 * authoritative when fidelity degrades (VIZ-V-02). Layouts: dagre/elk
 * only — everything else substitutes with disclosure (Part IV §8).
 */

import type {
  AuditReport,
  CapabilitySet,
  EngineAdapter,
  EngineId,
  EngineLimits,
  LayoutHint,
  LayoutOutcome,
  RenderHandle,
  RenderOptions,
  ViewSpec,
} from '../types.js';
import { gate } from '../gate.js';
import { recomputeRenderHash } from '../hash.js';
import { audit as provenanceAudit } from '../audit.js';

export const MERMAID_LAYOUTS = ['dagre', 'elk'] as const;

export interface MermaidOutput {
  /** flowchart | sequenceDiagram — chosen by semantic shape. */
  kind: 'flowchart' | 'sequenceDiagram';
  text: string;
  classDefs: string[];
  layout: LayoutOutcome;
}

/** classDefs — the DEGRADED encoding table (Part IV §7). */
export const MERMAID_CLASSDEFS = [
  'classDef cieFact stroke:#2ea043,stroke-width:2px',
  'classDef cieInference stroke:#d29922,stroke-width:1.5px,stroke-dasharray:6 4',
  'classDef cieHypothesis stroke:#f85149,stroke-width:1.5px,stroke-dasharray:2 4',
  // Degraded: no hatched fill in Mermaid — grey fill is the honest remnant.
  'classDef cieFog fill:#8b949e,stroke:#8b949e,color:#ffffff',
];

function classFor(mode: string): string {
  return { Fact: 'cieFact', Inference: 'cieInference', Hypothesis: 'cieHypothesis', Fog: 'cieFog' }[
    mode
  ] ?? 'cieFact';
}

function sanitizeId(id: string): string {
  return id.replace(/[^A-Za-z0-9_]/g, '_');
}

function sanitizeLabel(label: string): string {
  return label.replace(/"/g, "'").replace(/[\r\n]+/g, ' ');
}

function layoutOutcome(spec: ViewSpec): LayoutOutcome {
  const requested = spec.layoutHint?.strategy;
  if (!requested) {
    return { requested: undefined, actual: 'dagre', fidelity: 'Exact' };
  }
  if ((MERMAID_LAYOUTS as readonly string[]).includes(requested)) {
    return { requested, actual: requested, fidelity: 'Exact' };
  }
  return { requested, actual: 'dagre', fidelity: 'Substituted' };
}

/** Sequence diagrams: lifeline elements + `message` edges (B3 in catalog). */
function isSequence(spec: ViewSpec): boolean {
  return (
    spec.elements.some((e) => e.elementKind === 'lifeline') &&
    spec.edges.some((e) => e.edgeKind === 'message')
  );
}

export function adapt(spec: ViewSpec): MermaidOutput {
  gate(spec);
  const layout = layoutOutcome(spec);

  if (isSequence(spec)) {
    const lines = ['sequenceDiagram'];
    for (const el of spec.elements) {
      lines.push(`participant ${sanitizeId(el.id)} as ${sanitizeLabel(el.label)}`);
    }
    for (const e of spec.edges) {
      const arrow =
        e.displayMode === 'Fact' ? '->>' : e.displayMode === 'Fog' ? '-->>' : '->>';
      lines.push(`${sanitizeId(e.source)}${arrow}${sanitizeId(e.target)}: ${sanitizeLabel(e.edgeKind)}`);
    }
    return { kind: 'sequenceDiagram', text: lines.join('\n'), classDefs: [], layout };
  }

  const lines = ['flowchart TD'];
  for (const el of spec.elements) {
    const shape =
      el.displayMode === 'Hypothesis' || el.displayMode === 'Fog' ? ('[/' + sanitizeLabel(el.label) + '/]') : ('[' + sanitizeLabel(el.label) + ']');
    lines.push(`${sanitizeId(el.id)}${shape}`);
    lines.push(`${sanitizeId(el.id)}:::${classFor(el.displayMode)}`);
  }
  for (const e of spec.edges) {
    const link = e.displayMode === 'Fact' ? '-->' : '-.->';
    lines.push(`${sanitizeId(e.source)} ${link} |${sanitizeLabel(e.edgeKind)}| ${sanitizeId(e.target)}`);
  }
  return {
    kind: 'flowchart',
    text: lines.join('\n'),
    classDefs: [...MERMAID_CLASSDEFS],
    layout,
  };
}

const MERMAID_CAPABILITIES: CapabilitySet = {
  fisheye: false,
  customNodes: false,
  customEdges: false,
  customLayouts: false,
  plugins: false,
  interactive: false,
  editable: false,
  combos: false,
  animatedLayout: false,
  svgExport: true,
  canvasExport: false,
  webgl: false,
};

const MERMAID_LIMITS: EngineLimits = {
  maxNodes: 500,
  maxEdges: 1_000,
  supportedLayouts: [...MERMAID_LAYOUTS],
  unsupportedFeatures: ['fisheye', 'hatched fog fill', 'combo containment', 'interaction'],
};

export class MermaidAdapter implements EngineAdapter {
  readonly id: EngineId = 'mermaid';
  readonly capabilities = MERMAID_CAPABILITIES;
  readonly limits = MERMAID_LIMITS;

  gate(spec: ViewSpec): void {
    gate(spec);
  }
  adapt(spec: ViewSpec): MermaidOutput {
    return adapt(spec);
  }
  configure(spec: ViewSpec, _options: RenderOptions): Record<string, unknown> {
    const out = adapt(spec);
    return { text: out.text, kind: out.kind, classDefs: out.classDefs, layout: out.layout };
  }
  async render(_c: HTMLElement, _s: ViewSpec, _o: RenderOptions): Promise<RenderHandle> {
    throw new Error('mermaid render requires the mermaid host runtime');
  }
  async relayout(_h: RenderHandle, _l: LayoutHint): Promise<LayoutOutcome> {
    throw new Error('mermaid relayout requires the mermaid host runtime');
  }
  audit(spec: ViewSpec, layout: LayoutOutcome = layoutOutcome(spec)): AuditReport {
    return {
      findings: provenanceAudit(spec),
      // VIZ-V-01: Fog degrades to grey fill only — DISCLOSED, never silent.
      encodingFidelity: 'Degraded',
      layout,
    };
  }
  renderHashOf(spec: ViewSpec): string {
    return recomputeRenderHash(spec, this.id);
  }
}
