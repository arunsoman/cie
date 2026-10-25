export interface AtlasDimension {
    id: string;
    field: string;
    type: 'string' | 'number' | 'enum';
    unit?: string;
    unknownLabel: string;
    boundaries?: number[];
    values?: string[];
}
export interface AtlasMeasure {
    id: string;
    aggregate: 'count' | 'sum' | 'average' | 'min' | 'max';
    field?: string;
    unit?: string;
}
export class AtlasDimensionRegistry {
    private dimensions = new Map<string, AtlasDimension>();
    private measures = new Map<string, AtlasMeasure>();
    registerDimension(d: AtlasDimension) { if (!d.id || !d.field || !d.unknownLabel || this.dimensions.has(d.id))
        throw new Error('Invalid/duplicate dimension'); if (d.boundaries?.some((n, i, a) => !Number.isFinite(n) || (i > 0 && n <= a[i - 1])))
        throw new Error('Bucket boundaries must increase'); this.dimensions.set(d.id, d); return this; }
    registerMeasure(m: AtlasMeasure) { if (this.measures.has(m.id) || !m.id || (m.aggregate !== 'count' && !m.field))
        throw new Error('Invalid/duplicate measure'); this.measures.set(m.id, m); return this; }
    cube(controls: import('./atlas-workspace').CubeControls, continuation: string | null) {
        const dimensions = controls.dimensions.map(id => { const d = this.dimensions.get(id); if (!d)
            throw new Error(`Unregistered dimension ${id}`); return { id, dimension: d.boundaries ? { numeric_bucket: { field: d.field, boundaries: d.boundaries } } : { field: d.field } }; });
        const measures = controls.measures.map(id => { const m = this.measures.get(id); if (!m)
            throw new Error(`Unregistered measure ${id}`); return { id, measure: m.aggregate === 'count' ? 'count' : { [m.aggregate]: m.field } }; });
        if (!measures.length || new Set(controls.dimensions).size !== controls.dimensions.length)
            throw new Error('Cube requires unique dimensions and a measure');
        const axes = [...controls.rows, ...controls.columns];
        if (new Set(axes).size !== axes.length || axes.some(id => !controls.dimensions.includes(id)))
            throw new Error('Invalid pivot axes');
        return { dimensions, measures, layout: axes.length ? { pivot: { rows: controls.rows, columns: controls.columns } } : 'cells', missing: 'include_unknown', drill_through: 'enabled', page: { limit: 30, continuation } };
    }
}
export const foundationDimensions = new AtlasDimensionRegistry()
    .registerDimension({ id: 'file', field: 'file.path', type: 'string', unknownLabel: 'Unknown file' })
    .registerDimension({ id: 'kind', field: 'kind', type: 'enum', unknownLabel: 'Unknown kind' })
    .registerDimension({ id: 'language', field: 'language', type: 'string', unknownLabel: 'Unknown language' })
    .registerMeasure({ id: 'count', aggregate: 'count', unit: 'entities' });
