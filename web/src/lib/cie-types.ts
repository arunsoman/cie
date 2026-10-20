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
  // Serde externally-tagged enum ObligationKind: named variants serialize as
  // plain strings ("MustAuthorize"), Custom carries the rule in the code's
  // own words ({"Custom": "must refuse transfers above the spending limit"}).
  obligation: string | { Custom: string } | Record<string, unknown>;
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

export interface TransitionView {
  from: string; event: string; to: string; provenance: string; display_mode: "Fact";
  evidence: { symbol: number; effect: string; external_write: boolean; reads: string[]; writes: string[] }[];
}
export interface StateModelView { transitions: TransitionView[]; domain_note: string }
export interface CieReport {
  state_model?: StateModelView | null;
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
  primary_edge: string; // PackageContainment | CallOwnership | FcaSubsumption | ClusterCohesion ...
  symbols: number[];
  behaviors: ConceptBehavior[];
  rules?: ConceptRule[];
  // Present on FcaSubsumption (primary_edge) nodes only: the shared
  // behavior/effect tags (cie-analysis/src/hierarchy.rs's FCA pass) that
  // put every symbol in `symbols` into this same formal concept.
  intent?: string[];
}

export interface ConceptTree {
  nodes: ConceptNode[];
  unattached_behaviors: ConceptBehavior[];
  unattached_rules: ConceptRule[];
  dirty: boolean;
}

// ---- cochange.json (the `cie cochange <dir> --json` export) ----
// Historical git co-change coupling — files that tend to change together.
// Needs real commit history: a shallow (--depth 1) clone has almost none,
// so this is honest but thin for URL-connected projects; full for a local
// folder read in place.

export interface CochangeEdge {
  a: string;
  b: string;
  support: number; // commits touching both paths
  cross_directory: boolean;
  supporting_commits: string[];
}

export interface CieCochange {
  sample_commits: number;
  counted_commits: number;
  excluded_bulk: number;
  rename_events: number;
  edges: CochangeEdge[];
  note: string;
}

// ---- impact.json (the `cie impact <dir> --symbol <name> --json` export)
// On-demand only — this one needs a live `cie` call per symbol, so it is
// never pre-computed at ingest time. Only available when the project's
// files are still on disk (a local folder read in place); a cloned URL's
// checkout is deleted right after ingest.

export interface CieImpact {
  symbol: string;
  affected_symbols: string[];
  minimum_test_set: string[];
  affected_concepts: string[];
}

// ---- drift.json (the `cie drift <docs...> --code <dir> --format json`
// export) — a real claim-extraction + verification pass over the project's
// own docs, not the git-date proxy quality.json's demo stand-in uses.

export interface DriftDocLoc {
  doc: string;
  section?: string;
  line: number;
  char_start: number;
  char_end: number;
}

export interface DriftClaim {
  id: number;
  doc: string;
  text: string;
  kind: string; // Capability | Prohibition | Invariant | ...
  modality: string; // Must | Will | ...
  loc: DriftDocLoc;
  confidence: number;
  extractor: string;
  created_ns: number;
}

export interface DriftClaimMapping {
  claim: number;
  symbols: number[];
  confidence: number;
  proposer: string;
}

export interface DriftVerification {
  claim: number;
  status: string; // Implemented | Partial | Missing | Contradicting | Drifted | Undocumented | Unverifiable | Unknown | Superseded
  checker: string;
  evidence: string[];
  counterexamples: string[];
  symbols: number[];
  reason: string | null;
  verified_ns: number;
  compute_ns: number;
}

export interface DriftReverseEntry {
  symbol: number;
  qual_name: string;
  reason: string;
}

export interface CieDrift {
  rev: number;
  docs: string[];
  claims: DriftClaim[];
  mappings: DriftClaimMapping[];
  verifications: DriftVerification[];
  reverse_drift: DriftReverseEntry[];
  aggregate: Record<string, number>; // status -> count
  complete: boolean;
  generated_ns: number;
}

// ---- overlap.json (the `cie overlap <dir> --json` export) — capability
// overlap over the project's own concept nodes (cie-concepts/src/overlap.rs,
// US-106): which named parts of the project share real symbols, so "does
// this already exist" is a structural comparison, not a guess from names.

export interface OverlapPair {
  a: string;
  b: string;
  shared: string[]; // symbol qual_names present in BOTH
  only_a: string[];
  only_b: string[];
  similarity: number; // 0..1, Jaccard over the three sets above
}

