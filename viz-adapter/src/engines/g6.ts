/**
 * G6 engine adapter (CIE-VIZ-MULTI PRD Part III) — the reference engine:
 * full epistemic encoding, the only fisheye/webgl/canvasExport owner, and
 * the custom-extension surface (flame graph = registered layout + node,
 * never adapter branching — Part V).
 */

import type {
  AuditReport,
  CapabilitySet,
  DisplayMode,
  EngineAdapter,
  EngineId,
  EngineLimits,
  G6EdgeDatum,
  G6NodeDatum,
  GraphData,
  HostRenderRuntime,
  LayoutHint,
  LayoutOutcome,
  Preferences,
  RenderHandle,
  RenderOptions,
  ViewSpec,
} from '../types';
import { gate } from '../gate';
import { recomputeRenderHash } from '../hash';
import { audit as provenanceAudit } from '../audit';

// ── Epistemic vocabulary (open maps, fixed law) ─────────────────────────

/** Open node-type vocabulary — adding a kind is adding ONE entry.
 *  RFC-0024 stage 1: process/decision/store/board-card are the FOUR new
 *  kinds the extension budget was spent on (catalog C10 + ux-spec §4.1). */
export const NODE_TYPE: Record<string, string> = {
  concept: 'circle',
  symbol: 'rect',
  file: 'rect',
  route: 'diamond',
  rule: 'hexagon',
  domain: 'donut',
  entity: 'ellipse',
  'stack-frame': 'cie-flame-frame',
  process: 'rect',
  decision: 'diamond',
  store: 'rect',
  'board-card': 'rect',
};

/** Unregistered name falls back to `circle`, never crashes. */
export function nodeTypeFor(kind: string): string {
  return NODE_TYPE[kind] ?? 'circle';
}

export const DASH: Record<DisplayMode, number[] | undefined> = {
  Fact: undefined,
  Inference: [6, 4],
  Hypothesis: [2, 4],
  Fog: [1, 3],
};

export const STROKE: Record<DisplayMode, string> = {
  Fact: '#2ea043',
  Inference: '#d29922',
  Hypothesis: '#f85149',
  Fog: '#8b949e',
};

export const EDGE_DASH: Record<DisplayMode, number[] | undefined> = DASH;

/** The redundant WORD channel. */
export const WORDS: Record<DisplayMode, string> = {
  Fact: 'Fact · statically proven',
  Inference: 'Inference · derived from cited evidence',
  Hypothesis: 'Hypothesis · cannot be proven statically',
  Fog: 'Fog · not determinable from static analysis',
};

/** G6 v5 layout registry names the adapter may be asked for (+ the
 *  registered custom layout `cie-flame`, Part V). */
export const G6_LAYOUTS = [
  'antv-dagre', 'circular', 'combo-combined', 'concentric', 'd3-force',
  'd3-force-3d', 'dagre', 'fishbone', 'force', 'force-atlas2', 'fruchterman',
  'grid', 'mds', 'radial', 'random', 'snake', 'compact-box', 'dendrogram',
  'mindmap', 'indented', 'cie-flame',
] as const;

/** Deterministic shape-based auto-selection (acyclic → dagre, tree →
 *  compact-box, dense/cyclic → force). Geometry itself stays in G6. */
