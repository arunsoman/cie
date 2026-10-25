import { gate, recomputeRenderHash, recomputeSemanticHash, type ViewSpec, type ViewElement, type ViewEdge, type EvidenceRef } from 'viz-adapter';
import type { AtlasPin, AtlasViewSpec } from './atlas-types';
import { binding, semanticProjection, semanticBindings, type SemanticViewSpec, type VisualBinding } from './atlas-semantic';
import { validateSemantic } from './atlas-semantic-validation';
import { planPresentation, type PresentationSpec, type PresentationProfile } from './atlas-presentation';
/** Semantics→sealed render contract. Geometry remains in the existing adapter host. */
export function compileSemanticView(model: SemanticViewSpec, request?: PresentationSpec): {
    view: ViewSpec;
    profile: PresentationProfile;
} {
    validateSemantic(model);
    const profile = planPresentation(model, request);
    if (profile.host !== 'viz-adapter')
        throw new Error(`${model.kind} uses the ${profile.host} host, not a graph conversion`);
    const elements: ViewElement[] = [], edges: ViewEdge[] = [], hidden: ViewSpec['hidden'] = [];
    const evidence = (b: VisualBinding): EvidenceRef | undefined => {
        const e = b.evidence[0];
        if (!e)
            return undefined;
        switch (e.kind) {
            case 'source':
            case 'document': return { file: e.file, start: e.start, end: e.end };
            case 'aggregate': return { kind: e.kind, repository: e.pin.repository, revision: e.pin.revision, descriptor: e.descriptor };
            case 'stored-fact': return { kind: e.kind, repository: e.pin.repository, revision: e.pin.revision, artifact: e.artifact, recordId: e.recordId };
            case 'runtime': return { kind: e.kind, repository: e.pin.repository, revision: e.pin.revision, runId: e.runId, sampleId: e.sampleId };
        }
    };
    const mode = (b: VisualBinding) => b.authority === 'Unknown' ? 'Fog' as const : b.authority;
    const add = (b: VisualBinding, kind: string, fields: Record<string, unknown> = {}, parentId?: string) => {
        const ev = evidence(b);
        if (!ev) {
            hidden.push({ id: b.visualId, label: b.label, kind, reason: 'Producer supplied no evidence; available in accessible outline', salienceWeight: 1 });
            return;
        }
        const displayMode = mode(b);
        elements.push({ id: b.visualId, elementKind: kind, label: b.label, evidence: ev, displayMode, parentId,
            ...(displayMode !== 'Fact' ? { claimId: `atlas:${b.visualId}`, counterArgument: 'Recorded static inference; execution is unobserved.', fogReason: 'Producer authority is unknown.' } : {}),
            salience: { factors: ['population'], weight: b.membership === 'member' ? 1 : 0.5, whyShown: b.membership }, fields: { ...fields, atlasItemId: b.ref.id } });
    };
    const link = (b: VisualBinding, from: string, to: string, kind: string) => { const ev = evidence(b); if (!ev || !elements.some(e => e.id === from) || !elements.some(e => e.id === to))
        return; edges.push({ id: b.visualId, source: from, target: to, edgeKind: kind, displayMode: mode(b), evidence: ev, fields: { atlasItemId: b.ref.id } }); };
    switch (model.kind) {
        case 'dependency':
        case 'data-flow':
            model.nodes.forEach(n => add(n, 'symbol', {}, n.parentId));
            model.relations.forEach(e => link(e, e.from, e.to, e.relation));
            break;
        case 'hierarchy':
            model.nodes.forEach(n => add(n, 'symbol', {}, n.parentId));
            for (const n of model.nodes)
                if (n.parentId) {
                    const edge = { ...n, visualId: `contains:${n.parentId}:${n.visualId}` };
                    link(edge, n.parentId, n.visualId, 'contains');
                }
            break;
        case 'control-flow':
            model.blocks.forEach(b => add({ ...b, label: b.statements.length ? b.statements.map(s => s.sourceText || s.operation).join('\n') : b.label }, b.blockKind === 'decision' ? 'decision' : b.blockKind === 'entry' ? 'start' : b.blockKind === 'exit' ? 'end' : 'process', { isEntry: b.visualId === model.entry, blockKind: b.blockKind, statements: b.statements }));
            model.branches.forEach(e => link(e, e.from, e.to, e.branch === 'true' ? 'Yes' : e.branch === 'false' ? 'No' : e.branch));
            break;
        case 'sequence': {
            const participants = new Map(model.participants.map(p => [p.visualId, p]));
            // Each message is one selectable visual occurrence; repeated/self calls survive.
            for (const m of model.messages)
                add(m, 'message', { sequence: { from: m.from, to: m.to, fromLabel: participants.get(m.from)!.label, toLabel: participants.get(m.to)!.label, msg: [m.label, m.guard && m.guard !== 'unconditional' ? `[${m.guard}]` : '', m.dispatch === 'candidate' ? '[declared target; runtime unobserved]' : '', m.recursive ? '[recursion]' : ''].filter(Boolean).join(' '), order: m.order, guard: m.guard, dispatch: m.dispatch, recursive: m.recursive }, fragments: model.fragments.filter(f => f.messages.includes(m.visualId)) });
            break;
        }
    }
    const revision = Number(model.population.pin.revision);
    if (!Number.isSafeInteger(revision))
        throw new Error('Current visualization adapter requires a numeric captured revision');
    const coverage = model.population.coverage as {
        complete?: boolean;
        ratio?: number;
    } | null;
    const view: ViewSpec = { elements, edges, hidden, gaps: [...model.gaps.map((why, i) => ({ id: `gap:${i}`, area: 'coverage', why })), ...(hidden.length ? [{ id: 'missing-evidence', area: 'evidence', why: `${hidden.length} elements lack evidence` }] : [])],
        footer: { facts: elements.filter(e => e.displayMode === 'Fact').length, inferences: elements.filter(e => e.displayMode === 'Inference').length, hypotheses: elements.filter(e => e.displayMode === 'Hypothesis').length, fog: elements.filter(e => e.displayMode === 'Fog').length, stale: 0, caption: `${model.title} · ${model.gaps.map((_, i) => `gap:${i}`).join(', ')}` },
        preferences: { minimap: request?.features?.minimap ?? false, fisheye: request?.features?.fisheye ? { trigger: 'pointermove' } : undefined }, scope: { revision, environments: [], coverage: { analyzed: coverage?.complete === true, ratio: coverage?.ratio } }, layoutHint: { strategy: profile.layout.family === 'sequence' ? 'sequence-lanes' : profile.layout.family === 'tree' ? 'compact-box' : profile.layout.family === 'grid' ? 'grid' : 'dagre', params: { rankdir: profile.layout.direction } }, semanticHash: '', renderHash: '' };
    view.semanticHash = recomputeSemanticHash(view);
    view.renderHash = recomputeRenderHash(view, '');
    gate(view);
    return { view, profile };
}
/** Legacy envelopes are accepted only for dependency graphs, tables and metrics. */
export function atlasVisualization(spec: AtlasViewSpec, pin: AtlasPin): ViewSpec {
    const model = spec.semantic ?? semanticProjection(spec, { pin, version: 0, query: { select: { kind: 'symbol' } } }, { repository: pin.repository, revision: pin.revision, matches: spec.total, outputs: [] });
    return compileSemanticView(model, spec.presentation).view;
}