export interface CieOverlap {
  pairs: OverlapPair[];
  unmapped: string[]; // concepts with no symbols — can't be compared
  min_shared: number;
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

// ---------------------------------------------------------------------------
// Quality / health — the ETL export produced by
// examples/bankapp/scripts/collect_quality.py (runtime-data/quality.json):
// real extracted sources (the sample's files, the hosting repo's git history,
// the GitHub issues API recorded as found, a measured coverage export) and
// two live probes (seeded fuzz for crash rate, timed fail→fix→recheck loops
// for MTTR). Every number traces back to the collector; absence is stated.
// ---------------------------------------------------------------------------

export interface QualityComplexityFn {
  file: string;
  function: string;
  cc: number;
  loc: number;
  line: number;
}

export interface QualityComplexity {
  tool: string;
  formula: string;
  totals: { files: number; functions: number; loc: number; avg_cc: number; max_cc: number };
  functions: QualityComplexityFn[];
  files: { file: string; loc: number; functions: number; avg_cc: number; max_cc: number }[];
  real_world_equivalents?: string[];
}

export interface QualityDupPair {
  a_file: string;
  a_line: number;
  b_file: string;
  b_line: number;
  lines: number;
  preview: string;
}

export interface QualitySecurityIssue {
  id: string;
  rule: string;
  cwe: string;
  name: string;
  severity: "Critical" | "High" | "Medium" | "Low" | string;
  file: string;
  line: number;
  snippet: string;
  why: string;
}

export interface QualityChurnFile {
  file: string;
  added: number;
  deleted: number;
  churn: number;
  commits: number;
  last_change: string | null;
}

export interface QualityHotspotRow {
  file: string;
  churn: number;
  max_cc: number;
  urgent_findings: number;
  coverage_percent: number | null;
  risk: number;
}

export interface QualityDriftDoc {
  doc: string;
  describes: string;
  doc_last_change: string | null;
  code_last_change: string | null;
  doc_age_days: number;
  code_newest_age_days: number;
  drift_days: number;
  note: string;
}

export interface QualityDriftCode {
  file: string;
  last_change: string | null;
  tests_last_change: string | null;
  test_lag_days: number | null;
  note: string;
}

export interface QualityMetricRow {
  id: string;
  name: string;
  what_it_measures: string;
  typical_calculation: string;
  value: number | null;
  unit: string;
  value_text: string;
  how_computed_here: string;
  source: string;
  scope: string;
}

export interface CieQuality {
  project: string;
  collected_at: string;
  honesty: string;
  scopes: { sample: string; hosting_repo: string };
  complexity: QualityComplexity;
  duplication: {
    tool: string;
    min_block: number;
    blocks_found: number;
    duplicated_lines: number;
    pairs: QualityDupPair[];
    real_world_equivalents?: string[];
  };
  security: {
    tool: string;
    honesty: string;
    rules: { id: string; cwe: string; name: string; severity: string; why: string }[];
    issues: QualitySecurityIssue[];
    by_category: Record<string, number>;
    by_severity: Record<string, number>;
    real_world_equivalents?: string[];
  };
  coverage_heatmap: {
    tool: string | null;
    command?: string;
    collected_at?: string;
    files: { file: string; statements: number | null; covered: number | null; percent: number; note?: string }[];
    totals: { percent?: number; statements?: number; covered_lines?: number; note?: string } | null;
    honesty: string;
  };
  churn: {
    tool: string;
    formula: string;
    sample: { per_file: QualityChurnFile[]; commits_touching_sample: number };
    hosting_repo: { commits_total: number; daily: { day: string; added: number; deleted: number; churn: number }[] };
    honesty: string;
  };
  hotspots: {
    formula: string;
    rows: QualityHotspotRow[];
    real_world_equivalents?: string[];
  };
  drift: {
    tool: string;
    document: QualityDriftDoc[];
    code: QualityDriftCode[];
    honesty: string;
  };
  defects: {
    tracker: {
      service: string;
      repo: string;
      api_ok: boolean;
      http_status: number;
      auth: string;
      rate_limit_remaining: number | null;
      issues_total: number | null;
      bugs: { number: number; title: string; state: string; created_at: string; closed_at: string | null; labels: string[]; is_bug: boolean }[];
      note: string;
      fetched_at: string;
    };
    fix_commits: {
      source: string;
      count: number;
      first: string | null;
      last: string | null;
      recent: { date: string; sha: string; title: string }[];
    };
    derived: {
      defect_density_per_kloc?: number;
      kloc_measured?: number;
      arrival_rate_per_day?: number;
      mtbf_hours?: number;
      proxy_note?: string;
    };
    real_world_equivalents?: string[];
  };
  crash_probe: {
    tool: string;
    definition: string;
    sessions: number;
    crashes: number;
    crash_rate: number | null;
    crashes_detail: { module: string; function: string; session: string; crashed: boolean; reason: string; timed_out: boolean }[];
    honesty: string;
  };
  repair_probe: {
    tool: string;
    iterations: { bug: string; test: string; detected: boolean; detect_s: number; repair_s: number; recheck_s: number; total_s: number; recheck_passed: boolean }[];
    mttr_s: number | null;
    mttr_text: string;
    honesty: string;
  };
  metrics: QualityMetricRow[];
  how_to_collect_on_your_project: { dataset: string; this_demo: string; on_real_projects: string }[];
}