export function autoLayout(data: GraphData): string {
  const n = data.nodes.length;
  if (n === 0 || data.edges.length === 0) return 'dagre';

  let hasCycle = false;
  const colour = new Map<string, 1 | 2>();
  const adj = new Map<string, string[]>();
  for (const e of data.edges) {
    (adj.get(e.source) ?? adj.set(e.source, []).get(e.source)!).push(e.target);
  }
  const visit = (start: string): void => {
    const stack: Array<[string, number]> = [[start, 0]];
    while (stack.length > 0) {
      const [node, phase] = stack.pop()!;
      if (phase === 0) {
        colour.set(node, 1);
        stack.push([node, 1]);
        for (const k of adj.get(node) ?? []) {
          const c = colour.get(k);
          if (c === 1) hasCycle = true;
          else if (c === undefined) stack.push([k, 0]);
        }
      } else {
        colour.set(node, 2);
      }
      if (hasCycle) return;
    }
  };
  for (const node of data.nodes) {
    if (!colour.has(node.id)) visit(node.id);
    if (hasCycle) break;
  }
  if (hasCycle) return 'force';
  if (data.edges.length > 2 * n) return 'force';

  const indegree = new Map<string, number>();
  for (const e of data.edges) {
    indegree.set(e.target, (indegree.get(e.target) ?? 0) + 1);
  }
  for (const d of indegree.values()) {
    if (d > 1) return 'dagre';
  }
  return 'compact-box';
}

/** Fisheye is exactly ONE plugins[] entry; documented options pass through
 *  unchanged. G6 owns the lens — zero geometry here. */
export function buildPlugins(p: Preferences = {}): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = [];
  if (p.fisheye) {
    out.push({
      type: 'fisheye',
      key: 'cie-fisheye',
      trigger: p.fisheye.trigger ?? 'pointermove',
      d: p.fisheye.d ?? 1.5,
      r: p.fisheye.r ?? 120,
      ...p.fisheye,
    });
  }
  if (p.minimap) out.push({ type: 'minimap', key: 'cie-minimap', size: [240, 160] });
  if (p.edgeBundling) out.push({ type: 'edge-bundling', key: 'cie-bundling' });
  if (p.history) out.push({ type: 'history', key: 'cie-history' });
  return out;
}

// ── Pure data layer ──────────────────────────────────────────────────────

export function adapt(spec: ViewSpec): GraphData {
  gate(spec);
  return {
    nodes: spec.elements.map((el) => ({
      id: el.id,
      data: {
        elementKind: el.elementKind,
        displayMode: el.displayMode,
        label: el.label,
        evidence: el.evidence,
        claimId: el.claimId,
        counterArgument: el.counterArgument,
        fogReason: el.fogReason,
        salienceWeight: el.salience.weight,
        fields: el.fields,
        parentId: el.parentId,
      },
    })),
    edges: spec.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
      data: {
        edgeKind: e.edgeKind,
        displayMode: e.displayMode,
        evidence: e.evidence,
        counterArgument: e.counterArgument,
      },
    })),
    combos: (spec.groups ?? []).map((g) => ({
      id: g.id,
      data: { label: g.label, kind: g.kind },
    })),
  };
}

function layoutOutcome(
  spec: ViewSpec,
  data: GraphData,
): { layout: Record<string, unknown>; outcome: LayoutOutcome } {
  const hint = spec.layoutHint;
  if (!hint) {
    const strategy = autoLayout(data);
    return {
      layout: { type: strategy },
      outcome: { requested: undefined, actual: strategy, fidelity: 'Exact' },
    };
  }
  if ((G6_LAYOUTS as readonly string[]).includes(hint.strategy)) {
    return {
      layout: { type: hint.strategy, ...(hint.params ?? {}) },
      outcome: { requested: hint.strategy, actual: hint.strategy, fidelity: 'Exact' },
    };
  }
  // Unavailable in the registry and not registered as a custom layout —
  // substitute by shape and DISCLOSE (never claim an unrendered layout).
  const actual = autoLayout(data);
  return {
    layout: { type: actual, ...(hint.params ?? {}) },
    outcome: { requested: hint.strategy, actual, fidelity: 'Substituted' },
  };
}

