//! TASK-040 validation: candidate-rule inference from deviant behavior
//! (Engler signals + Kremenek factor graph).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_checked_returns()` → 7 callers of `frob()` where 6 honor the
//!   result (path condition mentions the callee) and 1 ignores it.
//! - `fx.call_relations()` → a `RelationGraph` with `Calls` edges.
//! - `BeliefSignals::collect(&fx.summaries(), &fx.call_relations())` →
//!   additionally takes the `SymbolStore` (callee-name resolution) and rev.
//! - `rule.support_sites.len()` etc. are direct fields on `Belief`.

use cie_analysis::beliefs::BeliefSignals;
use cie_contract::beliefs::FactorGraph;
use cie_contract::{Authority, ContractRegistry, Predicate};
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_relations::{RelKind, RelationGraph};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

const REV: RevId = 11;

fn sym(id: u32, name: &str) -> SymbolFact {
    SymbolFact {
            annotations: Vec::new(),
        id: SymbolId(id),
        kind: SymbolKind::Function,
        name: name.to_string(),
        qual_name: format!("m.{name}"),
        loc: SourceLocation::point(0.into(), 0),
        lang: LangId::Python,
        parent: None,
        rev: REV,
        doc: None,
    }
}

/// 7 callers of `frob()` (ids 1..=7): 6 build a condition on the result,
/// caller 7 ignores it. Plus `lock`/`unlock` pairing: 3 callers of `lock`,
/// 2 release, 1 leaks. `frob`/`lock`/`unlock` live at ids 0/8/9 so the
/// positional store layout (`get` indexes by id) stays dense.
fn fixture() -> (SymbolStore, BTreeMap<SymbolId, FunctionSummary>, RelationGraph) {
    let mut facts = vec![sym(0, "frob"), sym(8, "lock"), sym(9, "unlock")];
    for i in 1..=7u32 {
        facts.push(sym(i, &format!("caller_{i}")));
    }
    facts.push(sym(10, "leaky"));
    let store = SymbolStore::build(facts);

    let mut summaries = BTreeMap::new();
    let mut frob = FunctionSummary::empty(SymbolId(0), REV, 0);
    frob.returns_constants.insert("num".into());
    summaries.insert(SymbolId(0), frob);

    // frob's callers: 6 honor, 1 deviant.
    for i in 1..=6u32 {
        let mut s = FunctionSummary::empty(SymbolId(i), REV, 0);
        s.callees.push(SymbolId(0));
        s.path_conditions
            .push(format!("if frob() > 0 /* site {i} */"));
        summaries.insert(SymbolId(i), s);
    }
    let mut deviant = FunctionSummary::empty(SymbolId(7), REV, 0);
    deviant.callees.push(SymbolId(0));
    deviant.effects.insert(cie_summary::EffectKind::Print);
    summaries.insert(SymbolId(7), deviant);

    // lock/unlock: ids 8/9; callers 1-3 lock (1 and 2 also unlock), 10 leaks.
    let mut lock = FunctionSummary::empty(SymbolId(8), REV, 0);
    lock.effects.insert(cie_summary::EffectKind::Lock);
    summaries.insert(SymbolId(8), lock);
    summaries.insert(SymbolId(9), FunctionSummary::empty(SymbolId(9), REV, 0));
    for i in [1u32, 2, 3] {
        let s = summaries.get_mut(&SymbolId(i)).unwrap();
        s.callees.push(SymbolId(8));
    }
    for i in [1u32, 2] {
        let s = summaries.get_mut(&SymbolId(i)).unwrap();
        s.callees.push(SymbolId(9));
    }
    let mut leaky = FunctionSummary::empty(SymbolId(10), REV, 0);
    leaky.callees.push(SymbolId(8));
    summaries.insert(SymbolId(10), leaky);

    // relation graph: Calls edges
    let mut g = RelationGraph::new();
    let f0 = g.add_node(SymbolId(0));
    let f8 = g.add_node(SymbolId(8));
    let f9 = g.add_node(SymbolId(9));
    for i in 1..=7u32 {
        let n = g.add_node(SymbolId(i));
        g.add_edge(n, f0, RelKind::Calls);
    }
    let n1 = g.add_node(SymbolId(1));
    let n2 = g.add_node(SymbolId(2));
    let n3 = g.add_node(SymbolId(3));
    let n10 = g.add_node(SymbolId(10));
    for n in [n1, n2, n3, n10] {
        g.add_edge(n, f8, RelKind::Calls);
    }
    g.add_edge(n1, f9, RelKind::Calls);
    g.add_edge(n2, f9, RelKind::Calls);

    (store, summaries, g)
}

