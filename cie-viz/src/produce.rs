//! ViewSpec producer (CIE-VIZ PRD §2, Rust side): `RevSnapshot` →
//! `ViewSpec`.
//!
//! The producer is where semantic honesty is DECIDED (the adapter only
//! re-checks it at the gate):
//!
//! * every element carries a byte-span `EvidenceRef` resolved through the
//!   snapshot's file table — a concept with no resolvable evidence is NOT
//!   drawn; it is recorded in `hidden[]` (incompleteness lives inside the
//!   spec, never outside it);
//! * display modes are derived from provenance/coverage/confidence, never
//!   invented: rule-extracted + full coverage + full confidence ⇒ Fact;
//!   LLM/FCA-derived ⇒ Inference; low confidence ⇒ Hypothesis (with its
//!   counter-argument); incomplete coverage ⇒ Fog (with the reason);
//! * the element cap (VIZ-I-06) is salience-bounded HERE, where the
//!   `hidden[]` ledger and the `gaps[]` entry live — the adapter and gate
//!   refuse over-cap specs, they never silently truncate;
//! * the footer is recomputed from the FINAL element set (VIZ-A-07) and
//!   gap ids stay reachable through the caption (VIZ-H-05);
//! * layout auto-selection follows VIZ-D-03 (acyclic → dagre, tree →
//!   compact-box, dense/cyclic → force) — geometry itself is always G6's
//!   job (VIZ-D-08: no layout algorithm lives here).

use crate::gate::{self, ELEMENT_CAP};
use crate::hash;
use crate::model::{
    DisplayMode, EpistemicFooter, EvidenceRef, Gap, HiddenCandidate, LayoutHint, Salience,
    ScopeAttestation, ScopeCoverage, ViewEdge, ViewElement, ViewSpec,
};
use cie_concepts::{ConceptDag, EdgeReason};
use cie_core::types::{Ns, SymbolId};
use cie_incremental::RevSnapshot;
use cie_semantic::SymbolFact;
use std::collections::{BTreeMap, BTreeSet};

/// Producer options. `max_elements` defaults to the gate cap (2 000).
#[derive(Debug, Clone)]
pub struct ProduceOptions {
    pub max_elements: usize,
    /// Force a specific layout strategy (verbatim G6 name); `None` uses
    /// the VIZ-D-03 auto-selection.
    pub layout: Option<String>,
}

impl Default for ProduceOptions {
    fn default() -> Self {
        ProduceOptions { max_elements: ELEMENT_CAP, layout: None }
    }
}

/// Resolve `SymbolId` → symbol fact once, up front.
struct SymbolIndex<'a> {
    by_id: BTreeMap<u32, &'a SymbolFact>,
    files: &'a [String],
}

impl<'a> SymbolIndex<'a> {
    fn new(snap: &'a RevSnapshot) -> Self {
        let mut by_id = BTreeMap::new();
        for s in &snap.symbols {
            by_id.insert(s.id.raw(), s);
        }
        SymbolIndex { by_id, files: &snap.files }
    }

    /// Byte-span evidence for a concept's first resolvable symbol.
    fn evidence_for(&self, syms: &[SymbolId]) -> Option<EvidenceRef> {
        for id in syms {
            // Try every symbol until one grounds the element — a symbol we
            // cannot resolve (or whose file entry is missing) must not hide
            // the whole node (SQL tables/views, for example, may interleave
            // with function-like symbols the snapshot stores differently).
            let Some(s) = self.by_id.get(&id.raw()) else { continue };
            let Some(path) = self.files.get(s.loc.file.raw() as usize) else { continue };
            return Some(EvidenceRef {
                file: path.clone(),
                start: s.loc.start as u64,
                end: s.loc.end as u64,
            });
        }
        None
    }

    /// Evidence for the call-site edge: the CALLER's span grounds a call
    /// edge (the call lives in the caller's source — VIZ-H-02).
    fn evidence_of_symbol(&self, id: SymbolId) -> Option<EvidenceRef> {
        let s = self.by_id.get(&id.raw())?;
        let path = self.files.get(s.loc.file.raw() as usize)?;
        Some(EvidenceRef {
            file: path.clone(),
            start: s.loc.start as u64,
            end: s.loc.end as u64,
        })
    }
}

