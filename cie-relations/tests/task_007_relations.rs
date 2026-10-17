//! TASK-007 validation sketch — instructor contract, bodies intact.
//! Single documented mapping: `cie_relations` → `cie_core::relations`.

use cie_core::SymbolId;
use cie_relations::{RelKind, RelationGraph};

#[test]
fn task_007_relation_graph_kinds_and_stability() {
    let mut g = RelationGraph::new();
    let a = g.add_node(SymbolId(1));
    let b = g.add_node(SymbolId(2));
    let c = g.add_node(SymbolId(3));

    g.add_edge(a, b, RelKind::Calls);
    g.add_edge(a, b, RelKind::Calls); // duplicate collapses
    assert_eq!(g.edge_count(), 1, "duplicate (from,to,kind) is deduped");
    g.add_edge(a, b, RelKind::DataFlow); // different kind coexists
    assert_eq!(g.edge_count(), 2);
    g.add_edge(b, c, RelKind::Throws);
    g.add_edge(c, b, RelKind::Catches);

    g.remove_node(a); // FR-8: deletions must not renumber survivors
    assert_eq!(
        g.node_weight(b),
        Some(SymbolId(2)),
        "StableGraph keeps b stable"
    );
    assert_eq!(
        g.node_weight(c),
        Some(SymbolId(3)),
        "StableGraph keeps c stable"
    );
}

/// TASK-007: builder from `LangFacts` — resolved calls → `Calls`,
/// containment → `Owns`; statement-level kinds stay with statement owners.
#[test]
fn task_007_builder_from_facts() {
    use cie_core::types::{FileId, SourceLocation};
    use cie_semantic::facts::{LangFacts, SymbolFact, SymbolKind, SymbolStore};

    let sym = |id: u32, parent: Option<u32>, qual: &str| SymbolFact {
        id: SymbolId(id),
        kind: if parent.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Class
        },
        name: qual.rsplit('.').next().unwrap_or(qual).to_string(),
        qual_name: qual.to_string(),
        loc: SourceLocation::span(FileId(0), "cart.py", 1, 10),
        lang: cie_semantic::LangId::Python,
        parent: parent.map(SymbolId),
        rev: 1,
        doc: None,
    };
    let store = SymbolStore::build(vec![sym(0, None, "Cart"), sym(1, Some(0), "Cart.add_item")]);

    let mut f = LangFacts::new("cart.py", cie_semantic::LangId::Python, "test");
    f.symbols = store.symbols.clone();
    f.calls.push(cie_semantic::facts::CallSite {
        caller: SymbolId(1),
        callee_text: "db.save".into(),
        callee: cie_semantic::facts::Callee::Resolved(SymbolId(0)),
        line: 3,
        in_loop: false,
        arg_vars: vec![],
    });

    let g = cie_semantic::graph::relation_graph_from_facts(&[f], &store);
    assert!(g.edge_count() >= 2, "Calls + Owns projected");
    assert!(g.has_edge(
        petgraph::graph::NodeIndex::new(1),
        petgraph::graph::NodeIndex::new(0),
        RelKind::Calls
    ));
}
