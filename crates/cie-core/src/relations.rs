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
use crate::types::{RevId, SymbolId};
use serde::{Deserialize, Serialize};

/// §14.3: exactly ten relation kinds. Adapters express language nuance in
/// fact types, not in new edge kinds — this enum is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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
/// collapse; distinct kinds on the same pair coexist. Serde is implemented
/// manually over the node/edge tables so the graph persists as plain JSON
/// (the petgraph serde feature is deliberately not required).
#[derive(Debug, Clone, Default)]
pub struct RelationGraph {
    inner: StableGraph<SymbolId, RelKind, petgraph::Directed, u32>,
}

/// Structural equality: same node weights and same edge set (StableGraph
/// itself is not `PartialEq`).
impl PartialEq for RelationGraph {
    fn eq(&self, other: &Self) -> bool {
        let self_nodes: std::collections::BTreeMap<u32, SymbolId> = self
            .inner
            .node_indices()
            .filter_map(|i| self.inner.node_weight(i).map(|w| (i.index() as u32, *w)))
            .collect();
        let other_nodes: std::collections::BTreeMap<u32, SymbolId> = other
            .inner
            .node_indices()
            .filter_map(|i| other.inner.node_weight(i).map(|w| (i.index() as u32, *w)))
            .collect();
        if self_nodes != other_nodes {
            return false;
        }
        let self_edges: std::collections::BTreeSet<(u32, u32, RelKind)> =
            self.edge_triples().map(|(a, b, k)| (a.index() as u32, b.index() as u32, k)).collect();
        let other_edges: std::collections::BTreeSet<(u32, u32, RelKind)> =
            other.edge_triples().map(|(a, b, k)| (a.index() as u32, b.index() as u32, k)).collect();
        self_edges == other_edges
    }
}

impl Serialize for RelationGraph {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Repr {
            nodes: Vec<(u32, SymbolId)>,
            edges: Vec<(u32, u32, RelKind)>,
        }
        let repr = Repr {
            nodes: self
                .inner
                .node_indices()
                .filter_map(|i| self.inner.node_weight(i).map(|w| (i.index() as u32, *w)))
                .collect(),
            edges: self
                .inner
                .edge_indices()
                .filter_map(|e| {
                    self.inner
                        .edge_endpoints(e)
                        .map(|(a, b)| (a.index() as u32, b.index() as u32, *self.inner.edge_weight(e).unwrap()))
                })
                .collect(),
        };
        repr.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RelationGraph {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Repr {
            nodes: Vec<(u32, SymbolId)>,
            edges: Vec<(u32, u32, RelKind)>,
        }
        let repr = Repr::deserialize(deserializer)?;
        let mut g = RelationGraph::new();
        for (idx, sym) in repr.nodes {
            let added = g.inner.add_node(sym);
            debug_assert_eq!(added.index() as u32, idx, "dense node replay");
        }
        for (a, b, kind) in repr.edges {
            let (a, b) = (NodeIndex::new(a as usize), NodeIndex::new(b as usize));
            g.add_edge(a, b, kind);
        }
        Ok(g)
    }
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
    /// Convenience: endpoints that were never added via `add_node` are
    /// auto-created carrying `SymbolId::PLACEHOLDER` (the TASK-009 sketch
    /// fixture relies on this; on a fresh graph sequential indices line up).
    pub fn add_edge(&mut self, from: NodeIndex<u32>, to: NodeIndex<u32>, kind: RelKind) -> Option<petgraph::graph::EdgeIndex<u32>> {
        self.ensure_node(from);
        self.ensure_node(to);
        if self.has_edge(from, to, kind) {
            return None;
        }
        Some(self.inner.add_edge(from, to, kind))
    }