/// Display mode decision tree — the honest mapping (see module docs).
fn display_mode_for(node: &cie_concepts::ConceptNode) -> (DisplayMode, Option<String>, Option<String>, Option<String>) {
    let claim = format!("C-{:04}", node.id);
    if !node.coverage_complete {
        return (
            DisplayMode::Fog,
            Some(claim),
            None,
            Some(format!(
                "coverage incomplete ({:.0}%) — not determinable from the analyzed span",
                node.coverage_ratio * 100.0
            )),
        );
    }
    if node.provenance == "rule" && node.confidence >= 1.0 {
        return (DisplayMode::Fact, None, None, None);
    }
    if node.confidence < 0.5 {
        return (
            DisplayMode::Hypothesis,
            Some(claim),
            Some(format!(
                "low-confidence extraction (confidence={:.2}); no verifying contract attached",
                node.confidence
            )),
            None,
        );
    }
    (
        DisplayMode::Inference,
        Some(claim),
        None,
        None,
    )
}

/// VIZ-D-03 auto-selection over the FINAL edge set. Deterministic.
/// `pub(crate)`: the RFC-0024 flow producer reuses the same rule.
pub(crate) fn auto_layout(n_elements: usize, edges: &[ViewEdge]) -> String {
    if n_elements == 0 || edges.is_empty() {
        return "dagre".into();
    }
    // Cycle detection (iterative DFS with colours) over the full edge set.
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in edges {
        adj.entry(e.source.as_str()).or_default().push(e.target.as_str());
    }
    let nodes: BTreeSet<&str> = edges
        .iter()
        .flat_map(|e| [e.source.as_str(), e.target.as_str()])
        .collect();
    let mut colour: BTreeMap<&str, u8> = BTreeMap::new(); // 0 white 1 grey 2 black
    let mut has_cycle = false;
    for &root in &nodes {
        if colour.get(root).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
        while let Some((n, i)) = stack.pop() {
            match i {
                0 => {
                    colour.insert(n, 1);
                    let kids = adj.get(n).cloned().unwrap_or_default();
                    stack.push((n, 1));
                    for k in kids {
                        match colour.get(k).copied().unwrap_or(0) {
                            1 => has_cycle = true,
                            0 => stack.push((k, 0)),
                            _ => {}
                        }
                    }
                }
                _ => {
                    colour.insert(n, 2);
                }
            }
            if has_cycle {
                break;
            }
        }
        if has_cycle {
            break;
        }
    }
    if has_cycle {
        return "force".into();
    }
    // Dense graphs spread better under force (VIZ-D-03).
    if edges.len() > 2 * n_elements {
        return "force".into();
    }
    // Tree/forest: every node has at most one incoming edge.
    let mut indegree: BTreeMap<&str, u32> = BTreeMap::new();
    for e in edges {
        *indegree.entry(e.target.as_str()).or_default() += 1;
    }
    if indegree.values().all(|&d| d <= 1) {
        return "compact-box".into();
    }
    "dagre".into()
}

