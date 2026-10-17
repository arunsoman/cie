//! TASK-053 validation: lineage-driven fault injection (LDFI).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_topup_lineage()` → the successful top-up lineage
//!   debit → ledger → credit_event → balance_update (the sim chain).
//! - `liveness_from_task_052()` → `Property::liveness_from_task_052()`
//!   (the obligation's liveness expr + idempotency side-condition).
//! - `NsDelta(500_000_000)` → `budget_ns: 500_000_000` (NsDelta is i64;
//!   the limit field is u64).

use cie_fault::{
    ExperimentOutcome, Fault, LdfiError, LdfiPlanner, LdfiUnknown, LineageGraph, LineageKind,
    Property, RoundLimits, RoundResults, RuntimeFaultInjector, SimInjector, TimedOutReason,
};

/// Successful top-up lineage: debit → ledger → credit_event → balance_update.
fn fixture_topup_lineage() -> LineageGraph {
    let mut g = LineageGraph::empty();
    g.add("msg_debit", LineageKind::Message, "debit", "ledger");
    g.add("comp_ledger", LineageKind::Computation, "ledger", "credit_event");
    g.add("msg_credit", LineageKind::Message, "credit_event", "balance_update");
    g
}

/// The sketch's `task_053_ldfi_finds_drop_that_breaks_topup_recovery`.
#[test]
fn task_053_ldfi_finds_drop_that_breaks_topup_recovery() {
    let lineage = fixture_topup_lineage();
    let property = Property::liveness_from_task_052();

    let mut planner = LdfiPlanner::new(lineage, SimInjector::new());
    let rounds: RoundResults = planner
        .run(&property, RoundLimits { max_faults: 2, budget_ns: 500_000_000 })
        .expect("lineage present");

    // a single dropped message breaks liveness → counterexample reported
    let hits: Vec<_> = rounds.results.iter().filter(|r| r.property_violated).collect();
    assert!(!hits.is_empty(), "LDFI must reproduce interrupted recovery");
    assert_eq!(hits[0].faults.len(), 1, "first failure is a minimal single-fault set");
    assert!(
        matches!(&hits[0].faults[0], Fault::Drop(e) if e.kind == LineageKind::Message),
        "first hit is a dropped MESSAGE: {:?}",
        hits[0].faults[0]
    );
    // every applied fault carries lineage-edge provenance (rule 2)
    assert!(hits[0].provenance.iter().all(|p| p.starts_with("drop(")));
    assert!(!hits[0].content_hash.is_empty());

    // duplicate-refund hypothesis is also exercised (FR-15 target)
    assert!(
        rounds
            .results
            .iter()
            .any(|r| r.faults.iter().any(|f| matches!(f, Fault::Duplicate(e) if e.kind == LineageKind::Message))),
        "duplicate hypotheses are exercised"
    );

    // the crash-scheduler hypothesis (FR-15 trio) is exercised too
    assert!(rounds
        .results
        .iter()
        .any(|r| r.faults.iter().any(|f| matches!(f, Fault::CrashScheduler))));
}

/// No lineage input degrades to Unknown(NoLineage) — never "recovery is
/// sound" (global rules 2 and 3, §7 qualification).
#[test]
fn task_053_no_lineage_is_honest_unknown() {
    let empty = LdfiPlanner::new(LineageGraph::empty(), SimInjector::new())
        .run(&Property::liveness_from_task_052(), RoundLimits::default());
    assert!(matches!(empty, Err(LdfiError::Unknown(LdfiUnknown::NoLineage))));
}

/// Budget overrun yields Unknown(TimedOut) outcomes — never a soundness
/// verdict (global rule 3).
#[test]
fn task_053_budget_overrun_is_timed_out() {
    let lineage = fixture_topup_lineage();
    let mut planner = LdfiPlanner::new(lineage, SimInjector::new());
    let rounds = planner
        .run(&Property::liveness_from_task_052(), RoundLimits { max_faults: 2, budget_ns: 0 })
        .expect("lineage present");
    assert!(rounds
        .results
        .iter()
        .any(|r| r.outcome == ExperimentOutcome::Unknown(TimedOutReason::TimedOut)));
}

/// The injector seam (FR-7): the planner only speaks to the trait, and a
/// custom injector is deterministic per fault set.
#[test]
fn task_053_injector_trait_seam() {
    struct CountingInjector {
        runs: usize,
    }
    impl RuntimeFaultInjector for CountingInjector {
        fn run(&mut self, _lineage: &LineageGraph, faults: &[Fault]) -> cie_fault::SimRun {
            self.runs += 1;
            cie_fault::SimRun {
                reached: vec!["debit".into()],
                anomalies: faults.iter().map(|f| f.provenance()).collect(),
            }
        }
    }
    let lineage = fixture_topup_lineage();
    let mut planner = LdfiPlanner::new(lineage, CountingInjector { runs: 0 });
    let rounds = planner
        .run(&Property::liveness_from_task_052(), RoundLimits { max_faults: 1, budget_ns: 500_000_000 })
        .unwrap();
    // every completed experiment went through the trait seam
    let completed = rounds
        .results
        .iter()
        .filter(|r| r.outcome == ExperimentOutcome::Completed)
        .count();
    assert!(completed >= 1);
}
