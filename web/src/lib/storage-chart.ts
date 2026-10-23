import type { ChatChartSpec } from './chat-chart';
export interface CubeView {
  renderer: 'cie-cube';
  kind: 'table' | 'hbar' | 'heatmap' | 'stat' | 'donut';
  revision: string;
  cells: {
    dimensions: Array<{ id: string }>;
    measures: Record<string, unknown>;
    items: Array<{ members: Record<string, unknown>; measures: Record<string, number | null>; contributors?: string }>;
    truncated?: boolean;
    partition_verified?: boolean;
  };
}
const label = (value: unknown): string => value && typeof value === 'object' ? JSON.stringify(value) : String(value ?? 'Unknown');
/** Consume the storage projection through the existing gated chart renderer. */
export function storageChart(view: CubeView): ChatChartSpec {
  if (view.renderer !== 'cie-cube' || !view.cells || !Array.isArray(view.cells.items)) throw new Error('Invalid cube projection');
  if (view.kind === 'donut' && !view.cells.partition_verified) throw new Error('Donut partition is unverified');
  const dimensions = view.cells.dimensions.map(d => d.id);
  const measures = Object.keys(view.cells.measures);
  if (!measures.length) throw new Error('Cube has no measures');
  if (view.kind !== 'table' && measures.length !== 1) throw new Error('Choose one measure for this chart');
  const points = view.cells.items.map(cell => {
    const value = cell.measures[measures[0]];
    if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error('Chart requires finite numeric values');
    return { name: dimensions.map(d => label(cell.members[d])).join(' / ') || 'All matches', value };
  });
  return {
    kind: view.kind, title: 'Stored repository facts',
    tell: `Revision ${view.revision}${view.cells.truncated ? ' · partial page' : ''}`,
    source: `Storage revision ${view.revision}; distinct contributors`,
    columns: [...dimensions, ...measures],
    rows: view.cells.items.map(cell => [...dimensions.map(d => label(cell.members[d])), ...measures.map(m => cell.measures[m] ?? 'Unknown')]),
    data: points, tiles: points,
  };
}
