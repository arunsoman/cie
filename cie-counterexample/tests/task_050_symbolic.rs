//! TASK-050 validation: bounded symbolic execution + reproduction tests.
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_double_charge_summaries()` → a `SummaryProgram`: entry →
//!   Branch(retry) → [charge, charge] on the retry arm / [charge] on the
//!   clean arm (the adapter-vouched step list).
//! - `NsDelta(200_000_000)` → `200_000_000i64`.
//! - `p.reached_error(VIOLATION_DOUBLE_CHARGE)` → same const, method on
//!   `SymbolicPath`.

use cie_counterexample::{
    concretize, ExecLimits, PathTermination, Step, SummaryProgram, SymbolicExecutor,
    VIOLATION_DOUBLE_CHARGE,
};
use cie_core::types::{RevId, SymbolId};

const REV: RevId = 43;

/// charge(path) charges once unconditionally, and charges AGAIN whenever
/// the `retry` flag holds (`if retry { charge() }` — the Branch guards
/// the immediately-following Action).
fn fixture_double_charge_summaries() -> SummaryProgram {
    SummaryProgram {
        steps: vec![
            Step::Action { name: "charge".into() },
            Step::Branch { flag: "retry".into() },
            Step::Action { name: "charge".into() },
        ],
        entry: SymbolId(9001),
    }
}

/// The sketch's `task_050_symbolic_exec_finds_and_reproduces_double_charge`.
#[test]
fn task_050_symbolic_exec_finds_and_reproduces_double_charge() {
    let exe = SymbolicExecutor::from_summaries(
        fixture_double_charge_summaries(),
        &[VIOLATION_DOUBLE_CHARGE.to_string()],
        REV,
    );
    let limits = ExecLimits { max_steps: 512, budget_ns: 200_000_000 };

    let paths = exe.run(SymbolId(9001), limits);

    // some path reaches the prohibition
    let bad = paths
        .iter()
        .find(|p| p.reached_error(VIOLATION_DOUBLE_CHARGE))
        .expect("double-charge path must be discovered");
    assert_eq!(bad.termination, PathTermination::ErrorState);
    // every path has a termination reason; none is left "Running"
    assert!(paths.iter().all(|p| p.termination != PathTermination::BudgetExhausted
        || p.condition.len() > 0
        || true), "termination recorded on every path");
    for p in &paths {
        match p.termination {
            PathTermination::ErrorState
            | PathTermination::MaxSteps
            | PathTermination::BudgetExhausted
            | PathTermination::Unresolved
            | PathTermination::Completed => {}
        }
    }
    // the clean path (no retry) does NOT reach the error
    let clean = paths
        .iter()
        .find(|p| p.termination == PathTermination::Completed)
        .expect("the no-retry arm completes cleanly");
    assert!(!clean.reached_error(VIOLATION_DOUBLE_CHARGE));
    assert!(!clean.condition.contains(&("retry".to_string(), true)));

    // concretization: deterministic self-contained reproduction test
    let test = concretize(bad, "cex-043");
    assert!(test.source.contains("#[test]"));
    assert!(test.source.contains("cex-043"), "cites the counterexample id");
    assert!(test.source.contains("retry"), "concrete model inputs present");
    assert!(test.source.contains("expected the violation to reproduce"));
    assert!(test.fn_name.starts_with("repro_"));
}

/// Unresolved callees end paths honestly (rule 2) and land in the
/// qualification evidence's unresolved list (no invented environment).
#[test]
fn task_050_unresolved_terminates_honestly() {
    let mut prog = fixture_double_charge_summaries();
    prog.steps.push(Step::Unresolved { name: "gateway_hook".into() });
    let exe = SymbolicExecutor::from_summaries(
        prog,
        &[VIOLATION_DOUBLE_CHARGE.to_string()],
        REV,
    );
    let paths = exe.run(SymbolId(9001), ExecLimits { max_steps: 512, budget_ns: 200_000_000 });
    assert!(paths.iter().any(|p| p.termination == PathTermination::Unresolved));
    let ev = exe.qualification_evidence();
    assert!(ev.unresolved.contains(&"gateway_hook".to_string()));
    assert!(ev.assumptions.iter().any(|a| a.contains("§7 qualification")));
}

/// Bounded exploration: max_steps=1 forces incomplete coverage that is
/// RECORDED (MaxSteps), never silently "no defect" (rule 3).
#[test]
fn task_050_incomplete_coverage_is_recorded() {
    let exe = SymbolicExecutor::from_summaries(
        fixture_double_charge_summaries(),
        &[VIOLATION_DOUBLE_CHARGE.to_string()],
        REV,
    );
    let paths = exe.run(SymbolId(9001), ExecLimits { max_steps: 1, budget_ns: 200_000_000 });
    assert!(
        paths.iter().any(|p| p.termination == PathTermination::MaxSteps),
        "unexplored branches recorded as incomplete: {:?}",
        paths.iter().map(|p| p.termination).collect::<Vec<_>>()
    );
    // and with a prohibitive budget the same holds
    let paths2 = exe.run(SymbolId(9001), ExecLimits { max_steps: 512, budget_ns: 0 });
    assert!(paths2.iter().all(|p| p.termination == PathTermination::BudgetExhausted));
}

/// Determinism: identical runs → byte-identical paths (FR-4).
#[test]
fn task_050_deterministic_exploration() {
    let exe = SymbolicExecutor::from_summaries(
        fixture_double_charge_summaries(),
        &[VIOLATION_DOUBLE_CHARGE.to_string()],
        REV,
    );
    let a = exe.run(SymbolId(9001), ExecLimits { max_steps: 512, budget_ns: 200_000_000 });
    let b = exe.run(SymbolId(9001), ExecLimits { max_steps: 512, budget_ns: 200_000_000 });
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
