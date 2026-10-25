import type { AtlasQuery } from './atlas-types';
import type { EntityKind } from './atlas-populations';
export interface AtlasSearch {
    text?: string;
    kind?: EntityKind;
    language?: string;
    pathPrefix?: string;
    calls?: {
        id: string;
        incoming?: boolean;
    };
    readsWrites?: {
        id: string;
    };
}
/** Uses the existing storage query API; never add an SQL/parallel search path. */
export function searchQuery(spec: AtlasSearch): AtlasQuery {
    if (spec.readsWrites)
        throw new Error('Reads/writes search is unavailable: storage has no registered Reads/Writes relation');
    const conditions: Record<string, unknown>[] = [];
    if (spec.text?.trim())
        conditions.push({ or: [{ field: { field: 'name', op: 'contains_fold', value: spec.text.trim() } }, { field: { field: 'qualified_name', op: 'contains_fold', value: spec.text.trim() } }] });
    if (spec.language)
        conditions.push({ field: { field: 'language', op: 'eq', value: spec.language } });
    if (spec.pathPrefix)
        conditions.push({ field: { field: (spec.kind ?? 'symbol') === 'file' ? 'path' : 'file.path', op: 'prefix', value: spec.pathPrefix } });
    const query: AtlasQuery = { select: { kind: spec.kind ?? 'symbol', filter: conditions.length ? { and: conditions } : {} } };
    if (spec.calls)
        return { intersect: [query, { follow: { input: { select: { kind: 'symbol', filter: { ids: [spec.calls.id] } } }, relation: 'declared_calls', incoming: spec.calls.incoming ?? true, max_depth: 1 } }] };
    return query;
}
export interface SpatialScope {
    id: string;
    label: string;
    kind: EntityKind;
    query: AtlasQuery;
}
