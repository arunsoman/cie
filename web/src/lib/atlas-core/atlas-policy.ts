import type { AtlasViewSpec, AtlasEvidencePolicy } from './atlas-types';
import type { SemanticViewSpec } from './atlas-semantic';
function filterModel(model: SemanticViewSpec, allowed: Set<string>): SemanticViewSpec {
    const keep = (b: {
        visualId: string;
    }) => allowed.has(b.visualId);
    switch (model.kind) {
        case 'architecture': {
            const nodes = model.nodes.filter(keep), ids = new Set(nodes.map(n => n.visualId));
            return { ...model, nodes, relations: model.relations.filter(e => keep(e) && ids.has(e.from) && ids.has(e.to)) };
        }
        case 'dependency': {
            const nodes = model.nodes.filter(keep), ids = new Set(nodes.map(n => n.visualId));
            return { ...model, nodes, relations: model.relations.filter(e => keep(e) && ids.has(e.from) && ids.has(e.to)) };
        }
        case 'data-flow': {
            const nodes = model.nodes.filter(keep), ids = new Set(nodes.map(n => n.visualId));
            return { ...model, nodes, relations: model.relations.filter(e => keep(e) && ids.has(e.from) && ids.has(e.to)) };
        }
        case 'hierarchy': {
            const nodes = model.nodes.filter(keep), ids = new Set(nodes.map(n => n.visualId));
            const visible = nodes.map(n => ({ ...n, parentId: n.parentId && ids.has(n.parentId) ? n.parentId : undefined }));
            return { ...model, nodes: visible, roots: visible.filter(n => !n.parentId).map(n => n.visualId) };
        }
        case 'control-flow': {
            const blocks = model.blocks.filter(keep).map(b => ({ ...b, statements: b.statements.filter(keep) })), ids = new Set(blocks.map(b => b.visualId));
            return { ...model, entry: model.entry && ids.has(model.entry) ? model.entry : null, blocks, branches: model.branches.filter(e => keep(e) && ids.has(e.from) && ids.has(e.to)) };
        }
        case 'sequence': {
            const participants = model.participants.filter(keep), ids = new Set(participants.map(p => p.visualId));
            return { ...model, participants, messages: model.messages.filter(m => keep(m) && ids.has(m.from) && ids.has(m.to)), fragments: [] };
        }
        case 'metric': return { ...model, cells: model.cells.filter(keep) };
        case 'table': return { ...model, rows: model.rows.filter(keep) };
    }
}
/** Facts-only is enforced before rendering; hidden semantic data cannot leak back through a canvas. */
export function applyEvidencePolicy(spec: AtlasViewSpec, policy: AtlasEvidencePolicy): AtlasViewSpec {
    if (!policy.factsOnly)
        return spec;
    const items = spec.items.filter(i => i.authority === 'Fact'), ids = new Set(items.map(i => i.key));
    const links = spec.links?.filter(e => e.authority === 'Fact' && ids.has(e.from) && ids.has(e.to));
    const hidden = spec.items.length - items.length + (spec.links?.length ?? 0) - (links?.length ?? 0);
    const gaps = [...spec.gaps, ...(hidden ? [`${hidden} elements withheld by Facts-only policy; population counts are unchanged.`] : [])];
    const semantic = spec.semantic ? { ...filterModel(spec.semantic, new Set([...ids, ...(links ?? []).map(e => e.key)])), gaps } : undefined;
    return { ...spec, items, links, semantic, gaps };
}
