import type { AtlasViewDefinition, AtlasResponse, AtlasItemRef, ProjectionContext, AtlasViewSpec, AtlasDatum, AtlasLink } from './atlas-types';
import { binding, type SemanticViewSpec, type SemanticNode, type FlowBlock, type ControlFlowView, type HierarchyView, type SequenceView, type SequenceMessage, type SequenceParticipant } from './atlas-semantic';
interface Entity {
    id: string;
    fields: Record<string, unknown>;
}
function read<T>(r: AtlasResponse, id: string): T { const o = r.outputs.find(o => o.id === id); if (!o || o.schema_version !== 2 || o.error || !o.data || ['failed', 'unavailable'].includes(o.status))
    throw new Error(o?.error?.message ?? `Missing captured ${id}`); return o.data as T; }
function population(r: AtlasResponse, c: ProjectionContext) { return { pin: c.pin, version: c.version, universe: c.universe ?? 'symbol' as const, expression: c.query, matches: r.matches, coverage: r.coverage ?? null }; }
function symbolRef(e: Entity, c: ProjectionContext): AtlasItemRef { const loc = e.fields.location as {
    file?: string;
    start?: number;
    end?: number;
} | undefined; return { id: e.id, label: String(e.fields.qualified_name ?? e.fields.name ?? e.id), pin: c.pin, entityKind: 'symbol', group: 'symbol', operand: { select: { kind: 'symbol', filter: { ids: [e.id] } } }, evidence: loc?.file ? [{ file: loc.file, ...(loc.start != null && loc.end! > loc.start ? { startByte: loc.start, endByte: loc.end } : {}) }] : [] }; }
/** Outline is a projection of the model, never the source of semantic structure. */
export function semanticEnvelope(model: SemanticViewSpec): AtlasViewSpec {
    let nodes: ReturnType<typeof binding>[] = [], relations: ({
        from: string;
        to: string;
    } & ReturnType<typeof binding>)[] = [];
    switch (model.kind) {
        case 'dependency':
        case 'data-flow':
            nodes = model.nodes;
            relations = model.relations;
            break;
        case 'hierarchy':
            nodes = model.nodes;
            break;
        case 'control-flow':
            nodes = [...model.blocks, ...model.blocks.flatMap(b => b.statements)];
            relations = model.branches;
            break;
        case 'sequence':
            nodes = [...model.participants, ...model.messages];
            break;
        case 'metric':
            nodes = model.cells;
            break;
        case 'table':
            nodes = model.rows;
            break;
    }
    const items: AtlasDatum[] = nodes.map(n => ({ key: n.visualId, label: n.label, ref: n.ref, authority: n.authority, role: n.membership }));
    const links: AtlasLink[] = relations.map(e => ({ key: e.visualId, from: e.from, to: e.to, label: e.label, ref: e.ref, authority: e.authority }));
    return { kind: model.kind, semantic: model, title: model.title, items, links, total: model.population.matches, continuation: null, gaps: model.gaps };
}
export const hierarchyView: AtlasViewDefinition = { id: 'hierarchy', binding: 'selection', outputs: () => [{ id: 'rows', output: { entities: { page: { limit: 100 } } } }],
    project(r, c) {
        const rows = read<{
            items: Entity[];
            truncated: boolean;
        }>(r, 'rows');
        const nodes: SemanticNode[] = [], files = new Map<string, string>(), byNumeric = new Map(rows.items.map(e => [Number(e.fields.id), e.id]));
        for (const e of rows.items) {
            const ref = symbolRef(e, c), file = ref.evidence[0]?.file;
            if (file && !files.has(file)) {
                const id = `file:${file}`, fileRef: AtlasItemRef = { id, label: file, pin: c.pin, group: 'file.path', operand: { select: { kind: 'symbol', filter: { field: { field: 'file.path', op: 'eq', value: file } } } }, evidence: [{ file }], provenance: [{ kind: 'stored-fact', pin: c.pin, artifact: 'defined_in', recordId: file }] };
                files.set(file, id);
                nodes.push({ ...binding(id, file, fileRef, 'Fact', 'context'), subjectKind: 'file' });
            }
        }
        for (const e of rows.items) {
            const ref = symbolRef(e, c), parent = e.fields.parent == null ? undefined : byNumeric.get(Number(e.fields.parent));
            nodes.push({ ...binding(e.id, ref.label, ref, 'Fact'), parentId: parent ?? files.get(ref.evidence[0]?.file ?? ''), subjectKind: String(e.fields.kind ?? 'symbol') });
        }
        const model: HierarchyView = { kind: 'hierarchy', population: population(r, c), title: 'Recorded file and symbol hierarchy', nodes, roots: nodes.filter(n => !n.parentId).map(n => n.visualId), gaps: rows.truncated ? ['Hierarchy limited to 100 selected symbols; omitted parents are not inferred.'] : [] };
        return semanticEnvelope(model);
    } };
