import type { PopulationSpec, SemanticViewSpec, SemanticEvidence } from './atlas-semantic';
import type { PresentationSpec } from './atlas-presentation';
import type { AtlasWorkspace, ChangeContext, AtlasSearchState, AtlasFreshness, AtlasDigest, CubeControls } from './atlas-workspace';
import type { AtlasSearch, SpatialScope } from './atlas-navigation';
import type { ViewSpec } from 'viz-adapter';
import type { EntityKind } from './atlas-populations';
/** Atlas contracts are independent of React, persona layouts and renderers. */
export interface AtlasPin {
    repository: string;
    revision: string;
}
export type AtlasQuery = {
    select: {
        kind: string;
        filter?: Record<string, unknown>;
    };
} | {
    union: AtlasQuery[];
} | {
    intersect: AtlasQuery[];
} | {
    difference: {
        left: AtlasQuery;
        right: AtlasQuery;
    };
} | {
    follow: {
        input: AtlasQuery;
        relation: string;
        incoming?: boolean;
        max_depth?: number;
        min_depth?: number;
    };
} | {
    cell_contributors: {
        descriptor: string;
    };
};
export type AtlasAuthority = 'Fact' | 'Inference' | 'Hypothesis' | 'Unknown';
export interface AtlasEvidencePolicy {
    factsOnly: boolean;
    locked?: boolean;
}
export interface EvidenceRef {
    label?: string;
    role?: 'code' | 'document' | 'from' | 'to' | 'supporting';
    pin?: AtlasPin;
    contentHash?: string;
    file: string;
    startByte?: number;
    endByte?: number;
    line?: number;
}
export interface AtlasItemRef {
    id: string;
    pin: AtlasPin;
    label: string;
    /** A query in the core's declared entity universe, never a display label. */
    provenance?: SemanticEvidence[];
    entityKind?: EntityKind;
    replayOperand?: AtlasQuery;
    operand: AtlasQuery;
    group: string;
    evidence: EvidenceRef[];
}
export type AtlasOperation = 'union' | 'intersection' | 'exclude';
export interface Contribution {
    item: AtlasItemRef;
    operation: AtlasOperation;
}
export interface AtlasSelection {
    contributions: Contribution[];
    mode: AtlasOperation;
}
export type AtlasCommand = {
    type: 'toggle';
    item: AtlasItemRef;
} | {
    type: 'remove';
    itemId: string;
} | {
    type: 'clear';
} | {
    type: 'restoreContribution';
    itemId: string;
    contribution?: Contribution;
} | {
    type: 'mode';
    operation: AtlasOperation;
} | {
    type: 'operation';
    itemId: string;
    operation: AtlasOperation;
} | {
    type: 'persona';
    persona: string;
} | {
    type: 'openEvidence';
    item: AtlasItemRef;
    index?: number;
} | {
    type: 'closeEvidence';
} | {
    type: 'page';
    viewId: string;
    direction: 'next' | 'previous';
} | {
    type: 'policy';
    factsOnly: boolean;
} | {
    type: 'focus';
    itemId?: string;
} | {
    type: 'scope';
    scope: SpatialScope;
} | {
    type: 'back';
} | {
    type: 'search';
    spec: AtlasSearch;
    continuation?: string | null;
} | {
    type: 'workspace';
    patch: Partial<Pick<AtlasWorkspace, 'layoutId' | 'expandedPanel' | 'dockOpen'>>;
} | {
    type: 'placeView';
    area: string;
    viewId: string;
} | {
    type: 'presentation';
    viewId: string;
    presentation: PresentationSpec;
} | {
    type: 'cube';
    viewId: string;
    controls: CubeControls;
} | {
    type: 'changeContext';
    context?: ChangeContext;
} | {
    type: 'freshness';
    value: AtlasFreshness;
} | {
    type: 'digest';
    value: AtlasDigest;
} | {
    type: 'refresh';
};
export interface AtlasDatum {
    key: string;
    label: string;
    value?: number;
    authority?: AtlasAuthority;
    fields?: Record<string, unknown>;
    detail?: string;
    detailRef?: AtlasItemRef;
    cellRefs?: Record<string, AtlasItemRef>;
    ref: AtlasItemRef;
    role: 'member' | 'context';
}
export interface AtlasLink {
    key: string;
    from: string;
    to: string;
    label: string;
    authority?: AtlasAuthority;
    ref: AtlasItemRef;
}
export interface AtlasViewSpec {
    semantic?: SemanticViewSpec;
    presentation?: PresentationSpec;
    kind: string;
    renderer?: string;
    visualization?: ViewSpec;
    columns?: {
        id: string;
        label: string;
        unit?: string;
    }[];
    dataPin?: AtlasPin;
    availability?: 'ready' | 'not-collected' | 'unsupported' | 'incomplete';
    coverage?: unknown;
    title: string;
    items: AtlasDatum[];
    links?: AtlasLink[];
    total: number;
    continuation: string | null;
    gaps: string[];
}
export interface AtlasViewState {
    binding: 'selection' | 'repository';
    status: 'loading' | 'ready' | 'error';
    population?: PopulationSpec;
    version: number;
    page: number;
    spec?: AtlasViewSpec;
    error?: string;
}
export interface CapturedEvidence {
    text: string;
    path: string;
    revision: string;
    repository?: string;
    content_hash?: string;
    truncated: boolean;
    returned: {
        start: number;
        end: number;
    };
}
export interface EvidenceState {
    status: 'closed' | 'choose' | 'loading' | 'ready' | 'unavailable';
    activeLocation?: EvidenceRef;
    item?: AtlasItemRef;
    source?: CapturedEvidence;
    error?: string;
}
export interface AtlasState {
    pin: AtlasPin;
    persona: string;
    version: number;
    selection: AtlasSelection;
    workspace: AtlasWorkspace;
    changeContext?: ChangeContext;
    search: AtlasSearchState;
    freshness: AtlasFreshness;
    digest?: AtlasDigest;
    notice?: string;
    policy: AtlasEvidencePolicy;
    focusedItemId?: string;
    views: Record<string, AtlasViewState>;
    evidence: EvidenceState;
}
export interface AtlasResponse {
    repository: string;
    revision: string;
    matches: number;
    coverage?: unknown;
    outputs: {
        id: string;
        status: string;
        schema_version: number;
        data: unknown;
        error?: {
            message: string;
        };
    }[];
}
export interface AtlasStorage {
    retrieve(query: AtlasQuery, outputs: {
        id: string;
        output: Record<string, unknown>;
    }[], signal: AbortSignal): Promise<AtlasResponse>;
    source(evidence: EvidenceRef, signal: AbortSignal): Promise<CapturedEvidence>;
    close(): Promise<void>;
}
export interface ProjectionContext {
    pin: AtlasPin;
    query: AtlasQuery;
    version: number;
    controls?: CubeControls;
    universe?: EntityKind;
    subjects?: AtlasItemRef[];
    replayQuery?: AtlasQuery;
}
export interface AtlasViewDefinition {
    purpose?: 'analysis' | 'navigation';
    id: string;
    binding: 'selection' | 'repository';
    outputs(continuation: string | null, context?: ProjectionContext): {
        id: string;
        output: Record<string, unknown>;
    }[];
    expand?: {
        relation: string;
        maxDepth: number;
        incoming?: boolean;
    };
    targetKind?: EntityKind;
    /** Include manifest-only targets at the unrestricted repository root. */
    unfilteredTargetUniverse?: boolean;
    filter?: Record<string, unknown>;
    project(response: AtlasResponse, context: ProjectionContext): AtlasViewSpec;
}
