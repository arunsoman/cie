//! The story→capability registry: every catalogue story is dispositioned
//! against the engine's *real* shipped capabilities. Honesty rules:
//! * `Delivered` names capability refs (crate::item / cli:cmd / host:artifact);
//! * `Partial` additionally names the gap in `note` — partial stays visible;
//! * `Planned` carries no capabilities and says why in `note`;
//! * capabilities are never claimed from a name alone — refs point at
//!   modules/commands that exist in this workspace.

use serde::{Deserialize, Serialize};

/// How much of a story the shipped engine actually delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// The story's engine-level obligation is met; refs prove it.
    Delivered,
    /// Some obligations are met (refs), a named gap remains (note).
    Partial,
    /// Not shipped; `note` carries the honest reason.
    Planned,
}

impl Disposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Disposition::Delivered => "delivered",
            Disposition::Partial => "partial",
            Disposition::Planned => "planned",
        }
    }
}

/// One registry entry: story ↔ disposition ↔ evidence.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// Catalogue id, e.g. "US-001".
    pub story: &'static str,
    pub disposition: Disposition,
    /// Capability refs: `cie-<crate>::<item>`, `cli:<command>`,
    /// `host:viz-adapter/...` (host-layer artifact in the TS package).
    pub capabilities: &'static [&'static str],
    /// Honest note: named gap for `Partial`, reason for `Planned`,
    /// scoping remark for `Delivered` (e.g. view-side artifact).
    pub note: &'static str,
}

macro_rules! entry {
    ($id:literal, $d:expr, [$($cap:literal),* $(,)?], $note:literal) => {
        Entry { story: $id, disposition: $d, capabilities: &[$($cap),*], note: $note }
    };
}

use Disposition::{Delivered, Partial, Planned};