/// Produce a sealed ViewSpec from a revision snapshot.
pub fn from_snapshot(snap: &RevSnapshot, opts: &ProduceOptions) -> ViewSpec {
    let syms = SymbolIndex::new(snap);
    let dag: &ConceptDag = &snap.concepts;

    // ---- elements -------------------------------------------------------
    let mut elements: Vec<ViewElement> = Vec::new();
    let mut hidden: Vec<HiddenCandidate> = Vec::new();
    let mut kept: BTreeMap<u32, String> = BTreeMap::new(); // concept id -> element id

    for node in &dag.nodes {
        let Some(evidence) = syms.evidence_for(&node.symbols) else {
            hidden.push(HiddenCandidate {
                id: format!("c{}", node.id),
                reason: "no resolvable symbol evidence — never drawn unevidenced".into(),
                salience_weight: (node.coverage_ratio * node.confidence).clamp(0.0, 1.0),
                label: Some(node.label.clone()),
                kind: Some(node.kind.as_str().to_ascii_lowercase()),
            });
            continue;
        };
        let (mode, claim, counter, fog) = display_mode_for(node);
        let mut fields = BTreeMap::new();
        fields.insert("confidence".into(), serde_json::json!(node.confidence));
        fields.insert("coverage".into(), serde_json::json!(node.coverage_ratio));
        fields.insert("provenance".into(), serde_json::json!(node.provenance));
        fields.insert("symbols".into(), serde_json::json!(node.symbols.len()));
        let id = format!("c{}", node.id);
        kept.insert(node.id, id.clone());
        elements.push(ViewElement {
            id,
            element_kind: node.kind.as_str().to_ascii_lowercase(),
            display_mode: mode,
            label: node.label.clone(),
            evidence,
            claim_id: claim,
            counter_argument: counter,
            fog_reason: fog,
            salience: Salience {
                factors: vec!["COVERAGE".into(), "CONFIDENCE".into()],
                weight: (node.coverage_ratio * node.confidence).clamp(0.0, 1.0),
                why_shown: format!(
                    "{} concept with {} traced symbol(s), provenance {}",
                    node.kind.as_str(),
                    node.symbols.len(),
                    node.provenance
                ),
            },
            fields,
            parent_id: node.parent.map(|p| format!("c{p}")),
        });
    }

    // ---- edges ----------------------------------------------------------
    let mut edges: Vec<ViewEdge> = Vec::new();
    let evidence_of = |eid: &str| -> EvidenceRef {
        elements
            .iter()
            .find(|e| e.id == eid)
            .map(|e| e.evidence.clone())
            .unwrap_or(EvidenceRef { file: String::new(), start: 0, end: 0 })
    };
    for node in &dag.nodes {
        let Some(child_eid) = kept.get(&node.id) else { continue };
        // Primary parent: containment proven by the child's own span.
        if let Some(p) = node.parent {
            if let Some(parent_eid) = kept.get(&p) {
                edges.push(ViewEdge {
                    id: format!("h{}", node.id),
                    source: parent_eid.clone(),
                    target: child_eid.clone(),
                    edge_kind: node.primary_edge.as_str().into(),
                    display_mode: DisplayMode::Fact,
                    evidence: evidence_of(child_eid),
                    counter_argument: None,
                });
            }
        }
        // Extra DAG edges: derived cohesion links — Inference with the
        // traceable reason as the verb.
        for (p, reason) in &node.extra_edges {
            if let Some(parent_eid) = kept.get(p) {
                edges.push(ViewEdge {
                    id: format!("x{}-{}", p, node.id),
                    source: parent_eid.clone(),
                    target: child_eid.clone(),
                    edge_kind: EdgeReason::as_str(reason).into(),
                    display_mode: DisplayMode::Inference,
                    evidence: evidence_of(child_eid),
                    counter_argument: None,
                });
            }
        }
    }
    // Aggregated call edges between concepts (statically resolved callees
    // only — unresolved calls never become edges, they stay in evidence).
    let mut sym_to_concept: BTreeMap<u32, u32> = BTreeMap::new();
    for node in &dag.nodes {
        for s in &node.symbols {
            sym_to_concept.insert(s.raw(), node.id);
        }
    }
    let mut seen_calls: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut call_evidence: BTreeMap<(u32, u32), EvidenceRef> = BTreeMap::new();
    for summary in &snap.summaries {
        let Some(&caller_concept) = sym_to_concept.get(&summary.sym.raw()) else { continue };
        for callee in &summary.callees {
            let Some(&callee_concept) = sym_to_concept.get(&callee.raw()) else { continue };
            if callee_concept == caller_concept {
                continue;
            }
            let key = (caller_concept, callee_concept);
            if seen_calls.insert(key) {
                if let Some(ev) = syms.evidence_of_symbol(summary.sym) {
                    call_evidence.insert(key, ev);
                }
            }
        }
    }
    for (a, b) in &seen_calls {
        let (Some(sa), Some(sb)) = (kept.get(a), kept.get(b)) else { continue };
        let evidence = call_evidence.get(&(*a, *b)).cloned().unwrap_or_else(|| {
            EvidenceRef { file: String::new(), start: 0, end: 0 }
        });
        edges.push(ViewEdge {
            id: format!("k{a}-{b}"),
            source: sa.clone(),
            target: sb.clone(),
            edge_kind: "calls".into(),
            display_mode: DisplayMode::Fact,
            evidence,
            counter_argument: None,
        });
    }

    // ---- salience-bounded cap (VIZ-I-06) --------------------------------
    let gaps: Vec<Gap>;
    if elements.len() > opts.max_elements {
        // Stable order: weight desc, then id asc (deterministic seal).
        elements.sort_by(|a, b| {
            b.salience
                .weight
                .partial_cmp(&a.salience.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        let dropped: Vec<ViewElement> = elements.split_off(opts.max_elements);
        let dropped_ids: BTreeSet<String> = dropped.iter().map(|e| e.id.clone()).collect();
        let mut dropped_hidden: Vec<HiddenCandidate> = dropped
            .into_iter()
            .map(|e| HiddenCandidate {
                id: e.id.clone(),
                reason: "salience-bounded element cap (VIZ-I-06)".into(),
                salience_weight: e.salience.weight,
                label: Some(e.label.clone()),
                kind: Some(e.element_kind.clone()),
            })
            .collect();
        dropped_hidden.extend(hidden.drain(..));
        hidden = dropped_hidden;
        // Edges and parents referencing dropped elements never dangle.
        edges.retain(|e| !dropped_ids.contains(&e.source) && !dropped_ids.contains(&e.target));
        for el in &mut elements {
            if let Some(p) = &el.parent_id {
                if dropped_ids.contains(p) {
                    el.parent_id = None;
                }
            }
        }
        let cap_gap = Gap {
            id: "gap-element-cap".into(),
            area: "elements".into(),
            why: format!(
                "{} concept(s) beyond the {}-element salience cap are listed in hidden[]",
                hidden.len(),
                opts.max_elements
            ),
        };
        gaps = vec![cap_gap];
    } else {
        gaps = Vec::new();
    }

    // ---- footer (recomputed from the FINAL set, VIZ-A-07) ---------------
    let mut footer = EpistemicFooter::empty();
    for el in &elements {
        match el.display_mode {
            DisplayMode::Fact => footer.facts += 1,
            DisplayMode::Inference => footer.inferences += 1,
            DisplayMode::Hypothesis => footer.hypotheses += 1,
            DisplayMode::Fog => footer.fog += 1,
        }
    }
    // VIZ-H-05: every gap stays reachable from the footer caption.
    if !gaps.is_empty() {
        footer.caption = format!("{} Gaps: {}.", footer.caption, gaps.iter().map(|g| g.id.as_str()).collect::<Vec<_>>().join(", "));
    }

    // ---- layout hint (VIZ-D-03, or the caller's verbatim override) ------
    let layout_hint = Some(match &opts.layout {
        Some(s) => LayoutHint { strategy: s.clone(), params: None },
        None => LayoutHint {
            strategy: auto_layout(elements.len(), &edges),
            params: None,
        },
    });

    let mut spec = ViewSpec {
        elements,
        edges,
        groups: None,
        footer,
        gaps,
        hidden,
        scope: ScopeAttestation {
            revision: snap.rev,
            environments: vec![],
            coverage: ScopeCoverage {
                analyzed: snap.coverage.complete,
                ratio: Some(snap.coverage.symbol_ratio()),
            },
        },
        layout_hint,
        preferences: None,
        semantic_hash: String::new(),
        render_hash: String::new(),
    };
    debug_assert!(gate::validate(&spec).is_ok(), "producer output must pass its own gate");
    hash::seal(&mut spec);
    spec
}

/// Re-exported for CLI ergonomics: a produced spec is always gate-clean
/// and freshly sealed.
pub fn produce(snap: &RevSnapshot, opts: &ProduceOptions) -> Result<ViewSpec, String> {
    let spec = from_snapshot(snap, opts);
    match gate::validate(&spec) {
        Ok(()) => Ok(spec),
        Err(v) => Err(v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; ")),
    }
}

/// `Ns` re-export guard: keeps the time primitive visible for downstream
/// callers building attestations without adding cie-core directly.
#[allow(unused)]
fn _time_primitive(now: Ns) -> Ns {
    now
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_concepts::{ConceptKind, ConceptNode};
    use cie_core::evidence::Evidence as CoreEvidence;
    use cie_semantic::{LangId, SymbolKind};

    /// Build a two-concept snapshot: Cart(Aggregate, rule, complete) →
    /// CartItem(Entity, llm-derived). Cart has one symbol with a span.
    fn snapshot() -> RevSnapshot {
        let now: Ns = 1_700_000_000_000_000_000;
        let mut dag = ConceptDag::default();
        let root = dag.push(ConceptNode::new(0, "Cart", ConceptKind::Aggregate, now));
        let child = dag.push(ConceptNode::new(0, "CartItem", ConceptKind::Entity, now));
        dag.link_child(root, child);
        dag.get_mut(root).unwrap().symbols = vec![SymbolId(1)];
        dag.get_mut(root).unwrap().provenance = "rule".into();
        dag.get_mut(root).unwrap().confidence = 1.0;
        dag.get_mut(root).unwrap().coverage_complete = true;
        dag.get_mut(child).unwrap().symbols = vec![SymbolId(2)];
        dag.get_mut(child).unwrap().provenance = "llm".into();
        dag.get_mut(child).unwrap().confidence = 0.8;

        let mut files = vec!["src/cart.py".to_string(), "src/item.py".to_string()];
        files.shrink_to_fit();
        let symbols = vec![
            SymbolFact {
            annotations: Vec::new(),
                id: SymbolId(1),
                kind: SymbolKind::Class,
                name: "Cart".into(),
                qual_name: "cart.Cart".into(),
                loc: cie_core::SourceLocation::span_bytes(cie_core::FileId(0), 10, 40),
                lang: LangId::parse("python"),
                parent: None,
                rev: 1,
                doc: None,
            },
            SymbolFact {
            annotations: Vec::new(),
                id: SymbolId(2),
                kind: SymbolKind::Class,
                name: "CartItem".into(),
                qual_name: "item.CartItem".into(),
                loc: cie_core::SourceLocation::span_bytes(cie_core::FileId(1), 5, 25),
                lang: LangId::parse("python"),
                parent: None,
                rev: 1,
                doc: None,
            },
        ];

        RevSnapshot {
            rev: 42,
            files,
            symbols,
            usecase_view: Vec::new(),
            summaries: vec![],
            findings: vec![],
            contracts: vec![],
            concepts: dag,
            coverage: cie_core::evidence::Coverage {
                files_total: 2,
                files_analyzed: 2,
                symbols_total: 2,
                symbols_analyzed: 2,
                complete: true,
                notes: vec![],
                wall_ns: 1_000,
            },
            evidence: CoreEvidence::new(42, "test", now),
            concepts_tier: None,
            warnings: vec![],
            ingest_wall_ns: 1_000,
            incremental: false,
            summary_cache: vec![],
            summary_reuse: None,
            state_model: None,
            stack: None,
        }
    }

    /// End-to-end producer → gate → seal → verify on a synthetic snapshot.
    #[test]
    fn produce_gate_seal_verify() {
        let snap = snapshot();
        let spec = produce(&snap, &ProduceOptions::default()).expect("produced spec passes gate");
        assert_eq!(spec.elements.len(), 2);
        // Cart: rule + complete + confidence 1.0 → Fact, no claim needed.
        let cart = spec.elements.iter().find(|e| e.label == "Cart").unwrap();
        assert_eq!(cart.display_mode, DisplayMode::Fact);
        assert!(cart.claim_id.is_none());
        assert_eq!(cart.evidence.file, "src/cart.py");
        // CartItem: llm provenance, confidence 0.8 → Inference with claim.
        let item = spec.elements.iter().find(|e| e.label == "CartItem").unwrap();
        assert_eq!(item.display_mode, DisplayMode::Inference);
        assert!(item.claim_id.is_some());
        // Hierarchy edge survived with containment provenance.
        assert_eq!(spec.edges.len(), 1);
        assert_eq!(spec.edges[0].edge_kind, "package_containment");
        assert_eq!(spec.edges[0].source, cart.id);
        // Footer recomputed from the final set.
        assert_eq!(spec.footer.facts, 1);
        assert_eq!(spec.footer.inferences, 1);
        // Sealed and verifiable.
        assert!(hash::verify_semantic(&spec));
    }

    /// Cap: 2 500 concepts → exactly the cap kept, everything else in
    /// hidden[] with a gap entry reachable from the footer (VIZ-I-06,
    /// VIZ-H-05) — and no dangling edges/parents.
    #[test]
    fn cap_truncates_with_ledger() {
        let now: Ns = 0;
        let mut snap = snapshot();
        let mut dag = ConceptDag::default();
        for i in 0..2_500u32 {
            let mut n = ConceptNode::new(0, format!("C{i}"), ConceptKind::Module, now);
            n.symbols = vec![]; // would be hidden as unevidenced — instead give span:
            n.provenance = "rule".into();
            dag.push(n);
        }
        // Re-wire symbols: intern 2 500 synthetic file paths + facts.
        let mut files: Vec<String> = Vec::new();
        let mut symbols: Vec<SymbolFact> = Vec::new();
        for i in 0..2_500u32 {
            let path = format!("src/m{i}.py");
            let fid = cie_core::FileId(files.len() as u32);
            files.push(path);
            let sid = SymbolId(i + 10);
            symbols.push(SymbolFact {
            annotations: Vec::new(),
                id: sid,
                kind: SymbolKind::Function,
                name: format!("f{i}"),
                qual_name: format!("m{i}.f{i}"),
                loc: cie_core::SourceLocation::span_bytes(fid, 0, 9),
                lang: LangId::parse("python"),
                parent: None,
                rev: 1,
                doc: None,
            });
            dag.get_mut(i).unwrap().symbols = vec![sid];
        }
        snap.files = files;
        snap.symbols = symbols;
        snap.concepts = dag;

        let spec = produce(&snap, &ProduceOptions::default()).expect("gate-clean");
        assert_eq!(spec.elements.len(), ELEMENT_CAP);
        assert_eq!(spec.hidden.len(), 500);
        assert!(spec.gaps.iter().any(|g| g.id == "gap-element-cap"));
        assert!(spec.footer.caption.contains("gap-element-cap"), "VIZ-H-05");
        assert_eq!(
            spec.footer.facts + spec.footer.inferences + spec.footer.hypotheses + spec.footer.fog,
            ELEMENT_CAP as u32
        );
        // Every edge endpoint survived the truncation.
        let ids = spec.element_ids();
        assert!(spec.edges.iter().all(|e| ids.contains(e.source.as_str())
            && ids.contains(e.target.as_str())));
        assert!(hash::verify_semantic(&spec));
    }

    /// VIZ-D-03: acyclic sparse graph → dagre; cyclic → force.
    #[test]
    fn auto_layout_selection() {
        let snap = snapshot();
        let mut spec = produce(&snap, &ProduceOptions::default()).unwrap();
        assert_eq!(spec.layout_hint.as_ref().unwrap().strategy, "compact-box",
            "two nodes, one containment edge → tree");

        // Close a cycle with an extra edge (b → a) to flip the heuristic.
        let a = spec.elements[0].id.clone();
        let b = spec.elements[1].id.clone();
        spec.edges.push(ViewEdge {
            id: "back".into(),
            source: b.clone(),
            target: a.clone(),
            edge_kind: "calls".into(),
            display_mode: DisplayMode::Fact,
            evidence: spec.elements[1].evidence.clone(),
            counter_argument: None,
        });
        assert_eq!(auto_layout(spec.elements.len(), &spec.edges), "force");
    }

    /// Honesty: a concept with no resolvable evidence is hidden, never
    /// drawn unevidenced (VIZ-A-03 by construction).
    #[test]
    fn unevidenced_concept_is_hidden_not_drawn() {
        let mut snap = snapshot();
        snap.concepts.get_mut(1).unwrap().symbols = vec![];
        let spec = produce(&snap, &ProduceOptions::default()).unwrap();
        assert_eq!(spec.elements.len(), 1);
        assert!(spec.hidden.iter().any(|h| h.id == "c1"));
    }

    /// VIZ-10 — CIE output contains NO engine id: the wire form of a
    /// produced spec must not name any engine, chart or renderer choice.
    #[test]
    fn produced_spec_names_no_engine() {
        let snap = snapshot();
        let spec = produce(&snap, &ProduceOptions::default()).unwrap();
        let wire = serde_json::to_string(&spec).unwrap();
        for banned in ["\"engineId\"", "\"engine\"", "\"renderer\"", "chartType"] {
            assert!(!wire.contains(banned), "VIZ-10 violation: {banned} in {wire}");
        }
    }
}
