//! TASK-072/076 drift acceptance scenarios (items #24, #25, #17). Bodies
//! live in `tests/common/mod.rs` for harness dispatch.

mod common;

use common::scenarios;

/// acc #24: drift pipeline re-run ≤ 5% of cold (release-gated strictness).
#[test]
fn task_024_acc24_drift_rerun_within_5pct() {
    scenarios::task_024_acc24_drift_rerun_within_5pct();
}

/// acc #25: LLM fully off ⇒ ≥ 60% explicit claims extracted.
#[test]
fn task_025_acc25_llm_off_extracts_60pct_of_explicit_claims() {
    scenarios::task_025_acc25_llm_off_extracts_60pct_of_explicit_claims();
}

/// acc #17: ≥ 90% of claims resolve non-Unknown.
#[test]
fn task_017_acc17_ninety_pct_claims_resolve() {
    scenarios::task_017_acc17_ninety_pct_claims_resolve();
}
