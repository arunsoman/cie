/**
 * JointJS engine adapter (PRD Part III/IV) — SVG element/link shapes with
 * strokeDasharray epistemic encoding: FULL fidelity (Part IV §7 row 2).
 * Layouts map to the nearest JointJS layout with disclosed substitution.
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
} from '../types';
import { gate } from '../gate';
import { recomputeRenderHash } from '../hash';
import { audit as provenanceAudit } from '../audit';

export const JOINTJS_LAYOUTS = ['directed-graph', 'spring', 'force', 'cycle', 'dagre'] as const;

export interface JointElement {
  id: string;
  shape: string;
  attrs: {
    label: { text: string };
    body: {
      stroke: string;
      strokeWidth: number;
      strokeDasharray?: string;
      fill?: string;
    };
  };
  parent?: string;
}

export interface JointLink {
  id: string;
  source: { id: string };
  target: { id: string };
  labels: Array<{ attrs: { text: { text: string } } }>;
  attrs: { line: { stroke: string; strokeDasharray?: string; targetMarker: string } };
}

export interface JointGraph {
  elements: JointElement[];
  links: JointLink[];
  layout: LayoutOutcome;
}

export const JOINT_STROKE: Record<string, string> = {
  Fact: '#2ea043',
  Inference: '#d29922',
  Hypothesis: '#f85149',
  Fog: '#8b949e',
};

/** Part IV §7 JointJS row: solid / strokeDasharray / finer dash / pattern. */
export const JOINT_DASH: Record<string, string | undefined> = {
  Fact: undefined,
  Inference: '6 4',
  Hypothesis: '2 4',
  Fog: '1 3',
};

function layoutOutcome(spec: ViewSpec): LayoutOutcome {
  const requested = spec.layoutHint?.strategy;
  if (!requested) {
    return { requested: undefined, actual: 'directed-graph', fidelity: 'Exact' };
  }
  if ((JOINTJS_LAYOUTS as readonly string[]).includes(requested)) {
    return { requested, actual: requested, fidelity: 'Exact' };
  }
  // Map to the nearest JointJS layout, disclosed (Part IV §8 row 4).
  const nearest: Record<string, string> = {
    dagre: 'dagre',
    'antv-dagre': 'dagre',
    force: 'force',
    'd3-force': 'force',
    'force-atlas2': 'force',
    fruchterman: 'spring',
    radial: 'cycle',
    circular: 'cycle',
  };
  const actual = nearest[requested] ?? 'directed-graph';
  return { requested, actual, fidelity: actual === requested ? 'Exact' : 'Substituted' };
}

export function adapt(spec: ViewSpec): JointGraph {
  gate(spec);
  const layout = layoutOutcome(spec);
  const elements: JointElement[] = spec.elements.map((el) => {
    const dash = JOINT_DASH[el.displayMode];
    const body: JointElement['attrs']['body'] = {
      stroke: JOINT_STROKE[el.displayMode] ?? JOINT_STROKE.Fact!,
      strokeWidth: el.displayMode === 'Fact' ? 2 : 1.5,
      ...(dash ? { strokeDasharray: dash } : {}),
      ...(el.displayMode === 'Fog' ? { fill: 'url(#cie-hatch)' } : {}),
    };
    return {
      id: el.id,
      shape: el.elementKind === 'concept' ? 'circle' : 'rect',
      attrs: { label: { text: el.label }, body },
      ...(el.parentId ? { parent: el.parentId } : {}),
    };
  });
  const links: JointLink[] = spec.edges.map((e) => ({
    id: e.id,
    source: { id: e.source },
    target: { id: e.target },
    labels: [{ attrs: { text: { text: e.edgeKind } } }],
    attrs: {
      line: {
        stroke: JOINT_STROKE[e.displayMode] ?? JOINT_STROKE.Fact!,
        ...(JOINT_DASH[e.displayMode] ? { strokeDasharray: JOINT_DASH[e.displayMode] } : {}),
        targetMarker: 'triangle',
      },
    },
  }));
  return { elements, links, layout };
}

const JOINT_CAPABILITIES: CapabilitySet = {
  fisheye: false,
  customNodes: true,
  customEdges: true,
  customLayouts: true,
  plugins: true,
  interactive: true,
  editable: true,
  combos: true,
  animatedLayout: true,
  svgExport: true,
  canvasExport: false,
  webgl: false,
};

const JOINT_LIMITS: EngineLimits = {
  maxNodes: 5_000,
  maxEdges: 10_000,
  supportedLayouts: [...JOINTJS_LAYOUTS],
  unsupportedFeatures: ['fisheye', 'canvas export', 'webgl'],
};

export class JointJSAdapter implements EngineAdapter {
  readonly id: EngineId = 'jointjs';
  readonly capabilities = JOINT_CAPABILITIES;
  readonly limits = JOINT_LIMITS;

  gate(spec: ViewSpec): void {
    gate(spec);
  }
  adapt(spec: ViewSpec): JointGraph {
    return adapt(spec);
  }
  configure(spec: ViewSpec, _options: RenderOptions): Record<string, unknown> {
    const g = adapt(spec);
    return { elements: g.elements, links: g.links, layout: g.layout };
  }
  async render(_c: HTMLElement, _s: ViewSpec, _o: RenderOptions): Promise<RenderHandle> {
    throw new Error('jointjs render requires the jointjs host runtime');
  }
  async relayout(_h: RenderHandle, _l: LayoutHint): Promise<LayoutOutcome> {
    throw new Error('jointjs relayout requires the jointjs host runtime');
  }
  audit(spec: ViewSpec, layout: LayoutOutcome = layoutOutcome(spec)): AuditReport {
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
