import type { AtlasPin, AtlasQuery, AtlasEvidencePolicy, AtlasSelection } from './atlas-types';
import type { SpatialScope, AtlasSearch } from './atlas-navigation';
export interface CubeControls {
    dimensions: string[];
    measures: string[];
    rows: string[];
    columns: string[];
    order?: {
        field: string;
        direction: 'asc' | 'desc';
    }[];
}
export interface AtlasWorkspace {
    placements?: Record<string, string>;
    layoutPlacements?: Record<string, Record<string, string>>;
    layoutId: string;
    expandedPanel: string | null;
    dockOpen: boolean;
    breadcrumbs: SpatialScope[];
    charts: Record<string, CubeControls>;
    presentations?: Record<string, import('./atlas-presentation').PresentationSpec>;
}
export interface ChangeContext {
    base: AtlasPin;
    head: AtlasPin;
    changedFiles: string[];
    deletedFiles?: string[];
    unknownFiles?: string[];
}
export interface AtlasSession {
    schemaVersion: 2;
    pin: AtlasPin;
    persona: string;
    selection: AtlasSelection;
    policy: AtlasEvidencePolicy;
    workspace: AtlasWorkspace;
    changeContext?: ChangeContext;
}
export interface AtlasSearchState {
    status: 'idle' | 'loading' | 'ready' | 'error';
    spec?: AtlasSearch;
    items: import('./atlas-types').AtlasItemRef[];
    continuation?: string | null;
    total: number;
    error?: string;
}
export interface AtlasFreshness {
    status: 'unknown' | 'known';
    indexedRevision: string;
    headRevision?: string;
    indexedAt?: string;
    parsedRatio?: number;
    languages?: Record<string, number>;
    reason?: string;
}
export interface AtlasDigest {
    sinceRevision: string;
    metrics: {
        id: string;
        label: string;
        value: number | null;
        targetViewId?: string;
        query?: AtlasQuery;
    }[];
}
export const defaultWorkspace = (): AtlasWorkspace => ({ layoutId: 'reviewer.reference', expandedPanel: null, dockOpen: true, breadcrumbs: [], charts: {} });
const key = (pin: AtlasPin) => `cie.atlas.saved.v2:${pin.repository}:${pin.revision}`;
export interface WorkspacePersistence {
    getItem(key: string): string | null;
    setItem(key: string, value: string): void;
}
export function saveNamedView(storage: WorkspacePersistence, name: string, session: AtlasSession) { if (!name.trim() || name.length > 100)
    throw new Error('Name must contain 1–100 characters'); const all = Object.assign(Object.create(null), JSON.parse(storage.getItem(key(session.pin)) ?? '{}')); all[name] = session; storage.setItem(key(session.pin), JSON.stringify(all)); }
export function loadNamedViews(storage: WorkspacePersistence, pin: AtlasPin): Record<string, AtlasSession> { const value = JSON.parse(storage.getItem(key(pin)) ?? '{}'); if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error('Invalid saved views'); return value; }
/** Fragment state is never sent to the server. Contains source references, not code or credentials. */
export function sessionPermalink(session: AtlasSession) { return '#atlas=' + encodeURIComponent(JSON.stringify(session)); }
export function readPermalink(fragment: string): AtlasSession { if (!fragment.startsWith('#atlas=') || fragment.length > 512000)
    throw new Error('Invalid Atlas permalink'); const value = JSON.parse(decodeURIComponent(fragment.slice(7))); if (value.schemaVersion !== 2)
    throw new Error('Unsupported Atlas permalink'); return value; }