/// The full registry — exactly one entry per catalogue story, in id order.
pub const REGISTRY: &[Entry] = &[
    // ---- Repository access and orientation -------------------------------
    entry!("US-001", Delivered, ["cli:ingest", "cie-ingest::run_ingest", "cie-incremental::snapshot"],
        "Every report names the studied revision; Coverage.complete=false marks a partial pass so connecting never implies complete analysis."),
    entry!("US-002", Delivered, ["cie-core::evidence::Coverage", "cie-adapters::LanguageRegistry", "cli:analyze"],
        "Coverage matrix + per-language adapter registry; unresolvable constructs surface as findings/unknowns, never guessed."),
    entry!("US-003", Delivered, ["cie-incremental::plan_ingest", "cli:ingest", "cie-drift::pipeline"],
        "Incremental re-ingest keyed by content hash; records are rev-scoped so old answers keep their revision identity."),
    entry!("US-004", Delivered, ["cli:ingest-org", "cie-ingest::run_org_ingest"],
        "Multi-repo ingest with per-repo stores and merged concept view; identical names stay separate per repository."),
    entry!("US-005", Planned, [],
        "Offline-first engine: hosted-source pull (PRs, credential freshness, held-aside records) is scheduled, not shipped."),
    entry!("US-006", Delivered, ["cie-ingest::IngestOptions.budget_ms", "cie-llm::Budget", "cli:ingest --budget-ms"],
        "TASK-029 budgets + cancellation checkpoints; a budget-cut run reports incomplete (Coverage.complete=false), never pretends saved."),
    entry!("US-007", Delivered, ["cie-ingest::sample::generate", "cli:sample"],
        "On-demand sample repository: deterministic demo app with REAL dated git history (local git binary, offline); refuses non-empty targets unless --force, missing git is a named error. Forces re-runs append, never rewrite."),
    // ---- Asking questions and working with answers ------------------------
    entry!("US-008", Delivered, ["cie-summary", "cie-evidence::assemble", "cli:report"],
        "Answers are evidence-assembled summaries over the revision; conversational NLU is the model layer (cie-llm providers)."),
    entry!("US-009", Delivered, ["cie-llm::readings", "cie-llm::offline"],
        "Readings enumerated deterministically from the query (ranked, grounded terms, low-confidence verdict stated instead of guessed); a chosen reading pins (never second-guessed). One-click UX is product-layer; the readings are engine data."),
    entry!("US-010", Delivered, ["cie-analysis::query", "cli:impact"],
        "Composed conditions over the symbol universe with per-condition explanations; unevidenced absence lands in a suspected-gaps ledger, never a silent drop. Natural-language routing stays model-layer."),
    entry!("US-011", Delivered, ["cli:analyze", "cie-ingest::run_drift"],
        "Multi-step read-only pipelines compose in the CLI; per-step failures are reported individually (findings vs errors)."),
    entry!("US-012", Partial, ["cie-analysis::focus::FocusContext", "cie-analysis::focus::resolve_referent"],
        "Chips (ignore/pin/focus), zoom-out and ambiguous-referent resolution (named, \
         never guessed) are shipped engine primitives over SymbolId sets. Recognising \
         the steering phrase itself ('ignore X', 'zoom out') and editor-selection \
         context are product/NLU layer."),
    entry!("US-013", Delivered, ["cli:report --json", "cli:concepts --format", "cli:analyze --format sarif"],
        "Same facts rendered human/json/md/sarif — the level-of-detail seam; prose audience-tuning is model-layer."),
    entry!("US-014", Delivered, ["cie-claims::model", "cie-claims::persist", "cie-drift::pipeline"],
        "Claims carry evidence, verification status and reverse-drift entries; agreement never upgrades a claim's modality."),
    entry!("US-015", Delivered, ["cie-claims::model::Modality", "cie-core::evidence", "cie-viz::gate"],
        "Proven/inferred/hypothesised/unknown labels flow through claims, findings and the viz gate's display modes."),
    entry!("US-016", Delivered, ["cie-store::records"],
        "Derived records are typed, rev-scoped and content-addressed; answer-tab workspace UX is product-layer."),
    entry!("US-017", Delivered, ["cie-store::records::TimedRecord", "cie-fault"],
        "Fault-injection suite (TASK-052/053) proves interrupted runs report 'nothing saved' honestly and completed work survives."),
    // ---- Interactive visual exploration -----------------------------------
    entry!("US-018", Delivered, ["cie-viz::produce", "host:viz-adapter/negotiator"],
        "ViewSpec carries chart hints per question shape; negotiator picks the engine, evidence card preferred over forced graphs."),
    entry!("US-019", Delivered, ["cie-viz::hash", "host:viz-adapter"],
        "semanticHash is stable across representation/engine swaps — the same population stays identifiable (VIZ-02)."),
    entry!("US-020", Delivered, ["host:viz-adapter/catalog", "cie-viz::model"],
        "76-chart catalog as data with availability reasons; audit proves internal consistency (VIZ-23/25)."),
    entry!("US-021", Delivered, ["cie-viz::produce", "cli:impact"],
        "Spec elements carry SourceLocation down to spans; blast-radius lands on the exact symbol."),
    entry!("US-022", Delivered, ["host:viz-adapter/interaction", "cie-viz::produce"],
        "LOD tiers decided deterministically with a disclosed why (labels-only/clustered/full); settle detector collapses pan/zoom bursts before re-layout."),
    entry!("US-023", Delivered, ["host:viz-adapter/interaction"],
        "Hover-expansion lens assembles aggregate + members from spec groups without mutating the map; member/aggregate data is spec-derived (source stated)."),
    entry!("US-024", Delivered, ["cie-analysis::query", "cli:impact"],
        "Scoped selection queries: members identified, scope respected, and an EMPTY selection explained from universe facts (empty/empty-selection/stale ids are distinct reasons)."),
    entry!("US-025", Delivered, ["cie-viz::produce"],
        "Salience-bounded cap with hidden[]/gaps[] ledger: why shown, why not, and what was left out (VIZ-I-06)."),
    entry!("US-026", Delivered, ["cie-viz::model", "cie-claims::model", "cie-core::types::SourceLocation"],
        "Element payloads carry role, evidence-backed status, endpoints and exact source locations."),
    entry!("US-027", Delivered, ["cie-viz::hash", "host:viz-adapter"],
        "Same spec → same element ids across engines; unmapped items keep explicit ids instead of name-matching (VIZ-16)."),
    entry!("US-028", Delivered, ["host:viz-adapter/interaction", "cli:impact"],
        "Gestures map to typed proposals (proposed, never applied) with the engine consequences they would query (impact/deletion); unknown targets refuse."),
    entry!("US-029", Delivered, ["cie-analysis::testing::build_test_map", "cie-viz::produce"],
        "Test confidence overlays structure from the test map; only project-tooling signals are read, nothing executed."),
    entry!("US-030", Delivered, ["host:viz-adapter/interaction", "host:viz-adapter/negotiator"],
        "Layout quality measured from geometry (overlaps + crossings) with the strategy disclosed (requested vs actual); quality is asserted by measurement, not vibes."),
    // ---- The visual catalogue ---------------------------------------------
    entry!("US-031", Delivered, ["cie-analysis::hierarchy", "cie-analysis::fca", "cie-concepts"],
        "Groupings derived from concept lattice/hierarchy over code facts; unresolved boundaries render unknown."),
    entry!("US-032", Delivered, ["cie-fault::trace", "cie-fault::ldfi"],
        "Multi-language trace parser (python/rust/java/javascript) feeds LDFI: suspects ranked with factor-by-factor frame evidence; parse failures are named errors, never guessed frames."),
    entry!("US-033", Delivered, ["cie-fault::error_handling", "cie-analysis::findings"],
        "Failure sites enumerated from error-handling detectors (Yuan et al.) + findings; static branches are code-proven."),
    entry!("US-034", Delivered, ["cie-analysis::steps", "cie-relations::RelationGraph"],
        "Call-graph-ordered transaction steps with adapter-classified async hand-off marking (spawn/await/channel as lane-transition data); never invented from naming. The swim-lane VIEW stays host."),
    entry!("US-035", Delivered, ["cie-analysis::ifds", "cie-relations::DfgEdge"],
        "IFDS dataflow traces readers/writers with per-object identity; limits stated in the analysis output."),
    entry!("US-036", Delivered, ["cie-incremental::plan_ingest", "cli:report --as-of-rev"],
        "Revision comparison via incremental plans + as-of reports; unindexed revisions cannot be compared (by construction)."),
    entry!("US-037", Delivered, ["cie-ingest::history", "cie-ingest::churn"],
        "Git-history archaeology ingest via the SystemGit seam (log/numstat/blame porcelain): archaeology chains bind commits to lines oldest-first; unavailable history is a named gap, never fabricated."),
    entry!("US-038", Delivered, ["cie-adapters::effects", "cie-analysis::security"],
        "Sources/sinks/guards classified from code evidence; escape routes stay hypotheses, never asserted closed."),
    entry!("US-039", Delivered, ["cie-fault::trace", "cie-analysis::testing"],
        "Runtime exception feeds ingest through the fingerprinted exceptions inbox; the test map supplies the failing-test tinting the feed drives."),
    entry!("US-040", Delivered, ["cie-analysis::concurrency"],
        "Interleaving windows cite both statements (writer line + reader line) with spawn-reachability established from async facts; permanently hypothesis-labelled, never 'a race happened'."),
    entry!("US-041", Delivered, ["cie-incremental::proptests", "cli:impact"],
        "Deletion counterfactual with clean-recompute oracle (TASK-058); 'no consequence found' is an outcome, never a guarantee."),
    entry!("US-042", Delivered, ["cie-analysis::testing::build_test_map", "cie-analysis::testing::coverage_gaps"],
        "Behaviours-by-tests matrix with gaps as findings; reaching-code-is-not-asserting distinction enforced."),
    entry!("US-043", Planned, [],
        "No ownership/authorship analysis shipped; also privacy-constrained (US-088) — would need declared policy input."),
    entry!("US-044", Delivered, ["cie-concepts", "cli:concepts"],
        "Concept cards pinned to implementing sites; hierarchy renders only for the current revision."),
    entry!("US-045", Delivered, ["cie-contract::obligations", "cie-contract::registry"],
        "Routes×rules enforcement statuses derived from refusal-site evidence; convention-only rules marked as such."),
    entry!("US-046", Delivered, ["cie-analysis::risk", "cli:report"],
        "Composite change-risk assembles findings+drift+test-gap+churn with adjustable weights, per-factor contributions, neutral missing-data handling and the permanent 'composite, not fact' label."),
    entry!("US-047", Partial, ["cie-evidence::profile::attribute_hotspots"],
        "Imported-profile hotspot attribution with a dropped-sample-derived \
         uncertainty half-width; unresolved stack frames stay unattributed \
         rather than guessed onto a known symbol. Profile import tooling for \
         specific vendor formats and the visual hotspot view are host layer."),
    entry!("US-048", Delivered, ["cie-adapters::effects", "cie-analysis::security"],
        "Routes+guards from framework-evidence classification; no-evidence case renders as explicit unknown."),
    entry!("US-049", Delivered, ["cie-viz::produce", "host:viz-adapter"],
        "Bounded ViewSpec over indexed entities; engine names no engine (VIZ-10) and plans are reviewable descriptions."),
    // ---- Behaviour, concepts and business rules ---------------------------
    entry!("US-050", Delivered, ["cie-concepts", "cie-concepts::persist", "cli:concepts"],
        "Concept extraction as reviewable tree with persisted cards; confidence is reported, never asserted."),
    entry!("US-051", Delivered, ["cie-analysis::hierarchy", "cie-analysis::fca"],
        "Two-axis organisation: structural containment + specialisation from behaviour analysis; honesty tiers on invariants."),
    entry!("US-052", Delivered, ["cie-analysis::endpoints", "cie-concepts"],
        "Endpoint/screen extraction from route-shaped calls + literals with per-site evidence; files-scanned coverage and adapter caveats are stated."),
    entry!("US-053", Delivered, ["cie-contract::naming", "cie-contract::registry", "cli:concepts --with-rules"],
        "Rule catalogue from naming lexicon + guarded registry; enforcement locations cited per rule."),
    entry!("US-054", Delivered, ["cie-drift::pipeline::verify_claims", "cie-claims::DriftReport"],
        "Intended (document claims) vs actual (verified code) compared; mismatches classified, missing expectations separated."),
    entry!("US-055", Delivered, ["cie-contract::obligations::StateModel", "cie-contract::protocols"],
        "FR-15 state machines with RefundCalled ≠ Refunded discipline; invalid transitions and terminal states from code."),
    entry!("US-056", Delivered, ["cie-contract::naming", "cie-analysis::behavior"],
        "Naming-vs-behaviour comparison (authorize that only prints is caught by the lexicon + auth/money predicates)."),
    // ---- Architecture, dependencies and impact ----------------------------
    entry!("US-057", Delivered, ["cie-analysis::hierarchy", "cie-relations::RelationGraph"],
        "Discovered relationships (CFG/DFG/call) distinguished from directory structure; boundaries challengeable to code."),
    entry!("US-058", Delivered, ["cli:impact", "cie-ingest::IngestOutcome.reverse_deps", "cie-analysis::testing::minimum_test_set"],
        "Direct references, transitive dependents and minimum test set separated; unexamined consumers stay visible."),
    entry!("US-059", Delivered, ["cie-analysis::alternatives", "cli:impact"],
        "Alternatives comparison lists affected symbols (transitive from impact), contracts, data boundaries, risks and unresolved assumptions; outcomes are 'proposed' unless a workload citation is attached."),
    entry!("US-060", Delivered, ["cie-analysis::fca", "cie-ingest::churn"],
        "Candidate separations from concept lattices + co-change coupling from history ingest (shared-commit pairs, deterministic ordering)."),
    entry!("US-061", Delivered, ["cli:ingest-org", "cie-ingest::run_org_ingest"],
        "Cross-repo ingest with concept merge; consumers outside bounds appear as coverage gaps, not silence."),
    entry!("US-062", Delivered, ["cie-analysis::deps", "cli:deps"],
        "Manifest dependency graph (package.json/pyproject.toml/requirements.txt/Cargo.toml/go.mod; unparseable manifests named, never guessed), adapter-facts usage evidence (Python ImportFacts today — other ecosystems carry a named-gap note, never a fabricated zero), and upgrade what-if whose compatibility is ALWAYS Unknown (an available version is never presented as automatically compatible)."),
    entry!("US-063", Planned, [],
        "Binary artifact probing out of static-source scope; not shipped."),
    // ---- Debugging and investigations -------------------------------------
    entry!("US-064", Delivered, ["cie-fault::trace", "cie-fault::ldfi"],
        "Exceptions inbox: fingerprinted entries with counted occurrences (repeats collapse, nothing lost) and investigate/dismiss lifecycle; LDFI supplies the symptom→hypothesis structure."),
    entry!("US-065", Delivered, ["cie-fault::ldfi", "cie-analysis::beliefs"],
        "Competing hypotheses as inspectable lineage graphs; ranking is explained, never dressed as probability."),
    entry!("US-066", Delivered, ["cie-incremental::steer", "cie-ingest::IngestOptions"],
        "Investigation steering: pause (resumable), stop with a stated reason, scope exclusions recorded as visible limitations, and a live board separating ongoing/completed/blocked."),
    entry!("US-067", Delivered, ["cie-store::records", "cie-fault"],
        "Records validate() their bookkeeping; cancelled work is stored but marked, only retrieved evidence counts."),
    entry!("US-068", Delivered, ["cie-counterexample::package", "cie-counterexample::bounded"],
        "Counterexamples packaged with minimized inputs (min_hits shrinker); non-reproduction is reported as exactly that."),
    entry!("US-069", Delivered, ["cie-fault::next_steps"],
        "Ranked next-step list from the LDFI lineage: each check names which hypotheses it supports/challenges; unavailable tools become concrete blockers, never fabricated results."),
    entry!("US-070", Delivered, ["cie-incremental::snapshot", "cie-ingest::load_outcome"],
        "Investigations resume from persisted snapshots; staleness visible via revision comparison."),
    entry!("US-071", Delivered, ["cie-drift::causal", "cie-drift::pipeline"],
        "Causal-explanation assembly: chronology separated from the causal chain (verified-only links with symbols + counterexamples); alternatives and unresolved causes stay visible with reasons."),
    entry!("US-072", Delivered, ["cie-analysis::findings", "cie-drift::sarif"],
        "Defect candidates with rule, witness location and coverage gaps; candidates never verdicts, SARIF-exportable."),
    entry!("US-073", Delivered, ["cie-fault"],
        "Injection-based corroboration over reviewed sources; external runtimes are not invoked — refused guarantees are named."),
    entry!("US-074", Delivered, ["cie-evidence::workload", "cie-adapters::effects"],
        "Named-workload manifest: a claim is 'measured' only with a workload-backed timing entry (orphan timings refuse); candidates carry affected behaviour + correctness constraints."),
    // ---- Runtime, concurrency and data consistency ------------------------
    entry!("US-075", Partial, ["cie-evidence::runtime::join_structure_with_runtime"],
        "Static symbol joined against imported runtime observations with Exact/ \
         Partial/Unknown exactness (revision+deployment agreement); no 'unused' \
         verdict exists in the type, so a quiet window can never produce one. \
         The runtime overlay view and a live ingest feed are host layer."),
    entry!("US-076", Planned, [],
        "Recorded-request replay not shipped."),
    entry!("US-077", Partial, ["cie-evidence::environment::compare_environments", "cie-evidence::environment::explore_flag_change"],
        "Version/flag differences enumerated between two declared snapshots; \
         explore_flag_change returns a new, clearly-labelled snapshot and never \
         mutates the base or applies anywhere. Live configuration ingest and the \
         environment-topology view are host layer."),
    entry!("US-078", Delivered, ["cie-fault::ldfi", "cie-evidence::readiness::Calibration"],
        "Lineage separates causal chains from co-occurrence within static scope; runtime timing evidence arrives through the calibration import (revision- and environment-bound, staleness exposed)."),
    entry!("US-079", Delivered, ["cie-analysis::concurrency"],
        "Witness proposals per suspected race cite both statements + exploration scope; the permanent caveat (passing bounded tests is never a race-free certificate) ships with every proposal."),
    entry!("US-080", Delivered, ["cie-analysis::concurrency"],
        "Wait-for ordering risks from lock sites + call graph, each with its call path; static analysis has no observed cycles — every edge labelled suspected."),
    entry!("US-081", Planned, [],
        "Distributed consistency hazard modelling not shipped."),
    // ---- Performance and technical health ---------------------------------
    entry!("US-082", Partial, ["cie-evidence::profile::category_breakdown"],
        "CPU/wait/data-work/external-call/missing shares distinguished over an \
         imported Profile; dropped samples count only as missing, never a \
         guessed category. Hotspot attribution is US-047. The trace-waterfall \
         view and speculative-mechanism labelling UI are host layer."),
    entry!("US-083", Partial, ["cie-evidence::profile::compare_profiles"],
        "Declared-equivalence duration comparison: a failed same_workload/same_build \
         declaration or a missing measured duration rejects the comparison with named \
         reasons instead of comparing anyway. The metric-table view is host layer."),
    entry!("US-084", Partial, ["cie-evidence::profile::regression_report"],
        "Wraps compare_profiles with an improvement_confirmed guard: a candidate \
         that is faster only by raising error_count or dropped_samples is never \
         marked confirmed. Workload/environment-difference narration and the \
         baseline/candidate chart view are host layer."),
    entry!("US-085", Delivered, ["cie-evidence::workload", "cie-analysis::findings"],
        "Pattern candidates with evidence and correctness constraints; claim classes split measured/potential by construction — no measured-speedup claim without an imported workload run."),
    entry!("US-086", Delivered, ["cie-ingest::churn"],
        "Churn factors with exclusion ledger (bulk edits, authors, subjects) reporting each rule's re-ranking effect, rank-stability under weight sweeps, and the lower-bound note (sampled history, no calibrated probabilities)."),
    entry!("US-087", Delivered, ["cie-ingest::cochange::co_change_report", "cie-ingest::history::SystemGit", "cli:cochange"],
        "Co-change coupling from REAL git history (SystemGit seam): rename rows resolved to final canonical names (chains closed transitively), bulk-edit commits excluded with the effect counted, per-edge support counts + supporting commit ids + cross-directory flags exposed; labelled historical coupling — never presented as a static or runtime dependency."),
    // ---- History, collaboration and knowledge ------------------------------
    entry!("US-088", Planned, [],
        "History analysis not shipped; no contributor-identity fields exist anywhere in the stores (nothing to protect yet), and privacy-at-source stays a design constraint for when history lands."),
    entry!("US-089", Delivered, ["cie-incremental::timeline"],
        "Contract timelines: per-revision presence/contract record; removal/reinstatement/contract-change concerns cite revisions + affected clients from import maps; unknown consumers remain a named coverage gap."),
    entry!("US-090", Delivered, ["cie-drift::pipeline", "cli:drift"],
        "Documentation gaps via claim extraction + verification; undocumented behaviour is never called approved."),
    entry!("US-091", Delivered, ["cie-claims::share", "cie-claims::persist"],
        "Share manifests cut to the reader's visibility with redaction listed WITHOUT revelation; decision trails are append-only (disagreement recorded, never erased) with private-note scoping."),
    entry!("US-092", Delivered, ["cli:analyze --format", "cie-drift::sarif"],
        "Export to json/md/sarif with caveats preserved in-band."),
    entry!("US-093", Delivered, ["cie-store", "cie-incremental::snapshot", "cie-concepts::persist"],
        "Workspace state persisted (snapshots, concept cards, claims); resume is load_outcome + freshness checks."),
    // ---- Security, permissions and supply chain ---------------------------
    entry!("US-094", Delivered, ["cie-analysis::security", "cie-analysis::behavior"],
        "Authorization posture from guard evidence; guard names alone never earn 'protected'."),
    entry!("US-095", Delivered, ["cie-analysis::ifds", "cie-adapters::effects"],
        "Source→sink taint traces per object identity; coverage limits stated."),
    entry!("US-096", Delivered, ["cie-contract::obligations", "cie-contract::registry"],
        "Enforcement-gap matrix from refusal-site evidence; bypassable routes counted, not averaged away."),
    entry!("US-097", Delivered, ["cie-analysis::security", "cie-drift::sarif"],
        "Findings are candidates with severity + witness paths; 'no finding' never means 'safe'."),
    entry!("US-098", Partial, ["cie-analysis::deps::licence_report", "cli:deps --licences"],
        "GAP NAMED: advisory/vulnerability feed scanning — offline-first refuses to fabricate feed data. Shipped: licence obligations read as written from manifests (undeclared licences named, never guessed), provenance stated as unverified, per-package source named."),
    // ---- Counterfactuals and digital twin ---------------------------------
    entry!("US-099", Delivered, ["cie-incremental::whatif", "cli:impact"],
        "What-if change-set simulation (remove/split) on a cloned world: structural consequences read off the diff, inferred adaptations named; measured numbers deliberately not borrowed; permanently labelled not-implemented."),
    entry!("US-100", Delivered, ["cie-evidence::readiness"],
        "Capacity sandbox over a stated linear domain with provenance-carrying assumptions; unsupported inputs are refused (no invented numbers) and every outcome is labelled exploratory."),
    entry!("US-101", Planned, [],
        "Sandbox experiment execution not shipped (offline-first refuses overreach)."),
    entry!("US-102", Delivered, ["cie-contract::obligations::StateModel", "cie-evidence::readiness::Calibration"],
        "State-machine twin stays static-labelled; runtime calibration imports bind observed histograms to revision + environment with observed/assumed/unmodelled states and staleness."),
    entry!("US-103", Delivered, ["cie-claims::model::Modality", "cie-store::records"],
        "Every derived record carries modality + provenance; predictions never display as fact."),
    // ---- Prompt-to-feature discovery and planning -------------------------
    entry!("US-104", Partial, ["cie-contract::feature::request::FeatureRequest"],
        "FeatureRequest carries repository + deliverable (plan/preview/draft-PR) + \
         detail level (prompt/brief/specification/examples) with no field requiring \
         an internal framework/component name. The intake UI is product layer."),
    entry!("US-105", Partial, ["cie-contract::feature::clarify::ClarificationSet"],
        "ClarificationQuestion::new refuses a question with no stated reason; \
         fully_answered only becomes true once every question has an explicit \
         answer — silence never authorizes one. Deciding which questions are \
         material (vs noise) is product/model layer."),
    entry!("US-106", Delivered, ["cie-concepts::overlap"],
        "Concept overlap matrix with reuse/add decomposition — duplicate detection is structural (shared symbols), so 'B is X plus Y' names what reuses X and what Y adds despite differing names."),
    entry!("US-107", Delivered, ["cie-drift::conflicts", "cie-drift::pipeline"],
        "Conflict ledger: source statements + applicability + concrete scenario + options; resolution requires an owner, history is append-only, and resolution returns the affected expectations to update."),
    entry!("US-108", Partial, ["cie-contract::feature::contract::FeatureContract"],
        "Scope/actors/rules/failure-paths/assumptions/acceptance-criteria with every \
         requirement carrying a RequirementOrigin and open_omissions keeping material \
         gaps visible — a real, tested contract assembly. Not promoted: the story \
         also names correcting the brief before or during implementation, which \
         nothing in this crate implements yet (no revision/edit path exists for a \
         contract once assembled), and there is no view exposing it; that's host layer."),
    entry!("US-109", Delivered, ["cie-analysis::impact_dims", "cli:impact"],
        "Impact dimensions (permission/data/operations/performance/interface) from classified effect classes, every hit cited; empty dimensions state themselves (no auth surface is a statement, not a blank); planned label permanent."),
    entry!("US-110", Planned, [],
        "Scope-revision workflow is product layer."),
    // ---- Feature implementation and inspection ----------------------------
    entry!("US-111", Planned, [],
        "Candidate-build workflow is product layer; the engine never mutates working trees (by design)."),
    entry!("US-112", Planned, [],
        "Feature wizard is product layer."),
    entry!("US-113", Delivered, ["cie-incremental::attrib", "cie-incremental::plan_ingest"],
        "Requirement attribution ledger: six change classes (added/modified/deleted/renamed/reused/affected-but-unchanged) searchable per requirement; renames/reuse require the caller's identity mapping (never guessed)."),
    entry!("US-114", Delivered, ["cli:report--as-of-rev", "cie-incremental::diffgen"],
        "Exact comparisons: LCS-based unified + side-by-side renderers with disclosed truncation and binary disclosure; full-code view is the stored content unmodified."),
    entry!("US-115", Planned, [],
        "Behaviour preview/execution not shipped."),
    entry!("US-116", Planned, [],
        "Dependency/data-change review not shipped (see US-062)."),
    // ---- Validation and safe delivery -------------------------------------
    entry!("US-117", Planned, [],
        "Build execution is outside the offline static engine; would be refused, honestly."),
    entry!("US-118", Delivered, ["cie-analysis::testing::build_test_map", "cie-analysis::testing::minimum_test_set"],
        "Requirement-adjacent tests via test map + minimum test set for changed symbols."),
    entry!("US-119", Delivered, ["cie-evidence::readiness", "cie-incremental::proptests"],
        "External evidence imports with expected-result source, assertion-weakening downgrade and human-confirmation gating for generated expectations; the internal 126+ check harness remains the acceptance floor."),
    entry!("US-120", Delivered, ["cie-evidence::readiness", "cie-analysis::security"],
        "Readiness rollup over security/dependency/performance/operational/functional checks: missing tools are not_determined with the tool named; waivers disclose without upgrading status."),
    entry!("US-121", Planned, [],
        "Fix-request loop is product layer."),
    entry!("US-122", Planned, [],
        "Patch export is product layer."),
    entry!("US-123", Planned, [],
        "Apply-to-destination is product layer."),
    entry!("US-124", Planned, [],
        "Draft-PR creation is product layer."),
    entry!("US-125", Planned, [],
        "Mutation tracing is product layer."),
    entry!("US-126", Planned, [],
        "Multi-change coordination is product layer."),
    // ---- Release and delivery oversight -----------------------------------
    entry!("US-127", Delivered, ["cie-evidence::scope", "cie-store::snapshot"],
        "Scope freeze: post-freeze arrivals marked for assessment (never silently admitted), committed/stretch/late counts exposed, and the scope replays totally from its recorded history."),
    entry!("US-128", Delivered, ["cie-evidence::scope", "cli:report"],
        "Stakeholder lenses sort blocking-first per role with the four states distinct and the tone stated plainly (ready/conditional/blocked/unknown); unknowns stay unknowns."),
    entry!("US-129", Planned, [],
        "Approval workflow is product layer."),
    entry!("US-130", Planned, [],
        "Rollout/recovery planning is product layer."),
    entry!("US-131", Planned, [],
        "Anomaly↔change linking needs runtime ingest (US-075 gap)."),
    // ---- Editor and agent integrations ------------------------------------
    entry!("US-132", Planned, [],
        "Editor integrations not shipped."),
    entry!("US-133", Delivered, ["cie-semantic::graph", "cli:impact", "cie-ingest::IngestOutcome.reverse_deps"],
        "Definitions/references navigable from the semantic graph + reverse deps."),
    entry!("US-134", Delivered, ["cie-server::mcp", "cie-viz", "cie-store::records"],
        "MCP server seam: JSON-RPC 2.0 over stdio, read-only declared-scope tools, budget exhaustion as an explicit error (no partial fabrications), honesty envelope leading with claim classes/citations/gaps/revision/staleness."),
    // ---- Privacy, models and product control ------------------------------
    entry!("US-135", Delivered, ["cie-llm::offline", "cli:ingest --llm-provider"],
        "Offline-first default: no network needed for any core flow."),
    entry!("US-136", Delivered, ["cie-llm::Budget", "cli:ingest --no-llm"],
        "LLM use is explicit opt-in with provider choice + budget; the offline default sends nothing anywhere."),
    entry!("US-137", Delivered, ["cie-llm::ProviderKind", "cie-llm::cache"],
        "Provider swap (offline/ollama/openai) + response cache; failures surface as errors, offline provider never invents."),
    entry!("US-138", Delivered, ["cie-llm::Budget::exceeded", "cli:ingest --llm-budget-usd"],
        "USD + time budgets with cancellation checkpoints; exceeded budget stops cleanly."),
    entry!("US-139", Delivered, ["cie-incremental::proptests", "cie-store"],
        "Content-addressed deletion with clean-recompute oracle (TASK-058): deletion provably leaves no orphans."),
    // ---- Accessibility and interaction parity -----------------------------
    entry!("US-140", Delivered, ["host:viz-adapter/interaction", "cli:ingest"],
        "Keyboard parity is a verified contract: every pointer action has a key path (select/zoom/reset/move/connect/delete) and the verifier fails when one goes missing; CLI flows stay keyboard-first."),
    entry!("US-141", Delivered, ["host:viz-adapter/outline", "host:viz-adapter/interaction"],
        "Announcements derive from the spec (label + how-known + evidence file) with roving order following the authoritative outline; screen-reader UX tuning stays host."),
    entry!("US-142", Delivered, ["host:viz-adapter/interaction", "host:viz-adapter/outline"],
        "Never-colour-alone: every mode carries shape + text-prefix channels; a renderer override that collides on shape is a first-class audit finding (VIZ-V-03)."),
    entry!("US-143", Delivered, ["host:viz-adapter/interaction"],
        "Theme contract is role-keyed semantic tokens (never raw colours); the theme verifier refuses partial themes and the text outline carries the information under any theme."),
    // ---- Honest operation --------------------------------------------------
    entry!("US-144", Delivered, ["cie-incremental", "cie-drift", "cie-store::records"],
        "Records are rev-scoped; drift flags stale conclusions; old answers keep revision identity, never relabelled."),
    entry!("US-145", Delivered, ["cie-viz::gate", "cie-docs::ocr", "cie-core::evidence::Coverage"],
        "Honest degradation codes (ocr_unavailable, gate violations, incomplete coverage) — unknown is explicit, never guessed."),
    entry!("US-146", Delivered, ["cie-store::records", "cie-fault"],
        "Fault-injection suite proves interruption saves nothing or only validated state; validate() on read."),
    entry!("US-147", Delivered, ["cie-viz::gate", "cie-viz::hash", "cie-incremental::proptests"],
        "Gate+hash roundtrip, proptest oracles and the acceptance harness verify the product against its own claims."),
];

