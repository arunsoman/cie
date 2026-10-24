//! Concept persistence (TASK-078): the six §15.1 `concept_node:{id}` keys
//! over the generic store (TASK-001 layering — no crate cycle).
//!
//! Per node: `:node` (identity/coverage core), `:behaviors`, `:rules`,
//! `:parent`, `:children`, `:rollup` (the rollup aggregate view). The
//! DAG-level unattached lists ride alongside as `unattached.json`.
//! Layout: revs/{rev}/concepts/{id}/{node,behaviors,rules,parent,
//! children,rollup}.json — JSON behind the same Store API; the L1 KV
//! backend maps each file to its namespace key.

use crate::ConceptDag;
use cie_core::types::{Ns, RevId, SymbolId};
use cie_store::Store;
use serde::{Deserialize, Serialize};
use std::fs;

/// TASK-078 `:node` key payload — the node identity/coverage core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptNodeCore {
    pub id: u32,
    pub label: String,
    pub kind: crate::ConceptKind,
    pub primary_edge: crate::EdgeReason,
    #[serde(default)]
    pub entry_symbol: Option<SymbolId>,
    pub symbols: Vec<SymbolId>,
    pub intent: Vec<String>,
    pub coverage_ratio: f32,
    pub coverage_complete: bool,
    pub confidence: f32,
    pub provenance: String,
    pub finding_ids: Vec<String>,
    pub created_ns: Ns,
    pub compute_ns: i64,
}

/// TASK-078 `:rollup` key payload — the rollup aggregate view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollupView {
    pub behaviors: Vec<SymbolId>,
    pub rules: Vec<cie_core::types::ContractId>,
    pub symbols: Vec<SymbolId>,
    pub coverage_ratio: f32,
    pub coverage_complete: bool,
    pub confidence: f32,
}

/// The six-key bundle as written (kept for documentation/tests).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptNodeSix {
    pub node: ConceptNodeCore,
    pub behaviors: Vec<crate::BehaviorRef>,
    pub rules: Vec<crate::RuleRef>,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub rollup: RollupView,
}

/// Extension trait: six-key concept persistence over the generic store.
pub trait ConceptStoreExt {
    /// Persist a concept DAG split across the six per-node keys.
    fn save_concept_six(&self, rev: RevId, dag: &ConceptDag) -> std::io::Result<()>;
    /// Rebuild a concept DAG from the six-key layout (roundtrip partner of
    /// [`ConceptStoreExt::save_concept_six`]).
    fn load_concept_six(&self, rev: RevId) -> std::io::Result<Option<ConceptDag>>;
}

impl ConceptStoreExt for Store {
    fn save_concept_six(&self, rev: RevId, dag: &ConceptDag) -> std::io::Result<()> {
        let base = self
            .root()
            .join("revs")
            .join(rev.to_string())
            .join("concepts");
        for node in &dag.nodes {
            let dir = base.join(node.id.to_string());
            let six = ConceptNodeSix {
                node: ConceptNodeCore {
                    id: node.id,
                    label: node.label.clone(),
                    kind: node.kind,
                    primary_edge: node.primary_edge,
                    entry_symbol: node.entry_symbol,
                    symbols: node.symbols.clone(),
                    intent: node.intent.clone(),
                    coverage_ratio: node.coverage_ratio,
                    coverage_complete: node.coverage_complete,
                    confidence: node.confidence,
                    provenance: node.provenance.clone(),
                    finding_ids: node.finding_ids.clone(),
                    created_ns: node.created_ns,
                    compute_ns: node.compute_ns,
                },
                behaviors: node.behaviors.clone(),
                rules: node.rules.clone(),
                parent: node.parent,
                children: node.children.clone(),
                rollup: RollupView {
                    behaviors: node.behaviors.iter().map(|b| b.summary).collect(),
                    rules: node.rules.iter().map(|r| r.contract).collect(),
                    symbols: node.symbols.clone(),
                    coverage_ratio: node.coverage_ratio,
                    coverage_complete: node.coverage_complete,
                    confidence: node.confidence,
                },
            };
            // `:children` carries (child, edge reason) pairs so the
            // multi-parent DAG roundtrips with its traceable reasons (FR-42).
            let children_with_reasons: Vec<(u32, String)> = node
                .children
                .iter()
                .map(|&c| {
                    let reason = if node.parent == Some(c) {
                        // self-reference cannot happen (TASK-080 rejects);
                        // fall back to the node's own primary edge.
                        node.primary_edge
                    } else if let Some(child) = dag.get(c) {
                        if child.parent == Some(node.id) {
                            child.primary_edge
                        } else {
                            child
                                .extra_edges
                                .iter()
                                .find(|(p, _)| *p == node.id)
                                .map(|(_, r)| *r)
                                .unwrap_or(node.primary_edge)
                        }
                    } else {
                        node.primary_edge
                    };
                    (c, reason.as_str().to_string())
                })
                .collect();
            self.write_json(&dir.join("node.json"), &six.node)?;
            self.write_json(&dir.join("behaviors.json"), &six.behaviors)?;
            self.write_json(&dir.join("rules.json"), &six.rules)?;
            self.write_json(&dir.join("parent.json"), &six.parent)?;
            self.write_json(&dir.join("children.json"), &children_with_reasons)?;
            self.write_json(&dir.join("rollup.json"), &six.rollup)?;
        }
        self.write_json(
            &base.join("unattached.json"),
            &(
                dag.unattached_behaviors.clone(),
                dag.unattached_rules.clone(),
            ),
        )?;
        Ok(())
    }

