/** RFC-0023: pure sheet planning over sealed data. Geometry stays in hosts. */
import type { EngineAdapter, EngineId, ViewSpec } from './types';
import { gate } from './gate';
import { chooseEngine } from './negotiator';

export interface ChartPlan {
  chartKind: 'component-board' | 'tree' | 'flow';
  engineId: EngineId;
  elementSubset: string[];
  actions: ('drill' | 'filter')[];
  detail: 'summary' | 'full';
  why: string;
}
export function planSheet(view: ViewSpec, registry: Map<EngineId, EngineAdapter>, opts: {
  scope?: string[];
  factsOnly?: boolean;
  detail?: 'summary' | 'full';
  viewport?: { width: number; height: number };
} = {}): ChartPlan[] {
  gate(view);
  const scope = opts.scope && new Set(opts.scope);
  const elements = view.elements.filter(e => (!scope || scope.has(e.id)) && (!opts.factsOnly || e.displayMode === 'Fact'));
  if (!elements.length) return [];
  const ids = new Set(elements.map(e => e.id));
  const edges = view.edges.filter(e => ids.has(e.source) && ids.has(e.target) && (!opts.factsOnly || e.displayMode === 'Fact'));
  const engine = chooseEngine({required: {interactive:true, canvasExport:true}, graphSize:{nodes:elements.length,edges:edges.length}}, registry);
  const base = {engineId:engine.id, elementSubset:[...ids], detail:opts.detail ?? 'summary' as const};
  const rows: ChartPlan[] = [{...base, chartKind:'component-board', actions:['filter'], why:'Recorded elements and their cited source spans'}];
  if (elements.some(e => e.parentId && ids.has(e.parentId))) rows.unshift({...base, chartKind:'tree', actions:['drill','filter'], why:'Recorded parent containment'});
  if (edges.length) rows.push({...base, chartKind:'flow', actions:['filter'], why:'Recorded relations between the selected elements'});
  // Narrow viewports keep one chart at a time; never drop its elements.
  return opts.viewport && opts.viewport.width < 480 ? rows.slice(0,1) : rows;
}