interface RawIr {
    symbol: Entity;
    blocks: {
        id: number;
        block_index: number;
        kind: string;
    }[];
    instructions: {
        block_id: number;
        inst_index: number;
        op: string;
        span_start: number;
        span_end: number;
    }[];
    edges: {
        from_block_id: number;
        to_block_id: number;
        kind: string;
    }[];
}
export const controlFlowView: AtlasViewDefinition = { id: 'controlflow', binding: 'selection', outputs: () => [
        { id: 'one', output: { entities: { page: { limit: 2 } } } }, { id: 'ir', output: { detail: { function_ir: { include_calls: true } } } }, { id: 'source', output: { detail: { source: { range: 'declaration' } } } },
    ], project(r, c) {
        const empty = (why: string) => semanticEnvelope({ kind: 'control-flow', population: population(r, c), title: 'Captured control flow', entry: null, blocks: [], branches: [], gaps: [why] });
        const selected = read<{
            items: Entity[];
            total: number;
        }>(r, 'one');
        if (selected.total !== 1)
            return empty('Select one captured function to inspect its actual control flow.');
        if (!['Function', 'Method', 'Test'].includes(String(selected.items[0].fields.kind)))
            return empty('The selected entity is not a function.');
        let ir: RawIr, source: {
            text: string;
            returned: {
                start: number;
                end: number;
            };
            truncated: boolean;
        };
        try {
            ir = read(r, 'ir');
            source = read(r, 'source');
        }
        catch (e) {
            return empty(`Captured function IR/source unavailable: ${String(e)}`);
        }
        const root = symbolRef(ir.symbol, c), bytes = new TextEncoder().encode(source.text), decode = new TextDecoder();
        const spanRef = (a: number, b: number): AtlasItemRef => a >= source.returned.start && b <= source.returned.end && b > a ? { ...root, evidence: [{ file: root.evidence[0].file, startByte: a, endByte: b }] } : { ...root, evidence: [], provenance: [{ kind: 'stored-fact', pin: c.pin, artifact: 'function_ir', recordId: `${root.id}:${a}:${b}` }] };
        const blockId = (id: number) => `block:${root.id}:${id}`;
        const blocks: FlowBlock[] = ir.blocks.map(b => { const instructions = ir.instructions.filter(i => i.block_id === b.id); const statements = instructions.map(i => { const ref = spanRef(i.span_start, i.span_end), text = i.span_start >= source.returned.start && i.span_end <= source.returned.end ? decode.decode(bytes.slice(i.span_start - source.returned.start, i.span_end - source.returned.start)).trim() : ''; return { ...binding(`statement:${root.id}:${b.id}:${i.inst_index}`, text || i.op, ref, 'Fact'), operation: i.op, sourceText: text }; }); return { ...binding(blockId(b.id), b.kind === 'entry' ? 'Start' : b.kind === 'exit' ? 'End' : `Block ${b.block_index}`, root, 'Fact'), blockKind: instructions.some(i => i.op === 'Cmp') ? 'decision' : b.kind === 'entry' ? 'entry' : b.kind === 'exit' ? 'exit' : 'basic', statements }; });
        const model: ControlFlowView = { kind: 'control-flow', population: population(r, c), title: `Control flow — ${root.label}`, entry: ir.blocks.some(b => b.kind === 'entry') ? blockId(ir.blocks.find(b => b.kind === 'entry')!.id) : null, blocks,
            branches: ir.edges.map((e, i) => ({ ...binding(`branch:${root.id}:${i}`, e.kind, { ...root, provenance: [{ kind: 'stored-fact', pin: c.pin, artifact: 'function_ir', recordId: `${root.id}:edge:${i}` }] }, 'Fact'), from: blockId(e.from_block_id), to: blockId(e.to_block_id), relation: e.kind, branch: e.kind === 'branch_true' ? 'true' : e.kind === 'branch_false' ? 'false' : e.kind === 'back_edge' ? 'back' : e.kind === 'jump' ? 'jump' : e.kind === 'exception' ? 'exception' : 'unknown' })),
            gaps: ['Captured static IR; unsupported language constructs, exception paths and runtime dispatch may be incomplete.', ...(source.truncated ? ['Captured source response is truncated; missing text is not reconstructed.'] : [])] };
        return semanticEnvelope(model);
    } };
