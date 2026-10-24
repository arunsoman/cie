import type {EntityKind} from './atlas-populations';
/** Atlas contracts are independent of React, persona layouts and renderers. */
export interface AtlasPin { repository: string; revision: string }
export type AtlasQuery =
  | { select: { kind: string; filter?: Record<string, unknown> } }
  | { union: AtlasQuery[] } | { intersect: AtlasQuery[] }
  | { difference: { left: AtlasQuery; right: AtlasQuery } }
  | { follow: { input: AtlasQuery; relation: string; incoming?: boolean; max_depth?: number; min_depth?: number } }
  | { cell_contributors: { descriptor: string } };
export type AtlasAuthority='Fact'|'Inference'|'Hypothesis'|'Unknown';
export interface AtlasEvidencePolicy {factsOnly:boolean;locked?:boolean}
export interface EvidenceRef { label?:string; role?:'code'|'document'|'from'|'to'|'supporting'; pin?:AtlasPin; contentHash?:string; file: string; startByte?: number; endByte?: number; line?: number }
export interface AtlasItemRef {
  id: string; pin: AtlasPin; label: string;
  /** A query in the core's declared entity universe, never a display label. */
  entityKind?:EntityKind;
  operand: AtlasQuery;
  group: string;
  evidence: EvidenceRef[];
}
export type AtlasOperation = 'union' | 'intersection' | 'exclude';
export interface Contribution { item: AtlasItemRef; operation: AtlasOperation }
export interface AtlasSelection { contributions: Contribution[]; mode: AtlasOperation }
export type AtlasCommand =
  | { type: 'toggle'; item: AtlasItemRef }
  | { type: 'remove'; itemId: string }
  | { type: 'clear' }
  | { type: 'restoreContribution'; itemId: string; contribution?: Contribution }
  | { type: 'mode'; operation: AtlasOperation }
  | { type: 'operation'; itemId: string; operation: AtlasOperation }
  | { type: 'persona'; persona: string }
  | { type: 'openEvidence'; item: AtlasItemRef; index?: number }
  | { type: 'closeEvidence' }
  | { type: 'page'; viewId: string; direction: 'next' | 'previous' }
  | { type: 'policy'; factsOnly:boolean }
  | { type: 'focus'; itemId?:string }
  | { type: 'refresh' };
export interface AtlasDatum { key: string; label: string; value?: number; authority?:AtlasAuthority; detail?: string; detailRef?: AtlasItemRef; ref: AtlasItemRef; role: 'member' | 'context' }
export interface AtlasLink { key: string; from: string; to: string; label: string; authority?:AtlasAuthority; ref: AtlasItemRef }
export interface AtlasViewSpec {
  kind: string; renderer?:string; availability?:'ready'|'not-collected'|'unsupported'|'incomplete'; coverage?:unknown; title: string;
  items: AtlasDatum[]; links?: AtlasLink[];
  total: number; continuation: string | null; gaps: string[];
}
export interface AtlasViewState {
  binding: 'selection' | 'repository'; status: 'loading' | 'ready' | 'error';
  version: number; page: number; spec?: AtlasViewSpec; error?: string;
}
export interface CapturedEvidence { text: string; path: string; revision: string; repository?:string; content_hash?:string; truncated: boolean; returned: { start: number; end: number } }
export interface EvidenceState {
  status: 'closed' | 'choose' | 'loading' | 'ready' | 'unavailable';
  item?: AtlasItemRef; source?: CapturedEvidence; error?: string;
}
export interface AtlasState {
  pin: AtlasPin; persona: string; version: number; selection: AtlasSelection;
  policy:AtlasEvidencePolicy; focusedItemId?:string; views: Record<string, AtlasViewState>; evidence: EvidenceState;
}
export interface AtlasResponse {
  repository: string; revision: string; matches: number; coverage?: unknown;
  outputs: { id: string; status: string; schema_version: number; data: unknown; error?: { message: string } }[];
}
export interface AtlasStorage {
  retrieve(query: AtlasQuery, outputs: { id: string; output: Record<string, unknown> }[], signal: AbortSignal): Promise<AtlasResponse>;
  source(evidence: EvidenceRef, signal: AbortSignal): Promise<CapturedEvidence>;
  close(): Promise<void>;
}
export interface ProjectionContext { pin: AtlasPin; query: AtlasQuery; version: number }
export interface AtlasViewDefinition {
  id: string; binding: 'selection' | 'repository';
  outputs(continuation: string | null): { id: string; output: Record<string, unknown> }[];
  targetKind?:EntityKind;
  project(response: AtlasResponse, context: ProjectionContext): AtlasViewSpec;
}
