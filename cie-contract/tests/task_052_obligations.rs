//! TASK-052 validation: recovery flows as checkable safety/liveness
//! obligations; the call-vs-effective refund distinction (FR-15).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_refund_logs_only()` → BTreeMap<SymbolId, FunctionSummary>
//!   where `refund()` has effects {Log} only (external: false) and the
//!   ledger write lives in a separate `apply_ledger` summary (DbWrite) —
//!   the codebase HAS the external write; the refund method just never
//!   invokes it.
//! - `Obligation::Safety/Liveness` → `ObligationKind::Safety/Liveness`.
//! - `StateModelBuilder::from_effects(&summaries)` takes the SymbolStore
//!   too (summaries carry ids; names come from the store).
//! - `eff.provenance.iter().all(|p| p.external_write)` — direct field.

use cie_contract::obligations::{
    obligations, Event, State, StateModelBuilder,
};
use cie_contract::{Authority, ContractRegistry, ObligationKind};
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::{EffectKind, FunctionSummary};
use std::collections::{BTreeMap, BTreeSet};

const REV: RevId = 31;

fn sym(id: u32, name: &str) -> SymbolFact {
    SymbolFact {
        id: SymbolId(id),
        kind: SymbolKind::Function,
        name: name.to_string(),
        qual_name: format!("pay.{name}"),
        loc: SourceLocation::point(0.into(), 0),
        lang: LangId::Python,
        parent: None,
        rev: REV,
        doc: None,
    }
}

/// `fixture_refund_logs_only()`: refund() only logs — no external write
/// ever occurs in IT; the ledger write exists in `apply_ledger`.
fn fixture() -> (SymbolStore, BTreeMap<SymbolId, FunctionSummary>) {
    let store = SymbolStore::build(vec![
        sym(0, "refund"),
        sym(1, "apply_ledger"),
        sym(2, "authorize_payment"),
        sym(3, "charge_card"),
    ]);
    let mut summaries = BTreeMap::new();

    // refund(): logs only — external: false (writes: [log])
    let mut refund = FunctionSummary::empty(SymbolId(0), REV, 0);
    refund.effects.insert(EffectKind::Log);
    refund.writes.insert("log".into());
    summaries.insert(SymbolId(0), refund);

    // the external write exists elsewhere (ledger)
    let mut ledger = FunctionSummary::empty(SymbolId(1), REV, 0);
    ledger.effects.insert(EffectKind::DbWrite);
    let mut w = BTreeSet::new();
    w.insert("ledger.balance".into());
    ledger.writes = w;
    summaries.insert(SymbolId(1), ledger);

    let mut auth = FunctionSummary::empty(SymbolId(2), REV, 0);
    auth.effects.insert(EffectKind::DbRead);
    summaries.insert(SymbolId(2), auth);

    let mut charge = FunctionSummary::empty(SymbolId(3), REV, 0);
    charge.effects.insert(EffectKind::Network);
    summaries.insert(SymbolId(3), charge);

    (store, summaries)
}

/// The sketch's `task_052_refund_call_does_not_establish_refund_effective`.
#[test]
fn task_052_refund_call_does_not_establish_refund_effective() {
    let (store, summaries) = fixture();
    let model = StateModelBuilder::from_effects(&store, &summaries);
    let obs = obligations(&model, vec![]);

    // both FR-15 obligation kinds registered, distinct contracts
    let safety = obs
        .iter()
        .find(|c| c.obligation == ObligationKind::Safety)
        .expect("safety obligation");
    let liveness = obs
        .iter()
        .find(|c| c.obligation == ObligationKind::Liveness)
        .expect("liveness obligation");
    assert_ne!(safety.expr, liveness.expr);
    assert_eq!(
        safety.authority,
        Authority::InferredCandidate,
        "inferred transitions never auto-verify"
    );
    assert_eq!(liveness.authority, Authority::InferredCandidate);

    // the CRITICAL fact — refund *called* ≠ money *returned*
    let t = model
        .on(State::RefundRequested, Event::RefundCalled)
        .expect("call transition exists");
    assert_ne!(
        t.to,
        State::Refunded,
        "a refund method being called does not establish that money was returned"
    );
    assert_eq!(t.to, State::RefundPending);

    // only an effective-refund transition (external write evidence)
    // reaches Refunded
    let eff = model
        .on(State::RefundPending, Event::RefundEffective)
        .expect("effective transition");
    assert_eq!(eff.to, State::Refunded);
    assert!(
        eff.provenance.iter().all(|p| p.external_write),
        "effectiveness needs external-write evidence"
    );
    // the logs-only refund's provenance must NOT appear on the effective edge
    assert!(eff
        .provenance
        .iter()
        .all(|p| p.symbol != SymbolId(0)));

    // every edge is provenance-backed (global rule 2)
    assert!(model.edges().iter().all(|e| !e.provenance.is_empty()));

    // the registry admits both as distinct candidate contracts
    let mut reg = ContractRegistry::new();
    for c in obs {
        reg.insert(c).expect("candidate obligations admitted");
    }
    assert!(reg.all().iter().any(|c| c.obligation == ObligationKind::Safety));
    assert!(reg.all().iter().any(|c| c.obligation == ObligationKind::Liveness));
}

/// The logs-only refund is NOT evidenced as effective (the builder's
/// honesty check used by the liveness obligation).
#[test]
fn task_052_effective_evidence_requires_external_write() {
    let (store, mut summaries) = fixture();
    // case 1: ledger write present → effective evidenced
    let m1 = StateModelBuilder::from_effects(&store, &summaries);
    assert!(StateModelBuilder::refund_effective_is_evidenced(&m1));

    // case 2: strip ALL external writes → effective edge exists as a
    // template but carries template provenance; the honest flag is false
    summaries.get_mut(&SymbolId(1)).unwrap().effects.clear();
    summaries.get_mut(&SymbolId(3)).unwrap().effects.clear();
    let m2 = StateModelBuilder::from_effects(&store, &summaries);
    assert!(!StateModelBuilder::refund_effective_is_evidenced(&m2));
    // ...and the call transition STILL does not reach Refunded
    let t = m2.on(State::RefundRequested, Event::RefundCalled).unwrap();
    assert_ne!(t.to, State::Refunded);
}

/// Determinism: same inputs → byte-identical model (FR-4).
#[test]
fn task_052_model_determinism() {
    let (store, summaries) = fixture();
    let a = StateModelBuilder::from_effects(&store, &summaries);
    let b = StateModelBuilder::from_effects(&store, &summaries);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
