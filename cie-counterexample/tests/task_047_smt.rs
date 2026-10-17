//! TASK-047 validation: budgeted SMT feasibility + model extraction.
//!
//! Sketch deviations (documented, single-point):
//! - `NsDelta(50_000_000)` → `50_000_000i64` (NsDelta IS i64).
//! - `enc.assume_range(x, 0, 100)` carries an explicit SourceLocation
//!   (rule 2 provenance; the sketch's no-arg form is the default-loc call).
//! - The default backend is the pure-Rust interval narrowing (documented
//!   in TRACEABILITY §2: the `smt-z3` feature swaps in system libz3 via
//!   C FFI; the outcome type is backend-neutral).

use cie_counterexample::{SmtEncoder, SmtOutcome, SmtUnknownReason};
use cie_core::types::{SourceLocation, ValueId};

fn loc(n: u32) -> SourceLocation {
    SourceLocation::point(n.into(), n)
}

/// The sketch's `task_047_smt_budgeted_feasibility_and_model_extraction`.
#[test]
fn task_047_smt_budgeted_feasibility_and_model_extraction() {
    let mut enc = SmtEncoder::new();
    // x has extracted range [0,100]; obligation x <= 99; branch condition x > 50
    let x = enc.declare_int(ValueId(7));
    enc.assume_range(x, 0, 100, loc(1));
    let branch = enc.gt(x, enc.int_const(50));
    let obligation = enc.le(x, enc.int_const(99));

    // 1: branch feasible under the obligation → model assigns x in 51..=99
    match enc.solve_path(
        &[branch.clone()],
        &[obligation.clone().with_provenance(loc(2))],
        50_000_000,
    ) {
        SmtOutcome::Feasible(model) => {
            let v = model.int_of(x).expect("model assigns x");
            assert!((51..=99).contains(&v), "model value {v} in 51..=99");
            // provenance rode along (rule 2)
            assert!(!model.provenance.is_empty());
            assert!(model.provenance.iter().any(|(id, _)| *id == ValueId(7)));
        }
        other => panic!("expected Feasible within 50 ms budget, got {other:?}"),
    }

    // 2: `x > 100` contradicts the range fact → Infeasible, not Unknown
    let contradiction = enc.gt(x, enc.int_const(100));
    assert!(matches!(
        enc.solve_path(
            &[contradiction],
            &[obligation.clone().with_provenance(loc(2))],
            50_000_000
        ),
        SmtOutcome::Infeasible
    ));

    // 3: a 1 ns budget must yield Unknown(TimedOut) — never Feasible/Infeasible
    assert!(matches!(
        enc.solve_path(&[branch.clone()], &[], 1),
        SmtOutcome::Unknown(SmtUnknownReason::TimedOut)
    ));
    // zero budget likewise (rule 3: check BEFORE solving)
    assert!(matches!(
        enc.solve_path(&[branch], &[], 0),
        SmtOutcome::Unknown(SmtUnknownReason::TimedOut)
    ));
}

/// Model extraction is per-ValueId and deterministic (same inputs → same
/// model). Multiple variables keep independent ranges.
#[test]
fn task_047_multi_var_model_determinism() {
    let mut enc = SmtEncoder::new();
    let a = enc.declare_int(ValueId(1));
    let b = enc.declare_int(ValueId(2));
    enc.assume_range(a, 0, 10, loc(3));
    enc.assume_range(b, 5, 20, loc(4));
    let cond_a = enc.gt(a, enc.int_const(3));
    let cond_b = enc.le(b, enc.int_const(9));

    let m1 = enc.solve_path(&[cond_a.clone(), cond_b.clone()], &[], 50_000_000);
    let m2 = enc.solve_path(&[cond_a, cond_b], &[], 50_000_000);
    assert_eq!(m1, m2, "deterministic model (FR-4)");
    match m1 {
        SmtOutcome::Feasible(m) => {
            assert!((4..=10).contains(&m.int_of(a).unwrap()));
            assert!((5..=9).contains(&m.int_of(b).unwrap()));
        }
        other => panic!("expected feasible, got {other:?}"),
    }
}

/// Obligations narrow too: an unsatisfiable obligation alone → Infeasible
/// (a violation-free proof), never silently feasible.
#[test]
fn task_047_obligation_contradiction_is_infeasible() {
    let mut enc = SmtEncoder::new();
    let x = enc.declare_int(ValueId(9));
    enc.assume_range(x, 0, 10, loc(5));
    let impossible = enc.gt(x, enc.int_const(10));
    assert!(matches!(
        enc.solve_path(&[], &[impossible.with_provenance(loc(6))], 50_000_000),
        SmtOutcome::Infeasible
    ));
}
