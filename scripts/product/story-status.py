#!/usr/bin/env python3
"""Generate the public acceptance register; no capability is auto-promoted to done."""
import json, pathlib, re
root = pathlib.Path(__file__).resolve().parents[2]
source = (root / 'docs/product/USER_STORIES.md').read_text()
partial = {
1: ('Local connection, exact captured revision; hosted and branch selection pending.', ['index_publishes_immutable_sources_without_modifying_repository']),
2: ('Engine coverage and unresolved/unsupported records shown; full mixed-language boundary view pending.', ['static_test_map_never_claims_executed_coverage']),
3: ('Freshness rechecked against current sources; granular invalidation and incremental view refresh pending.', ['changed_sources_are_visibly_stale_and_export_keeps_the_same_caveats']),
4: ('Search across indexed snapshots preserves repository identity; cross-service contract resolution pending.', ['cross_repository_search_keeps_identical_names_separate']),
6: ('Persistent jobs and cancellation before publication; browser reload acceptance included.', ['cancel_queued_job_blocks_publication','restart_marks_interrupted_jobs_failed_without_automatic_resume','browser:repository-index-reload']),
8: ('Deterministic evidence questions and explicit unknowns; unrestricted natural-language analysis pending.', ['unknown_question_does_not_return_unrelated_finding_as_answer']),
9: ('Interpretation and explicit intent controls; ranked alternatives/confidence calibration pending.', ['explicit_route_and_symbol_selection_are_honored']),
12: ('Visible removable parent/selection context; general steering and editor context pending.', ['browser:selection-follow-up']),
14: ('Citations and required explanation for review/refutation; complete dependency invalidation/counterarguments pending.', ['refutation_hides_claim_and_edges_without_upgrading_confirmation','browser:review-refutation']),
15: ('Static observations and analyzer inference labelled consistently; runtime/hypothesis evidence classes pending.', ['browser:representation-parity']),
16: ('Saved answer tabs, close/export and evidence-gone refusal; concurrent answer generation pending.', ['saved_workspace_validates_answer_references','browser:workspace-reload']),
17: ('Saved results survive reload and interrupted indexing fails explicitly; durable multi-step investigation continuation pending.', ['restart_marks_interrupted_jobs_failed_without_automatic_resume']),
18: ('Static relationships use graph/table; unsupported timeline/measurement views explain missing data.', ['browser:gallery-unavailable']),
19: ('Graph/table/outline preserve identical items and edges; other evidence-specific representations pending.', ['node:projection-layout','browser:representation-parity']),
20: ('Small truthful available/unavailable gallery; full catalogue pending.', ['browser:gallery-unavailable']),
21: ('Concept/symbol items open exact source citations; full multi-level navigation pending.', ['browser:source-inspection']),
22: ('Readable fixed rows and scroll; semantic zoom and viewport indicators pending.', ['node:projection-layout']),
24: ('Individual symbol selection used in follow-up; lasso/matrix selection pending.', ['browser:selection-follow-up']),
25: ('Local filter/hide/reveal preserves source answer; salience factor explanations and pin/boost pending.', ['node:projection-layout']),
26: ('Shared inspector shows evidence, analyzer detail and relationships; editor URI integration pending.', ['browser:source-inspection']),
27: ('Static views share item identity; runtime mappings pending.', ['browser:representation-parity']),
30: ('Disjoint rows and edge lanes outside nodes; crossings documented, sophisticated layout pending.', ['node:projection-layout']),
42: ('Static test reachability only; assertion/execution evidence pending.', ['static_test_map_never_claims_executed_coverage']),
44: ('Engine concept nodes exposed as inference with citations; business interpretation requires review.', ['answers_have_real_citations_and_shared_graph_population']),
50: ('Concept candidates and claim review; full concept-card curation pending.', ['refutation_hides_claim_and_edges_without_upgrading_confirmation']),
51: ('Concept hierarchy relationships exposed; level-aware navigation pending.', ['answers_have_real_citations_and_shared_graph_population']),
52: ('Lexical symbol search with citations; behavior-equivalent feature discovery pending.', ['cross_repository_search_keeps_identical_names_separate']),
57: ('Static symbol/call graph and source inspection; architecture grouping pending.', ['answers_have_real_citations_and_shared_graph_population']),
58: ('Reverse static call reachability; non-call/configuration effects pending.', ['explicit_route_and_symbol_selection_are_honored']),
72: ('Analyzer findings labelled inference with evidence; runtime confirmation and full triage workflow pending.', ['unknown_question_does_not_return_unrelated_finding_as_answer']),
88: ('Workspace does not collect history/authorship; privacy-aware historical analytics pending.', ['browser:privacy-disclosure']),
92: ('JSON/Markdown exports preserve full answer caveats; visual artifact export and bookmarks pending.', ['node:export-caveats','browser:export']),
93: ('Persistent workspaces and saved answers reopen; collaboration and wider view-state persistence pending.', ['saved_workspace_validates_answer_references','browser:workspace-reload']),
97: ('Engine static findings and secret-shaped excerpt removal; full security assessment pending.', ['answers_have_real_citations_and_shared_graph_population']),
104: ('Feature prompt and acceptance criteria saved as planning only; implementation pending.', ['feature_request_records_criteria_without_pretending_to_build']),
106: ('Lexical overlap hints explicitly not equivalence; semantic comparison pending.', ['feature_request_records_criteria_without_pretending_to_build']),
108: ('Explicit draft criteria and snapshot reference; complete contract/authority validation pending.', ['feature_request_records_criteria_without_pretending_to_build']),
112: ('Dedicated planning area; full candidate/diff/validation wizard pending.', ['browser:feature-planning']),
134: ('Read-only bounded evidence HTTP questions; agent protocol adapter pending.', ['request:session-origin','source_text_and_input_limits_fail_explicitly']),
135: ('Offline deterministic engine/API/UI; broader stories still bounded by available adapters.', ['browser:privacy-disclosure']),
136: ('Cloud egress disabled; configurable approved-provider flow and send audit pending.', ['browser:privacy-disclosure']),
138: ('Staged progress and cancel publication controls; token costs and resumable computation pending.', ['cancel_queued_job_blocks_publication']),
139: ('Local deletion cascades derived records; tenant isolation/RBAC/retention governance pending.', ['deletion_removes_all_derived_views_but_not_connected_source']),
140: ('Native controls and graph arrow navigation; all future canvases need acceptance testing.', ['browser:keyboard-graph']),
141: ('Outline/table list all current items/links; full screen-reader audit pending.', ['browser:representation-parity']),
142: ('Text status/class labels and dashed inference; full notation audit pending.', ['browser:representation-parity']),
143: ('Persistent light/dark/high-contrast themes and reduced motion CSS; contrast audit pending.', ['browser:theme-persistence']),
144: ('Freshness, unknown, no-index and failed states separate; full denial-aware scope accounting pending.', ['missing_index_and_outside_roots_are_distinct_errors','changed_sources_are_visibly_stale_and_export_keeps_the_same_caveats']),
145: ('Unknowns and missing evidence disclosed; future adapters need degradation tests.', ['unknown_question_does_not_return_unrelated_finding_as_answer']),
146: ('Atomic persistence, process lock and interruption handling; backups/restore and complete idempotency pending.', ['failed_transaction_does_not_change_persisted_or_in_memory_state','storage_rejects_concurrent_owners_and_corruption']),
147: ('Full 147-story acceptance register with named checks; independent evaluation harness pending.', ['node:story-register'])}
partial.update({
29: ('Imported trace/profile/test records with exact source mapping; overlays on every structural map and time-window filtering pending.', ['imported_observations_map_only_exact_source_and_never_claim_execution','browser:runtime-import']),
39: ('Recorded error events are visible with their reported timing; structural overlays and incident ingestion adapters pending.', ['browser:runtime-import']),
47: ('Profile imports require actual sample counts and exact hash mapping; flame graphs and profiler-specific adapters pending.', ['observations_reject_mismatch_cycles_duplicates_and_invalid_intervals']),
64: ('Symptom-based investigation records persist with explicit hypotheses; automated evidence collection pending.', ['investigations_checkpoint_competing_hypotheses_without_promoting_them']),
65: ('Competing hypotheses can record supporting and counter-evidence; automatic ranking and falsification pending.', ['investigations_checkpoint_competing_hypotheses_without_promoting_them']),
66: ('Investigators add evidence and checkpoint notes while retaining exact scope; automated running-step steering pending.', ['browser:investigation-checkpoint']),
67: ('Exact-snapshot reference validation and missing/disputed evidence remain challenged; automated action bookkeeping pending.', ['investigation_with_deleted_answer_reference_remains_challenged_after_resume']),
69: ('Explicit next discriminating step saved per hypothesis; automatic discriminating-step selection pending.', ['investigations_checkpoint_competing_hypotheses_without_promoting_them']),
70: ('Durable paused checkpoints reopen with independent freshness checks; automatic work continuation pending.', ['browser:investigation-checkpoint']),
71: ('Investigation export preserves hypotheses, evidence IDs and uncertainty; synthesized incident explanation pending.', ['investigation_with_deleted_answer_reference_remains_challenged_after_resume']),
75: ('Exact-source-hash runtime mappings and unmapped records exposed; linked structural/runtime comparison pending.', ['imported_observations_map_only_exact_source_and_never_claim_execution']),
76: ('Recorded event timeline and parent relationships available; execution replay and alternate paths pending.', ['browser:runtime-import']),
77: ('Reports retain explicit environment and revision; side-by-side environment/flag comparison pending.', ['imported_observations_become_stale_when_source_changes']),
78: ('Imported timing and parent links explicitly do not establish causal necessity; intervention evidence pending.', ['imported_observations_map_only_exact_source_and_never_claim_execution']),
82: ('Per-event observed durations shown without extrapolation; bottleneck attribution and percentiles need representative measurements.', ['browser:runtime-import']),
83: ('Multiple profile reports can be retained; normalized cross-profile comparison pending.', ['imported_observations_map_only_exact_source_and_never_claim_execution'])
})
partial.update({
7: ('On-demand checkout fixture with real, explicitly synthetic dated Git history; advanced unsupported views remain unavailable.', ['generated_sample_has_real_dated_synthetic_history_without_mutating_user_source','browser:sample-history']),
36: ('Retained source snapshots can be compared with exact source inspection; candidate semantic/behavioral comparison pending.', ['snapshot_comparison_reports_text_changes_without_claiming_semantic_compatibility']),
86: ('Bounded changed-path Git history without contributor identity; validated risk ranking pending.', ['generated_sample_has_real_dated_synthetic_history_without_mutating_user_source']),
87: ('Per-commit changed-path groups available; co-change clustering/confidence and visual explanations pending.', ['generated_sample_has_real_dated_synthetic_history_without_mutating_user_source']),
89: ('Dated commits and retained source comparisons available; symbol-aware interface evolution pending.', ['snapshot_comparison_reports_text_changes_without_claiming_semantic_compatibility'])
})
partial.update({
10: ('Explicit bounded intersection/union/complement queries preserve true/false/unknown membership; unrestricted natural-language compound routing pending.', ['compound_queries_preserve_unknown_under_negative_missing_effect','compound_query_union_and_complement_use_one_exact_population','browser:compound-query']),
11: ('Up to six explicit read-only steps with dependency selection and durable independent results; autonomous natural-language planning pending.', ['composed_analysis_retains_completed_steps_when_dependency_is_ambiguous','browser:composed-analysis']),
17: ('Durable per-step results and idempotent resume across answer/checkpoint interruption; broader provider recovery pending.', ['resuming_after_answer_publication_before_step_checkpoint_is_idempotent']),
146: ('Atomic storage, explicit lock release and request deduplication; verified backup/restore and broader rollback pending.', ['ownership_release_is_not_delayed_by_an_inherited_descriptor','question_request_ids_deduplicate_and_refuse_changed_input_or_revision'])
})
partial.update({
50: ('Concept extraction runs with cancellable indexing; exact-snapshot refutations persist across answers, with minimum-sample judgment summaries. Card-set version comparison remains pending.', ['refuted_concept_remains_withheld_across_answers_and_answer_deletion','browser:concept-curation']),
53: ('Extracted obligation catalogue separates naming/declaration authority from approval, with exact source locations; complete cross-layer enforcement and conflict analysis pending.', ['naming_anomaly_compares_promised_permission_check_with_observed_printing']),
54: ('Naming expectations, related declarations and modeled effects are inspectable; approved-requirement binding and complete mismatch classification pending.', ['naming_anomaly_compares_promised_permission_check_with_observed_printing'])
})
completed = {
7: ('On-demand multi-language checkout fixture has real explicitly synthetic dated history, source/rules/concept material and an intentional naming regression. It never modifies connected user source.', ['generated_sample_has_real_dated_synthetic_history_without_mutating_user_source','browser:sample-history','browser:naming-anomaly']),
56: ('Naming expectations and modeled effects appear side by side with source. The authorize/print example is detected; naming alone is never presented as a confirmed business violation.', ['naming_anomaly_compares_promised_permission_check_with_observed_printing','browser:naming-anomaly']),
143: ('Persistent light/dark/high-contrast themes recolor all current renderers. Text/control contrast checks, visible focus and reduced-motion browser checks pass.', ['node:theme-contrast','browser:theme-persistence'])
}
rows=[]
for number,title in re.findall(r'^### US-(\d+): (.+)$',source,re.M):
    n=int(number); note,evidence=completed.get(n,partial.get(n,('Not implemented in the product workspace. No acceptance completion is claimed.',[])))
    rows.append(dict(id=f'US-{n:03}',title=title,status='complete' if n in completed else 'partial' if n in partial else 'pending',note=note,evidence=evidence))
assert len(rows)==147 and len({r['id'] for r in rows})==147
(root/'cie-server/web/stories.json').write_text(json.dumps(rows,indent=2)+'\n')
lines=['# Product acceptance register','','147 stories. Complete stories have explicit acceptance checks; partial capabilities do not count as complete stories.','', '| Story | Status | Delivered scope and remaining obligations | Named checks |','|---|---|---|---|']
lines += [f"| {r['id']}: {r['title']} | {r['status']} | {r['note']} | {', '.join(r['evidence']) or 'None'} |" for r in rows]
(root/'docs/product/STATUS.md').write_text('\n'.join(lines)+'\n')
print({status:sum(r["status"]==status for r in rows) for status in ["complete","partial","pending"]})
