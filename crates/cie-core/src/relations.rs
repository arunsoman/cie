//! Typed relation graph (PRD §14.3 / TASK-007) — the shared currency that
//! behavior extraction, slicing, and clustering all consume (FR-2, §5.1).
//!
//! `RelationGraph` wraps a `petgraph::stable_graph::StableGraph` so
//! deletions (FR-8) never shift surviving node indices, and deduplicates
//! `(from, to, kind)` triples deterministically while distinct kinds on the
//! same pair coexist. No language knowledge enters this module (FR-3): it
//! stores only what adapters vouched for upstream.

use crate::facts::{AssignmentFact, Callee, LangFacts, RhsKind};
use petgraph::visit::EdgeRef;
use crate::types::{SymbolId};
use serde::{Deserialize, Serialize};

/// §14.3: exactly ten relation kinds. Adapters express language nuance in
/// fact types, not in new edge kinds — this enum is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RelKind {
    Calls,
    Returns,
    ControlFlow,
    DataFlow,
    Reads,
    Writes,
    Throws,
    Catches,
    Owns,
    DependsOn,
}

impl RelKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelKind::Calls => "calls",
            RelKind::Returns => "returns",
            RelKind::ControlFlow => "control_flow",
            RelKind::DataFlow => "data_flow",
            RelKind::Reads => "reads",
            RelKind::Writes => "writes",
            RelKind::Throws => "throws",
            RelKind::Catches => "catches",
            RelKind::Owns => "owns",
            RelKind::DependsOn => "depends_on",
        }
    }
}

/// §16.1 carrier for operation-level control-flow edges — the adapter
/// `extract_control_flow` methods return these, so they live here, not in
/// `cie-adapters`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgEdge {
    pub from_op: crate::types::OperationId,
    pub to_op: crate::types::OperationId,
}

/// §16.1 carrier for operation-level data-flow edges (def → use).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DfgEdge {
    pub from_op: crate::types::OperationId,
    pub to_op: crate::types::OperationId,
    /// The flowing variable when known (def-use, not name equality alone).
    pub var: Option<String>,
}

pub use petgraph::graph::NodeIndex;
pub use petgraph::stable_graph::StableGraph;

/// Typed symbol-level relation graph. Duplicate `(from, to, kind)` edges
/// collapse; distinct kinds on the same pair coexist.
#[derive(Debug, Clone, Default)]
pub struct RelationGraph {
    inner: StableGraph<SymbolId, RelKind, petgraph::Directed, u32>,
}

