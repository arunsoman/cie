//! ConceptNode DAG — hierarchical concept behavior (PRD §27 / FR-40–44).
//!
//! One unified node binds concept + hierarchy + behaviors + rules + symbols
//! + evidence. The graph is a DAG: a node has one primary parent plus
//! optional extra edges, each with a traceable reason.

use crate::contract::Authority;
use crate::types::{ConceptNodeId, Ns, NsDelta, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ConceptKind {
    Domain,
    Subdomain,
    Module,
    Service,
    Entity,
    Value,
    Aggregate,
    UseCase,
    Rule,
    Invariant,
}

impl ConceptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConceptKind::Domain => "Domain",
            ConceptKind::Subdomain => "Subdomain",
            ConceptKind::Module => "Module",
            ConceptKind::Service => "Service",
            ConceptKind::Entity => "Entity",
            ConceptKind::Value => "Value",
            ConceptKind::Aggregate => "Aggregate",
            ConceptKind::UseCase => "UseCase",
            ConceptKind::Rule => "Rule",
            ConceptKind::Invariant => "Invariant",
        }
    }
}

/// Why does this parent–child edge exist? Every edge is traceable (FR-42).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeReason {
    PackageContainment,
    ClusterCohesion,
    FcaSubsumption,
    CallOwnership,
}

