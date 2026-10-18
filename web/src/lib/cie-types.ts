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

// ---- deps.json (the `cie deps <dir> --json` export, US-062) ----

export interface DeclaredDep {
  name: string;
  ecosystem: string; // pypi | npm | cargo | go | maven | gradle | composer | nuget
  version_req: string; // exactly as written, never interpreted
  licence: string | null;
  scope: string; // runtime | dev
  source: string; // manifest file that declared it
}

export interface DepUsage {
  name: string;
  ecosystem: string;
  imported_by: string[];
  resolved: boolean;
  note: string; // the honesty statement, never empty
}

export interface CieDeps {
  table: {
    deps: DeclaredDep[];
    unparsed: string[]; // manifests CIE could not follow — named, never guessed
    files_scanned: number;
  };
  usage: DepUsage[];
}

// ---- runtime.json (the sample project's collected runtime evidence) ----

export interface RuntimeCpuRow {
  function: string;
  file: string;
  line: number;
  self_ms: number;
  cum_ms: number;
  calls: number;
}

export interface RuntimeSpan {
  thread: string;
  task: number;
  start_ms: number;
  end_ms: number;
}

export interface RuntimeTrial {
  threads: number;
  per_thread: number;
  expected: number;
  final: number;
  lost: number;
  wall_ms: number;
  widened?: boolean;
}

export interface RuntimeCoverageFile {
  file: string;
  statements: number | null;
  covered: number | null;
  percent: number;
  note?: string;
}

export interface CieRuntime {
  project: string;
  collected_at: string;
  environment: { python: string; platform: string; cpu_count?: number; collected_at: string };
  honesty: string;
  datasets: {
    cpu?: {
      tool: string;
      command: string;
      collected_at: string;
      top_functions: RuntimeCpuRow[];
      honesty: string;
      real_world_equivalents: string[];
    };
    concurrency?: {
      tool: string;
      command: string;
      collected_at: string;
      workers: number;
      tasks: number;
      makespan_ms: number;
      sum_of_task_ms: number;
      observed_overlap: number;
      spans: RuntimeSpan[];
      honesty: string;
      real_world_equivalents: string[];
    };
    memory?: {
      tool: string;
      command: string;
      collected_at: string;
      allocated_bytes: number;
      peak_bytes: number;
      blocks: number;
      block_size: number;
      top_sites: { where: string; bytes: number; count: number }[];
      honesty: string;
      real_world_equivalents: string[];
    };
    deadlock?: {
      tool: string;
      command: string;
      collected_at: string;
      detected: boolean;
      cycle: string[];
      waits_for: Record<string, string>;
      held_by: Record<string, string>;
      threads: {
        thread: string;
        holds: string | null;
        wants: string | null;
        blocked_at: { line: number; file: string } | null;
        stack_captured: boolean;
      }[];
      honesty: string;
      real_world_equivalents: string[];
    };
    race?: {
      tool: string;
      command: string;
      collected_at: string;
      natural_trials: RuntimeTrial[];
      widened_trials: RuntimeTrial[];
      lost_natural: number;
      lost_widened: number;
      honesty: string;
      real_world_equivalents: string[];
    };
    coverage?: {
      tool: string;
      command: string;
      collected_at: string;
      files: RuntimeCoverageFile[];
      totals: { percent: number; statements: number; covered_lines: number; note?: string };
      honesty: string;
      real_world_equivalents: string[];
    };
  };
  how_to_collect_on_your_project: {
    dataset: string;
    this_demo: string;
    on_real_projects: string;
  }[];
}

// ---------------------------------------------------------------------------
// Stories — the 33 remaining registry stories, each shown with real data
// from the sample project. Produced by
// examples/bankapp/scripts/collect_stories.py (see runtime-data/stories/):
// "measured" = a tool captured a number from a real run on a machine;
// "recorded" = the workflow walkthrough was really performed and its real
// artifacts saved. Nothing is invented; gaps stay named.
// ---------------------------------------------------------------------------

export interface StoryDemo {
  shape: "hbar" | "table" | "stat" | "donut" | "steps" | "timeline" | "kv" | "diff";
  // hbar
  data?: { name: string; value: number; color?: string }[];
  unit?: string;
  title?: string;
  // table / diff
  columns?: string[];
  rows?: (string | number)[][];
  // stat
  headline?: string;
  items?: { label: string; value: string; note?: string }[];
  // hbar extras (US-043 thinly-known list)
  thinly_known?: { part: string; authors: number; days_quiet: number }[];
  [key: string]: unknown;
}

export interface CieStory {
  id: string; // registry id, e.g. "US-043"
  title: string;
  kind: "measured" | "recorded";
  demo: StoryDemo;
  source: { label: string; detail: string };
  etl: { extract: string; transform: string; load: string };
  gap: string; // what honestly remains
  honesty: string;
  real_world_equivalents?: string[];
}

export interface CieStories {
  project: string;
  collected_at: string;
  environment: Record<string, unknown>;
  honesty: string;
  counts: { measured: number; recorded: number; total: number };
  registry: { total: number; delivered: number; partial: number; planned_shown_here: number };
  stories: Record<string, CieStory>;
  how_to_collect_on_your_project: { story: string; extract: string; on_real_projects: string }[];
}
