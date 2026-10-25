import type { AtlasViewDefinition, AtlasResponse, AtlasItemRef, ProjectionContext, AtlasQuery } from './atlas-types';
import { binding, type DependencyView } from './atlas-semantic';
import { semanticEnvelope } from './atlas-semantic-producers';
interface Entity { id: string; kind?: string; fields: Record<string, unknown> }
interface Edge { source: string; target: string; relation: string }
function read<T>(r: AtlasResponse, id: string): T {
    const output = r.outputs.find(o => o.id === id);
    if (!output || output.error || output.schema_version !== 2 || !output.data || ['failed', 'unavailable'].includes(output.status))
        throw new Error(output?.error?.message ?? `Concept output unavailable: ${id}`);
    return output.data as T;
}
/** Primary and extra parents are both real edges: never flatten a concept DAG into a tree. */
export const conceptView: AtlasViewDefinition = {
    id: 'concepts', binding: 'selection', targetKind: 'concept',
    outputs: () => [
        { id: 'members', output: { entities: { page: { limit: 500 } } } },
        { id: 'parents', output: { graph: { max_nodes: 100, max_edges: 300, expansion: { relations: ['child_concept_of'], max_depth: 8 } } } },
        { id: 'evidence', output: { graph: { max_nodes: 500, max_edges: 500, expansion: { relations: ['member_of_concept'], incoming: true, max_depth: 1 } } } },
    ],
    project(r, c) {
        const members = read<{items: Entity[]; truncated: boolean}>(r, 'members');
        const graph = read<{nodes: Entity[]; edges: Edge[]; truncated: boolean}>(r, 'parents');
        const sources = read<{nodes: Entity[]; edges: Edge[]; truncated: boolean}>(r, 'evidence');
        const memberIds = new Set(members.items.map(e => e.id)), evidenceById = new Map(sources.nodes.map(e => [e.id, e]));
        const gaps = ['Concept labels, membership and grouping are generated interpretations, not verified business facts. Selecting a concept includes its recorded descendants up to eight levels; source previews represent one recorded member, not proof of the label.'];
        if (graph.truncated) gaps.push('Concept graph limited to 100 nodes / 300 parent edges; some structure is omitted.');
        if (members.truncated) gaps.push('Concept membership limited to 500 rows; additional graph nodes are conservatively marked context.');
        if (sources.truncated) gaps.push('Representative member evidence limited to 500 nodes / edges; some concepts have no source preview.');
        const lowConfidence = graph.nodes.filter(e => typeof e.fields.confidence === 'number' && e.fields.confidence < 0.5).length;
        const unknownConfidence = graph.nodes.filter(e => typeof e.fields.confidence !== 'number').length;
        const incomplete = graph.nodes.filter(e => e.fields.coverage_complete === false).length;
        if (lowConfidence) gaps.push(`${lowConfidence} displayed concept records have confidence below 0.5; inspect their membership before relying on the labels.`);
        if (unknownConfidence) gaps.push(`${unknownConfidence} displayed concept records have no numeric confidence recorded.`);
        if (incomplete) gaps.push(`${incomplete} displayed concept records report incomplete coverage.`);
        const nodes: DependencyView['nodes'] = [];
        for (const entity of graph.nodes) {
            const label = entity.fields.label;
            if (typeof label !== 'string' || !label.trim()) { gaps.push(`Unlabelled concept withheld: ${entity.id}`); continue; }
            const own: AtlasQuery = {select: {kind: 'concept', filter: {ids: [entity.id]}}};
            const operand: AtlasQuery = {union: [own, {follow: {input: own, relation: 'child_concept_of', incoming: true, min_depth: 1, max_depth: 8}}]};
            const ref: AtlasItemRef = {
                id: entity.id, label, pin: c.pin, entityKind: 'concept', group: 'concept', operand,
                evidence: [], provenance: [{kind: 'stored-fact', pin: c.pin, artifact: 'concepts', recordId: entity.id}],
            };
            // Only a recorded membership edge can provide representative source.
            for (const edge of sources.edges.filter(e => e.relation === 'member_of_concept' && e.target === entity.id)) {
                const location = evidenceById.get(edge.source)?.fields.location as {file?: string; start?: number; end?: number} | undefined;
                if (location?.file) {
                    ref.evidence = [{file: location.file, label: `Representative member of ${label}`, ...(location.start != null && location.end! > location.start ? {startByte: location.start, endByte: location.end} : {})}];
                    break;
                }
            }
            nodes.push({...binding(entity.id, label, ref, 'Inference', memberIds.has(entity.id) ? 'member' : 'context'), subjectKind: String(entity.fields.kind ?? 'concept')});
        }
        const byId = new Map(nodes.map(n => [n.visualId, n]));
        const seen = new Set<string>();
        const relations: DependencyView['relations'] = [];
        for (const edge of graph.edges.filter(e => e.relation === 'child_concept_of')) {
            if (!byId.has(edge.source) || !byId.has(edge.target)) continue;
            const id = `${edge.source}:child_concept_of:${edge.target}`;
            if (seen.has(id)) continue;
            seen.add(id);
            const from = byId.get(edge.source)!, to = byId.get(edge.target)!;
            const ref: AtlasItemRef = {...from.ref, id, label: `${from.label} → ${to.label}`, operand: {union: [from.ref.operand, to.ref.operand]}, evidence: [...from.ref.evidence, ...to.ref.evidence], provenance: [{kind: 'stored-fact', pin: c.pin, artifact: 'child_concept_of', recordId: id}]};
            relations.push({...binding(id, 'Recorded concept parent', ref, 'Inference'), from: edge.source, to: edge.target, relation: edge.relation});
        }
        if (!nodes.length) gaps.push('No generated concepts are recorded for this selection.');
        return semanticEnvelope({kind: 'dependency', title: 'Generated concept graph', nodes, relations, gaps,
            population: {pin: c.pin, version: c.version, universe: 'concept', expression: c.query, matches: r.matches, coverage: r.coverage ?? null}});
    },
};
