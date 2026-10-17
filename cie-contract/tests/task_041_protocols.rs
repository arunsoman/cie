//! TASK-041 validation: protocol mining + Houdini-style refinement.
//!
//! Sketch deviations (documented, single-point):
//! - `fx.traces()` → a `Traces` fixture derived from summary/relation
//!   event sequences (trace derivation belongs to the summary layer; the
//!   mining contract starts from traces).
//! - `Predicate::before(..)` / `Predicate::never_after(..)` →
//!   `ProtocolExpr::before(..)` / `ProtocolExpr::never_after(..)` (the
//!   TASK-041 predicate space; `beliefs::Predicate` is TASK-040's).
//! - `Houdini::new(miner.registry_view())` → same shape, with survivor
//!   registration flowing through the guarded registry via
//!   `Houdini::register` (the single write path, §8 rule 4).
//! - `r.by_trace == fx.trace_commit_after_rollback` → the refuting trace
//!   is compared as the exact event sequence.

use cie_contract::{Authority, ContractRegistry, Houdini, ObligationKind, ProtocolExpr,
                   ProtocolMiner, Traces};

/// Fixture `fixture_txn_api()`: traces over begin/commit/rollback.
/// - t1: begin, commit                  (happy path)
/// - t2: begin, begin, commit           (nested begin; commit at distance 2)
/// - t3: begin, rollback                (abort path)
/// - t4: begin, rollback, commit        (the refuting trace: commit AFTER rollback)
/// One unresolved call (`middleware_hook`) excluded from the traces and
/// recorded (FR-4).
fn fixture_txn_api() -> Traces {
    Traces {
        traces: vec![
            vec!["begin".into(), "commit".into()],
            vec!["begin".into(), "begin".into(), "commit".into()],
            vec!["begin".into(), "rollback".into()],
            vec!["begin".into(), "rollback".into(), "commit".into()],
        ],
        unresolved: vec!["middleware_hook".into()],
    }
}

/// The sketch's `task_041_protocol_mining_houdini`.
#[test]
fn task_041_protocol_mining_houdini() {
    let fx = fixture_txn_api();
    let mut miner = ProtocolMiner::bounded(2); // Ammons et al., k-bounded
    let cands = miner.mine(&fx);

    // candidate "commit requires prior begin" is mined
    let want = ProtocolExpr::before("commit", "begin");
    assert!(
        cands.contains(&want),
        "before(commit, begin) must be mined, got {:?}",
        cands.iter().map(|c| c.as_str()).collect::<Vec<_>>()
    );

    let view = miner.registry_view();
    let mut houdini = Houdini::new(&view); // Flanagan & Leino
    let survivors = houdini.refine(cands, &fx); // mechanically remove refuted

    let keep = survivors
        .iter()
        .find(|c| c.expr == ProtocolExpr::before("commit", "begin"))
        .expect("commit-requires-prior-begin survives");
    assert_eq!(keep.authority, Authority::InferredCandidate);
    assert_eq!(keep.survived_checks, 4, "checked against all traces");
    // FR-4: unresolved calls ride in evidence
    assert!(keep.evidence.unresolved.contains(&"middleware_hook".to_string()));

    // the too-strong candidate "commit never follows rollback" is refuted
    let too_strong = ProtocolExpr::never_after("commit", "rollback");
    assert!(!survivors.iter().any(|c| c.expr == too_strong));

    let refuted = houdini.refuted_log(); // append-only, evidenced
    let entry = refuted
        .iter()
        .find(|r| r.candidate == too_strong)
        .expect("refutation logged");
    assert_eq!(
        entry.by_trace,
        vec!["begin".to_string(), "rollback".to_string(), "commit".to_string()],
        "refutation cites the exact commit-after-rollback trace"
    );
    // content-addressed audit key (FR-8 spirit, §15.1 keys)
    assert!(entry.key().starts_with("refuted:"));
    assert!(!entry.content_hash.is_empty());
}

/// Survivors register as `ObligationKind::Protocol` contracts with
/// InferredCandidate authority ONLY (guarded registry rejects the rest).
#[test]
fn task_041_survivors_register_as_protocol_candidates() {
    let fx = fixture_txn_api();
    let miner = ProtocolMiner::bounded(2);
    let cands = miner.mine(&fx);
    let view = miner.registry_view();
    let mut houdini = Houdini::new(&view);
    let survivors = houdini.refine(cands, &fx);

    let mut reg = ContractRegistry::new();
    let ids = houdini.register(&mut reg, &survivors, vec![]).expect("all admitted");
    assert_eq!(ids.len(), survivors.len());
    let protocols = reg.all().iter().filter(|c| c.obligation == ObligationKind::Protocol);
    assert!(protocols.clone().count() >= 1);
    for c in protocols {
        assert_eq!(c.authority, Authority::InferredCandidate);
        assert!(c.expr.starts_with("before(") || c.expr.starts_with("never_after("));
    }

    // the guard: a Protocol contract with Explicit authority is rejected
    let attempt = cie_contract::Contract {
        id: 0,
        obligation: ObligationKind::Protocol,
        expr: "before(commit, begin)".into(),
        authority: Authority::Explicit,
        symbols: vec![],
        source: cie_contract::ContractSource::Docstring,
    };
    assert!(reg.insert(attempt).is_err(), "§8 rule 4: protocol rules stay candidates");
}

/// Termination + determinism: bounded rounds, byte-identical runs.
#[test]
fn task_041_bounded_and_deterministic() {
    let fx = fixture_txn_api();
    let cands1 = ProtocolMiner::bounded(2).mine(&fx);
    let cands2 = ProtocolMiner::bounded(2).mine(&fx);
    assert_eq!(
        serde_json::to_string(&cands1).unwrap(),
        serde_json::to_string(&cands2).unwrap()
    );

    let mut h1 = Houdini::new(&ContractRegistry::new());
    let s1 = h1.refine(cands1, &fx);
    let mut h2 = Houdini::new(&ContractRegistry::new());
    let s2 = h2.refine(cands2, &fx);
    assert_eq!(
        serde_json::to_string(&s1).unwrap(),
        serde_json::to_string(&s2).unwrap()
    );
    // refutation logs match (append-only, deterministic)
    assert_eq!(h1.refuted_log().len(), h2.refuted_log().len());
}

/// Mechanical check semantics: `holds_on` is exact per trace.
#[test]
fn task_041_expr_semantics() {
    let t = vec!["begin".to_string(), "rollback".to_string(), "commit".to_string()];
    assert!(ProtocolExpr::before("commit", "begin").holds_on(&t));
    assert!(!ProtocolExpr::never_after("commit", "rollback").holds_on(&t));
    assert!(!ProtocolExpr::before("commit", "authorize").holds_on(&t));
    // every commit has prior begin even with nested begins
    let nested = vec!["begin".to_string(), "begin".to_string(), "commit".to_string()];
    assert!(ProtocolExpr::before("commit", "begin").holds_on(&nested));
}
