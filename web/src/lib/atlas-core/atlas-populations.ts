import type { AtlasQuery, AtlasItemRef } from './atlas-types';
export type EntityKind = 'symbol' | 'file' | 'concept' | 'entry_point' | 'route' | 'finding' | 'contract' | 'summary' | 'component' | 'container' | 'dependency' | 'cluster' | 'duplicate_pair' | 'workflow' | 'state_machine' | 'state' | 'framework' | 'evidence';
export interface PopulationMapping {
    from: EntityKind;
    to: EntityKind;
    relation: string;
    incoming?: boolean;
}
/** Only declared storage relations may bridge universes. No path/name guesses. */
export const storageMappings: PopulationMapping[] = [
    { from: 'symbol', to: 'container', relation: 'member_of_container' }, { from: 'container', to: 'symbol', relation: 'member_of_container', incoming: true },
    { from: 'file', to: 'container', relation: 'member_of_container' }, { from: 'container', to: 'file', relation: 'member_of_container', incoming: true },

    { from: 'route', to: 'symbol', relation: 'source_symbol' }, { from: 'symbol', to: 'route', relation: 'source_symbol', incoming: true },
    { from: 'entry_point', to: 'symbol', relation: 'source_symbol' }, { from: 'symbol', to: 'entry_point', relation: 'source_symbol', incoming: true },

    { from: 'symbol', to: 'file', relation: 'defined_in' }, { from: 'file', to: 'symbol', relation: 'defined_in', incoming: true },
    { from: 'symbol', to: 'concept', relation: 'member_of_concept' }, { from: 'concept', to: 'symbol', relation: 'member_of_concept', incoming: true },
    { from: 'symbol', to: 'component', relation: 'member_of_component' }, { from: 'component', to: 'symbol', relation: 'member_of_component', incoming: true },
    { from: 'route', to: 'file', relation: 'defined_in' }, { from: 'dependency', to: 'file', relation: 'uses_dependency', incoming: true },
    { from: 'file', to: 'route', relation: 'defined_in', incoming: true }, { from: 'file', to: 'dependency', relation: 'uses_dependency' },
    { from: 'summary', to: 'symbol', relation: 'summarizes' }, { from: 'contract', to: 'symbol', relation: 'constrains' },
    { from: 'symbol', to: 'summary', relation: 'summarizes', incoming: true }, { from: 'symbol', to: 'contract', relation: 'constrains', incoming: true },
    { from: 'cluster', to: 'symbol', relation: 'member_of_cluster', incoming: true }, { from: 'symbol', to: 'cluster', relation: 'member_of_cluster' },
    { from: 'duplicate_pair', to: 'symbol', relation: 'left_symbol' }, { from: 'workflow', to: 'symbol', relation: 'source_symbol' },
    { from: 'state_machine', to: 'symbol', relation: 'source_symbol' }, { from: 'state', to: 'state_machine', relation: 'state_of' },
];
export function mapPopulation(query: AtlasQuery, from: EntityKind, to: EntityKind, mappings = storageMappings): AtlasQuery {
    if (from === to)
        return query;
    const queue: [
        EntityKind,
        AtlasQuery
    ][] = [[from, query]], seen = new Set([from]);
    for (let i = 0; i < queue.length; i++) {
        const [kind, q] = queue[i];
        for (const m of mappings.filter(m => m.from === kind)) {
            if (seen.has(m.to))
                continue;
            const next: AtlasQuery = { follow: { input: q, relation: m.relation, incoming: m.incoming ?? false, max_depth: 1 } };
            // Storage narrows polymorphic traversals before composing homogeneous sets.
            const typed: AtlasQuery = { intersect: [{ select: { kind: m.to } }, next] };
            if (m.to === to)
                return typed;
            seen.add(m.to);
            queue.push([m.to, typed]);
        }
    }
    throw new Error(`No registered population mapping: ${from} → ${to}`);
}
export function itemPopulation(item: AtlasItemRef, target: EntityKind, mappings = storageMappings): AtlasQuery {
    return mapPopulation(item.operand, item.entityKind ?? target, target, mappings);
}