interface Site {
    callee: string;
    target_symbols?: number[];
    source_order: number;
    guard?: string;
    dispatch?: string;
    receiver_type?: string;
    span: {
        start: number;
        end: number;
    };
}
interface SummaryEntity extends Entity {
    fields: Record<string, unknown> & {
        sym: number;
        symbol_ref: string;
        call_sites?: Site[];
    };
}
export const sequenceView: AtlasViewDefinition = { id: 'sequence', binding: 'selection', targetKind: 'summary', expand: { relation: 'declared_calls', maxDepth: 8 }, outputs: () => [
        { id: 'summaries', output: { entities: { page: { limit: 100 } } } }, { id: 'symbols', output: { graph: { max_nodes: 500, max_edges: 500, expansion: { relations: ['summarizes'], max_depth: 1 } } } },
    ], project(r, c) {
        const rows = read<{
            items: SummaryEntity[];
            truncated: boolean;
        }>(r, 'summaries'), graph = read<{
            nodes: Entity[];
            truncated: boolean;
        }>(r, 'symbols'), byId = new Map(graph.nodes.map(e => [e.id, e]));
        const sums = new Map(rows.items.map(e => [e.fields.sym, e]));
        const called = new Set(rows.items.flatMap(e => (e.fields.call_sites ?? []).flatMap(s => s.target_symbols ?? [])));
        const explicit = c.subjects?.filter(s => s.group === 'symbol' && rows.items.some(e => e.fields.symbol_ref === s.id)) ?? [];
        const roots = explicit.length === 1 ? rows.items.filter(e => e.fields.symbol_ref === explicit[0].id) : rows.items.filter(e => !called.has(e.fields.sym));
        const participants: SequenceParticipant[] = [], messages: SequenceMessage[] = [], known = new Set<string>(), gaps: string[] = [];
        const actor = (summary: SummaryEntity | undefined, boundary?: {
            label: string;
            ref: AtlasItemRef;
            key: string;
        }) => {
            const entity = summary ? byId.get(summary.fields.symbol_ref) : undefined;
            const ref = entity ? symbolRef(entity, c) : boundary?.ref ?? { id: summary!.fields.symbol_ref, pin: c.pin, label: summary!.fields.symbol_ref, group: 'symbol', operand: { select: { kind: 'symbol', filter: { ids: [summary!.fields.symbol_ref] } } }, evidence: [], provenance: [{ kind: 'stored-fact', pin: c.pin, artifact: 'summary', recordId: summary!.id }] } as AtlasItemRef;
            const id = summary ? `participant:${summary.fields.symbol_ref}` : boundary!.key;
            if (!known.has(id)) {
                known.add(id);
                participants.push({ ...binding(id, entity ? ref.label : boundary?.label ?? ref.label, ref, entity ? 'Fact' : 'Unknown', summary && roots.some(root => root.fields.sym === summary.fields.sym) ? 'member' : 'context'), participantKind: entity ? 'function' : 'unresolved' });
            }
            return { id, ref };
        };
        const visit = (summary: SummaryEntity, path: Set<number>, depth: number) => {
            const from = actor(summary);
            const sites = summary.fields.call_sites;
            if (!sites) {
                gaps.push('Legacy summary has no call occurrences.');
                return;
            }
            for (const site of [...sites].sort((a, b) => a.source_order - b.source_order)) {
                for (const targetId of site.target_symbols?.length ? site.target_symbols : [undefined]) {
                    if (messages.length >= 150) {
                        gaps.push('Sequence capped at 150 message occurrences.');
                        return;
                    }
                    const target = targetId == null ? undefined : sums.get(targetId), evidence = from.ref.evidence[0];
                    const callRef: AtlasItemRef = { ...from.ref, evidence: evidence && site.span.end > site.span.start ? [{ file: evidence.file, startByte: site.span.start, endByte: site.span.end }] : from.ref.evidence };
                    const to = target ? actor(target) : actor(undefined, { label: site.receiver_type ?? site.callee, ref: callRef, key: `boundary:${summary.fields.symbol_ref}:${site.callee}` });
                    const recursive = target ? path.has(target.fields.sym) : false;
                    messages.push({ ...binding(`message:${summary.fields.symbol_ref}:${site.source_order}:${messages.length}`, site.callee, target ? to.ref : callRef, site.dispatch === 'exact' ? 'Fact' : 'Inference'), from: from.id, to: to.id, order: messages.length, dispatch: site.dispatch === 'exact' ? 'exact' : site.dispatch === 'candidate' ? 'candidate' : site.dispatch === 'external' ? 'external' : 'unknown', guard: site.guard, recursive, callSite: callRef.evidence[0] ? { kind: 'source', pin: c.pin, file: callRef.evidence[0].file, start: site.span.start, end: site.span.end } : undefined });
                    if (target && !recursive) {
                        if (depth < 8)
                            visit(target, new Set([...path, target.fields.sym]), depth + 1);
                        else
                            gaps.push('Sequence depth capped at eight declared calls.');
                    }
                }
            }
        };
        if (roots.length === 1)
            visit(roots[0], new Set([roots[0].fields.sym]), 0);
        else
            gaps.push('Select one function to establish an unambiguous sequence entry.');
        if (rows.truncated || graph.truncated)
            gaps.push('Recorded summary/symbol closure is bounded; missing calls remain boundaries.');
        gaps.push('Static call occurrences, not a runtime trace. Guards are recorded; asynchronous scheduling and unrecorded fragments are not inferred.');
        const model: SequenceView = { kind: 'sequence', population: population(r, c), title: roots.length === 1 ? `Static sequence — ${String(byId.get(roots[0].fields.symbol_ref)?.fields.qualified_name ?? 'selected function')}` : 'Static call sequence', participants, messages, fragments: [], gaps: [...new Set(gaps)] };
        return semanticEnvelope(model);
    } };
