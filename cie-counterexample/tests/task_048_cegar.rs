//! TASK-048 validation: CEGAR refines spurious counterexamples.
//!
//! Sketch deviations (documented, single-point):
//! - `smt_backend_from_task_047()` → `()` (the default pure-Rust backend
//!   is baked into `CegarLoop`; the `smt-z3` feature swaps the backend).
//! - `NsDelta(10_000_000)` → `10_000_000i64`.
//! - `fixture_cfa_with_error_needing(pred_q())` → a 3-node CFA: entry
//!   --[p]--> mid --[q]--> error, with the extracted fact q == FALSE
//!   (the concretization that makes round-1's trace spurious).

use cie_counterexample::{
    AbstractPred, CegarLoop, CegarOutcome, CfaProgram, PredAbstractDomain, RefineLimits,
    RefinementExhausted,
};
use cie_core::types::{SourceLocation, SymbolId};

fn pred(n: u32, name: &str) -> AbstractPred {
    AbstractPred {
        symbol: SymbolId(n),
        name: name.into(),
        loc: SourceLocation::point(n.into(), n),
    }
}

/// The sketch's fixture: the error state needs `q`, but q is concretely
/// FALSE (extracted fact) — so the round-1 trace is spurious and q must
/// join the domain to block it.
fn fixture_cfa_with_error_needing_q() -> CfaProgram {
    CfaProgram {
        edges: vec![
            (0, "p".into(), true, 1),  // entry --[p]--> mid
            (1, "q".into(), true, 2),  // mid --[q]--> error
            (0, "p".into(), false, 3), // entry --[¬p]--> safe
        ],
        error_node: 2,
        facts: vec![("p".into(), true), ("q".into(), false)],
    }
}

/// The sketch's `task_048_cegar_refines_spurious_counterexample`.
#[test]
fn task_048_cegar_refines_spurious_counterexample() {
    let mut cegar = CegarLoop::new(
        PredAbstractDomain::new(vec![pred(1, "p")]),
        (),
    );
    let program = fixture_cfa_with_error_needing_q();

    let outcome = cegar.check(
        &program,
        RefineLimits { max_rounds: 8, round_budget_ns: 10_000_000 },
    );

    // round-1 trace was spurious → refined, never reported as a violation
    assert!(
        matches!(outcome, CegarOutcome::Safe(_) | CegarOutcome::Counterexample(_)),
        "got {outcome:?}"
    );
    if let CegarOutcome::Counterexample(t) = outcome {
        panic!("spurious trace leaked as counterexample: {t:?}");
    }
    assert_eq!(
        cegar.metrics().spurious_traces,
        1,
        "p-only trace must be classified spurious"
    );
    assert_eq!(cegar.metrics().rounds_used, 2, "refined once then safe");
    assert!(
        cegar.refinement_predicates().iter().any(|p| p.name == "q"),
        "distinguishing predicate q joined the domain: {:?}",
        cegar.refinement_predicates().iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    );
    // refinement steps are auditable (rule 2)
    assert!(cegar.evidence().assumptions.iter().any(|a| a.contains("q")));
}

/// A real counterexample IS reported when concretization succeeds:
/// the same CFA without the false-q fact reaches the error concretely.
#[test]
fn task_048_real_counterexample_is_emitted() {
    let mut cegar = CegarLoop::new(PredAbstractDomain::new(vec![pred(1, "p")]), ());
    let mut program = fixture_cfa_with_error_needing_q();
    program.facts = vec![("p".into(), true), ("q".into(), true)]; // q TRUE now

    let outcome = cegar.check(
        &program,
        RefineLimits { max_rounds: 8, round_budget_ns: 10_000_000 },
    );
    match outcome {
        CegarOutcome::Counterexample(trace) => {
            assert!(trace.contains(&("q".to_string(), true)));
            assert_eq!(cegar.metrics().spurious_traces, 0);
        }
        other => panic!("expected concrete counterexample, got {other:?}"),
    }
}

/// Bounded refinement: exhaustion yields Unknown — the possibly-spurious
/// alarm is never emitted as a violation (global rule 3).
#[test]
fn task_048_exhaustion_is_unknown_never_violation() {
    // no distinguishing predicates available: the domain cannot grow, but
    // the trace stays abstractly reachable and concretely infeasible
    let mut cegar = CegarLoop::new(
        PredAbstractDomain::new(vec![pred(1, "p"), pred(2, "q")]), // both tracked
        (),
    );
    let program = fixture_cfa_with_error_needing_q(); // q concretely false
    let outcome = cegar.check(
        &program,
        RefineLimits { max_rounds: 2, round_budget_ns: 10_000_000 },
    );
    // with both predicates tracked the abstraction blocks the trace → Safe
    assert!(matches!(outcome, CegarOutcome::Safe(_)), "got {outcome:?}");

    // true exhaustion: max_rounds=0 → unknown, never a violation
    let mut cegar2 = CegarLoop::new(PredAbstractDomain::new(vec![pred(1, "p")]), ());
    let outcome2 = cegar2.check(
        &program,
        RefineLimits { max_rounds: 0, round_budget_ns: 10_000_000 },
    );
    assert!(matches!(
        outcome2,
        CegarOutcome::Unknown(RefinementExhausted::RefinementExhausted)
    ));
    // and a spurious alarm never became a Counterexample
    assert!(!matches!(outcome2, CegarOutcome::Counterexample(_)));
}

/// Deterministic refinement: same program → same outcome and metrics.
#[test]
fn task_048_deterministic_refinement() {
    let run = || {
        let mut cegar = CegarLoop::new(PredAbstractDomain::new(vec![pred(1, "p")]), ());
        let outcome = cegar.check(
            &fixture_cfa_with_error_needing_q(),
            RefineLimits { max_rounds: 8, round_budget_ns: 10_000_000 },
        );
        (format!("{outcome:?}"), cegar.metrics(), format!("{:?}", cegar.evidence().assumptions))
    };
    assert_eq!(run(), run(), "deterministic refinement (FR-4)");
}
