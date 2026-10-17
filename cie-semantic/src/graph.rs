//! Relation-graph builders over this crate's fact families (TASK-007
//! builder relocated here by the TASK-001 layering: `cie-relations` is
//! core-only per its spec; TypedFacts owns the graph).

use cie_core::types::SymbolId;
use cie_relations::{RelKind, RelationGraph};
use petgraph::graph::NodeIndex;

use crate::facts::{AssignmentFact, Callee, LangFacts, RhsKind, SymbolStore};

/// Symbol-level projection of one extraction pass (TASK-007 builder):
/// resolved calls → `Calls`, call-valued assignments → `DataFlow`,
/// containment → `Owns`, resolved imports → `DependsOn`. Statement-level
/// relations (`ControlFlow`, `Throws`/`Catches`, `Reads`/`Writes`) are
/// added by consumers that own statement facts (e.g. the PDG fixtures).
pub fn relation_graph_from_facts(facts: &[LangFacts], store: &SymbolStore) -> RelationGraph {
    let mut g = RelationGraph::new();
    // One node per store symbol, indexed by SymbolId.
    let mut idx_of: std::collections::BTreeMap<SymbolId, NodeIndex<u32>> = Default::default();
    for sym in &store.symbols {
        idx_of.insert(sym.id, g.add_node(sym.id));
    }
    let resolve = |qual: &str| -> Option<SymbolId> { store.resolve_qual(qual) };

    for f in facts {
        for c in &f.calls {
            if let Callee::Resolved(callee) = c.callee {
                if let (Some(&a), Some(&b)) = (idx_of.get(&c.caller), idx_of.get(&callee)) {
                    g.add_edge(a, b, RelKind::Calls);
                }
            }
        }
        // `x = f(...)` also makes the value flow from callee to caller.
        let call_assigns: Vec<&AssignmentFact> = f
            .assignments
            .iter()
            .filter(|a| a.rhs_kind == RhsKind::Call)
            .collect();
        for a in call_assigns {
            if let Some(c) = f
                .calls
                .iter()
                .find(|c| c.caller == a.symbol && c.line == a.line)
            {
                if let Callee::Resolved(callee) = c.callee {
                    if let (Some(&s), Some(&t)) = (idx_of.get(&callee), idx_of.get(&a.symbol)) {
                        g.add_edge(s, t, RelKind::DataFlow);
                    }
                }
            }
        }
        for sym in &f.symbols {
            if let Some(parent) = sym.parent {
                if let (Some(&p), Some(&c)) = (idx_of.get(&parent), idx_of.get(&sym.id)) {
                    g.add_edge(p, c, RelKind::Owns);
                }
            }
        }
        for im in &f.imports {
            let target = resolve(&im.module)
                .or_else(|| resolve(&format!("{}.{}", stem_of(&f.path), im.module)));
            if let (Some(&src), Some(&dst)) = (
                idx_of.get(&im.module_symbol),
                target.and_then(|t| idx_of.get(&t)),
            ) {
                g.add_edge(src, dst, RelKind::DependsOn);
            }
        }
    }
    g
}

fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
