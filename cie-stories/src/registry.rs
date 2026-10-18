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
    entry!("US-007", Partial, ["cie-ingest::sample::generate_sample_repo", "cli:sample"],
        "A dated 4-commit demo app (git history, no network) is generated on demand \
         via `cie sample`; refuses to overwrite a non-empty directory. Wiring the \
         result into a one-click evaluator workspace is host/product layer."),
    // ---- Asking questions and working with answers ------------------------
    entry!("US-008", Delivered, ["cie-summary", "cie-evidence::assemble", "cli:report"],
        "Answers are evidence-assembled summaries over the revision; conversational NLU is the model layer (cie-llm providers)."),
    entry!("US-009", Partial, ["cie-llm::offline"],
        "Deterministic offline provider is inspectable; interpretation-alternative readings (one-click) are product-layer."),
    entry!("US-010", Partial, ["cli:impact", "cie-analysis::testing"],
        "Condition composition exists as engine queries (symbol × tests × changes); natural-language multi-condition routing is model-layer."),
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
    entry!("US-022", Partial, ["host:viz-adapter"],
        "Level-of-detail/settle detection is host interaction; the spec carries grouping + labels for it."),
    entry!("US-023", Partial, ["host:viz-adapter"],
        "Hover-expansion lens is host-side; member/aggregate data is provided by the spec."),
    entry!("US-024", Partial, ["cli:impact"],
        "Selection scoping is host/product layer; the engine answers scoped queries (impact --symbol) it would drive."),
    entry!("US-025", Delivered, ["cie-viz::produce"],
        "Salience-bounded cap with hidden[]/gaps[] ledger: why shown, why not, and what was left out (VIZ-I-06)."),
    entry!("US-026", Delivered, ["cie-viz::model", "cie-claims::model", "cie-core::types::SourceLocation"],
        "Element payloads carry role, evidence-backed status, endpoints and exact source locations."),
    entry!("US-027", Delivered, ["cie-viz::hash", "host:viz-adapter"],
        "Same spec → same element ids across engines; unmapped items keep explicit ids instead of name-matching (VIZ-16)."),
    entry!("US-028", Partial, ["cli:impact"],
        "Gesture→proposal is host layer; consequence computation (impact/deletion) is shipped engine work."),
    entry!("US-029", Delivered, ["cie-analysis::testing::build_test_map", "cie-viz::produce"],
        "Test confidence overlays structure from the test map; only project-tooling signals are read, nothing executed."),
    entry!("US-030", Partial, ["host:viz-adapter"],
        "Auto-layout with disclosed strategy (VIZ-D-03); layout quality reporting is host-side."),
    // ---- The visual catalogue ---------------------------------------------
    entry!("US-031", Delivered, ["cie-analysis::hierarchy", "cie-analysis::fca", "cie-concepts"],
        "Groupings derived from concept lattice/hierarchy over code facts; unresolved boundaries render unknown."),
    entry!("US-032", Partial, ["cie-fault::ldfi"],
        "LDFI ranks fault candidates from evidence; raw stack-trace parsing is not shipped."),
    entry!("US-033", Delivered, ["cie-fault::error_handling", "cie-analysis::findings"],
        "Failure sites enumerated from error-handling detectors (Yuan et al.) + findings; static branches are code-proven."),
    entry!("US-034", Partial, ["cie-relations::RelationGraph"],
        "Step order from CFG/call graph; async hand-off marking is partial (adapter effects classify, swim-lane view is host)."),
    entry!("US-035", Delivered, ["cie-analysis::ifds", "cie-relations::DfgEdge"],
        "IFDS dataflow traces readers/writers with per-object identity; limits stated in the analysis output."),
    entry!("US-036", Delivered, ["cie-incremental::plan_ingest", "cli:report --as-of-rev"],
        "Revision comparison via incremental plans + as-of reports; unindexed revisions cannot be compared (by construction)."),
    entry!("US-037", Partial, ["cie-drift::pipeline"],
        "Comment-claims extracted and verified; git-history archaeology ingest not shipped (working-tree analysis only)."),
    entry!("US-038", Delivered, ["cie-adapters::effects", "cie-analysis::security"],
        "Sources/sinks/guards classified from code evidence; escape routes stay hypotheses, never asserted closed."),
    entry!("US-039", Partial, ["cie-analysis::testing"],
        "Failing-test tinting available from the test map; runtime exception feeds are not ingested."),
    entry!("US-040", Partial, ["cie-adapters::effects"],
        "Shared-state access classified; interleaving windows are hypothesis-only and labelled as such."),
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
    entry!("US-046", Partial, ["cie-drift::outputs", "cli:report"],
        "Ingredients shipped (findings, drift, coverage); the composite change-risk formula is not assembled."),
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
    entry!("US-052", Partial, ["cie-concepts", "cli:concepts"],
        "Concept→site mapping shipped; screen/endpoint extraction limited to what shipped adapters resolve."),
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
    entry!("US-059", Partial, ["cli:impact"],
        "Impact ingredients shipped; the alternatives/tradeoff comparison is product reasoning over them."),
    entry!("US-060", Partial, ["cie-analysis::fca", "cie-analysis::hierarchy"],
        "Candidate separations from concept lattices; coupling view partial (co-change needs history ingest)."),
    entry!("US-061", Delivered, ["cli:ingest-org", "cie-ingest::run_org_ingest"],
        "Cross-repo ingest with concept merge; consumers outside bounds appear as coverage gaps, not silence."),
    entry!("US-062", Partial, ["cie-contract::dependency::DependencyRiskMap"],
        "Usage evidence, imported advisories and upgrade candidates are kept as \
         separate sections; CompatibilityVerdict defaults to Unknown so an \
         available version is never auto-presented as compatible. Manifest \
         parsing per ecosystem and advisory-feed import are not shipped."),
    entry!("US-063", Planned, [],
        "Binary artifact probing out of static-source scope; not shipped."),
    // ---- Debugging and investigations -------------------------------------
    entry!("US-064", Partial, ["cie-fault::ldfi"],
        "Symptom→hypothesis structure via LDFI planner; exceptions inbox collection not shipped."),
    entry!("US-065", Delivered, ["cie-fault::ldfi", "cie-analysis::beliefs"],
        "Competing hypotheses as inspectable lineage graphs; ranking is explained, never dressed as probability."),
    entry!("US-066", Partial, ["cie-ingest::IngestOptions"],
        "Scope/lang/budget constraints shipped; live pause/steer of long investigations is partial."),
    entry!("US-067", Delivered, ["cie-store::records", "cie-fault"],
        "Records validate() their bookkeeping; cancelled work is stored but marked, only retrieved evidence counts."),
    entry!("US-068", Delivered, ["cie-counterexample::package", "cie-counterexample::bounded"],
        "Counterexamples packaged with minimized inputs (min_hits shrinker); non-reproduction is reported as exactly that."),
    entry!("US-069", Partial, ["cie-fault::ldfi"],
        "Lineage names discriminating sites; the ranked next-step list is not assembled."),
    entry!("US-070", Delivered, ["cie-incremental::snapshot", "cie-ingest::load_outcome"],
        "Investigations resume from persisted snapshots; staleness visible via revision comparison."),
    entry!("US-071", Partial, ["cie-drift::pipeline::build_report"],
        "Claim chronology shipped; causal-explanation assembly is partial."),
    entry!("US-072", Delivered, ["cie-analysis::findings", "cie-drift::sarif"],
        "Defect candidates with rule, witness location and coverage gaps; candidates never verdicts, SARIF-exportable."),
    entry!("US-073", Delivered, ["cie-fault"],
        "Injection-based corroboration over reviewed sources; external runtimes are not invoked — refused guarantees are named."),
    entry!("US-074", Partial, ["cie-adapters::effects"],
        "I/O-under-lock class patterns classified; measured claims require named workloads (none imported yet)."),
    // ---- Runtime, concurrency and data consistency ------------------------
    entry!("US-075", Planned, [],
        "Runtime observation ingest not shipped; structure-only comparisons would be dishonest."),
    entry!("US-076", Planned, [],
        "Recorded-request replay not shipped."),
    entry!("US-077", Planned, [],
        "Environment/flag comparison not shipped."),
    entry!("US-078", Partial, ["cie-fault::ldfi"],
        "Lineage separates causal chains from co-occurrence within static scope; runtime timing evidence absent."),
    entry!("US-079", Partial, ["cie-adapters::effects"],
        "Static race suspicion only, labelled as such; witness execution not shipped (passing tests are never a certificate)."),
    entry!("US-080", Partial, ["cie-adapters::effects"],
        "Lock primitives classified; wait/resource graph not assembled."),
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
    entry!("US-085", Partial, ["cie-adapters::effects", "cie-analysis::findings"],
        "Pattern-based opportunity candidates only; no measured-speedup claims without experiments, by construction."),
    entry!("US-086", Partial, ["cie-analysis::findings", "cli:report"],
        "Complexity/coverage factors shipped; churn/history factors require git ingest (US-037 gap)."),
    entry!("US-087", Partial, ["cie-ingest::churn::co_change_coupling"],
        "Directory-crossing coupling with support counts, inspectable supporting \
         commit ids, and bulk-edit/author/subject exclusions (same ledger as \
         churn_factors); same-commit rename pairs dropped under a labelled \
         heuristic. A distinct type from CoChangeEdge/RelKind, never a static \
         or runtime dependency claim. Visual change-coupling map is host layer."),
    // ---- History, collaboration and knowledge ------------------------------
    entry!("US-088", Planned, [],
        "History analysis not shipped; no contributor-identity fields exist anywhere in the stores (nothing to protect yet), and privacy-at-source stays a design constraint for when history lands."),
    entry!("US-089", Partial, ["cie-incremental::differential"],
        "Relation-level change traces shipped; named-interface contract timeline is partial."),
    entry!("US-090", Delivered, ["cie-drift::pipeline", "cli:drift"],
        "Documentation gaps via claim extraction + verification; undocumented behaviour is never called approved."),
    entry!("US-091", Partial, ["cie-claims::persist"],
        "Claims/evidence attach to elements; sharing surfaces are product layer."),
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
    entry!("US-098", Planned, [],
        "Package licence/advisory scanning not shipped."),
    // ---- Counterfactuals and digital twin ---------------------------------
    entry!("US-099", Partial, ["cli:impact", "cie-incremental::proptests"],
        "Removal counterfactual shipped (deletion oracle); broader what-if simulation is partial."),
    entry!("US-100", Partial, ["cie-fault"],
        "Fault injection over the static model; capacity modelling not shipped."),
    entry!("US-101", Planned, [],
        "Sandbox experiment execution not shipped (offline-first refuses overreach)."),
    entry!("US-102", Partial, ["cie-contract::obligations::StateModel"],
        "State-machine twin from code; runtime calibration absent — labelled static."),
    entry!("US-103", Delivered, ["cie-claims::model::Modality", "cie-store::records"],
        "Every derived record carries modality + provenance; predictions never display as fact."),
    // ---- Prompt-to-feature discovery and planning -------------------------
    entry!("US-104", Planned, [],
        "Feature-request intake is product layer."),
    entry!("US-105", Planned, [],
        "Clarification flow is product layer."),
    entry!("US-106", Partial, ["cie-concepts"],
        "Concept-level overlap detection shipped; capability-level equivalence is product reasoning."),
    entry!("US-107", Partial, ["cie-drift::pipeline"],
        "Contradictory claims detected by drift; requirement-conflict management is product layer."),
    entry!("US-108", Planned, [],
        "Feature-contract assembly is product layer."),
    entry!("US-109", Partial, ["cli:impact"],
        "Code-impact shipped; permission/data/ops impact dimensions not modelled."),
    entry!("US-110", Planned, [],
        "Scope-revision workflow is product layer."),
    // ---- Feature implementation and inspection ----------------------------
    entry!("US-111", Planned, [],
        "Candidate-build workflow is product layer; the engine never mutates working trees (by design)."),
    entry!("US-112", Planned, [],
        "Feature wizard is product layer."),
    entry!("US-113", Partial, ["cie-incremental::plan_ingest"],
        "Changed-file/symbol accounting shipped for re-ingests; requirement attribution is product layer."),
    entry!("US-114", Partial, ["cli:report --as-of-rev"],
        "Revision comparison shipped; full-file diff rendering is product layer."),
    entry!("US-115", Planned, [],
        "Behaviour preview/execution not shipped."),
    entry!("US-116", Planned, [],
        "Dependency/data-change review not shipped (see US-062)."),
    // ---- Validation and safe delivery -------------------------------------
    entry!("US-117", Planned, [],
        "Build execution is outside the offline static engine; would be refused, honestly."),
    entry!("US-118", Delivered, ["cie-analysis::testing::build_test_map", "cie-analysis::testing::minimum_test_set"],
        "Requirement-adjacent tests via test map + minimum test set for changed symbols."),
    entry!("US-119", Partial, ["cie-incremental::proptests"],
        "Internal acceptance harness (126+ checks) shipped; external independent evidence integration partial."),
    entry!("US-120", Partial, ["cie-analysis::security", "cie-analysis::findings"],
        "Security findings shipped; performance readiness needs profiles (US-082 gap)."),
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
    entry!("US-127", Partial, ["cie-store::snapshot"],
        "Revision-pinned snapshots shipped; release-scope management is product layer."),
    entry!("US-128", Partial, ["cli:report"],
        "Readiness ingredients shipped (coverage, findings, drift); stakeholder lenses are product layer."),
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
    entry!("US-134", Partial, ["cie-viz", "cie-store::records"],
        "Machine-readable evidence protocol shipped (sealed ViewSpec + typed records); MCP server seam scheduled."),
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
    entry!("US-140", Partial, ["cli:ingest", "cli:analyze"],
        "CLI flows are keyboard-first by nature; canvas keyboard parity is host layer."),
    entry!("US-141", Partial, ["host:viz-adapter/outline"],
        "A11Y text outline shipped and authoritative when degraded (VIZ-V-02); screen-reader UX is host."),
    entry!("US-142", Partial, ["host:viz-adapter"],
        "Degraded-fidelity banners + text labels ship; full never-colour-alone audit is host."),
    entry!("US-143", Partial, ["host:viz-adapter"],
        "Theme-independent text outline ships; theming is host."),
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