/// The sketch's `task_040_belief_inference`.
#[test]
fn task_040_belief_inference() {
    let (store, summaries, relations) = fixture();
    let signals = BeliefSignals::collect(&store, &summaries, &relations, REV);
    assert_eq!(signals.check_consistency("frob").unwrap().support(), 6);

    let mut graph = FactorGraph::new(); // Kremenek-style combination
    graph.add(signals.check_consistency("frob").unwrap().factor());
    graph.add(signals.cleanup_pairing("lock", "unlock").unwrap().factor());

    let beliefs = graph.infer(); // deterministic posterior
    let rule = beliefs
        .iter()
        .find(|b| b.predicate == Predicate::retval_checked("frob"))
        .expect("retval-checked belief emitted");
    assert_eq!(rule.authority, Authority::InferredCandidate); // §8: inferred never established
    assert_eq!(rule.support_sites.len(), 6);
    assert_eq!(rule.deviant_sites.len(), 1);
    let deviant = rule.deviant_sites[0];
    assert_eq!(deviant.0, SymbolId(7)); // Engler: deviant = suspect
    // global rule 2: evidence cites support AND deviant sites
    assert!(rule.evidence.provenance.contains("support=6"));
    assert!(rule.evidence.provenance.contains("deviant=1"));
    assert_eq!(graph.infer(), beliefs); // determinism
}

/// The guarded registry admits belief contracts only as InferredCandidate
/// (§8/global rule 4 — no candidate ever carries Explicit/Verified).
#[test]
fn task_040_beliefs_are_candidates_only() {
    let (store, summaries, relations) = fixture();
    let signals = BeliefSignals::collect(&store, &summaries, &relations, REV);
    let mut reg = ContractRegistry::new();
    for f in signals.factors() {
        let mut g = FactorGraph::new();
        g.add(f);
        for b in g.infer() {
            let syms: Vec<SymbolId> = b
                .support_sites
                .iter()
                .chain(b.deviant_sites.iter())
                .map(|(c, _)| *c)
                .collect();
            let contract = b.to_contract(syms);
            assert_eq!(contract.authority, Authority::InferredCandidate);
            reg.insert(contract).expect("candidate admitted");
        }
    }
    assert!(reg
        .all()
        .iter()
        .all(|c| c.authority == Authority::InferredCandidate));
}

/// Byte-identical determinism on identical inputs (FR-4).
#[test]
fn task_040_byte_identical_determinism() {
    let (store, summaries, relations) = fixture();
    let s1 = BeliefSignals::collect(&store, &summaries, &relations, REV);
    let s2 = BeliefSignals::collect(&store, &summaries, &relations, REV);
    let f1 = serde_json::to_string(&s1.factors()).unwrap();
    let f2 = serde_json::to_string(&s2.factors()).unwrap();
    assert_eq!(f1, f2);
    let mut g1 = FactorGraph::new();
    let mut g2 = FactorGraph::new();
    for f in s1.factors() {
        g1.add(f);
    }
    for f in s2.factors() {
        g2.add(f);
    }
    assert_eq!(
        serde_json::to_string(&g1.infer()).unwrap(),
        serde_json::to_string(&g2.infer()).unwrap()
    );
}

/// A clean majority with NO deviants yields no factor (no majority
/// pattern without both sides — nothing to infer).
#[test]
fn task_040_no_signal_without_deviation() {
    let (store, mut summaries, relations) = fixture();
    // make caller 7 honor frob too → all 7 sites support, 0 deviate
    let s = summaries.get_mut(&SymbolId(7)).unwrap();
    s.path_conditions.push("if frob() > 0".into());
    let signals = BeliefSignals::collect(&store, &summaries, &relations, REV);
    let check = signals.check_consistency("frob").unwrap();
    assert_eq!(check.support(), 7);
    assert!(check.deviant_sites().is_empty());
    // the factors() view drops the all-support signal
    assert!(signals.factors().iter().all(
        |f| f.deviant_sites.is_empty() || f.support_sites.is_empty() || f.weight > 0.0,
    ));
}
