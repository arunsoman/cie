import type { AtlasViewDefinition, AtlasResponse, ProjectionContext, AtlasItemRef, AtlasDatum } from './atlas-types';
interface Entry {id: string; fields: Record<string, unknown>}
function read(r: AtlasResponse) {
    const output = r.outputs.find(o => o.id === 'rows');
    if (!output || output.schema_version !== 2 || output.error || !output.data || ['failed','unavailable'].includes(output.status))
        throw new Error(output?.error?.message ?? 'Captured entry records unavailable');
    return output.data as {items: Entry[]; total: number; continuation: string | null; truncated: boolean};
}
function entryItem(entry: Entry, context: ProjectionContext, kind: 'route' | 'entry_point'): AtlasDatum {
    const f = entry.fields, file = String(f.rel ?? ''), handler = typeof f.source_symbol_ref === 'string' ? f.source_symbol_ref : undefined;
    const label = kind === 'route' ? `${f.verb} ${f.path}` : String(f.name ?? 'Unnamed entry');
    const location = f.handler_location as {file?: string; start?: number; end?: number} | undefined;
    const resolved = f.handler_resolution === 'unique' && !!handler;
    const ref: AtlasItemRef = {
        id: resolved ? handler! : entry.id, label, pin: context.pin, group: 'symbol', entityKind: resolved ? 'symbol' : kind,
        operand: {select: {kind: resolved ? 'symbol' : kind, filter: {ids: [resolved ? handler! : entry.id]}}},
        evidence: resolved && location?.file ? [{file: location.file, ...(location.start != null && location.end! > location.start ? {startByte: location.start, endByte: location.end} : {})}]
            : file ? [{file, ...(typeof f.line === 'number' && f.line > 0 ? {line: f.line} : {})}] : [],
        provenance: [{kind: 'stored-fact', pin: context.pin, artifact: 'stack', recordId: entry.id}],
    };
    const fileRef: AtlasItemRef = {id: `file:${file}`, label: file, pin: context.pin, group: 'file.path', entityKind: 'symbol',
        operand: {select: {kind: 'symbol', filter: {field: {field: 'file.path', op: 'eq', value: file}}}}, evidence: file ? [{file}] : []};
    return {key: entry.id, label, ref, role: 'member', authority: 'Fact', fields: {
        entry: label, handler: kind === 'route' ? `${f.owner ?? ''}.${f.method ?? ''}` : String(f.kind ?? ''), file,
        resolution: resolved ? 'Unique captured symbol' : `${f.handler_resolution ?? 'unresolved'} · no API scope`,
    }, cellRefs: file ? {file: fileRef} : undefined};
}
function entries(kind: 'route' | 'entry_point', id: string, title: string): AtlasViewDefinition {
    return {id, binding: 'selection', targetKind: kind,
        outputs: continuation => [{id: 'rows', output: {entities: {page: {limit: 30, continuation}}}}],
        project(r, c) { const rows = read(r); return {
            kind: 'table', title, items: rows.items.map(e => entryItem(e, c, kind)), total: rows.total, continuation: rows.continuation,
            columns: [{id: 'entry', label: kind === 'route' ? 'API' : 'Entry'}, {id: 'handler', label: kind === 'route' ? 'Recorded target' : 'Entry kind'}, {id: 'file', label: 'File'}, {id: 'resolution', label: 'Resolution'}],
            gaps: ['Cross-filtering includes only entries with a uniquely resolved captured target symbol. Ambiguous and unsupported entries may remain in the captured stack but are not included by the symbol-to-entry mapping.',
                kind === 'route' ? 'Select an API to scope its handler. Static call graphs and sequences expand recorded calls with explicit limits; this is not proof of complete runtime dispatch.' : 'Application entries may identify a bootstrap class rather than a function. A class does not imply an executable control-flow trace.',
                ...(rows.truncated ? ['Additional captured entries are available on the next page.'] : [])],
        }; },
    };
}
export const routeView = entries('route', 'routes', 'Service entry points');
export const applicationEntryView = entries('entry_point', 'entries', 'Application entry points');
