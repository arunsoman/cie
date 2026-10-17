//! TASK-010 validation sketch — instructor contract, adapted per
//! docs/TRACEABILITY.md §2 (`cie_semantic::prelude` → `cie_core::facts`;
//! the sketch's `CallFact{caller,callee,kind,unresolved}` maps to
//! `CallSite{caller, callee: Callee, …}` where `Callee::Unresolved(reason)`
//! carries the unresolved flag plus its §13#6 reason).

use cie_core::types::{FileId, SourceLocation, SymbolId};
use cie_core::CieError;
use cie_semantic::facts::{CallSite, Callee, SymbolFact, SymbolKind, TypedFacts};

fn sym_fact(id: u32, kind: SymbolKind) -> SymbolFact {
    SymbolFact {
        id: SymbolId(id),
        kind,
        name: format!("sym{id}"),
        qual_name: format!("mod.sym{id}"),
        loc: SourceLocation::span_bytes(FileId(0), id * 8, (id + 1) * 8),
        lang: cie_semantic::LangId::Python,
        parent: None,
        rev: 1,
        doc: None,
    }
}

fn call_fact(caller: u32, callee: u32, _kind_note: &str, resolved: bool) -> CallSite {
    CallSite {
        caller: SymbolId(caller),
        callee_text: format!("target{callee}"),
        callee: if resolved {
            Callee::Resolved(SymbolId(callee))
        } else {
            Callee::Unresolved("dynamic_attribute".into())
        },
        line: callee,
        in_loop: false,
        arg_vars: vec![],
    }
}

#[test]
fn task_010_typed_facts_dedup_and_integrity() {
    let mut facts = TypedFacts::default();
    facts.push_symbol(sym_fact(1, SymbolKind::Function));
    facts.push_symbol(sym_fact(1, SymbolKind::Function)); // exact duplicate
    facts.push_call(call_fact(1, 2, "dynamic", true));
    facts.push_call(call_fact(1, 2, "dynamic", true)); // duplicate

    facts.dedup();
    assert_eq!(facts.symbols.len(), 1, "deterministic dedup");
    assert_eq!(facts.calls.len(), 1);
    facts.dedup();
    assert_eq!(facts.symbols.len(), 1, "dedup is idempotent");

    facts.push_call(call_fact(1, 99, "direct", true));
    assert!(
        matches!(
            facts.check_integrity(),
            Err(CieError::DanglingReference { .. })
        ),
        "dangling ids are errors, not silent edges"
    );

    let snapshot = facts.clone();
    facts.dedup();
    assert_eq!(facts, snapshot, "same revision ⇒ byte-stable facts (FR-4)");
}

/// TASK-010 DoD: the aggregate roundtrips through serde (§15.1 storage).
#[test]
fn task_010_typed_facts_serde_roundtrip() {
    let mut facts = TypedFacts::default();
    facts.push_symbol(sym_fact(2, SymbolKind::Method));
    facts.relations.add_edge(
        petgraph::graph::NodeIndex::new(0),
        petgraph::graph::NodeIndex::new(1),
        cie_relations::RelKind::Calls,
    );
    let bytes = serde_json::to_vec(&facts).unwrap();
    let back: TypedFacts = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(facts, back, "serde roundtrip is byte-stable");
}