    fn ensure_node(&mut self, idx: NodeIndex<u32>) {
        while !self.inner.contains_node(idx) {
            self.inner.add_node(SymbolId::PLACEHOLDER);
        }
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

/// §14.3 CSR freeze (TASK-008): the cache-friendly hot-read view of a
/// `RelationGraph`. Per §15.3 the adjacency of one node is a contiguous
/// slice, so p99 traversals get the ≤ 20 ns/edge scan §15.4 assumes.
/// The snapshot is rkyv-serializable for zero-copy L2 graph snapshots
/// (FR-30); freezing never mutates the source graph, and a new revision
/// builds a new snapshot while old revisions stay queryable (FR-23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct CsrGraph {
    /// `offsets.len() == node_len + 1` — the §14.3 invariant, checked on build.
    pub offsets: Vec<u32>,
    /// `(dense_target, kind)`, sorted per node by target for deterministic
    /// iteration (§15.1 global rule 7).
    pub edges: Vec<(u32, RelKind)>,
    /// Dense node weights: `nodes[i]` is the `SymbolId` of dense node `i`.
    /// (§14.3-exact plus the dense weight table §15.1 storage needs.)
    pub nodes: Vec<SymbolId>,
}

impl CsrGraph {
    /// Freeze a snapshot of `g`. Surviving nodes map to dense slots in
    /// `NodeIndex` order; per-node edge lists are sorted by target.
    pub fn build_from(g: &RelationGraph) -> Self {
        let mut dense_of: std::collections::BTreeMap<NodeIndex<u32>, u32> = Default::default();
        let mut nodes: Vec<SymbolId> = Vec::with_capacity(g.node_count());
        for (dense, idx) in g.node_indices().enumerate() {
            dense_of.insert(idx, dense as u32);
            nodes.push(g.node_weight(idx).unwrap_or(SymbolId::PLACEHOLDER));
        }

        let mut per_node: Vec<Vec<(u32, RelKind)>> = vec![Vec::new(); nodes.len()];
        let mut n_edges = 0usize;
        for (a, b, kind) in g.edge_triples() {
            let (da, db) = (dense_of[&a], dense_of[&b]);
            per_node[da as usize].push((db, kind));
            n_edges += 1;
        }
        let mut offsets = Vec::with_capacity(nodes.len() + 1);
        let mut edges = Vec::with_capacity(n_edges);
        offsets.push(0u32);
        for mut row in per_node {
            row.sort_unstable_by_key(|(t, _)| *t);
            edges.extend(row);
            offsets.push(edges.len() as u32);
        }
        debug_assert_eq!(offsets.len(), nodes.len() + 1, "§14.3 offsets invariant");
        CsrGraph { offsets, edges, nodes }
    }

    /// Contiguous adjacency of node `n` — cache-friendly, no per-edge
    /// allocation (§15.3/§15.5).
    pub fn neighbors(&self, n: u32) -> impl Iterator<Item = (u32, RelKind)> + '_ {
        let start = self.offsets[n as usize] as usize;
        let end = self.offsets[(n + 1) as usize] as usize;
        self.edges[start..end].iter().map(|(t, k)| (*t, *k))
    }

    pub fn node_len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn edge_len(&self) -> usize {
        self.edges.len()
    }

    /// rkyv bytes for the zero-copy snapshot path (FR-30).
    pub fn to_rkyv(&self) -> Result<Vec<u8>, rkyv::rancor::Error> {
        Ok(rkyv::to_bytes::<rkyv::rancor::Error>(self)?.into_vec())
    }

    /// Deserialize an rkyv snapshot (FR-30).
    pub fn from_rkyv(bytes: &[u8]) -> Result<Self, rkyv::rancor::Error> {
        rkyv::from_bytes::<CsrGraph, rkyv::rancor::Error>(bytes)
    }
}

/// Ferrante–Ottenstein–Warren program dependence graph (TASK-009, §7 FOW
/// row). Strictly derived: `Pdg::build` never mutates its input and stamps
/// provenance with the input revision (FR-4, global rule 2). Control
/// dependence is computed from the `ControlFlow` subgraph via post-dominator
/// sets (FOW 1987); data dependence comes from `DataFlow`/`Reads`/`Writes`
/// def-use relations. Node identity is `SymbolId` end to end (FR-1).
///
/// Statements with no incoming `ControlFlow` edge carry their (trivial)
/// control dependence to the virtual entry node; the entry is virtual, so
/// it never appears inside slices — it is tracked in `entry_trivial`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pdg {
    /// Input revision this graph was derived from (provenance, FR-4).
    pub rev: RevId,
    /// data dependence u -> v: v consumes a value produced by u.
    pub data_edges: std::collections::BTreeSet<(SymbolId, SymbolId)>,
    /// control dependence guard -> guarded (FOW post-dominator derivation).
    pub control_edges: std::collections::BTreeSet<(SymbolId, SymbolId)>,
    /// Nodes trivially controlled by the virtual entry (no CF predecessors).
    pub entry_trivial: std::collections::BTreeSet<SymbolId>,
}

impl Pdg {
    /// Derive the PDG from a relation graph (pure function of its input).
    pub fn build(g: &RelationGraph) -> Pdg {
        Pdg::build_with_rev(g, 0)
    }