impl RelationGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node carrying a `SymbolId` weight (stable across deletions).
    pub fn add_node(&mut self, sym: SymbolId) -> NodeIndex<u32> {
        self.inner.add_node(sym)
    }

    /// Add a relation edge; a duplicate `(from, to, kind)` collapses into
    /// the existing edge (returns `None`). Distinct kinds coexist.
    pub fn add_edge(&mut self, from: NodeIndex<u32>, to: NodeIndex<u32>, kind: RelKind) -> Option<petgraph::graph::EdgeIndex<u32>> {
        if self.has_edge(from, to, kind) {
            return None;
        }
        Some(self.inner.add_edge(from, to, kind))
    }

    /// True when the exact `(from, to, kind)` relation already exists.
    pub fn has_edge(&self, from: NodeIndex<u32>, to: NodeIndex<u32>, kind: RelKind) -> bool {
        self.inner
            .edges_directed(from, petgraph::Direction::Outgoing)
            .any(|e| e.target() == to && e.weight() == &kind)
    }

    pub fn remove_node(&mut self, idx: NodeIndex<u32>) -> Option<SymbolId> {
        self.inner.remove_node(idx)
    }

    pub fn remove_edge(&mut self, idx: petgraph::graph::EdgeIndex<u32>) -> Option<RelKind> {
        self.inner.remove_edge(idx)
    }

    pub fn node_weight(&self, idx: NodeIndex<u32>) -> Option<SymbolId> {
        self.inner.node_weight(idx).copied()
    }

    pub fn node_indices(&self) -> impl Iterator<Item = NodeIndex<u32>> + '_ {
        self.inner.node_indices()
    }

    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Outgoing edges of `from` (target, kind) pairs.
    pub fn edges_from(
        &self,
        from: NodeIndex<u32>,
    ) -> impl Iterator<Item = (NodeIndex<u32>, RelKind)> + '_ {
        self.inner
            .edges_directed(from, petgraph::Direction::Outgoing)
            .map(|e| (e.target(), *e.weight()))
    }

    /// Incoming edges of `to` (source, kind) pairs.
    pub fn edges_to(
        &self,
        to: NodeIndex<u32>,
    ) -> impl Iterator<Item = (NodeIndex<u32>, RelKind)> + '_ {
        self.inner
            .edges_directed(to, petgraph::Direction::Incoming)
            .map(|e| (e.source(), *e.weight()))
    }

    pub fn edge_weight(&self, idx: petgraph::graph::EdgeIndex<u32>) -> Option<RelKind> {
        self.inner.edge_weight(idx).copied()
    }

    pub fn edge_endpoints(
        &self,
        idx: petgraph::graph::EdgeIndex<u32>,
    ) -> Option<(NodeIndex<u32>, NodeIndex<u32>)> {
        self.inner.edge_endpoints(idx)
    }

    /// All edges as `(from_idx, to_idx, kind)` triples.
    pub fn edge_triples(&self) -> impl Iterator<Item = (NodeIndex<u32>, NodeIndex<u32>, RelKind)> + '_ {
        self.inner
            .edge_indices()
            .filter_map(|e| self.inner.edge_endpoints(e).map(|(a, b)| (a, b, *self.inner.edge_weight(e).unwrap())))
    }

    /// Symbol-level projection of one extraction pass (TASK-007 builder):
    /// resolved calls → `Calls`, call-valued assignments → `DataFlow`,
    /// containment → `Owns`, resolved imports → `DependsOn`. Statement-level
    /// relations (`ControlFlow`, `Throws`/`Catches`, `Reads`/`Writes`) are
    /// added by consumers that own statement facts (e.g. the PDG fixtures).
    pub fn from_facts(facts: &[LangFacts], store: &crate::facts::SymbolStore) -> Self {
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
                if let Some(c) = f.calls.iter().find(|c| {
                    c.caller == a.symbol && c.line == a.line
                }) {
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
                if let (Some(&src), Some(&dst)) = (idx_of.get(&im.module_symbol), target.and_then(|t| idx_of.get(&t))) {
                    g.add_edge(src, dst, RelKind::DependsOn);
                }
            }
        }
        g
    }
}

fn stem_of(path: &str) -> String {
    crate::types::lang_for_path(path); // keep detection referenced
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relkind_exactly_ten() {
        let all = [
            RelKind::Calls,
            RelKind::Returns,
            RelKind::ControlFlow,
            RelKind::DataFlow,
            RelKind::Reads,
            RelKind::Writes,
            RelKind::Throws,
            RelKind::Catches,
            RelKind::Owns,
            RelKind::DependsOn,
        ];
        assert_eq!(all.len(), 10, "§14.3: exactly ten kinds");
        // Distinct Debug strings prove the variants are distinct.
        let names: std::collections::BTreeSet<&str> = all.iter().map(|k| k.as_str()).collect();
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn kind_coexistence_and_dedup() {
        let mut g = RelationGraph::new();
        let a = g.add_node(SymbolId(1));
        let b = g.add_node(SymbolId(2));
        assert!(g.add_edge(a, b, RelKind::Calls).is_some());
        assert!(g.add_edge(a, b, RelKind::Calls).is_none(), "duplicate collapses");
        assert_eq!(g.edge_count(), 1);
        assert!(g.add_edge(a, b, RelKind::DataFlow).is_some(), "distinct kind coexists");
        assert_eq!(g.edge_count(), 2);
    }
}
