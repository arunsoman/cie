//! Hierarchy construction and ConceptNode DAG (PRD §26–27 / FR-40–44).
//!
//! Merge sources (FR-42): package containment (files/classes), FCA
//! subsumption (concept lattice), and call ownership (rule nodes). Every
//! edge carries a reason. Behaviors/rules roll up to ancestors (FR-43).
//! Output matches the user-facing artifact shape of §27 (acceptance #27).

use crate::behavior::BehaviorResult;
use crate::fca;
use cie_concepts::{BehaviorRef, ConceptDag, ConceptKind, ConceptNode, EdgeReason, RuleRef};
use cie_contract::Contract;
use cie_core::types::{ConceptNodeId, Ns, SymbolId};
use cie_semantic::facts::{SymbolKind, SymbolStore};
use std::collections::{BTreeMap, BTreeSet};

/// Label function hook: the CLI supplies either the offline deterministic
/// labeler or an LLM-backed one. Labels never decide membership.
/// Label provider: returns (label, source) where source is the
/// LabelRecord source ("llm" | "offline" | "cache") — TASK-079 binds
/// LLM-proposed labels as inert `LlmCandidate` rules.
pub type LabelFn<'a> = &'a dyn Fn(&[String], &str) -> (String, &'static str);

pub fn build_hierarchy(
    store: &SymbolStore,
    behavior: &BehaviorResult,
    contracts: &[Contract],
    label_fn: LabelFn,
    now: Ns,
) -> ConceptDag {
    let mut dag = ConceptDag::default();

    // ---- Domain root ----
    let root = dag.push(ConceptNode::new(0, "Project", ConceptKind::Domain, now));

    // ---- Module nodes (package containment) ----
    let mut module_nodes: BTreeMap<String, ConceptNodeId> = BTreeMap::new();
    for path in store.files() {
        let mut n = ConceptNode::new(0, path.clone(), ConceptKind::Module, now);
        n.primary_edge = EdgeReason::PackageContainment;
        n.provenance = "rule:package".into();
        let id = dag.push(n);
        dag.link_child(root, id);
        module_nodes.insert(path, id);
    }

    // ---- Class / service nodes + symbol membership ----
    let mut node_of_symbol: BTreeMap<SymbolId, ConceptNodeId> = BTreeMap::new();
    let mut behaviors_added: BTreeSet<SymbolId> = BTreeSet::new();

    for sym in &store.symbols {
        match sym.kind {
            SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Interface => {
                let methods = store.children_of(sym.id);
                let moneyish = methods.iter().any(|m| {
                    store
                        .get(*m)
                        .map(|s| {
                            let n = s.name.to_ascii_lowercase();
                            n.contains("charge")
                                || n.contains("pay")
                                || n.contains("refund")
                                || n.contains("transfer")
                        })
                        .unwrap_or(false)
                });
                let kind = if moneyish {
                    ConceptKind::Aggregate
                } else if methods.is_empty() {
                    ConceptKind::Value
                } else {
                    ConceptKind::Entity
                };
                let mut n = ConceptNode::new(0, sym.name.clone(), kind, now);
                n.primary_edge = EdgeReason::PackageContainment;
                n.provenance = "rule:structure".into();
                n.symbols = vec![sym.id];
                let id = dag.push(n);
                if let Some(pn) = module_nodes.get(store.files.path(sym.loc.file)) {
                    dag.link_child(*pn, id);
                } else {
                    dag.link_child(root, id);
                }
                node_of_symbol.insert(sym.id, id);
            }
            _ => {}
        }
    }

    // Attach function behaviors: to class node when method, else module node.
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let Some(summary) = behavior.summaries.get(&sym.id) else {
            continue;
        };
        let lower_name = sym.name.to_ascii_lowercase();
        let label = if summary.tags.contains("auth")
            && !lower_name.starts_with("auth")
            && !lower_name.starts_with("verify")
        {
            format!("Authorize {}", humanize(&sym.name))
        } else if summary.tags.contains("auth") {
            humanize(&sym.name)
        } else if summary.tags.contains("money") {
            format!("Move money via {}", humanize(&sym.name))
        } else if summary.tags.contains("query") {
            format!("Query data via {}", humanize(&sym.name))
        } else if summary.tags.contains("mutation") {
            format!("Mutate data via {}", humanize(&sym.name))
        } else {
            humanize(&sym.name)
        };
        let bref = BehaviorRef {
            summary: sym.id,
            kind: label,
            confidence: 1.0, // mechanical derivation from summaries
        };
        let target: ConceptNodeId = match sym.parent {
            Some(p) if node_of_symbol.contains_key(&p) => node_of_symbol[&p],
            _ => module_nodes.get(store.files.path(sym.loc.file)).copied().unwrap_or(root),
        };
        if let Some(n) = dag.get_mut(target) {
            n.behaviors.push(bref);
        }
        behaviors_added.insert(sym.id);
        // symbol membership rolls up later
        if let Some(n) = dag.get_mut(target) {
            if !n.symbols.contains(&sym.id) {
                n.symbols.push(sym.id);
            }
        }
    }

    // ---- FCA concepts -> UseCase/Subdomain nodes (FcaSubsumption edges) ----
    let objects: Vec<(SymbolId, BTreeSet<String>)> = behavior
        .summaries
        .iter()
        .filter(|(id, _)| {
            store
                .get(**id)
                .map(|s| s.is_function_like() && !s.is_test())
                .unwrap_or(false)
        })
        .map(|(id, s)| {
            let mut attrs: BTreeSet<String> = s.tags.clone();
            for e in &s.effects {
                attrs.insert(e.as_str().to_string());
            }
            (*id, attrs)
        })
        .collect();

    let lattice = fca::build_lattice(&objects);
    for concept in &lattice {
        // Skip top (empty intent) and tiny single-symbol concepts already
        // attached via structure — overlap is fine, noise is not.
        if concept.intent.is_empty() || concept.extent.len() < 2 {
            continue;
        }
        let (label, label_source) = label_fn(
            &concept.intent.iter().cloned().collect::<Vec<_>>(),
            &fca::concept_label(&concept.intent),
        );
        let mut n = ConceptNode::new(0, label, ConceptKind::UseCase, now);
        n.intent = concept.intent.iter().cloned().collect();
        n.primary_edge = EdgeReason::FcaSubsumption;
        n.provenance = "fca:godin-v1".into();
        n.symbols = concept.extent.iter().copied().collect();
        n.confidence = 0.8; // derived concept, mechanical but heuristic scope
        let id = dag.push(n);
        dag.link_child(root, id);

        // TASK-079: an LLM-proposed label binds as an inert LlmCandidate
        // rule — visible, traceable, never decisive (acceptance #28).
        if label_source == "llm" {
            if let Some(node) = dag.get_mut(id) {
                node.rules.push(RuleRef {
                    contract: 9_000_000u32 + id,
                    kind: format!("llm-label:{}", node.label),
                    authority: cie_contract::Authority::LlmCandidate,
                });
                node.provenance = "label:llm".into();
            }
        }

        // Extra DAG edge: majority module (ClusterCohesion) when most of the
        // extent lives in one file (acceptance #30 — multi-parent, traceable).
        let mut per_module: BTreeMap<String, usize> = BTreeMap::new();
        for s in &concept.extent {
            if let Some(sym) = store.get(*s) {
                *per_module.entry(store.files.path(sym.loc.file).to_string()).or_default() += 1;
            }
        }
        if let Some((best_path, _)) = per_module.iter().max_by_key(|(_, c)| **c) {
            if concept.extent.len() >= 2 {
                if let Some(&mn) = module_nodes.get(best_path) {
                    if let Some(node) = dag.get_mut(id) {
                        node.extra_edges.push((mn, EdgeReason::ClusterCohesion));
                    }
                    if let Some(mnode) = dag.get_mut(mn) {
                        if !mnode.children.contains(&id) {
                            mnode.children.push(id);
                        }
                    }
                }
            }
        }
    }

    // ---- Rule nodes from explicit contracts (CallOwnership edges) ----
    let mut rules_added: BTreeSet<u32> = BTreeSet::new();
    for c in contracts {
        let kind = if matches!(c.authority, cie_contract::Authority::Explicit) {
            ConceptKind::Rule
        } else {
            ConceptKind::Invariant
        };
        let mut n = ConceptNode::new(0, c.expr.clone(), kind, now);
        n.primary_edge = EdgeReason::CallOwnership;
        n.provenance = "rule:contract".into();
        n.symbols = c.symbols.clone();
        n.rules.push(RuleRef {
            contract: c.id,
            kind: c.obligation.as_str(),
            authority: c.authority,
        });
        n.confidence = match c.authority {
            cie_contract::Authority::Explicit => 1.0,
            _ => 0.6,
        };
        let id = dag.push(n);
        rules_added.insert(c.id);
        // Parent: node containing the first bound symbol.
        let parent = c
            .symbols
            .first()
            .and_then(|s| node_of_symbol.get(s).copied())
            .or_else(|| {
                c.symbols
                    .first()
                    .and_then(|s| store.get(*s))
                    .and_then(|sym| module_nodes.get(store.files.path(sym.loc.file)).copied())
            })
            .unwrap_or(root);
        dag.link_child(parent, id);
    }

    // ---- Rollup (FR-43) ----
    dag.rollup();

    // ---- Unattached lists (acceptance #28) ----
    let mut attached_summaries: BTreeSet<SymbolId> = BTreeSet::new();
    let mut attached_contracts: BTreeSet<u32> = BTreeSet::new();
    for n in &dag.nodes {
        for b in &n.behaviors {
            attached_summaries.insert(b.summary);
        }
        for r in &n.rules {
            attached_contracts.insert(r.contract);
        }
    }
    for (id, summary) in &behavior.summaries {
        if !attached_summaries.contains(id) {
            dag.unattached_behaviors.push(BehaviorRef {
                summary: *id,
                kind: store
                    .get(*id)
                    .map(|s| s.qual_name.clone())
                    .unwrap_or_default(),
                confidence: 1.0,
            });
        }
        let _ = summary;
    }
    for c in contracts {
        if !attached_contracts.contains(&c.id) {
            dag.unattached_rules.push(RuleRef {
                contract: c.id,
                kind: c.obligation.as_str(),
                authority: c.authority,
            });
        }
    }

    let _ = behaviors_added; // retained for future incremental invalidation
    dag
}

fn humanize(name: &str) -> String {
    let mut out = String::new();
    for (i, part) in name.split('_').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        name.to_string()
    } else {
        out
    }
}