    /// `build` with an input revision for provenance stamping.
    pub fn build_with_rev(g: &RelationGraph, rev: RevId) -> Pdg {
        // Split edges by family, keyed through NodeIndex -> SymbolId.
        let mut sym_of: std::collections::BTreeMap<NodeIndex<u32>, SymbolId> = Default::default();
        for idx in g.node_indices() {
            if let Some(s) = g.node_weight(idx) {
                sym_of.insert(idx, s);
            }
        }

        let mut cf_succ: std::collections::BTreeMap<SymbolId, Vec<SymbolId>> = Default::default();
        let mut data: std::collections::BTreeSet<(SymbolId, SymbolId)> = Default::default();
        let mut has_cf_pred: std::collections::BTreeSet<SymbolId> = Default::default();

        for (a, b, kind) in g.edge_triples() {
            let (sa, sb) = (sym_of.get(&a).copied(), sym_of.get(&b).copied());
            let (Some(sa), Some(sb)) = (sa, sb) else { continue };
            match kind {
                RelKind::ControlFlow => {
                    cf_succ.entry(sa).or_default().push(sb);
                    has_cf_pred.insert(sb);
                }
                RelKind::DataFlow | RelKind::Reads | RelKind::Writes => {
                    data.insert((sa, sb));
                }
                _ => {}
            }
        }

        // Trivial control dependence on the virtual entry (FOW): every node
        // with no ControlFlow predecessor. The entry is virtual, so it is
        // recorded but never appears inside slices.
        let entry_trivial: std::collections::BTreeSet<SymbolId> = sym_of
            .values()
            .filter(|n| !has_cf_pred.contains(n))
            .copied()
            .collect();

        // ---- Post-dominator sets (iterative fixpoint, FOW step 1) ----
        // Nodes with no CF successors are exits. postdom(v) = {v} ∪ ∩ postdom(s)
        // over successors s; exits postdominate only themselves.
        let nodes: Vec<SymbolId> = sym_of.values().copied().collect();
        let universe: std::collections::BTreeSet<SymbolId> = nodes.iter().copied().collect();
        let mut postdom: std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>> =
            nodes.iter().map(|n| (*n, universe.clone())).collect();
        for n in &nodes {
            if !cf_succ.contains_key(n) {
                let mut only = std::collections::BTreeSet::new();
                only.insert(*n);
                postdom.insert(*n, only);
            }
        }
        let mut changed = true;
        let mut rounds = 0usize;
        let max_rounds = nodes.len() + 2; // FR-7 bound: cannot loop forever
        while changed && rounds < max_rounds {
            changed = false;
            rounds += 1;
            for n in &nodes {
                if !cf_succ.contains_key(n) {
                    continue; // exit: fixed
                }
                let succs = &cf_succ[n];
                // Intersection of successors' postdom sets.
                let mut inter: Option<std::collections::BTreeSet<SymbolId>> = None;
                for s in succs {
                    match &inter {
                        None => inter = Some(postdom[s].clone()),
                        Some(cur) => {
                            let next: std::collections::BTreeSet<SymbolId> =
                                cur.intersection(&postdom[s]).copied().collect();
                            inter = Some(next);
                        }
                    }
                }
                let mut set = inter.unwrap_or_default();
                set.insert(*n);
                if set != postdom[n] {
                    postdom.insert(*n, set);
                    changed = true;
                }
            }
        }

        // ---- Control dependence (FOW Lemma) ----
        // B is control-dependent on A iff some CF edge A -> C exists where
        // B postdominates C (B ∈ postdom(C)) and B does not strictly
        // postdominate A (B ∉ postdom(A) \ {A}).
        let mut control: std::collections::BTreeSet<(SymbolId, SymbolId)> = Default::default();
        for (a, succs) in &cf_succ {
            for c in succs {
                for b in &nodes {
                    let b_postdom_c = postdom[c].contains(b);
                    let b_strict_postdom_a = *b != *a && postdom[a].contains(b);
                    if b_postdom_c && !b_strict_postdom_a {
                        control.insert((*a, *b));
                    }
                }
            }
        }

        Pdg {
            rev,
            data_edges: data,
            control_edges: control,
            entry_trivial,
        }
    }

    /// Deterministic backward slice (sorted set): the seed plus every
    /// transitive data/control ancestor.
    pub fn backward_slice(&self, seed: SymbolId) -> std::collections::BTreeSet<SymbolId> {
        self.backward_slice_bounded(seed, usize::MAX)
    }

    /// Bounded backward slice (FR-7): stops after `budget` nodes including
    /// the seed, so slicing can never run away.
    pub fn backward_slice_bounded(
        &self,
        seed: SymbolId,
        budget: usize,
    ) -> std::collections::BTreeSet<SymbolId> {
        let mut slice = std::collections::BTreeSet::new();
        if budget == 0 {
            return slice;
        }
        let mut queue = std::collections::VecDeque::new();
        slice.insert(seed);
        queue.push_back(seed);
        while let Some(n) = queue.pop_front() {
            if slice.len() >= budget {
                break;
            }
            let preds = self
                .data_edges
                .iter()
                .filter(|(_, v)| *v == n)
                .map(|(u, _)| *u)
                .chain(self.control_edges.iter().filter(|(_, v)| *v == n).map(|(u, _)| *u));
            for p in preds {
                if slice.insert(p) {
                    queue.push_back(p);
                    if slice.len() >= budget {
                        break;
                    }
                }
            }
        }
        slice
    }
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
