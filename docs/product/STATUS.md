# Product acceptance register

147 stories. Complete stories have explicit acceptance checks; partial capabilities do not count as complete stories.

| Story | Status | Delivered scope and remaining obligations | Named checks |
|---|---|---|---|
| US-001: Connect a repository | partial | Local connection, exact captured revision; hosted and branch selection pending. | index_publishes_immutable_sources_without_modifying_repository |
| US-002: Know what the product understands | partial | Engine coverage and unresolved/unsupported records shown; full mixed-language boundary view pending. | static_test_map_never_claims_executed_coverage |
| US-003: Keep understanding current | partial | Freshness rechecked against current sources; granular invalidation and incremental view refresh pending. | changed_sources_are_visibly_stale_and_export_keeps_the_same_caveats |
| US-004: Work across several repositories | partial | Search across indexed snapshots preserves repository identity; cross-service contract resolution pending. | cross_repository_search_keeps_identical_names_separate |
| US-005: Connect a hosted source | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-006: Watch and control background work | partial | Persistent jobs and cancellation before publication; browser reload acceptance included. | cancel_queued_job_blocks_publication, restart_marks_interrupted_jobs_failed_without_automatic_resume, browser:repository-index-reload |
| US-007: Try a ready-made sample repository | complete | On-demand multi-language checkout fixture has real explicitly synthetic dated history, source/rules/concept material and an intentional naming regression. It never modifies connected user source. | generated_sample_has_real_dated_synthetic_history_without_mutating_user_source, browser:sample-history, browser:naming-anomaly |
| US-008: Ask a code-grounded question | partial | Deterministic evidence questions and explicit unknowns; unrestricted natural-language analysis pending. | unknown_question_does_not_return_unrelated_finding_as_answer |
| US-009: See how my question was read | partial | Interpretation and explicit intent controls; ranked alternatives/confidence calibration pending. | explicit_route_and_symbol_selection_are_honored |
| US-010: Ask several conditions at once | partial | Explicit bounded intersection/union/complement queries preserve true/false/unknown membership; unrestricted natural-language compound routing pending. | compound_queries_preserve_unknown_under_negative_missing_effect, compound_query_union_and_complement_use_one_exact_population, browser:compound-query |
| US-011: Combine several questions into one analysis | partial | Up to six explicit read-only steps with dependency selection and durable independent results; autonomous natural-language planning pending. | composed_analysis_retains_completed_steps_when_dependency_is_ambiguous, browser:composed-analysis |
| US-012: Continue with follow-ups and steering | partial | Visible removable parent/selection context; general steering and editor context pending. | browser:selection-follow-up |
| US-013: Choose the audience and level of detail | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-014: Inspect and challenge a claim | partial | Citations and required explanation for review/refutation; complete dependency invalidation/counterarguments pending. | refutation_hides_claim_and_edges_without_upgrading_confirmation, browser:review-refutation |
| US-015: Distinguish proven, inferred, hypothesised and unknown | partial | Static observations and analyzer inference labelled consistently; runtime/hypothesis evidence classes pending. | browser:representation-parity |
| US-016: Keep several answers side by side | partial | Saved answer tabs, close/export and evidence-gone refusal; concurrent answer generation pending. | saved_workspace_validates_answer_references, browser:workspace-reload |
| US-017: Recover when something goes wrong mid-answer | partial | Durable per-step results and idempotent resume across answer/checkpoint interruption; broader provider recovery pending. | resuming_after_answer_publication_before_step_checkpoint_is_idempotent |
| US-018: Receive a view that fits the question | partial | Static relationships use graph/table; unsupported timeline/measurement views explain missing data. | browser:gallery-unavailable |
| US-019: Switch representation without changing meaning | partial | Graph/table/outline preserve identical items and edges; other evidence-specific representations pending. | node:projection-layout, browser:representation-parity |
| US-020: Browse everything the product can draw | partial | Small truthful available/unavailable gallery; full catalogue pending. | browser:gallery-unavailable |
| US-021: Drill from system to code | partial | Concept/symbol items open exact source citations; full multi-level navigation pending. | browser:source-inspection |
| US-022: Zoom without losing readability | partial | Readable fixed rows and scroll; semantic zoom and viewport indicators pending. | node:projection-layout |
| US-023: Magnify locally without disturbing the map | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-024: Point at things and ask about them | partial | Individual symbol selection used in follow-up; lasso/matrix selection pending. | browser:selection-follow-up |
| US-025: Control focus and hidden information | partial | Local filter/hide/reveal preserves source answer; salience factor explanations and pin/boost pending. | node:projection-layout |
| US-026: Inspect an element's full story | partial | Shared inspector shows evidence, analyzer detail and relationships; editor URI integration pending. | browser:source-inspection |
| US-027: Keep multiple views aligned | partial | Static views share item identity; runtime mappings pending. | browser:representation-parity |
| US-028: Express a proposed change visually | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-029: Overlay tests and runtime on any map | partial | Imported trace/profile/test records with exact source mapping; overlays on every structural map and time-window filtering pending. | imported_observations_map_only_exact_source_and_never_claim_execution, browser:runtime-import |
| US-030: Trust the drawing itself | partial | Disjoint rows and edge lanes outside nodes; crossings documented, sophisticated layout pending. | node:projection-layout |
| US-031: See the architecture that answers my question | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-032: Investigate a failure from a stack trace | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-033: See every way a failure can arise | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-034: Walk through a transaction step by step | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-035: Trace who reads and writes data | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-036: Compare code before and after a change | partial | Retained source snapshots can be compared with exact source inspection; candidate semantic/behavioral comparison pending. | snapshot_comparison_reports_text_changes_without_claiming_semantic_compatibility |
| US-037: Ask why unusual code exists | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-038: See trust boundaries and privilege walls | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-039: See recent trouble on the structure | partial | Recorded error events are visible with their reported timing; structural overlays and incident ingestion adapters pending. | browser:runtime-import |
| US-040: Find where races are possible | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-041: Preview removing a component | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-042: See how well behaviour is tested | partial | Static test reachability only; assertion/execution evidence pending. | static_test_map_never_claims_executed_coverage |
| US-043: See ownership and knowledge risk | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-044: See implicit concepts in the code | partial | Engine concept nodes exposed as inference with citations; business interpretation requires review. | answers_have_real_citations_and_shared_graph_population |
| US-045: Audit which policies are enforced | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-046: Judge where change is risky | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-047: Connect profiling data to code | partial | Profile imports require actual sample counts and exact hash mapping; flame graphs and profiler-specific adapters pending. | observations_reject_mismatch_cycles_duplicates_and_invalid_intervals |
| US-048: See routes and what guards them | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-049: Ask for a diagram of my own | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-050: Extract and curate concept cards | partial | Concept extraction runs with cancellable indexing; exact-snapshot refutations persist across answers, with minimum-sample judgment summaries. Card-set version comparison remains pending. | refuted_concept_remains_withheld_across_answers_and_answer_deletion, browser:concept-curation |
| US-051: Browse the business-concept hierarchy | partial | Concept hierarchy relationships exposed; level-aware navigation pending. | answers_have_real_citations_and_shared_graph_population |
| US-052: Locate an existing feature | partial | Lexical symbol search with citations; behavior-equivalent feature discovery pending. | cross_repository_search_keeps_identical_names_separate |
| US-053: Discover distributed business rules | partial | Extracted obligation catalogue separates naming/declaration authority from approval, with exact source locations; complete cross-layer enforcement and conflict analysis pending. | naming_anomaly_compares_promised_permission_check_with_observed_printing |
| US-054: Compare intended and actual behaviour | partial | Naming expectations, related declarations and modeled effects are inspectable; approved-requirement binding and complete mismatch classification pending. | naming_anomaly_compares_promised_permission_check_with_observed_printing |
| US-055: Understand workflow states and failures | partial | User-declared states, events and guard assumptions can be inspected and explored; automatic implementation-derived workflow/recovery extraction pending. | declared_scenarios_expose_unknown_guards_and_never_execute_source_or_refunds |
| US-056: Detect misleading names | complete | Naming expectations and modeled effects appear side by side with source. The authorize/print example is detected; naming alone is never presented as a confirmed business violation. | naming_anomaly_compares_promised_permission_check_with_observed_printing, browser:naming-anomaly |
| US-057: Explore system structure | partial | Static symbol/call graph and source inspection; architecture grouping pending. | answers_have_real_citations_and_shared_graph_population |
| US-058: Find the change blast radius | partial | Reverse static call reachability; non-call/configuration effects pending. | explicit_route_and_symbol_selection_are_honored |
| US-059: Compare architectural alternatives | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-060: Discover potential service boundaries | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-061: Follow cross-repository contracts | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-062: Judge dependency importance and upgrades | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-063: Ask questions of a built artifact | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-064: Start an investigation from a symptom | partial | Symptom-based investigation records persist with explicit hypotheses; automated evidence collection pending. | investigations_checkpoint_competing_hypotheses_without_promoting_them |
| US-065: Compare competing explanations | partial | Competing hypotheses can record supporting and counter-evidence; automatic ranking and falsification pending. | investigations_checkpoint_competing_hypotheses_without_promoting_them |
| US-066: Direct an investigation while it runs | partial | Investigators add evidence and checkpoint notes while retaining exact scope; automated running-step steering pending. | browser:investigation-checkpoint |
| US-067: Trust the investigation's bookkeeping | partial | Exact-snapshot reference validation and missing/disputed evidence remain challenged; automated action bookkeeping pending. | investigation_with_deleted_answer_reference_remains_challenged_after_resume |
| US-068: Reproduce an intermittent defect | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-069: Ask for the next discriminating step | partial | Explicit next discriminating step saved per hypothesis; automatic discriminating-step selection pending. | investigations_checkpoint_competing_hypotheses_without_promoting_them |
| US-070: Resume an investigation later | partial | Durable paused checkpoints reopen with independent freshness checks; automatic work continuation pending. | browser:investigation-checkpoint |
| US-071: Assemble an incident explanation | partial | Investigation export preserves hypotheses, evidence IDs and uncertainty; synthesized incident explanation pending. | investigation_with_deleted_answer_reference_remains_challenged_after_resume |
| US-072: Triage defect candidates honestly | partial | Analyzer findings labelled inference with evidence; runtime confirmation and full triage workflow pending. | unknown_question_does_not_return_unrelated_finding_as_answer |
| US-073: Back candidates with runtime evidence and isolated runs | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-074: Collect performance candidates | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-075: Compare structure with observed runtime | partial | Exact-source-hash runtime mappings and unmapped records exposed; linked structural/runtime comparison pending. | imported_observations_map_only_exact_source_and_never_claim_execution |
| US-076: Replay a recorded request | partial | Recorded event timeline and parent relationships available; execution replay and alternate paths pending. | browser:runtime-import |
| US-077: Compare environments and flags | partial | Reports retain explicit environment and revision; side-by-side environment/flag comparison pending. | imported_observations_become_stale_when_source_changes |
| US-078: Distinguish causes from coincidence | partial | Imported timing and parent links explicitly do not establish causal necessity; intervention evidence pending. | imported_observations_map_only_exact_source_and_never_claim_execution |
| US-079: Find possible race windows | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-080: Find deadlocks and lock contention | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-081: Investigate distributed consistency | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-082: Explain a slow operation | partial | Capture metadata, reported intervals, omissions and exact source links remain inspectable; complete CPU/wait/resource attribution and validated bottleneck analysis pending. | equivalent_profile_comparison_describes_recorded_delta_without_claiming_improvement |
| US-083: Compare two profiles | complete | Two imported profiles are compared under explicit declared equivalence. Verdicts include reasons/limits, incompatible populations are rejected, and failed captures remain visible in the metric table. | equivalent_profile_comparison_describes_recorded_delta_without_claiming_improvement, profile_comparison_rejects_dropped_work_or_failures_but_retains_every_row, profiles_without_workload_metadata_remain_importable_but_not_fairly_comparable, browser:profile-comparison |
| US-084: Investigate a performance regression | partial | Profile comparisons expose declared workload/environment differences, failed and omitted populations, and reject unfair improvement claims; independent behavior regression execution pending. | profile_comparison_rejects_dropped_work_or_failures_but_retains_every_row |
| US-085: Identify optimization opportunities | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-086: Prioritize maintenance work | partial | Bounded changed-path Git history without contributor identity; validated risk ranking pending. | generated_sample_has_real_dated_synthetic_history_without_mutating_user_source |
| US-087: Find code that changes together | partial | Per-commit changed-path groups available; co-change clustering/confidence and visual explanations pending. | generated_sample_has_real_dated_synthetic_history_without_mutating_user_source |
| US-088: Protect contributor privacy | partial | Workspace does not collect history/authorship; privacy-aware historical analytics pending. | browser:privacy-disclosure |
| US-089: Track how an interface evolved | partial | Dated commits and retained source comparisons available; symbol-aware interface evolution pending. | snapshot_comparison_reports_text_changes_without_claiming_semantic_compatibility |
| US-090: Find documentation gaps | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-091: Collaborate without widening access | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-092: Export and bookmark understanding | partial | JSON/Markdown exports preserve full answer caveats; visual artifact export and bookmarks pending. | node:export-caveats, browser:export |
| US-093: Save and resume a workspace | partial | Persistent workspaces and saved answers reopen; collaboration and wider view-state persistence pending. | saved_workspace_validates_answer_references, browser:workspace-reload |
| US-094: Understand authorization | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-095: Trace sensitive information | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-096: Find policy enforcement gaps | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-097: Review code-security and secret findings | partial | Engine static findings and secret-shaped excerpt removal; full security assessment pending. | answers_have_real_citations_and_shared_graph_population |
| US-098: Assess packages and licences | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-099: Explore a hypothetical change | partial | Explicit transition-disable counterfactuals produce conditional graph witnesses; code-change consequences and runtime validation pending. | declared_scenarios_expose_unknown_guards_and_never_execute_source_or_refunds |
| US-100: Explore capacity and failure scenarios | partial | Bounded declared failure scenarios are supported without runtime effects; quantitative capacity/latency predictions require validated workload models. | browser:declared-scenario |
| US-101: Compare isolated experiments | partial | Conditional graph results are retained with their exact assumptions; isolated execution experiments and normalized comparisons pending. | browser:scenario-reload |
| US-102: Study a workflow digital twin | partial | Declared workflow state/transition exploration and witness traces are supported; implementation-derived digital-twin execution pending. | reachability_matches_independent_transitive_closure_oracle |
| US-103: Know the limits of predictions | partial | Model assumptions, unknown guards, finite bounds and non-runtime result classes stay explicit; broader prediction validation pending. | scenario_models_reject_foreign_evidence_invalid_edges_and_unknown_disabled_actions, browser:declared-scenario |
| US-104: Describe a new feature naturally | partial | Feature prompt and acceptance criteria saved as planning only; implementation pending. | feature_request_records_criteria_without_pretending_to_build |
| US-105: Answer only material questions | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-106: Discover equivalent or overlapping features | partial | Lexical overlap hints explicitly not equivalence; semantic comparison pending. | feature_request_records_criteria_without_pretending_to_build |
| US-107: Resolve conflicting requirements | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-108: Review a complete feature contract | partial | Explicit draft criteria and snapshot reference; complete contract/authority validation pending. | feature_request_records_criteria_without_pretending_to_build |
| US-109: Understand feature impact before building | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-110: Revise scope while work proceeds | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-111: Build an isolated candidate | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-112: Follow a dedicated feature wizard | partial | Dedicated planning area; full candidate/diff/validation wizard pending. | browser:feature-planning |
| US-113: Inspect where changes occur | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-114: Inspect final code and exact differences | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-115: Preview feature behaviour | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-116: Review new dependencies and data changes | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-117: See whether the candidate builds | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-118: Inspect tests related to the requirements | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-119: Trust independent acceptance evidence | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-120: Inspect security and performance readiness | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-121: Request fixes and rerun checks | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-122: Download a diff or patch | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-123: Check and apply against my destination | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-124: Create and iterate a draft pull request | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-125: Trace prompt-caused mutations | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-126: Coordinate several related changes | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-127: Freeze a release scope | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-128: Read readiness through stakeholder lenses | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-129: Approve release items safely | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-130: Prepare rollout and recovery | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-131: Connect production anomalies to prior changes | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-132: Share editor context on my terms | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-133: Navigate definitions and references from the editor | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-134: Give coding agents evidence-shaped answers | partial | Read-only bounded evidence HTTP questions; agent protocol adapter pending. | request:session-origin, source_text_and_input_limits_fail_explicitly |
| US-135: Work locally by default | partial | Offline deterministic engine/API/UI; broader stories still bounded by available adapters. | browser:privacy-disclosure |
| US-136: Decide what may leave my machine | partial | Cloud egress disabled; configurable approved-provider flow and send audit pending. | browser:privacy-disclosure |
| US-137: Switch models and understand model failures | pending | Not implemented in the product workspace. No acceptance completion is claimed. | None |
| US-138: Control cost, progress and cancellation | partial | Staged progress and cancel publication controls; token costs and resumable computation pending. | cancel_queued_job_blocks_publication |
| US-139: Manage access, retention and deletion | partial | Server-owned record deletion cascades and clears cached evidence across open browser tabs; tenant isolation, RBAC and governed retention remain pending. | browser:deletion-clears-other-tabs, deletion_removes_all_derived_views_but_not_connected_source |
| US-140: Operate everything from the keyboard | partial | Native controls and graph arrow navigation; all future canvases need acceptance testing. | browser:keyboard-graph |
| US-141: Use the product with a screen reader | partial | Outline/table list all current items/links; full screen-reader audit pending. | browser:representation-parity |
| US-142: Never depend on colour alone | partial | Text status/class labels and dashed inference; full notation audit pending. | browser:representation-parity |
| US-143: Read comfortably in any theme | complete | Persistent light/dark/high-contrast themes recolor all current renderers. Text/control contrast checks, visible focus and reduced-motion browser checks pass. | node:theme-contrast, browser:theme-persistence |
| US-144: Never see stale answers as current | partial | Freshness, unknown, no-index and failed states separate; full denial-aware scope accounting pending. | missing_index_and_outside_roots_are_distinct_errors, changed_sources_are_visibly_stale_and_export_keeps_the_same_caveats |
| US-145: Get graceful degradation instead of guesses | partial | Unknowns and missing evidence disclosed; future adapters need degradation tests. | unknown_question_does_not_return_unrelated_finding_as_answer |
| US-146: Trust that interruptions never corrupt state | partial | Atomic storage, explicit lock release and request deduplication; verified backup/restore and broader rollback pending. | ownership_release_is_not_delayed_by_an_inherited_descriptor, question_request_ids_deduplicate_and_refuse_changed_input_or_revision |
| US-147: See the product verify itself | partial | Full 147-story acceptance register with named checks; independent evaluation harness pending. | node:story-register |
