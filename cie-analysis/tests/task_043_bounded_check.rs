//! TASK-043 validation: bounded predicate checking with budget semantics.
//!
//! Sketch deviations (documented, single-point):
//! - `registry_with(Contract::explicit(Obligation::Safety,
//!   Predicate::requires(Action::Charge, Action::Authorized), ev_src()))` →
//!   a `Contract` whose `expr` is the `requires(charge, authorized)` DSL
//!   form (the structured-predicate payload lives in the expr per the
//!   established expr-as-DSL contract shape).
//! - `fx.path_conditions()` → `realize_paths` over summaries (the
//!   TASK-032-derived path layer).
//! - `viol.counterexample.path.ends_with(fx.unauthorized_call)` → the
//!   counterexample trail's LAST step is the breaching call.

use cie_analysis::predicates::{parse_requires, PredicateExtractor, RealizedPath};
use cie_contract::{Authority, Contract, ContractRegistry, ContractSource, ObligationKind};
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_counterexample::{BoundedChecker, Conclusion, Status};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

const REV: RevId = 21;

fn fixture_payment_paths() -> (
    Vec<Contract>,
    BTreeMap<SymbolId, Vec<RealizedPath>>,
    BTreeMap<SymbolId, String>,
) {
    // contract: charge requires prior authorize (Explicit, docstring source)
    let contracts = vec![Contract {
        id: 0, // registry assigns
        obligation: ObligationKind::MustAuthorize,
        expr: "requires(charge, authorized)".into(),
        authority: Authority::Explicit,
        symbols: vec![],
        source: ContractSource::Docstring,
    }];

    // symbols: pay() at 0, charge() at 1 (positional store)
    let store = SymbolStore::build(vec![
        SymbolFact {
            id: SymbolId(0),
            kind: SymbolKind::Function,
            name: "pay".into(),
            qual_name: "m.pay".into(),
            loc: SourceLocation::point(0.into(), 0),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
        SymbolFact {
            id: SymbolId(1),
            kind: SymbolKind::Function,
            name: "charge".into(),
            qual_name: "m.charge".into(),
            loc: SourceLocation::point(0.into(), 1),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
    ]);
    let mut names = BTreeMap::new();
    for s in &store.symbols {
        names.insert(s.id, s.name.clone());
    }

    // pay() has two paths: authorized / unauthorized, both reaching charge.
    let mut pay = FunctionSummary::empty(SymbolId(0), REV, 0);
    pay.callees.push(SymbolId(1));
    pay.path_conditions.push("authorized == true".into());
    pay.path_conditions.push("authorized == false".into());
    let mut summaries = BTreeMap::new();
    summaries.insert(SymbolId(0), pay);
    summaries.insert(SymbolId(1), FunctionSummary::empty(SymbolId(1), REV, 0));

    let realized = PredicateExtractor::realize_paths(&summaries, &names, 16);
    (contracts, realized, names)
}

/// The sketch's `task_043_bounded_predicate_check`.
#[test]
fn task_043_bounded_predicate_check() {
    let (contracts, realized, _names) = fixture_payment_paths();

    // sketch: preds.len() == 1; provenance cites the contract
    let preds = PredicateExtractor::new(&contracts).extract(&realized);
    assert_eq!(preds.len(), 1);
    assert_eq!(preds[0].from_contract, Some(0));
    // two realized paths: authorized arm and unauthorized arm
    assert_eq!(preds[0].paths.len(), 2, "both arms realized: {:?}", preds[0].paths);

    // within budget: unauthorized path yields violation with counterexample
    let checker = BoundedChecker::new().budget_ns(5_000_000_000).depth(64);
    let r = checker.check(&preds, REV);
    assert_eq!(r.len(), 1);
    let viol = r
        .iter()
        .find(|x| x.conclusion == Conclusion::Violated)
        .expect("unauthorized path violates requires(charge, authorized)");
    assert_eq!(viol.status, Status::Complete);
    let cx = viol.counterexample.as_ref().unwrap();
    let last = cx.path.last().expect("non-empty trail");
    assert_eq!(last, "charge", "counterexample ends at the breaching call");
    // bounds and assumptions appear in the emitted evidence (FR-4)
    assert!(viol
        .evidence
        .assumptions
        .iter()
        .any(|a| a.starts_with("budget_ns=")));
    assert!(viol.evidence.assumptions.iter().any(|a| a.starts_with("depth=")));

    // sketch: budget-exhausted run returns Unknown/TimedOut for every
    // predicate (global rule 3: timeout is NEVER "no defect")
    let tight = BoundedChecker::new().budget_ns(1);
    let r2 = tight.check(&preds, REV);
    assert!(r2.iter().all(|x| x.conclusion == Conclusion::Unknown
        && x.status == Status::TimedOut));
    // rule-3 consistency: a TimedOut verdict must not claim clean evidence
    for v in &r2 {
        assert_ne!(v.conclusion, Conclusion::Clean);
    }
}

/// The authorized-only codebase checks clean, and the mechanical proof
/// upgrades the candidate ONLY through the registry's guarded transition.
#[test]
fn task_043_verified_upgrade_flows_through_registry() {
    let (contracts, realized, _names) = fixture_payment_paths();

    // authorized-only variant: drop the false arm
    let only_authorized: Vec<Contract> = contracts.clone();
    let mut realized2 = realized.clone();
    for (_id, paths) in realized2.iter_mut() {
        for p in paths.iter_mut() {
            p.conditions.retain(|c| !c.contains("== false"));
        }
    }

    let preds = PredicateExtractor::new(&only_authorized).extract(&realized2);
    let checker = BoundedChecker::new().budget_ns(5_000_000_000).depth(64);
    let r = checker.check(&preds, REV);
    assert!(r.iter().all(|x| x.conclusion == Conclusion::Clean));

    // the proof string exists for fully-clean runs...
    let proof = BoundedChecker::proof_string(&r).expect("clean run yields proof");
    assert!(proof.contains("clean"));

    // ...and the upgrade flows ONLY through the guarded registry API.
    let mut reg = ContractRegistry::new();
    let mut candidate = only_authorized[0].clone();
    candidate.authority = Authority::InferredCandidate;
    candidate.source = ContractSource::NamingConvention;
    let id = reg.insert(candidate).expect("candidate admitted");
    assert_eq!(
        reg.upgrade_to_verified(id, &proof).unwrap(),
        Authority::Verified
    );
    // empty proof rejected (no mechanical check, no upgrade)
    let id2 = reg
        .insert(Contract {
            id: 0,
            obligation: ObligationKind::MustAuthorize,
            expr: "requires(refund, authorized)".into(),
            authority: Authority::InferredCandidate,
            symbols: vec![],
            source: ContractSource::NamingConvention,
        })
        .unwrap();
    assert!(reg.upgrade_to_verified(id2, "").is_err());
}

/// The DSL parser is the single expr lowering point; malformed exprs are
/// unsupported, never guessed.
#[test]
fn task_043_requires_dsl() {
    assert_eq!(
        parse_requires("requires(charge, authorized)"),
        Some(("charge".into(), "authorized".into()))
    );
    assert_eq!(parse_requires("charge must be preceded by authorize"), None);
    assert_eq!(parse_requires("requires(a,)"), None);
    assert_eq!(parse_requires("requires(,b)"), None);
}

/// Path realization is deterministic and bounded (TASK-029 budget proxy).
#[test]
fn task_043_realization_determinism() {
    let (_c, realized, _n) = fixture_payment_paths();
    let a = serde_json::to_string(&realized.keys().collect::<Vec<_>>()).unwrap();
    let (c2, realized2, _n2) = fixture_payment_paths();
    let _ = c2;
    let b = serde_json::to_string(&realized2.keys().collect::<Vec<_>>()).unwrap();
    assert_eq!(a, b);
}
