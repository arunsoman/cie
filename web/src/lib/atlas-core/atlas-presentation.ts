import type { SemanticKind, SemanticViewSpec } from './atlas-semantic';
export type PresentationCapability = 'selection' | 'directed-edges' | 'containment' | 'control-flow' | 'sequence-order' | 'self-calls' | 'metrics' | 'table' | 'minimap' | 'fisheye' | 'custom-nodes' | 'custom-edges' | 'plugins' | 'combos' | 'incremental-update';
export interface PresentationSpec {
    hostExtension?: string;
    features?: {
        minimap?: boolean;
        fisheye?: boolean;
    };
    profile: string;
    kind: SemanticKind;
    required: PresentationCapability[];
    preferred: PresentationCapability[];
    layout: {
        family: 'layered' | 'tree' | 'sequence' | 'grid';
        direction: 'LR' | 'TB';
    };
    detail: 'summary' | 'full';
}
export interface PresentationProfile extends PresentationSpec {
    host: 'viz-adapter' | 'table' | 'metric';
}
export class PresentationRegistry {
    private profiles = new Map<string, PresentationProfile>();
    register(profile: PresentationProfile) { if (!profile.profile || this.profiles.has(profile.profile))
        throw new Error('Duplicate presentation profile'); this.profiles.set(profile.profile, profile); return this; }
    get(id: string, kind: SemanticKind) { const p = this.profiles.get(id); if (!p || p.kind !== kind)
        throw new Error(`Presentation ${id} does not support ${kind}`); return p; }
    defaultFor(kind: SemanticKind) { const p = [...this.profiles.values()].find(p => p.kind === kind); if (!p)
        throw new Error(`No presentation profile for ${kind}`); return p; }
}
export const presentations = new PresentationRegistry();
for (const [kind, host, family, required] of [
    ['dependency', 'viz-adapter', 'layered', ['selection', 'directed-edges']], ['hierarchy', 'viz-adapter', 'tree', ['selection', 'containment']],
    ['control-flow', 'viz-adapter', 'layered', ['selection', 'control-flow']], ['sequence', 'viz-adapter', 'sequence', ['selection', 'sequence-order', 'self-calls']],
    ['data-flow', 'viz-adapter', 'layered', ['selection', 'directed-edges']], ['metric', 'metric', 'grid', ['selection', 'metrics']], ['table', 'table', 'grid', ['selection', 'table']],
] as const)
    presentations.register({ profile: `${kind}.default`, kind, host, layout: { family, direction: family === 'tree' ? 'LR' : 'TB' }, required: [...required], preferred: host === 'viz-adapter' ? ['minimap', 'fisheye', 'custom-nodes', 'custom-edges', 'plugins'] : [], detail: 'full' });
/** Profiles cannot relabel semantics or require unsupported semantic substitutions. */
export function planPresentation(model: SemanticViewSpec, request?: PresentationSpec) { const p = request ? presentations.get(request.profile, model.kind) : presentations.defaultFor(model.kind); if (request && request.kind !== model.kind)
    throw new Error('Presentation cannot change semantic view kind'); return { ...p, ...request, host: p.host }; }
presentations.register({ profile: 'metric.table', kind: 'metric', host: 'table', layout: { family: 'grid', direction: 'TB' }, required: ['selection', 'table'], preferred: [], detail: 'full' });
