//! TASK-049 validation: SLAM-style protocol rule checking.
//!
//! Sketch deviations (documented, single-point):
//! - `NsDelta(50_000_000)` → `50_000_000i64`.
//! - `fixture_csr_caller_rejects_then_fulfills()` → the `Csr` projection:
//!   caller → `authorize_reject` (rejected auth) → `fulfill`.
//! - `check_rule` is module-level (not a method).
//! - The builder's `.on(event, from, to)` spells the sketch's
//!   `.on(event, |s| ..).to(..)` guard form as data.

use cie_contract::Authority;
use cie_counterexample::{
    check_rule, check_rule_budgeted, trace_evidence, CallEdge, Csr, ProtocolMachine,
    RuleOutcome,
};
use cie_core::types::{RevId, SourceLocation, SymbolId};

const REV: RevId = 42;

/// Rule "failure must prevent fulfillment": reject → fulfill is the
/// error edge.
fn rule() -> ProtocolMachine {
    ProtocolMachine::builder()
        .states(&["Idle", "Authorized", "Rejected", "Error"])
        .on("authorize_ok", "Idle", "Authorized")
        .on("authorize_reject", "Idle", "Rejected")
        .on("fulfill", "Rejected", "Error") // violation edge
        .on("fulfill", "Authorized", "Idle") // legitimate fulfillment
        .start("Idle")
        .authority(Authority::Explicit)
        .build()
}

/// Caller authorizes (rejected), then fulfills anyway — the PRD protocol
/// violation fixture.
fn fixture_csr_caller_rejects_then_fulfills() -> Csr {
    Csr {
        edges: vec![
            CallEdge {
                from: SymbolId(100),
                to: SymbolId(101),
                event: "authorize_reject".into(),
                loc: SourceLocation::point(0.into(), 10),
            },
            CallEdge {
                from: SymbolId(100),
                to: SymbolId(102),
                event: "fulfill".into(),
                loc: SourceLocation::point(0.into(), 14),
            },
        ],
    }
}

/// The sketch's `task_049_slam_detects_fulfillment_after_rejected_auth`.
#[test]
fn task_049_slam_detects_fulfillment_after_rejected_auth() {
    let machine = rule();
    let csr = fixture_csr_caller_rejects_then_fulfills();

    let outcome = check_rule(&machine, &csr, 50_000_000);
    match outcome {
        RuleOutcome::Violated { trace, suspected } => {
            assert!(!suspected, "Explicit rule violations are NOT suspected");
            // per-hop locations cited (rule 2)
            assert_eq!(trace.len(), 2, "reject then fulfill");
            assert_eq!(trace[0].event, "authorize_reject");
            assert_eq!(trace[0].machine_state, "Rejected");
            assert_eq!(trace[1].event, "fulfill");
            assert_eq!(trace[1].machine_state, "Error");
            assert_ne!(trace[0].loc, SourceLocation::point(0.into(), 0));
            // evidence ready for TASK-051 packaging
            let ev = trace_evidence(&trace, REV, suspected);
            assert!(ev.provenance.contains("authorize_reject"));
            assert!(ev.provenance.contains("fulfill"));
            assert!(!ev.provenance.contains("SUSPECTED"));
        }
        other => panic!("expected Violated, got {other:?}"),
    }
}

/// A legitimate flow (authorized then fulfill) HOLDS — no false alarm.
#[test]
fn task_049_legitimate_flow_holds() {
    let machine = rule();
    let csr = Csr {
        edges: vec![
            CallEdge {
                from: SymbolId(200),
                to: SymbolId(201),
                event: "authorize_ok".into(),
                loc: SourceLocation::point(1.into(), 5),
            },
            CallEdge {
                from: SymbolId(200),
                to: SymbolId(202),
                event: "fulfill".into(),
                loc: SourceLocation::point(1.into(), 9),
            },
        ],
    };
    assert!(matches!(check_rule(&machine, &csr, 50_000_000), RuleOutcome::Holds));
}

/// InferredCandidate rules are labeled SUSPECTED and never promoted
/// (global rule 4).
#[test]
fn task_049_inferred_rules_stay_suspected() {
    let mut machine = rule();
    machine.authority = Authority::InferredCandidate;
    let csr = fixture_csr_caller_rejects_then_fulfills();
    match check_rule(&machine, &csr, 50_000_000) {
        RuleOutcome::Violated { suspected, trace } => {
            assert!(suspected, "inferred rule hits are labeled suspected");
            let ev = trace_evidence(&trace, REV, suspected);
            assert!(ev.provenance.contains("SUSPECTED"));
        }
        other => panic!("expected Violated, got {other:?}"),
    }
}

/// Budget expiry → Unknown(TimedOut), never "rule holds" (global rule 3).
#[test]
fn task_049_budget_expiry_is_unknown() {
    let machine = rule();
    let csr = fixture_csr_caller_rejects_then_fulfills();
    assert!(matches!(
        check_rule_budgeted(&machine, &csr, 0),
        RuleOutcome::Unknown(_)
    ));
}

/// Cycle safety: a self-loop in the call graph terminates (on-stack set).
#[test]
fn task_049_cycles_terminate() {
    let machine = rule();
    let csr = Csr {
        edges: vec![
            CallEdge {
                from: SymbolId(300),
                to: SymbolId(300),
                event: "authorize_ok".into(),
                loc: SourceLocation::point(2.into(), 1),
            },
            CallEdge {
                from: SymbolId(300),
                to: SymbolId(301),
                event: "fulfill".into(),
                loc: SourceLocation::point(2.into(), 5),
            },
        ],
    };
    // authorize_ok loops forever, then fulfill from Idle → Idle: no Error
    assert!(matches!(check_rule(&machine, &csr, 50_000_000), RuleOutcome::Holds));
}