/// Registry entry for a story id.
pub fn entry_for(story_id: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.story == story_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::STORIES;

    #[test]
    fn registry_covers_every_story_exactly_once() {
        assert_eq!(REGISTRY.len(), STORIES.len(), "one entry per story");
        for (s, e) in STORIES.iter().zip(REGISTRY.iter()) {
            assert_eq!(s.id, e.story, "registry must follow catalogue order");
        }
        for e in REGISTRY {
            assert!(crate::catalog::Story::by_id(e.story).is_some());
        }
    }

    #[test]
    fn disposition_rules_are_honest() {
        for e in REGISTRY {
            match e.disposition {
                Delivered => {
                    assert!(
                        !e.capabilities.is_empty(),
                        "{}: Delivered needs capability refs",
                        e.story
                    );
                }
                Partial => {
                    assert!(
                        !e.capabilities.is_empty(),
                        "{}: Partial needs the delivered part named",
                        e.story
                    );
                    assert!(!e.note.is_empty(), "{}: Partial must name its gap", e.story);
                }
                Planned => {
                    assert!(
                        e.capabilities.is_empty(),
                        "{}: Planned must not claim capabilities",
                        e.story
                    );
                    assert!(!e.note.is_empty(), "{}: Planned must say why", e.story);
                }
            }
        }
    }

    #[test]
    fn capability_refs_are_well_formed() {
        for e in REGISTRY {
            for cap in e.capabilities {
                let ok = cap.starts_with("cie-")
                    || cap.starts_with("cli:")
                    || cap.starts_with("host:");
                assert!(ok, "{}: bad capability ref '{}'", e.story, cap);
            }
        }
    }

    #[test]
    fn delivered_references_real_workspace_items() {
        // Spot-check that cited crates exist in the workspace (name-level
        // honesty guard; full API-level checks live in the verify suites).
        let mut crates = std::collections::BTreeSet::new();
        for e in REGISTRY {
            for cap in e.capabilities {
                if let Some(rest) = cap.strip_prefix("cie-") {
                    let name = rest.split("::").next().unwrap();
                    crates.insert(format!("cie-{name}"));
                }
            }
        }
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("cie-stories sits directly under the workspace root");
        for c in crates {
            assert!(
                workspace_root.join(&c).join("Cargo.toml").exists(),
                "cited crate does not exist: {c}"
            );
        }
    }

    #[test]
    fn planned_entries_carry_no_capabilities() {
        for e in REGISTRY {
            if e.disposition == Planned {
                assert!(e.capabilities.is_empty());
            }
        }
    }
}
