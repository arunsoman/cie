import { assertItem, samePin } from './atlas-selection';
import { semanticBindings, type SemanticViewSpec } from './atlas-semantic';
export function validateSemantic(model: SemanticViewSpec) {
    const { pin } = model.population;
    const bindings = semanticBindings(model), ids = new Set<string>();
    if (!Number.isFinite(model.population.matches) || model.population.matches < 0)
        throw new Error('Invalid population count');
    for (const b of bindings) {
        if (!b.visualId || ids.has(b.visualId))
            throw new Error('Duplicate semantic occurrence');
        ids.add(b.visualId);
        assertItem(b.ref, pin);
        if (!['Fact', 'Inference', 'Hypothesis', 'Unknown'].includes(b.authority))
            throw new Error('Invalid semantic authority');
        for (const e of b.evidence) {
            if (e.pin.repository !== pin.repository)
                throw new Error('Foreign evidence repository');
            if ((e.kind === 'source' || e.kind === 'document') && (!e.file || e.start < 0 || e.end < e.start || !Number.isSafeInteger(e.start) || !Number.isSafeInteger(e.end)))
                throw new Error('Invalid semantic source span');
        }
    }
    const endpoints = (from: string, to: string, allowed: Set<string>) => { if (!allowed.has(from) || !allowed.has(to))
        throw new Error('Dangling semantic relation'); };
    if (model.kind === 'architecture' || model.kind === 'dependency' || model.kind === 'data-flow') {
        const nodes = new Set(model.nodes.map(n => n.visualId));
        model.relations.forEach(e => endpoints(e.from, e.to, nodes));
    }
    if (model.kind === 'architecture') {
        if (!['container', 'component'].includes(model.level)) throw new Error('Invalid C4 level');
        for (const node of model.nodes) {
            if (!['system', 'container', 'component'].includes(node.boundaryRole) || (model.level === 'container' && node.boundaryRole === 'component'))
                throw new Error('C4 node role does not match its level');
        }
    }
    if (model.kind === 'hierarchy') {
        const nodes = new Map(model.nodes.map(n => [n.visualId, n]));
        for (const n of model.nodes) {
            const seen = new Set([n.visualId]);
            let parent = n.parentId;
            while (parent) {
                if (!nodes.has(parent))
                    throw new Error('Dangling hierarchy parent');
                if (seen.has(parent))
                    throw new Error('Cyclic hierarchy');
                seen.add(parent);
                parent = nodes.get(parent)!.parentId;
            }
        }
        if (model.roots.some(id => !nodes.has(id) || nodes.get(id)!.parentId))
            throw new Error('Invalid hierarchy root');
    }
    if (model.kind === 'control-flow') {
        const blocks = new Set(model.blocks.map(b => b.visualId));
        if (model.entry != null && !blocks.has(model.entry))
            throw new Error('Invalid flow entry');
        model.branches.forEach(e => endpoints(e.from, e.to, blocks));
    }
    if (model.kind === 'sequence') {
        const participants = new Set(model.participants.map(p => p.visualId));
        let previous = -1;
        for (const m of model.messages) {
            endpoints(m.from, m.to, participants);
            if (!Number.isSafeInteger(m.order) || m.order <= previous)
                throw new Error('Message occurrence order must be explicit and increasing');
            previous = m.order;
        }
        for (const f of model.fragments)
            if (!f.evidence.length || f.messages.some(id => !model.messages.some(m => m.visualId === id)))
                throw new Error('Unsupported sequence fragment');
    }
    if (model.kind === 'table') {
        for (const row of model.rows)
            for (const [column, cell] of Object.entries(row.cellBindings ?? {})) {
                if (!model.columns.some(c => c.id === column))
                    throw new Error('Semantic cell binding must name a declared column');
                if (cell.authority !== row.authority)
                    throw new Error('Table cells must share their row authority');
            }
    }
    if (model.kind === 'metric') {
        for (const c of model.cells)
            for (const value of Object.values(c.measures))
                if (value != null && !Number.isFinite(value))
                    throw new Error('Non-finite semantic measure');
    }
}
