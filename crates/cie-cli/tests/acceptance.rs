//! CIE acceptance suite (PRD §70 subset). Scenario bodies live in
//! `tests/common/mod.rs` so the TASK-077 harness dispatches identical code.

mod common;

use common::scenarios;

#[test]
fn acc1_print_only_authorization_detected() {
    scenarios::acc1_print_only_authorization_detected();
}

#[test]
fn acc2_unconditional_success_detected() {
    scenarios::acc2_unconditional_success_detected();
}

#[test]
fn acc3_legitimate_delegation_not_flagged() {
    scenarios::acc3_legitimate_delegation_not_flagged();
}

#[test]
fn acc4_rejected_authorization_ignored_detected() {
    scenarios::acc4_rejected_authorization_ignored_detected();
}

#[test]
fn acc5_exceptions_converted_to_success_detected() {
    scenarios::acc5_exceptions_converted_to_success_detected();
}

#[test]
fn acc6_unresolved_external_marked_explicitly() {
    scenarios::acc6_unresolved_external_marked_explicitly();
}

#[test]
fn acc7_deletion_invalidates_earlier_finding() {
    scenarios::acc7_deletion_invalidates_earlier_finding();
}

#[test]
fn acc8_polyglot_repo_ingested_end_to_end() {
    scenarios::acc8_polyglot_repo_ingested_end_to_end();
}

#[test]
fn acc10_unchanged_reingest_fast() {
    scenarios::acc10_unchanged_reingest_fast();
}

#[test]
fn acc11_time_travel_returns_correct_results() {
    scenarios::acc11_time_travel_returns_correct_results();
}

#[test]
fn acc18_19_20_21_22_drift_statuses() {
    scenarios::acc18_19_20_21_22_drift_statuses();
}

#[test]
fn acc23_drift_delta_shows_only_changes() {
    scenarios::acc23_drift_delta_shows_only_changes();
}

#[test]
fn acc26_verification_timeout_yields_unknown() {
    scenarios::acc26_verification_timeout_yields_unknown();
}

#[test]
fn acc27_to_31_concept_behavior() {
    scenarios::acc27_to_31_concept_behavior();
}

#[test]
fn acc41_to_44_capability_spot_checks() {
    scenarios::acc41_to_44_capability_spot_checks();
}

#[test]
fn acc46_minimum_test_set_computed() {
    scenarios::acc46_minimum_test_set_computed();
}

#[test]
fn sarif_export_is_valid() {
    scenarios::sarif_export_is_valid();
}

#[test]
fn coverage_gaps_and_test_map() {
    scenarios::coverage_gaps_and_test_map();
}
