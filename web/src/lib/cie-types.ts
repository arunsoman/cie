// Types for CIE engine exports (report.json, viewspec.json, concepts.json).
// These mirror the real JSON produced by the `cie` CLI (v1.1.0).

export interface SymbolInfo {
  id: number;
  kind: string; // Function | Module | Class ...
  name: string;
  qual_name: string;
  loc: { file: number; start: number; end: number };
  lang: string;
  parent: number;
  doc?: string;
}

export interface SummaryInfo {
  sym: number;
  inputs: string[];
  outputs: string[];
  reads: string[];
  writes: string[];
  effects: string[];
  exceptions: string[];
  callees: string[];
  unresolved_callees: string[];
  tags: string[];
  rev: number;
  complete: boolean;
  has_failure_path?: boolean;
  has_loops?: boolean;
}

export interface Finding {
  id: string;
  kind: string;
  severity: "Critical" | "High" | "Medium" | "Low" | string;
  symbol: number;
  symbol_name: string;
  message: string;
  evidence_lines: number[];
  remediation: string;
}

export interface Contract {
  id: number;
  obligation: string;
  expr: string;
  authority: string; // Explicit | InferredCandidate | ...
  symbols: number[];
  source: string;
}

export interface Coverage {
  files_total: number;
  files_analyzed: number;
  symbols_total: number;
  symbols_analyzed: number;
  complete: boolean;
  notes: string[];
}

export interface EvidenceInfo {
  rev: number;
  provenance: string;
  assumptions: string[];
  unsupported: string[];
  unresolved: string[];
}

export interface CieReport {
  rev: number;
  files: string[];
  symbols: SymbolInfo[];
  summaries: SummaryInfo[];
  findings: Finding[];
  contracts: Contract[];
  coverage: Coverage;
  evidence: EvidenceInfo;
  warnings: string[];
}

// ---- viewspec.json ----

export interface VizElement {
  id: string;
  elementKind: "domain" | "module" | "usecase" | "rule" | "invariant" | string;
  displayMode: "Fact" | "Inference" | "Hypothesis" | string;
  label: string;
  evidence: { file: string; start: number; end: number };
  claimId?: string;
  salience?: { factors: string[]; weight: number; whyShown: string };
  fields?: Record<string, unknown>;
}

export interface VizEdge {
  id: string;
  source: string;
  target: string;
  edgeKind: string;
  displayMode: "Fact" | "Inference" | string;
  evidence?: { file: string; start: number; end: number };
}

export interface CieViewSpec {
  elements: VizElement[];
  edges: VizEdge[];
  footer?: {
    facts: number;
    inferences: number;
    hypotheses: number;
    fog: number;
    stale: number;
    caption: string;
  };
  gaps?: unknown[];
  hidden?: unknown[];
  scope?: { revision: number; coverage?: { analyzed: boolean; ratio: number } };
  layoutHint?: { strategy: string };
  semanticHash?: string;
  renderHash?: string;
}

// ---- concepts.json ----

export interface ConceptBehavior {
  summary: number;
  kind: string;
  confidence: number;
}

export interface ConceptRule {
  rule_id?: string;
  kind?: string;
  status?: string;
  confidence?: number;
  [k: string]: unknown;
}

export interface ConceptNode {
  id: number;
  label: string;
  kind: string; // Domain | Module | UseCase ...
  parent: number | null;
  children: number[];
  primary_edge: string;
  symbols: number[];
  behaviors: ConceptBehavior[];
  rules?: ConceptRule[];
}

export interface ConceptTree {
  nodes: ConceptNode[];
  unattached_behaviors: ConceptBehavior[];
  unattached_rules: ConceptRule[];
  dirty: boolean;
}
