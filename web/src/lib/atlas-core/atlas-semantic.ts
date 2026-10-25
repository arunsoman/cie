import type { AtlasPin, AtlasQuery, AtlasItemRef, AtlasAuthority, AtlasViewSpec, ProjectionContext, AtlasResponse } from './atlas-types';
import type { EntityKind } from './atlas-populations';
export interface PopulationSpec {
    pin: AtlasPin;
    version: number;
    universe: EntityKind;
    expression: AtlasQuery;
    matches: number;
    coverage: unknown;
}
export type SemanticEvidence = {
    kind: 'source' | 'document';
    pin: AtlasPin;
    file: string;
    start: number;
    end: number;
    contentHash?: string;
} | {
    kind: 'stored-fact';
    pin: AtlasPin;
    artifact: string;
    recordId: string;
} | {
    kind: 'aggregate';
    pin: AtlasPin;
    descriptor: string;
} | {
    kind: 'runtime';
    pin: AtlasPin;
    runId: string;
    sampleId: string;
};
export interface VisualBinding {
    subject?: {
        id: string;
        kind: EntityKind;
        pin: AtlasPin;
    };
    visualId: string;
    label: string;
    ref: AtlasItemRef;
    authority: AtlasAuthority;
    evidence: SemanticEvidence[];
    membership: 'member' | 'context';
}
export interface SemanticNode extends VisualBinding {
    parentId?: string;
    subjectKind?: string;
}
export interface SemanticRelation extends VisualBinding {
    from: string;
    to: string;
    relation: string;
}
interface SemanticBase {
    population: PopulationSpec;
    title: string;
    gaps: string[];
}
export interface ArchitectureView extends SemanticBase {
    kind: 'architecture';
    level: 'container' | 'component';
    nodes: (SemanticNode & { boundaryRole: 'system' | 'container' | 'component'; technology?: string })[];
    relations: SemanticRelation[];
}
export interface DependencyView extends SemanticBase {
    kind: 'dependency';
    nodes: SemanticNode[];
    relations: SemanticRelation[];
}
export interface HierarchyView extends SemanticBase {
    kind: 'hierarchy';
    nodes: SemanticNode[];
    roots: string[];
}
export interface FlowStatement extends VisualBinding {
    operation: string;
    sourceText: string;
}
export interface FlowBlock extends VisualBinding {
    blockKind: 'entry' | 'exit' | 'basic' | 'decision';
    statements: FlowStatement[];
}
export interface ControlFlowView extends SemanticBase {
    kind: 'control-flow';
    entry: string | null;
    blocks: FlowBlock[];
    branches: (SemanticRelation & {
        branch: 'true' | 'false' | 'back' | 'jump' | 'exception' | 'unknown';
    })[];
}
export interface SequenceParticipant extends VisualBinding {
    participantKind: 'function' | 'external' | 'unresolved';
}
export interface SequenceMessage extends VisualBinding {
    from: string;
    to: string;
    order: number;
    dispatch: 'exact' | 'candidate' | 'external' | 'unknown';
    guard?: string;
    recursive?: boolean;
    callSite?: SemanticEvidence;
}
export interface SequenceView extends SemanticBase {
    kind: 'sequence';
    participants: SequenceParticipant[];
    messages: SequenceMessage[];
    fragments: {
        kind: 'alternative' | 'loop';
        guard: string;
        messages: string[];
        evidence: SemanticEvidence[];
    }[];
}
export interface DataFlowView extends SemanticBase {
    kind: 'data-flow';
    nodes: (SemanticNode & {
        dataRole: 'producer' | 'consumer' | 'store' | 'transform';
    })[];
    relations: (SemanticRelation & {
        operation: 'read' | 'write' | 'transform';
    })[];
}
export type CellValue = string | number | boolean | null;
export interface MetricView extends SemanticBase {
    kind: 'metric';
    dimensions: {
        id: string;
        label: string;
    }[];
    measures: {
        id: string;
        label: string;
        unit?: string;
    }[];
    cells: (VisualBinding & {
        members: Record<string, CellValue>;
        measures: Record<string, number | null>;
    })[];
}
export interface TableView extends SemanticBase {
    kind: 'table';
    columns: {
        id: string;
        label: string;
        unit?: string;
    }[];
    rows: (VisualBinding & {
        values: Record<string, CellValue>;
        cellBindings?: Record<string, VisualBinding>;
    })[];
}
export type SemanticViewSpec = ArchitectureView | DependencyView | HierarchyView | ControlFlowView | SequenceView | DataFlowView | MetricView | TableView;
export type SemanticKind = SemanticViewSpec['kind'];
export function sourceEvidence(ref: AtlasItemRef): SemanticEvidence[] { return [...(ref.provenance ?? []), ...ref.evidence.flatMap(e => e.startByte != null && e.endByte != null ? [{ kind: e.role === 'document' ? 'document' as const : 'source' as const, pin: e.pin ?? ref.pin, file: e.file, start: e.startByte, end: e.endByte, contentHash: e.contentHash }] : [])]; }
export function binding(visualId: string, label: string, ref: AtlasItemRef, authority: AtlasAuthority = 'Unknown', membership: 'member' | 'context' = 'member'): VisualBinding { return { subject: ref.entityKind ? { id: ref.id, kind: ref.entityKind, pin: ref.pin } : undefined, visualId, label, ref, authority, membership, evidence: sourceEvidence(ref) }; }
export function semanticBindings(model: SemanticViewSpec): VisualBinding[] { switch (model.kind) {
    case 'architecture':
    case 'dependency':
    case 'data-flow': return [...model.nodes, ...model.relations];
    case 'hierarchy': return model.nodes;
    case 'control-flow': return [...model.blocks, ...model.blocks.flatMap(b => b.statements), ...model.branches];
    case 'sequence': return [...model.participants, ...model.messages];
    case 'metric': return model.cells;
    case 'table': return model.rows.flatMap(row => [row, ...Object.values(row.cellBindings ?? {})]);
} }
/** Compatibility is deliberately limited. No generic graph→flow/sequence conversion. */
export function semanticProjection(spec: AtlasViewSpec, c: ProjectionContext, r: AtlasResponse): SemanticViewSpec {
    const common = { population: { pin: c.pin, version: c.version, universe: c.universe ?? 'symbol' as const, expression: c.query, matches: r.matches, coverage: r.coverage ?? null }, title: spec.title, gaps: spec.gaps };
    const nodes = spec.items.map(i => binding(i.key, i.label, i.ref, i.authority, i.role));
    if (spec.kind === 'graph')
        return { ...common, kind: 'dependency', nodes, relations: (spec.links ?? []).map(e => ({ ...binding(e.key, e.label, e.ref, e.authority), from: e.from, to: e.to, relation: e.label })) };
    if (spec.kind === 'table' || spec.kind === 'pivot')
        return { ...common, kind: 'table', columns: spec.columns ?? [{ id: 'label', label: 'Symbol' }, { id: 'detail', label: 'File' }], rows: nodes.map((n, i) => ({ ...n, cellBindings: Object.fromEntries(Object.entries(spec.items[i].cellRefs ?? {}).map(([column, ref]) => [column, binding(`${n.visualId}:cell:${column}`, String(spec.items[i].fields?.[column] ?? column), ref, n.authority, n.membership)])), values: { label: n.label, detail: spec.items[i].detail ?? null, ...spec.items[i].fields as Record<string, CellValue> } })) };
    if (spec.kind === 'bars')
        return { ...common, kind: 'metric', dimensions: [{ id: 'group', label: 'Group' }], measures: [{ id: 'count', label: 'Count', unit: 'entities' }], cells: nodes.map((n, i) => ({ ...n, members: { group: n.label }, measures: { count: spec.items[i].value ?? null } })) };
    throw new Error(`A typed semantic model is required for ${spec.kind}`);
}
