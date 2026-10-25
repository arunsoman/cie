import type { AtlasPin, AtlasQuery, AtlasSelection, AtlasOperation, AtlasItemRef } from './atlas-types';
export const samePin = (a: AtlasPin, b: AtlasPin) => a.repository === b.repository && a.revision === b.revision;
export function assertItem(item: AtlasItemRef, pin: AtlasPin) {
    if (!samePin(item.pin, pin))
        throw new Error('Atlas item belongs to another repository or revision');
    if (!item.id || !item.label || !item.group || !item.operand || !Array.isArray(item.evidence))
        throw new Error('Atlas data items require identity, selection operand and evidence availability');
    validateQuery(item.operand);
    for (const e of item.evidence) {
        if (e.pin && e.pin.repository !== pin.repository)
            throw new Error('Evidence belongs to another repository');
        if (e.line != null && (!Number.isSafeInteger(e.line) || e.line < 1))
            throw new Error('Invalid evidence line');
        if (!e.file || e.file.startsWith('/') || e.file.split(/[\\/]/).includes('..'))
            throw new Error('Invalid evidence path');
        if (e.startByte != null && (!Number.isSafeInteger(e.startByte) || e.startByte < 0 || !Number.isSafeInteger(e.endByte) || e.endByte! <= e.startByte))
            throw new Error('Invalid evidence span');
    }
}
/** Restrict operands to the typed storage algebra. No SQL or arbitrary predicates. */
export function validateQuery(q: AtlasQuery, depth = 0): void {
    if (!q || typeof q !== 'object' || depth > 24 || Object.keys(q).length !== 1)
        throw new Error('Invalid or excessively nested Atlas query');
    if ('select' in q) {
        if (!q.select || !q.select.kind)
            throw new Error('Missing query universe');
        return;
    }
    if ('cell_contributors' in q) {
        if (!q.cell_contributors.descriptor)
            throw new Error('Missing contributor descriptor');
        return;
    }
    if ('union' in q || 'intersect' in q) {
        const inputs = 'union' in q ? q.union : q.intersect;
        if (!Array.isArray(inputs) || !inputs.length || inputs.length > 1000)
            throw new Error('Invalid set operands');
        inputs.forEach(i => validateQuery(i, depth + 1));
        return;
    }
    if ('difference' in q) {
        validateQuery(q.difference.left, depth + 1);
        validateQuery(q.difference.right, depth + 1);
        return;
    }
    if ('follow' in q) {
        if (!q.follow.relation)
            throw new Error('Missing relation');
        validateQuery(q.follow.input, depth + 1);
        return;
    }
    throw new Error('Unknown Atlas query operator');
}
export const initialSelection = (): AtlasSelection => ({ contributions: [], mode: 'union' });
export function toggle(selection: AtlasSelection, item: AtlasItemRef): AtlasSelection {
    return { ...selection, contributions: selection.contributions.some(c => c.item.id === item.id)
            ? selection.contributions.filter(c => c.item.id !== item.id)
            : [...selection.contributions, { item, operation: selection.mode }] };
}
const combine = (kind: 'union' | 'intersect', queries: AtlasQuery[]): AtlasQuery => queries.length === 1 ? queries[0] : kind === 'union' ? { union: queries } : { intersect: queries };
/** OR in a group, AND between groups; explicit intersection/exclusion override grouping. */
export function selectionQuery(base: AtlasQuery, selection: AtlasSelection, normalize: (item: AtlasItemRef) => AtlasQuery = item => item.operand): AtlasQuery {
    const groups = new Map<string, AtlasQuery[]>(), intersections: AtlasQuery[] = [], excluded: AtlasQuery[] = [];
    for (const c of selection.contributions) {
        if (c.operation === 'exclude') {
            excluded.push(normalize(c.item));
            continue;
        }
        if (c.operation === 'intersection') {
            intersections.push(normalize(c.item));
            continue;
        }
        const group = groups.get(c.item.group) ?? [];
        group.push(normalize(c.item));
        groups.set(c.item.group, group);
    }
    const constraints = [...groups.values()].map(q => combine('union', q)).concat(intersections);
    const positive = constraints.length ? combine('intersect', [base, ...constraints]) : base;
    return excluded.length ? { difference: { left: positive, right: combine('union', excluded) } } : positive;
}
export function setOperation(selection: AtlasSelection, itemId: string, operation: AtlasOperation): AtlasSelection {
    return { ...selection, contributions: selection.contributions.map(c => c.item.id === itemId ? { ...c, operation } : c) };
}
