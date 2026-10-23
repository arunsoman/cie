// Server-only compatibility adapter. Physical queries run inside cie-storage.
import { withRevision, storageCall, StorageFailure } from './storage-native';
type Row = Record<string, unknown>;
export interface PolyglotSummary {
  dbPath: string;
  ingest: { files: number; astNodes: number; symbols: number };
  ir: {
    blocks: number;
    instructions: number;
    calls: number;
    callsResolved: number;
    callsUnresolved: number;
  };
  frameworks: { totalRoles: number; roleSites: Row[] };
  c4: { containers: Row[]; components: Row[]; relationships: Row[] };
  stateMachines: { total: number; byTier: Record<string, number>; rows: Row[] };
  workflows: { total: number; totalNodes: number; rows: Row[] };
  duplicates: { totalPairs: number; byTier: Record<string, number> };
}
export interface DuplicateSymbolRef {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
  startByte: number;
  endByte: number;
}
export interface DuplicateRow {
  id: number;
  similarity: number;
  tier: string;
  detection: string;
  epistemic: string;
  counterArgument: string | null;
  a: DuplicateSymbolRef;
  b: DuplicateSymbolRef;
  canonical: DuplicateSymbolRef | null;
}
export interface DuplicatesPage {
  rows: DuplicateRow[];
  total: number;
  facets: { tiers: string[]; detections: string[]; epistemics: string[] };
}
export interface DuplicateClusterMember {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
  startByte: number;
  endByte: number;
}
export interface DuplicateCluster {
  hub: DuplicateClusterMember;
  members: DuplicateClusterMember[]; // excludes hub, capped at maxMembersPerCluster
  totalMembers: number; // before the cap
}
export interface DuplicateClustersResult {
  tierUsed: "canonical";
  totalFamilies: number;
  totalSymbolsInvolved: number;
  clusters: DuplicateCluster[];
}
export type PolyglotTableGroup =
  | "Ingest & AST"
  | "IR"
  | "Frameworks"
  | "C4 / Concepts"
  | "State machines"
  | "Workflows"
  | "Duplicates"
  | "Runtime (OTel)";
interface TableSpec {
  name: string;
  group: PolyglotTableGroup;
  note: string;
}
export interface PolyglotTableInfo {
  name: string;
  group: PolyglotTableGroup;
  note: string;
  count: number | null; // null if the table doesn't exist in this store (older schema version)
}
export interface PolyglotTablePage {
  table: string;
  columns: string[];
  rows: Record<string, unknown>[];
  total: number;
}
export interface SymbolSearchResult {
  symbolId: number;
  qualName: string;
  kind: string;
  path: string;
}
export interface FunctionIrInstruction {
  instIndex: number;
  op: string;
  spanStart: number;
  spanEnd: number;
}
export interface FunctionIrBlock {
  id: number;
  blockIndex: number;
  kind: string;
  instructions: FunctionIrInstruction[];
}
export interface FunctionIrEdge {
  fromBlockIndex: number;
  toBlockIndex: number;
  kind: string;
}
export interface FunctionIrCall {
  calleeText: string | null;
  calleeQualName: string | null;
  resolution: string;
  callKind: string;
  stmtIndex: number;
}
export interface FunctionIr {
  symbol: { symbolId: number; qualName: string; kind: string; path: string; startByte: number; endByte: number };
  blocks: FunctionIrBlock[];
  edges: FunctionIrEdge[];
  calls: FunctionIrCall[];
}

async function compatible<T>(root: string, kind: string, args: Record<string, unknown> = {}, revision?: string): Promise<T | null> {
  try { return await withRevision(root, context => storageCall<T>({op:'compatibility', context, kind, args}), revision); }
  catch (error) { if (error instanceof StorageFailure && ['RevisionUnavailable','RevisionNotFound','ArtifactUnavailable'].includes(error.code)) return null; throw error; }
}
export const getPolyglotSummary = (root: string, revision?: string) => compatible<PolyglotSummary>(root, 'summary', {}, revision);
export const searchPolyglotSymbols = (root: string, query: string, limit = 30, revision?: string) => compatible<SymbolSearchResult[]>(root, 'symbols', {query, limit}, revision);
export const getFunctionIr = (root: string, symbolId: number, revision?: string) => compatible<FunctionIr>(root, 'function_ir', {symbolId}, revision);
export const getDuplicates = (root: string, opts?: { tier?: string; detection?: string; epistemic?: string; limit?: number; offset?: number; revision?: string }) => compatible<DuplicatesPage>(root, 'duplicates', opts, opts?.revision);
export const getDuplicateClusters = (root: string, opts?: { maxClusters?: number; maxMembersPerCluster?: number; revision?: string }) => compatible<DuplicateClustersResult>(root, 'duplicate_clusters', opts, opts?.revision);
export const getPolyglotTableInventory = (root: string, revision?: string) => compatible<PolyglotTableInfo[]>(root, 'tables', {}, revision);
export const getPolyglotTableRows = (root: string, table: string, opts?: {limit?: number; offset?: number; revision?: string}) => compatible<PolyglotTablePage>(root, 'table_rows', {table, ...opts}, opts?.revision);