    fn load_concept_six(&self, rev: RevId) -> std::io::Result<Option<ConceptDag>> {
        let base = self
            .root()
            .join("revs")
            .join(rev.to_string())
            .join("concepts");
        if !base.exists() {
            return Ok(None);
        }
        let mut dag = ConceptDag::default();
        let mut ids: Vec<u32> = vec![];
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(id) = name.parse::<u32>() {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        for id in ids {
            let dir = base.join(id.to_string());
            let node: ConceptNodeCore = self
                .read_json(&dir.join("node.json"))?
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "node.json"))?;
            let behaviors: Vec<crate::BehaviorRef> = self
                .read_json(&dir.join("behaviors.json"))?
                .unwrap_or_default();
            let rules: Vec<crate::RuleRef> =
                self.read_json(&dir.join("rules.json"))?.unwrap_or_default();
            let parent: Option<u32> = self.read_json(&dir.join("parent.json"))?.unwrap_or(None);
            dag.nodes.push(crate::ConceptNode {
                id: node.id,
                label: node.label,
                kind: node.kind,
                parent,
                children: vec![],
                extra_edges: vec![],
                primary_edge: node.primary_edge,
                entry_symbol: node.entry_symbol,
                symbols: node.symbols,
                behaviors,
                rules,
                finding_ids: node.finding_ids,
                intent: node.intent,
                coverage_ratio: node.coverage_ratio,
                coverage_complete: node.coverage_complete,
                confidence: node.confidence,
                provenance: node.provenance,
                created_ns: node.created_ns,
                compute_ns: node.compute_ns,
            });
        }
        // Reassemble the DAG: `:children` pairs carry the edge reason; an
        // entry (C, reason) on parent P is C's primary link when C.parent
        // == P, otherwise an extra edge with its reason (FR-42).
        for pid in 0..dag.nodes.len() as u32 {
            let pairs: Vec<(u32, String)> = self
                .read_json(&base.join(pid.to_string()).join("children.json"))?
                .unwrap_or_default();
            for (cid, reason_str) in pairs {
                let reason = crate::EdgeReason::parse(&reason_str);
                let is_primary = dag.get(cid).map(|c| c.parent == Some(pid)).unwrap_or(false);
                if let Some(p) = dag.get_mut(pid) {
                    if !p.children.contains(&cid) {
                        p.children.push(cid);
                    }
                }
                if !is_primary {
                    if let Some(c) = dag.get_mut(cid) {
                        if c.parent != Some(pid) && !c.extra_edges.iter().any(|(p, _)| *p == pid) {
                            c.extra_edges.push((pid, reason));
                        }
                    }
                }
            }
        }
        if let Some((behaviors, rules)) = self
            .read_json::<(Vec<crate::BehaviorRef>, Vec<crate::RuleRef>)>(
                &base.join("unattached.json"),
            )?
        {
            dag.unattached_behaviors = behaviors;
            dag.unattached_rules = rules;
        }
        Ok(Some(dag))
    }
}
