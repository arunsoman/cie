import { semanticProjection } from './atlas-semantic';
import { planPresentation } from './atlas-presentation';
import { defaultWorkspace, type AtlasSession } from './atlas-workspace';
import { searchQuery } from './atlas-navigation';
import { foundationDimensions, type AtlasDimensionRegistry } from './atlas-dimensions';
import type { AtlasPlanRegistry } from './atlas-plans';
import { mapPopulation, itemPopulation, type EntityKind } from './atlas-populations';
import { standardAtlasRegistry, type AtlasRegistry } from './atlas-registry';
import { applyEvidencePolicy } from './atlas-policy';
import type { AtlasPin, AtlasQuery, AtlasStorage, AtlasViewDefinition, AtlasState, AtlasCommand, AtlasViewState, AtlasItemRef } from './atlas-types';
import { assertItem, initialSelection, samePin, selectionQuery, toggle, setOperation, validateQuery } from './atlas-selection';
import { assertDefinition, assertView } from './atlas-conformance';
export * from './atlas-types';
/** One instance per repository revision. Persona switches never recreate it. */
export function createAtlasCore(options: {
    pin: AtlasPin;
    base: AtlasQuery;
    storage: AtlasStorage;
    views: AtlasViewDefinition[];
    persona: string;
    registry?: AtlasRegistry;
    universe?: EntityKind;
    plans?: AtlasPlanRegistry;
    dimensions?: AtlasDimensionRegistry;
}) {
    validateQuery(options.base);
    let storage = options.storage;
    const dimensions = options.dimensions ?? foundationDimensions;
    const registry = options.registry ?? standardAtlasRegistry(options.views);
    const universe = options.universe ?? ('select' in options.base ? options.base.select.kind as EntityKind : 'symbol');
    const definitions = new Map<string, AtlasViewDefinition>();
    for (const definition of options.views) {
        assertDefinition(definition);
        if (definitions.has(definition.id))
            throw new Error('Duplicate Atlas view');
        definitions.set(definition.id, definition);
    }
    // Own every item and state snapshot: callers cannot mutate identity or selection indirectly.
    const copy = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
    const freeze = <T,>(value: T): T => { if (value && typeof value === 'object') {
        Object.values(value).forEach(freeze);
        Object.freeze(value);
    } return value; };
    const base = freeze(copy(options.base));
    let state: AtlasState = freeze({ pin: copy(options.pin), persona: options.persona, version: 0, workspace: defaultWorkspace(), search: { status: 'idle', items: [], total: 0 }, freshness: { status: 'unknown', indexedRevision: options.pin.revision, reason: 'Index metadata has not been supplied.' }, policy: { factsOnly: false }, selection: initialSelection(), views: {}, evidence: { status: 'closed' } });
    const listeners = new Set<() => void>(), pages = new Map<string, (string | null)[]>();
    let disposed = false, controller: AbortController | undefined, evidenceController: AbortController | undefined;
    let epoch = 0, evidenceEpoch = 0, searchEpoch = 0;
    let searchController: AbortController | undefined;
    const publish = (next: AtlasState) => { if (disposed)
        return; state = freeze(next); listeners.forEach(l => l()); };
    const selected = (id: string) => state.selection.contributions.some(c => c.item.id === id);
    async function refresh(reset = false) {
        if (disposed)
            return;
        controller?.abort();
        controller = new AbortController();
        const signal = controller.signal, requestEpoch = ++epoch;
        if (reset)
            pages.clear();
        const version = state.version, views = { ...state.views };
        definitions.forEach(v => { views[v.id] = { ...views[v.id], binding: v.binding, status: 'loading', version, page: pages.get(v.id)?.length ? pages.get(v.id)!.length - 1 : 0 }; });
        publish({ ...state, views });
        // Each binding uses ONE storage request for all its projections. UI commits
        // the entire result batch together so counts/tables cannot mix query versions.
        let query: AtlasQuery, replayQuery: AtlasQuery;
        try {
            const scope = state.workspace.breadcrumbs.slice(-1)[0];
            let population = scope ? { intersect: [base, mapPopulation(scope.query, scope.kind, universe)] } as AtlasQuery : base;
            if (state.changeContext) {
                const files: AtlasQuery = { select: { kind: 'file', filter: { field: { field: 'path', op: 'in', value: state.changeContext.changedFiles } } } };
                population = { intersect: [population, mapPopulation(files, 'file', universe)] };
            }
            query = selectionQuery(population, state.selection, item => itemPopulation(item, universe));
            replayQuery = selectionQuery(population, state.selection, item => itemPopulation({ ...item, operand: item.replayOperand ?? item.operand }, universe));
        }
        catch (error) {
            publish({ ...state, views: Object.fromEntries(Object.entries(views).map(([id, v]) => [id, { ...v, status: 'error', error: String(error) }])) });
            return;
        }
        const groups = new Map<string, AtlasViewDefinition[]>();
        for (const definition of definitions.values()) {
            const key = JSON.stringify([definition.binding, definition.targetKind ?? universe, definition.expand ?? null, definition.filter ?? null]);
            const group = groups.get(key) ?? [];
            group.push(definition);
            groups.set(key, group);
        }
        const batches = await Promise.all([...groups.values()].map(async (defs) => {
            const binding = defs[0].binding;
            try {
                const source = binding === 'repository' ? base : query;
                const expanded = defs[0].expand ? { union: [source, { follow: { input: source, relation: defs[0].expand.relation, incoming: defs[0].expand.incoming ?? false, max_depth: defs[0].expand.maxDepth, min_depth: 1 } }] } as AtlasQuery : source;
                const mapped = mapPopulation(expanded, universe, defs[0].targetKind ?? universe);
                const boundQuery: AtlasQuery = defs[0].filter ? {intersect: [mapped, {select: {kind: defs[0].targetKind ?? universe, filter: defs[0].filter}}]} : mapped;
                const outputs = defs.flatMap(d => d.outputs(pages.get(d.id)?.slice(-1)[0] ?? null, { pin: state.pin, query: boundQuery, version, controls: state.workspace.charts[d.id] }).map(o => ({ id: `${d.id}__${o.id}`, output: o.output })));
                const result = await storage.retrieve(boundQuery, outputs, signal);
                if (!samePin(state.pin, { repository: result.repository, revision: result.revision }))
                    throw new Error('Storage returned a mixed repository/revision');
                return defs.map(d => {
                    try {
                        const local = { ...result, outputs: result.outputs.filter(o => o.id.startsWith(`${d.id}__`)).map(o => ({ ...o, id: o.id.slice(d.id.length + 2) })) };
                        const context = { pin: state.pin, query: boundQuery, version, controls: state.workspace.charts[d.id], universe: d.targetKind ?? universe, replayQuery: binding === 'repository' ? base : replayQuery, subjects: state.selection.contributions.filter(s => s.operation !== 'exclude').map(s => s.item) };
                        const projected = d.project(local, context);
                        const semantic = projected.semantic ?? semanticProjection(projected, context, local);
                        const raw = { ...projected, semantic, presentation: state.workspace.presentations?.[d.id] ?? projected.presentation };
                        assertView(raw, state.pin);
                        registry.renderer(raw);
                        planPresentation(semantic, raw.presentation);
                        const spec = applyEvidencePolicy({ ...raw, coverage: raw.coverage ?? result.coverage }, state.policy);
                        return [d.id, { binding, status: 'ready', version, page: pages.get(d.id)?.length ? pages.get(d.id)!.length - 1 : 0, population: spec.semantic?.population, spec }] as [
                            string,
                            AtlasViewState
                        ];
                    }
                    catch (e) {
                        return [d.id, { ...views[d.id], status: 'error', error: String(e) }] as [
                            string,
                            AtlasViewState
                        ];
                    }
                });
            }
            catch (e) {
                return defs.map(d => [d.id, { ...views[d.id], status: 'error', error: String(e) }] as [
                    string,
                    AtlasViewState
                ]);
            }
        }));
        if (disposed || signal.aborted || requestEpoch !== epoch)
            return;
        publish({ ...state, views: Object.fromEntries(batches.flat()) });
    }
    async function openEvidence(item: AtlasItemRef, index?: number) {
        assertItem(item, state.pin);
        evidenceController?.abort();
        const requestEpoch = ++evidenceEpoch;
        if (!item.evidence.length) {
            publish({ ...state, evidence: { status: 'unavailable', item, error: 'No captured source or evidence location for this item.' } });
            return;
        }
        if (index == null && item.evidence.length > 1) {
            publish({ ...state, evidence: { status: 'choose', item } });
            return;
        }
        const evidence = item.evidence[index ?? 0];
        if (!evidence) {
            publish({ ...state, evidence: { status: 'unavailable', item, error: 'Invalid evidence choice' } });
            return;
        }
        evidenceController = new AbortController();
        const signal = evidenceController.signal;
        publish({ ...state, evidence: { status: 'loading', item } });
        try {
            const source = await storage.source(evidence, signal);
            if (source.revision !== (evidence.pin ?? state.pin).revision)
                throw new Error('Captured source belongs to another revision');
            if (source.repository && source.repository !== state.pin.repository)
                throw new Error('Captured source belongs to another repository');
            if (source.path !== evidence.file)
                throw new Error('Captured source file does not match requested evidence');
            if (evidence.contentHash && source.content_hash !== evidence.contentHash)
                throw new Error('Captured source hash mismatch');
            if (!source.returned || !Number.isSafeInteger(source.returned.start) || !Number.isSafeInteger(source.returned.end) || source.returned.start < 0 || source.returned.end < source.returned.start)
                throw new Error('Invalid captured source range');
            if (evidence.startByte != null && (source.returned.start > evidence.startByte || (!source.truncated && source.returned.end < evidence.endByte!)))
                throw new Error('Captured source does not cover requested evidence');
            if (!signal.aborted && !disposed && requestEpoch === evidenceEpoch)
                publish({ ...state, evidence: { status: 'ready', item, source, activeLocation: evidence } });
        }
        catch (e) {
            if (!signal.aborted && !disposed && requestEpoch === evidenceEpoch)
                publish({ ...state, evidence: { status: 'unavailable', item, error: String(e) } });
        }
    }
    async function runSearch(spec: import('./atlas-navigation').AtlasSearch, continuation?: string | null) {
        searchController?.abort();
        searchController = new AbortController();
        const signal = searchController.signal, id = ++searchEpoch;
        publish({ ...state, search: { status: 'loading', spec, items: [], total: 0 } });
        try {
            const q = searchQuery(spec), result = await storage.retrieve(q, [{ id: 'rows', output: { entities: { page: { limit: 30, continuation } } } }], signal);
            if (!samePin(state.pin, { repository: result.repository, revision: result.revision }))
                throw new Error('Mixed search revision');
            const output = result.outputs.find(o => o.id === 'rows');
            if (!output || output.schema_version !== 2 || output.error || !output.data)
                throw new Error(output?.error?.message ?? 'Search unavailable');
            const page = output.data as {
                items: {
                    id: string;
                    fields: Record<string, unknown>;
                }[];
                total: number;
                continuation: string | null;
            };
            const items = page.items.map(e => {
                const loc = e.fields.location as {
                    file?: string;
                    start?: number;
                    end?: number;
                } | undefined;
                const file = loc?.file ?? (spec.kind === 'file' ? String(e.fields.path) : undefined);
                const item: AtlasItemRef = { id: e.id, pin: state.pin, label: String(e.fields.qualified_name ?? e.fields.name ?? e.fields.label ?? e.fields.path ?? e.id), entityKind: spec.kind ?? 'symbol', group: spec.kind ?? 'symbol', operand: { select: { kind: spec.kind ?? 'symbol', filter: { ids: [e.id] } } }, evidence: file ? [{ file, ...(loc?.start != null && loc.end! > loc.start ? { startByte: loc.start, endByte: loc.end } : {}) }] : [] };
                assertItem(item, state.pin);
                return item;
            });
            if (!disposed && !signal.aborted && id === searchEpoch)
                publish({ ...state, search: { status: 'ready', spec, items, total: page.total, continuation: page.continuation } });
        }
        catch (e) {
            if (!disposed && !signal.aborted && id === searchEpoch)
                publish({ ...state, search: { status: 'error', spec, items: [], total: 0, error: String(e) } });
        }
    }
    function activatePlan(persona: string) {
        if (!options.plans) {
            publish({ ...state, persona });
            return;
        }
        const plan = options.plans.resolve(persona, !!state.changeContext);
        const layoutPlacements = { ...state.workspace.layoutPlacements, [state.workspace.layoutId]: state.workspace.placements ?? {} };
        const placements = layoutPlacements[plan.layout.id] ?? {};
        const next = registry.viewsFor([...new Set([...plan.views, ...Object.values(placements)])]);
        definitions.clear();
        next.forEach(v => definitions.set(v.id, v));
        publish({ ...state, persona, policy: plan.policy, workspace: { ...state.workspace, layoutId: plan.layout.id, placements, layoutPlacements, expandedPanel: state.workspace.layoutId === plan.layout.id ? state.workspace.expandedPanel : null }, version: state.version + 1 });
        void refresh(true);
    }
    const core = {
        getState: () => state,
        subscribe(listener: () => void) { if (disposed)
            throw new Error('Atlas disposed'); listeners.add(listener); return () => { listeners.delete(listener); }; },
        isSelected: selected,
        dispatch(command: AtlasCommand) {
            if (disposed)
                throw new Error('Atlas disposed');
            if ((command.type === 'mode' || command.type === 'operation') && !['union', 'intersection', 'exclude'].includes(command.operation))
                throw new Error('Invalid selection operation');
            switch (command.type) {
                case 'openEvidence':
                    assertItem(command.item, state.pin);
                    void openEvidence(copy(command.item), command.index);
                    return;
                case 'closeEvidence':
                    evidenceController?.abort();
                    ++evidenceEpoch;
                    publish({ ...state, evidence: { status: 'closed' } });
                    return;
                case 'persona':
                    activatePlan(command.persona);
                    return;
                case 'focus':
                    publish({ ...state, focusedItemId: command.itemId });
                    return;
                case 'policy':
                    if (state.policy.locked && command.factsOnly !== state.policy.factsOnly)
                        throw new Error('Persona evidence policy is locked');
                    publish({ ...state, policy: { ...state.policy, factsOnly: command.factsOnly }, version: state.version + 1 });
                    void refresh();
                    return;
                case 'mode':
                    publish({ ...state, selection: { ...state.selection, mode: command.operation } });
                    return;
                case 'placeView': {
                    if (!options.plans)
                        throw new Error('Chart placement requires a registered layout');
                    const plan = options.plans.resolve(state.persona, !!state.changeContext);
                    if (!plan.layout.slots.some(s => s.area === command.area))
                        throw new Error('Unknown layout slot');
                    const view = registry.view(command.viewId);
                    if (view.purpose === 'navigation')
                        throw new Error('Navigation utilities cannot replace analytical charts');
                    const needsData = !definitions.has(view.id);
                    definitions.set(view.id, view);
                    publish({ ...state, workspace: { ...state.workspace, placements: { ...state.workspace.placements, [command.area]: view.id } } });
                    if (needsData)
                        void refresh();
                    return;
                }
                case 'presentation': {
                    const view = state.views[command.viewId];
                    if (!view?.spec?.semantic)
                        throw new Error('Semantic view is not ready');
                    planPresentation(view.spec.semantic, command.presentation);
                    publish({ ...state, workspace: { ...state.workspace, presentations: { ...state.workspace.presentations, [command.viewId]: copy(command.presentation) } }, views: { ...state.views, [command.viewId]: { ...view, spec: { ...view.spec, presentation: copy(command.presentation) } } } });
                    return;
                }
                case 'workspace':
                    publish({ ...state, workspace: { ...state.workspace, ...command.patch } });
                    return;
                case 'scope':
                    validateQuery(command.scope.query);
                    publish({ ...state, workspace: { ...state.workspace, breadcrumbs: [...state.workspace.breadcrumbs, copy(command.scope)] }, version: state.version + 1 });
                    void refresh(true);
                    return;
                case 'back':
                    publish({ ...state, workspace: { ...state.workspace, breadcrumbs: state.workspace.breadcrumbs.slice(0, -1) }, version: state.version + 1 });
                    void refresh(true);
                    return;
                case 'cube':
                    if (!definitions.has(command.viewId))
                        throw new Error('Unknown cube view');
                    dimensions.cube(command.controls, null);
                    publish({ ...state, workspace: { ...state.workspace, charts: { ...state.workspace.charts, [command.viewId]: copy(command.controls) } }, version: state.version + 1 });
                    void refresh(true);
                    return;
                case 'changeContext': {
                    if (command.context && (!samePin(command.context.head, state.pin) || command.context.base.repository !== state.pin.repository))
                        throw new Error('Change context must end at the captured revision');
                    publish({ ...state, changeContext: copy(command.context ?? null) ?? undefined, version: state.version + 1 });
                    activatePlan(state.persona);
                    if (!options.plans)
                        void refresh(true);
                    return;
                }
                case 'freshness':
                    if (command.value.indexedRevision !== state.pin.revision)
                        throw new Error('Freshness metadata revision mismatch');
                    publish({ ...state, freshness: copy(command.value) });
                    return;
                case 'digest':
                    publish({ ...state, digest: copy(command.value) });
                    return;
                case 'search':
                    void runSearch(command.spec, command.continuation);
                    return;
                case 'refresh':
                    void refresh();
                    return;
                case 'page': {
                    const v = state.views[command.viewId];
                    if (!v || v.status !== 'ready')
                        return;
                    const history = pages.get(command.viewId) ?? [null];
                    if (command.direction === 'next') {
                        if (!v.spec?.continuation)
                            return;
                        pages.set(command.viewId, [...history, v.spec.continuation]);
                    }
                    else {
                        if (history.length === 1)
                            return;
                        pages.set(command.viewId, history.slice(0, -1));
                    }
                    void refresh();
                    return;
                }
            }
            let selection = state.selection;
            if (command.type === 'toggle') {
                assertItem(command.item, state.pin);
                selection = toggle(selection, copy(command.item));
            }
            else if (command.type === 'remove')
                selection = { ...selection, contributions: selection.contributions.filter(c => c.item.id !== command.itemId) };
            else if (command.type === 'restoreContribution') {
                if (command.contribution) {
                    assertItem(command.contribution.item, state.pin);
                    if (command.contribution.item.id !== command.itemId)
                        throw new Error('Contribution identity mismatch');
                }
                selection = { ...selection, contributions: [...selection.contributions.filter(c => c.item.id !== command.itemId), ...(command.contribution ? [copy(command.contribution)] : [])] };
            }
            else if (command.type === 'clear')
                selection = { ...selection, contributions: [] };
            else if (command.type === 'operation')
                selection = setOperation(selection, command.itemId, command.operation);
            publish({ ...state, version: state.version + 1, selection });
            void refresh(true);
        },
        configureViews(ids: string[], policy = state.policy) {
            const next = registry.viewsFor(ids);
            definitions.clear();
            next.forEach(v => definitions.set(v.id, v));
            publish({ ...state, policy, version: state.version + 1 });
            void refresh(true);
        },
        exportSession: (): AtlasSession => {
            const selection = { ...state.selection, contributions: state.selection.contributions.map(c => ({ ...c, item: { ...c.item, operand: c.item.replayOperand ?? c.item.operand } })) };
            if (JSON.stringify(selection.contributions.map(c => c.item.operand)).includes('"cell_contributors"'))
                throw new Error('This aggregate has no durable replay query; select entities before sharing');
            return copy({ schemaVersion: 2, pin: state.pin, persona: state.persona, selection, workspace: state.workspace, policy: state.policy, ...(state.changeContext ? { changeContext: state.changeContext } : {}) });
        },
        restoreSession(session: {
            schemaVersion: number;
            pin: AtlasPin;
            persona: string;
            selection: AtlasState['selection'];
            workspace?: AtlasState['workspace'];
            policy?: AtlasState['policy'];
            changeContext?: AtlasState['changeContext'];
        }) {
            if (disposed)
                throw new Error('Atlas disposed');
            if (![1, 2].includes(session.schemaVersion) || !samePin(session.pin, state.pin))
                throw new Error('Atlas session requires explicit revision revalidation');
            if (!['union', 'intersection', 'exclude'].includes(session.selection.mode))
                throw new Error('Invalid selection operation');
            const seen = new Set<string>();
            for (const c of session.selection.contributions) {
                assertItem(c.item, state.pin);
                if (seen.has(c.item.id) || !['union', 'intersection', 'exclude'].includes(c.operation))
                    throw new Error('Invalid session contribution');
                seen.add(c.item.id);
            }
            if (session.workspace) {
                for (const scope of session.workspace.breadcrumbs)
                    validateQuery(scope.query);
                for (const [id, controls] of Object.entries(session.workspace.charts)) {
                    registry.view(id);
                    dimensions.cube(controls, null);
                }
            }
            if (session.changeContext && !samePin(session.changeContext.head, state.pin))
                throw new Error('Saved change context is stale');
            const plan = options.plans?.resolve(session.persona, !!session.changeContext);
            if (plan) {
                if (session.workspace && session.workspace.layoutId !== plan.layout.id) throw new Error('Saved layout does not match persona');
                for (const [layoutId, placements] of Object.entries(session.workspace?.layoutPlacements ?? {})) {
                    const layout = options.plans!.layout(layoutId);
                    if (!placements || typeof placements !== 'object' || Array.isArray(placements)) throw new Error('Invalid saved placements');
                    for (const [area, id] of Object.entries(placements)) {
                        if (!layout.slots.some(s => s.area === area) || registry.view(id).purpose === 'navigation') throw new Error('Saved layout placement is invalid');
                    }
                }
                for (const area of Object.keys(session.workspace?.placements ?? {}))
                    if (!plan.layout.slots.some(s => s.area === area))
                        throw new Error('Saved layout slot is invalid');
                definitions.clear();
                registry.viewsFor([...new Set([...plan.views, ...Object.values(session.workspace?.placements ?? {})])]).forEach(v => definitions.set(v.id, v));
            }
            publish({ ...state, version: state.version + 1, persona: session.persona, selection: copy(session.selection), workspace: copy(session.workspace ?? state.workspace), policy: plan?.policy.locked ? plan.policy : copy(session.policy ?? state.policy), changeContext: session.changeContext ? copy(session.changeContext) : undefined });
            void refresh(true);
        },
        async changeRevision(pin: AtlasPin, nextStorage: AtlasStorage) {
            if (disposed || pin.repository !== state.pin.repository)
                throw new Error('Revision transition requires the same repository');
            controller?.abort();
            evidenceController?.abort();
            searchController?.abort();
            ++epoch;
            ++evidenceEpoch;
            ++searchEpoch;
            const previous = storage;
            storage = nextStorage;
            pages.clear();
            publish({ ...state, pin: copy(pin), version: state.version + 1, selection: initialSelection(), changeContext: undefined, views: {}, evidence: { status: 'closed' }, search: { status: 'idle', items: [], total: 0 }, workspace: { ...state.workspace, breadcrumbs: [] }, freshness: { status: 'unknown', indexedRevision: pin.revision }, notice: 'Revision changed; selections were cleared rather than silently rebased.' });
            await previous.close().catch(() => { });
            await refresh();
        },
        start: () => { if (options.plans) {
            const p = options.plans.resolve(state.persona);
            publish({ ...state, policy: p.policy, workspace: { ...state.workspace, layoutId: p.layout.id } });
        } return refresh(); },
        dispose() { if (disposed)
            return; disposed = true; controller?.abort(); evidenceController?.abort(); searchController?.abort(); listeners.clear(); void storage.close().catch(() => { }); },
    };
    return core;
}
export type AtlasCore = ReturnType<typeof createAtlasCore>;
