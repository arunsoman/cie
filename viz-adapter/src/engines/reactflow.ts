/**
 * ReactFlow engine adapter (PRD Part III/IV) — React nodes with CSS-class
 * epistemic encoding (FULL fidelity via the custom node: dashed border,
 * dotted border, hatched node background). Layouts: dagre/elk are run
 * BEFORE rendering; any other hint substitutes to the deterministic
 * built-in layered pass and is DISCLOSED (Part IV §8).
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

export const REACTFLOW_LAYOUTS = ['dagre', 'elk'] as const;

export interface RFNode {
  id: string;
  position: { x: number; y: number };
  data: { label: string; displayMode: string; elementKind: string; evidence: unknown };
  className: string;
  parentNode?: string;
  extent?: 'parent';
}

export interface RFEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  className: string;
  animated: boolean;
}

export interface RFGraph {
  nodes: RFNode[];
  edges: RFEdge[];
  layout: LayoutOutcome;
}

/** CSS classes — the FULL custom-node encoding (Part IV §7). */
export const RF_CLASSES: Record<string, string> = {
  Fact: 'cie-fact',               // solid border
  Inference: 'cie-inference',     // dashed border
  Hypothesis: 'cie-hypothesis',   // dotted border
  Fog: 'cie-fog',                 // hatched node background
};

/** Deterministic layered fallback (used ONLY when dagre/elk are absent in
 *  the host; disclosed as Substituted). Longest-path layering, BFS order. */
export function layeredPositions(
  ids: string[],
  edges: Array<{ source: string; target: string }>,
  nodeW = 172,
  nodeH = 48,
  gapX = 60,
  gapY = 90,
): Record<string, { x: number; y: number }> {
  const layer = new Map<string, number>(ids.map((id) => [id, 0]));
  // Relax to a longest-path layering (bounded by |V| passes — acyclic graphs).
  for (let i = 0; i < ids.length; i++) {
    let changed = false;
    for (const e of edges) {
      const next = (layer.get(e.source) ?? 0) + 1;
      if (next > (layer.get(e.target) ?? 0)) {
        layer.set(e.target, next);
        changed = true;
      }
    }
    if (!changed) break;
  }
  const perLayer = new Map<number, string[]>();
  for (const id of ids) {
    const l = layer.get(id) ?? 0;
    (perLayer.get(l) ?? perLayer.set(l, []).get(l)!).push(id);
  }
  const pos: Record<string, { x: number; y: number }> = {};
  for (const [l, members] of perLayer) {
    members.forEach((id, i) => {
      pos[id] = { x: i * (nodeW + gapX), y: l * (nodeH + gapY) };
    });
  }
  return pos;
}

function layoutOutcome(spec: ViewSpec): { outcome: LayoutOutcome; positions?: 'dagre' | 'elk' | 'layered' } {
  const requested = spec.layoutHint?.strategy;
  if (!requested) {
    return { outcome: { requested: undefined, actual: 'dagre', fidelity: 'Exact' }, positions: 'dagre' };
  }
  if ((REACTFLOW_LAYOUTS as readonly string[]).includes(requested)) {
    return { outcome: { requested, actual: requested, fidelity: 'Exact' }, positions: requested as 'dagre' | 'elk' };
  }
  return { outcome: { requested, actual: 'layered', fidelity: 'Substituted' }, positions: 'layered' };
}

export function adapt(spec: ViewSpec): RFGraph {
  gate(spec);
  const { outcome } = layoutOutcome(spec);
  // Positions: the HOST runs dagre/elk; headless, the deterministic
  // layered pass provides placeholder coordinates (same element set).
  const pos = layeredPositions(
    spec.elements.map((e) => e.id),
    spec.edges.map((e) => ({ source: e.source, target: e.target })),
  );
  const nodes: RFNode[] = spec.elements.map((el) => ({
    id: el.id,
    position: pos[el.id] ?? { x: 0, y: 0 },
    data: {
      label: el.label,
      displayMode: el.displayMode,
      elementKind: el.elementKind,
      evidence: el.evidence,
    },
    className: RF_CLASSES[el.displayMode] ?? 'cie-fact',
    ...(el.parentId ? { parentNode: el.parentId, extent: 'parent' as const } : {}),
  }));
  const edges: RFEdge[] = spec.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    label: e.edgeKind,
    className: e.displayMode === 'Fact' ? 'cie-edge-fact' : 'cie-edge-dashed',
    animated: e.displayMode !== 'Fact',
  }));
  return { nodes, edges, layout: outcome };
}

const RF_CAPABILITIES: CapabilitySet = {
  fisheye: false,
  customNodes: true,
  customEdges: true,
  customLayouts: false, // dagre/ELK are external, pre-render passes
  plugins: true,
  interactive: true,
  editable: true,
  combos: false, // parent-nodes approximates combos — declared, not claimed
  animatedLayout: true,
  svgExport: false,
  canvasExport: false,
  webgl: false,
};

const RF_LIMITS: EngineLimits = {
  maxNodes: 2_000,
  maxEdges: 4_000,
  supportedLayouts: [...REACTFLOW_LAYOUTS],
  unsupportedFeatures: ['fisheye', 'canvas export', 'webgl', 'native combos'],
};

export class ReactFlowAdapter implements EngineAdapter {
  readonly id: EngineId = 'reactflow';
  readonly capabilities = RF_CAPABILITIES;
  readonly limits = RF_LIMITS;

  gate(spec: ViewSpec): void {
    gate(spec);
  }
  adapt(spec: ViewSpec): RFGraph {
    return adapt(spec);
  }
  configure(spec: ViewSpec, _options: RenderOptions): Record<string, unknown> {
    const g = adapt(spec);
    return {
      nodes: g.nodes,
      edges: g.edges,
      layout: g.layout,
      focusApproximation: 'contextual-zoom', // PRD §9: labelled, never "FISHEYE"
    };
  }
  async render(_c: HTMLElement, _s: ViewSpec, _o: RenderOptions): Promise<RenderHandle> {
    throw new Error('reactflow render requires the reactflow host runtime');
  }
  async relayout(_h: RenderHandle, _l: LayoutHint): Promise<LayoutOutcome> {
    throw new Error('reactflow relayout requires the reactflow host runtime');
  }
  audit(spec: ViewSpec, layout: LayoutOutcome = layoutOutcome(spec).outcome): AuditReport {
    return {
      findings: provenanceAudit(spec),
      encodingFidelity: 'Full',
      layout,
    };
  }
  renderHashOf(spec: ViewSpec): string {
    return recomputeRenderHash(spec, this.id);
  }
}