impl EdgeReason {
    /// Parse the `as_str()` rendering (six-key storage roundtrip, TASK-078).
    pub fn parse(s: &str) -> EdgeReason {
        match s {
            "package_containment" => EdgeReason::PackageContainment,
            "cluster_cohesion" => EdgeReason::ClusterCohesion,
            "fca_subsumption" => EdgeReason::FcaSubsumption,
            "call_ownership" => EdgeReason::CallOwnership,
            _ => EdgeReason::ClusterCohesion,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeReason::PackageContainment => "package_containment",
            EdgeReason::ClusterCohesion => "cluster_cohesion",
            EdgeReason::FcaSubsumption => "fca_subsumption",
            EdgeReason::CallOwnership => "call_ownership",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorRef {
    pub summary: SymbolId,
    /// e.g. "Add item to cart".
    pub kind: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRef {
    pub contract: crate::types::ContractId,
    pub kind: String,
    pub authority: Authority,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptNode {
    pub id: ConceptNodeId,
    pub label: String,
    pub kind: ConceptKind,
    pub parent: Option<ConceptNodeId>,
    pub children: Vec<ConceptNodeId>,
    /// Additional DAG parents beyond the primary one, with edge reasons.
    pub extra_edges: Vec<(ConceptNodeId, EdgeReason)>,
    pub primary_edge: EdgeReason,
    pub symbols: Vec<SymbolId>,
    pub behaviors: Vec<BehaviorRef>,
    pub rules: Vec<RuleRef>,
    pub finding_ids: Vec<String>,
    /// FCA intent (attribute names) when this node came from the lattice.
    pub intent: Vec<String>,
    pub coverage_ratio: f32,
    pub coverage_complete: bool,
    pub confidence: f32,
    pub provenance: String,
    pub created_ns: Ns,
    pub compute_ns: NsDelta,
}

impl ConceptNode {
    pub fn new(id: ConceptNodeId, label: impl Into<String>, kind: ConceptKind, now: Ns) -> Self {
        ConceptNode {
            id,
            label: label.into(),
            kind,
            parent: None,
            children: vec![],
            extra_edges: vec![],
            primary_edge: EdgeReason::PackageContainment,
            symbols: vec![],
            behaviors: vec![],
            rules: vec![],
            finding_ids: vec![],
            intent: vec![],
            coverage_ratio: 1.0,
            coverage_complete: true,
            confidence: 1.0,
            provenance: "rule".into(),
            created_ns: now,
            compute_ns: 0,
        }
    }
}

/// The full DAG plus the "unattached" lists required by acceptance #28:
/// every behavior and rule attaches to >= 1 node or is listed here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConceptDag {
    pub nodes: Vec<ConceptNode>,
    pub unattached_behaviors: Vec<BehaviorRef>,
    pub unattached_rules: Vec<RuleRef>,
}

impl ConceptDag {
    pub fn push(&mut self, mut node: ConceptNode) -> ConceptNodeId {
        node.id = self.nodes.len() as ConceptNodeId;
        self.nodes.push(node.clone());
        node.id
    }

    pub fn get(&self, id: ConceptNodeId) -> Option<&ConceptNode> {
        self.nodes.get(id as usize)
    }

    pub fn get_mut(&mut self, id: ConceptNodeId) -> Option<&mut ConceptNode> {
        self.nodes.get_mut(id as usize)
    }

    /// TASK-080: cycle-checking edge insertion. Links `child` under
    /// `parent` with `reason`; rejects with `CieError::Cycle` when the edge
    /// would close a cycle through the primary-parent ancestor chain
    /// (merge must yield a DAG, never a graph with cycles — acceptance #29).
    pub fn try_link_child(
        &mut self,
        parent: ConceptNodeId,
        child: ConceptNodeId,
        reason: EdgeReason,
    ) -> Result<(), crate::protocol::CieError> {
        if parent == child {
            return Err(crate::protocol::CieError::Cycle(format!(
                "self-edge on node {parent}"
            )));
        }
        // Walk the ancestor chain of `parent` (primary parents); if `child`
        // appears, parent→child would close a cycle.
        let mut cursor = Some(parent);
        let mut hops = 0usize;
        while let Some(p) = cursor {
            if p == child {
                return Err(crate::protocol::CieError::Cycle(format!(
                    "edge {parent}->{child} closes a cycle: {child} is an ancestor of {parent}"
                )));
            }
            hops += 1;
            if hops > self.nodes.len() {
                // Defensive: corrupted parent chain cannot loop forever (FR-7).
                return Err(crate::protocol::CieError::Cycle(format!(
                    "parent chain of {parent} is longer than the node count"
                )));
            }
            cursor = self.get(p).and_then(|n| n.parent);
        }

        if let Some(p) = self.get_mut(parent) {
            if !p.children.contains(&child) {
                p.children.push(child);
            }
        }
        if let Some(c) = self.get_mut(child) {
            if c.parent.is_none() {
                // primary_edge is set by the caller before push (it carries
                // the semantically richer reason: FcaSubsumption,
                // PackageContainment, ...); `reason` here annotates extra
                // edges only.
                c.parent = Some(parent);
            } else if c.parent != Some(parent)
                && !c.extra_edges.iter().any(|(p, _)| *p == parent)
            {
                c.extra_edges.push((parent, reason));
            }
        }
        Ok(())
    }

    /// Legacy void form: links, silently ignoring cycles (pre-TASK-080
    /// behavior preserved for existing callers). New code should use
    /// [`ConceptDag::try_link_child`].
    pub fn link_child(&mut self, parent: ConceptNodeId, child: ConceptNodeId) {
        let _ = self.try_link_child(parent, child, EdgeReason::ClusterCohesion);
    }

    /// FR-43 behavior rollup over the primary-parent spanning tree:
    /// union of descendants' behaviors/rules/symbols, intersection of
    /// coverage completeness, weakest-link confidence and coverage ratio.
    /// Must equal a clean recomputation (acceptance #29).
    pub fn rollup(&mut self) {
        let n = self.nodes.len();
        let mut done = vec![false; n];
        let mut memo: Vec<(Vec<BehaviorRef>, Vec<RuleRef>, BTreeSet<SymbolId>, f32, bool, f32)> =
            Vec::with_capacity(n);
        memo.resize_with(n, || (vec![], vec![], BTreeSet::new(), 1.0, true, 1.0));

        for id in 0..n as ConceptNodeId {
            self.rollup_into(id, &mut done, &mut memo);
        }
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let m = &memo[i];
            node.behaviors = m.0.clone();
            node.rules = m.1.clone();
            node.symbols = m.2.iter().copied().collect();
            node.coverage_ratio = m.3;
            node.coverage_complete = m.4;
            node.confidence = m.5;
        }
    }

    #[allow(clippy::type_complexity)]
    fn rollup_into(
        &self,
        id: ConceptNodeId,
        done: &mut Vec<bool>,
        memo: &mut Vec<(
            Vec<BehaviorRef>,
            Vec<RuleRef>,
            BTreeSet<SymbolId>,
            f32,
            bool,
            f32,
        )>,
    ) {
        let idx = id as usize;
        if done[idx] {
            return;
        }
        let node = &self.nodes[idx];
        let mut behaviors = node.behaviors.clone();
        let mut rules = node.rules.clone();
        let mut symbols: BTreeSet<SymbolId> = node.symbols.iter().copied().collect();
        let mut ratio = node.coverage_ratio;
        let mut complete = node.coverage_complete;
        let mut confidence = node.confidence;

        for &child in &node.children {
            self.rollup_into(child, done, memo);
            let m = &memo[child as usize];
            for b in &m.0 {
                if !behaviors.iter().any(|x| x.summary == b.summary) {
                    behaviors.push(b.clone());
                }
            }
            for r in &m.1 {
                if !rules.iter().any(|x| x.contract == r.contract) {
                    rules.push(r.clone());
                }
            }
            symbols.extend(&m.2);
            ratio = ratio.min(m.3);
            complete &= m.4;
            confidence = confidence.min(m.5);
        }
        memo[idx] = (behaviors, rules, symbols, ratio, complete, confidence);
        done[idx] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_union_and_weakest_link() {
        let now = 0;
        let mut dag = ConceptDag::default();
        let root = dag.push(ConceptNode::new(0, "Cart", ConceptKind::Aggregate, now));
        let c1 = dag.push(ConceptNode::new(0, "CartItem", ConceptKind::Entity, now));
        let c2 = dag.push(ConceptNode::new(0, "Quantity", ConceptKind::Value, now));
        dag.link_child(root, c1);
        dag.link_child(root, c2);
        dag.get_mut(c1).unwrap().behaviors = vec![BehaviorRef {
            summary: SymbolId(1),
            kind: "Add item to cart".into(),
            confidence: 0.9,
        }];
        dag.get_mut(c1).unwrap().coverage_ratio = 0.8;
        dag.get_mut(c2).unwrap().confidence = 0.5;
        dag.get_mut(c2).unwrap().symbols = vec![SymbolId(7)];

        dag.rollup();

        let r = dag.get(root).unwrap();
        assert_eq!(r.behaviors.len(), 1);
        assert_eq!(r.behaviors[0].summary, SymbolId(1));
        assert!(r.symbols.contains(&SymbolId(7)));
        assert_eq!(r.confidence, 0.5); // weakest link
        assert_eq!(r.coverage_ratio, 0.8); // min coverage
    }
}