export function graphOptions(spec: ViewSpec): Record<string, unknown> {
  const data = adapt(spec);
  const prefs: Preferences = spec.preferences ?? {};
  const { layout } = layoutOutcome(spec, data);
  return {
    data,
    node: {
      type: (d: G6NodeDatum) => nodeTypeFor(d.data.elementKind),
      style: {
        lineDash: (d: G6NodeDatum) => DASH[d.data.displayMode],
        stroke: (d: G6NodeDatum) => STROKE[d.data.displayMode],
        lineWidth: (d: G6NodeDatum) => (d.data.displayMode === 'Fact' ? 2 : 1.5),
        size: (d: G6NodeDatum) => 20 + d.data.salienceWeight * 40,
        labelText: (d: G6NodeDatum) => d.data.label,
        labelPlacement: 'bottom',
        badge: (d: G6NodeDatum) =>
          d.data.displayMode === 'Fog' ? [{ text: '?', placement: 'right-top' }] : [],
      },
    },
    edge: {
      style: {
        lineDash: (d: G6EdgeDatum) => EDGE_DASH[d.data.displayMode],
        stroke: (d: G6EdgeDatum) => STROKE[d.data.displayMode],
        endArrow: true,
        labelText: (d: G6EdgeDatum) => d.data.edgeKind,
      },
    },
    layout,
    plugins: buildPlugins(prefs),
    renderer: prefs.renderer ?? 'canvas',
  };
}

export function layoutOutcomeFor(spec: ViewSpec): LayoutOutcome {
  return layoutOutcome(spec, adapt(spec)).outcome;
}

// ── The adapter object ───────────────────────────────────────────────────

const G6_CAPABILITIES: CapabilitySet = {
  fisheye: true,
  customNodes: true,
  customEdges: true,
  customLayouts: true,
  plugins: true,
  interactive: true,
  editable: true,
  combos: true,
  animatedLayout: true,
  svgExport: true,
  canvasExport: true,
  webgl: true,
};

const G6_LIMITS: EngineLimits = {
  maxNodes: 10_000,
  maxEdges: 20_000,
  supportedLayouts: [...G6_LAYOUTS],
  unsupportedFeatures: [],
};

export class G6Adapter implements EngineAdapter {
  readonly id: EngineId = 'g6';
  readonly capabilities = G6_CAPABILITIES;
  readonly limits = G6_LIMITS;
  /** RFC-0024 stage 1 — the host-injected runtime (the page bundles
   *  @antv/g6). When registered, render/relayout delegate to it; without
   *  one they remain the documented capability statement (throw), never
   *  a silent blank. */
  private hostRuntime: HostRenderRuntime | null = null;

  setHostRuntime(rt: HostRenderRuntime): void {
    this.hostRuntime = rt;
  }
  hasHostRuntime(): boolean {
    return this.hostRuntime !== null;
  }

  gate(spec: ViewSpec): void {
    gate(spec);
  }
  adapt(spec: ViewSpec): GraphData {
    return adapt(spec);
  }
  configure(spec: ViewSpec, options: RenderOptions): Record<string, unknown> {
    const base = graphOptions(spec) as Record<string, unknown>;
    if (options.renderer !== undefined) base.renderer = options.renderer;
    if (options.fisheye === false) {
      base.plugins = (base.plugins as Record<string, unknown>[]).filter(
        (p) => p.type !== 'fisheye',
      );
    }
    return { ...base, ...options };
  }
  /** Browser-only: requires the @antv/g6 host runtime — REGISTER one via
   *  setHostRuntime (the page bundles G6; the adapter stays headless).
   *  Without one, this throw is a documented capability statement. */
  async render(
    container: HTMLElement,
    spec: ViewSpec,
    options: RenderOptions,
  ): Promise<RenderHandle> {
    if (!this.hostRuntime) {
      throw new Error(
        'g6 render requires the @antv/g6 host runtime — register it with setHostRuntime (RFC-0024 stage 1)',
      );
    }
    return this.hostRuntime.render(this.id, container, spec, options);
  }
  async relayout(handle: RenderHandle, layout: LayoutHint): Promise<LayoutOutcome> {
    if (!this.hostRuntime?.relayout) {
      throw new Error('g6 relayout requires the @antv/g6 host runtime');
    }
    return this.hostRuntime.relayout(this.id, handle, layout);
  }
  audit(spec: ViewSpec, layout: LayoutOutcome = layoutOutcomeFor(spec)): AuditReport {
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
