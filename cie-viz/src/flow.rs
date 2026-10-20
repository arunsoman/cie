//! RFC-0024 stage 0.5 — the flow-chart producer over `LangFacts`
//! (ux-spec §4.1; RFC-0024 §3.1 row: "flow-chart producer mode over Pdg
//! facts: process/store/external/decision elements, reads/writes/call
//! edges — schema untouched; open kinds").
//!
//! Honesty rules this module enforces (ux-spec §5):
//! * a decision diamond is drawn ONLY where the engine recorded an
//!   `IfSite` (the RFC's DecisionFact) — until a slice has none the chart
//!   prints "decisions not indexed in this slice" client-side; this
//!   producer never guesses a diamond;
//! * stores (SQL/file sinks) carry their owning symbol's span as evidence
//!   — a sink call has no single code line, so the popup carve-out stays
//!   honest;
//! * every withheld element lands in `hidden[]` WITH its patch-2
//!   label/kind, so the G1 disclosure popup has real clickable rows;
//! * unresolved calls never become edges (the gate would reject them and
//!   the product would be lying).
//!
//! Element kinds emitted (open vocabulary, schema untouched):
//!   `process` (a callable body), `decision` (a recorded IfSite),
//!   `store` (a recorded sink). Edges: `calls`, `data-flow`,
//!   `writes` (process → store).

use crate::gate;
use crate::hash;
use crate::model::{
    DisplayMode, EpistemicFooter, EvidenceRef, Gap, HiddenCandidate, LayoutHint, Salience,
    ScopeAttestation, ScopeCoverage, ViewEdge, ViewElement, ViewSpec,
};
use crate::produce::auto_layout;
use cie_semantic::facts::{LangFacts, SinkKind, SymbolKind, SymbolStore};
use std::collections::{BTreeMap, BTreeSet};

/// Flow-producer options (mirrors `ProduceOptions`).
#[derive(Debug, Clone)]
pub struct FlowOptions {
    pub max_elements: usize,
    /// Force a layout strategy (verbatim G6 name); `None` auto-selects.
    pub layout: Option<String>,
}

impl Default for FlowOptions {
    fn default() -> Self {
        FlowOptions { max_elements: gate::ELEMENT_CAP, layout: None }
    }
}

fn truncated(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

fn sink_kind_str(k: SinkKind) -> &'static str {
    match k {
        SinkKind::SqlExecute => "sql_execute",
        SinkKind::ShellExec => "shell_exec",
        SinkKind::CodeEval => "code_eval",
        SinkKind::FileWrite => "file_write",
        SinkKind::HtmlRender => "html_render",
    }
}

fn function_like(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method | SymbolKind::Test)
}

/// Produce a sealed flow-chart ViewSpec from extracted facts. All ids and
/// evidence come from ONE extraction pass (`ExtractedProject`), so symbol
/// ids, the file table and the facts are mutually consistent.
pub fn flow_chart_from_facts(
    facts: &[LangFacts],
    store: &SymbolStore,
    opts: &FlowOptions,
) -> ViewSpec {
    let mut elements: Vec<ViewElement> = Vec::new();
    let mut hidden: Vec<HiddenCandidate> = Vec::new();
    let mut edges: Vec<ViewEdge> = Vec::new();

    // ---- fact census: which callables belong to the flow ----------------
    // A callable owns a fact (call/decision/sink) OR is the resolved target
    // of one — both are flow participants (a callee-only function still
    // shows as a process box, or the call chain could never be drawn).
    let mut has_fact: BTreeSet<u32> = BTreeSet::new();
    for f in facts {
        for c in &f.calls {
            has_fact.insert(c.caller.0);
            if let cie_semantic::facts::Callee::Resolved(id) = &c.callee {
                has_fact.insert(id.0);
            }
        }
        for i in &f.ifs {
            has_fact.insert(i.symbol.0);
        }
        for s in &f.sinks {
            has_fact.insert(s.symbol.0);
        }
    }

    let path_of_sym = |id: u32| -> Option<(String, u64, u64)> {
        let s = store.get(cie_core::types::SymbolId(id))?;
        let path = store.files.paths.get(s.loc.file.0 as usize)?;
        Some((path.clone(), s.loc.start as u64, s.loc.end as u64))
    };

    // ---- process elements (a callable body with recorded facts) --------
    let mut drawn: BTreeSet<u32> = BTreeSet::new();
    for s in &store.symbols {
        if !function_like(s.kind) || !has_fact.contains(&s.id.0) {
            continue;
        }
        let Some((path, start, end)) = path_of_sym(s.id.0) else {
            hidden.push(HiddenCandidate {
                id: format!("p{}", s.id.0),
                reason: "symbol's file is not resolvable — never drawn unevidenced".into(),
                salience_weight: 1.0,
                label: Some(s.name.clone()),
                kind: Some("process".into()),
            });
            continue;
        };
        drawn.insert(s.id.0);
        let line = store.files.line(s.loc);
        let mut fields = BTreeMap::new();
        fields.insert("file".into(), serde_json::json!(path));
        fields.insert("line".into(), serde_json::json!(line));
        fields.insert("qualName".into(), serde_json::json!(s.qual_name.clone()));
        elements.push(ViewElement {
            id: format!("p{}", s.id.0),
            element_kind: "process".into(),
            display_mode: DisplayMode::Fact,
            label: s.name.clone(),
            evidence: EvidenceRef { file: path, start, end },
            claim_id: None,
            counter_argument: None,
            fog_reason: None,
            salience: Salience {
                factors: vec!["RULE_EXTRACTION".into()],
                weight: 1.0,
                why_shown: "callable in the recorded flow (own facts or a resolved call)".into(),
            },
            fields,
            parent_id: None,
        });
    }

    // ---- decision + store elements, per file ---------------------------
    for (fid, f) in facts.iter().enumerate() {
        for (i, if_site) in f.ifs.iter().enumerate() {
            let id = format!("d{fid}-{i}");
            if !drawn.contains(&if_site.symbol.0) {
                // The owner is not a drawn callable (module-level `if`,
                // class-level guard): a ledger row, never an orphan node.
                hidden.push(HiddenCandidate {
                    id,
                    reason: "decision's owner is not a callable body in this slice".into(),
                    salience_weight: 0.8,
                    label: Some(truncated(&if_site.cond, 60)),
                    kind: Some("decision".into()),
                });
                continue;
            }
            let start = if_site.loc.start as u64;
            let end = if_site.loc.end as u64;
            let mut fields = BTreeMap::new();
            fields.insert("file".into(), serde_json::json!(f.path));
            fields.insert("line".into(), serde_json::json!(if_site.line));
            fields.insert("cond".into(), serde_json::json!(if_site.cond));
            elements.push(ViewElement {
                id,
                element_kind: "decision".into(),
                display_mode: DisplayMode::Fact,
                label: truncated(&if_site.cond, 48),
                evidence: EvidenceRef { file: f.path.clone(), start, end },
                claim_id: None,
                counter_argument: None,
                fog_reason: None,
                salience: Salience {
                    factors: vec!["DECISION_FACT".into()],
                    weight: 0.8,
                    why_shown: "recorded branch condition (IfSite) with file+line".into(),
                },
                fields,
                parent_id: Some(format!("p{}", if_site.symbol.0)),
            });
        }
        for (i, sink) in f.sinks.iter().enumerate() {
            if !drawn.contains(&sink.symbol.0) {
                hidden.push(HiddenCandidate {
                    id: format!("s{fid}-{i}"),
                    reason: "sink's owner is not a callable body in this slice".into(),
                    salience_weight: 0.7,
                    label: Some(truncated(&sink.call_text, 60)),
                    kind: Some("store".into()),
                });
                continue;
            }
            // A sink has no single code line: the OWNING SYMBOL's span is
            // the evidence (the call lives inside that body); the exact
            // line travels in fields.
            let Some((path, start, end)) = path_of_sym(sink.symbol.0) else { continue };
            let mut fields = BTreeMap::new();
            fields.insert("file".into(), serde_json::json!(path));
            fields.insert("line".into(), serde_json::json!(sink.line));
            fields.insert("kind".into(), serde_json::json!(sink_kind_str(sink.kind)));
            fields.insert("parameterized".into(), serde_json::json!(sink.parameterized));
            elements.push(ViewElement {
                id: format!("s{fid}-{i}"),
                element_kind: "store".into(),
                display_mode: DisplayMode::Fact,
                label: truncated(&sink.call_text, 48),
                evidence: EvidenceRef { file: path, start, end },
                claim_id: None,
                counter_argument: None,
                fog_reason: None,
                salience: Salience {
                    factors: vec!["SINK_FACT".into()],
                    weight: 0.7,
                    why_shown: "recorded data sink inside the selection".into(),
                },
                fields,
                parent_id: None,
            });
        }
    }

    // ---- edges: calls + data-flow (resolved only), writes (sinks) -------
    let graph = cie_semantic::graph::relation_graph_from_facts(facts, store);
    let mut seen: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for (a, b, kind) in graph.edge_triples() {
        let (Some(sa), Some(sb)) = (graph.node_weight(a), graph.node_weight(b)) else { continue };
        if !drawn.contains(&sa.0) || !drawn.contains(&sb.0) || sa == sb {
            continue;
        }
        let (edge_kind, eid) = match kind {
            cie_relations::RelKind::Calls => ("calls", "k"),
            cie_relations::RelKind::DataFlow => ("data-flow", "w"),
            _ => continue,
        };
        let key = (format!("p{}", sa.0), format!("p{}", sb.0), edge_kind);
        if !seen.insert(key) {
            continue;
        }
        let evidence = path_of_sym(sa.0)
            .map(|(file, start, end)| EvidenceRef { file, start, end })
            .unwrap_or(EvidenceRef { file: String::new(), start: 0, end: 0 });
        edges.push(ViewEdge {
            id: format!("{eid}{}-{}", sa.0, sb.0),
            source: format!("p{}", sa.0),
            target: format!("p{}", sb.0),
            edge_kind: edge_kind.into(),
            display_mode: DisplayMode::Fact,
            evidence,
            counter_argument: None,
        });
    }
    for (fid, f) in facts.iter().enumerate() {
        for (i, sink) in f.sinks.iter().enumerate() {
            if !drawn.contains(&sink.symbol.0) {
                continue;
            }
            let key = (format!("p{}", sink.symbol.0), format!("s{fid}-{i}"), "writes");
            if !seen.insert(key) {
                continue;
            }
            let evidence = path_of_sym(sink.symbol.0)
                .map(|(file, start, end)| EvidenceRef { file, start, end })
                .unwrap_or(EvidenceRef { file: String::new(), start: 0, end: 0 });
            edges.push(ViewEdge {
                id: format!("x{}-s{fid}-{i}", sink.symbol.0),
                source: format!("p{}", sink.symbol.0),
                target: format!("s{fid}-{i}"),
                edge_kind: "writes".into(),
                display_mode: DisplayMode::Fact,
                evidence,
                counter_argument: None,
            });
        }
    }

    // ---- salience-bounded cap (VIZ-I-06, same rule as from_snapshot) ----
    let gaps: Vec<Gap>;
    if elements.len() > opts.max_elements {
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
        edges.retain(|e| !dropped_ids.contains(&e.source) && !dropped_ids.contains(&e.target));
        for el in &mut elements {
            if let Some(p) = &el.parent_id {
                if dropped_ids.contains(p) {
                    el.parent_id = None;
                }
            }
        }
        gaps = vec![Gap {
            id: "gap-element-cap".into(),
            area: "elements".into(),
            why: format!(
                "{} flow element(s) beyond the {}-element salience cap are listed in hidden[]",
                hidden.len(),
                opts.max_elements
            ),
        }];
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
    if !gaps.is_empty() {
        footer.caption = format!(
            "{} Gaps: {}.",
            footer.caption,
            gaps.iter().map(|g| g.id.as_str()).collect::<Vec<_>>().join(", ")
        );
    }

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
            revision: 0,
            environments: vec![],
            coverage: ScopeCoverage { analyzed: true, ratio: None },
        },
        layout_hint,
        preferences: None,
        semantic_hash: String::new(),
        render_hash: String::new(),
    };
    debug_assert!(gate::validate(&spec).is_ok(), "flow producer output must pass its own gate");
    hash::seal(&mut spec);
    spec
}

/// Validate + seal helper mirroring `produce()`.
pub fn flow_chart(
    facts: &[LangFacts],
    store: &SymbolStore,
    opts: &FlowOptions,
) -> Result<ViewSpec, String> {
    let spec = flow_chart_from_facts(facts, store, opts);
    match gate::validate(&spec) {
        Ok(()) => Ok(spec),
        Err(v) => Err(v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_semantic::facts::{
        AssignmentFact, Callee, CallSite, IfSite, RhsKind, SinkKind, SinkSite,
        SymbolFact,
    };
    use cie_core::types::FileTable;
    use cie_semantic::LangId;

    /// One python file: `run()` calls `compute()` and `save()`;
    /// `run()` has an `if` (the decision); `save()` hits a SQL sink;
    /// `compute()` returns a call value (`x = helper()` data-flow).
    fn fixture() -> (Vec<LangFacts>, SymbolStore) {
        let mut f = LangFacts::new("svc/flows.py", LangId::Python, "test");
        // ids: 0 module, 1 run, 2 compute, 3 save
        let mk = |id: u32, name: &str, qual: &str, kind: SymbolKind, start: u32, end: u32| SymbolFact {
            id: cie_core::types::SymbolId(id),
            kind,
            name: name.into(),
            qual_name: qual.into(),
            loc: cie_core::types::SourceLocation::span_bytes(cie_core::types::FileId(0), start, end),
            lang: LangId::Python,
            parent: None,
            rev: 0,
            doc: None,
        };
        f.symbols.push(mk(0, "flows", "flows", SymbolKind::Module, 0, 400));
        f.symbols.push(mk(1, "run", "flows.run", SymbolKind::Function, 10, 90));
        f.symbols.push(mk(2, "compute", "flows.compute", SymbolKind::Function, 100, 150));
        f.symbols.push(mk(3, "save", "flows.save", SymbolKind::Function, 160, 220));
        f.calls.push(CallSite {
            caller: cie_core::types::SymbolId(1),
            callee_text: "compute()".into(),
            callee: Callee::Resolved(cie_core::types::SymbolId(2)),
            line: 12,
            in_loop: false,
            arg_vars: vec![],
        });
        f.calls.push(CallSite {
            caller: cie_core::types::SymbolId(1),
            callee_text: "save(rows)".into(),
            callee: Callee::Resolved(cie_core::types::SymbolId(3)),
            line: 20,
            in_loop: false,
            arg_vars: vec!["rows".into()],
        });
        f.ifs.push(IfSite {
            symbol: cie_core::types::SymbolId(1),
            line: 14,
            cond: "if not rows".into(),
            cond_vars: vec!["rows".into()],
            loc: cie_core::types::SourceLocation::span_bytes(cie_core::types::FileId(0), 150, 190),
        });
        f.assignments.push(AssignmentFact {
            symbol: cie_core::types::SymbolId(1),
            var: "x".into(),
            rhs_kind: RhsKind::Call,
            line: 12,
            rhs_data: vec!["compute()".into()],
        });
        f.sinks.push(SinkSite {
            symbol: cie_core::types::SymbolId(3),
            call_text: "cursor.execute(sql)".into(),
            arg_vars: vec!["sql".into()],
            line: 205,
            kind: SinkKind::SqlExecute,
            parameterized: true,
        });
        let mut files = FileTable::new();
        files.intern("svc/flows.py".to_string());
        let store = SymbolStore::build_with_files(f.symbols.clone(), files);
        (vec![f], store)
    }

    #[test]
    fn flow_elements_carry_file_and_line() {
        let (facts, store) = fixture();
        let spec = flow_chart(&facts, &store, &FlowOptions::default()).expect("gate-clean");
        let run = spec.elements.iter().find(|e| e.id == "p1").expect("run process");
        assert_eq!(run.element_kind, "process");
        assert_eq!(run.fields.get("file").and_then(|v| v.as_str()), Some("svc/flows.py"));
        assert!(run.fields.contains_key("line"));
        let dec = spec.elements.iter().find(|e| e.element_kind == "decision").expect("decision");
        assert_eq!(dec.fields.get("line").and_then(|v| v.as_u64()), Some(14));
        assert_eq!(dec.evidence.file, "svc/flows.py");
        assert_eq!(dec.parent_id.as_deref(), Some("p1"));
        let store_el = spec.elements.iter().find(|e| e.element_kind == "store").expect("store");
        assert_eq!(store_el.fields.get("kind").and_then(|v| v.as_str()), Some("sql_execute"));
    }

    #[test]
    fn calls_and_writes_edges_between_drawn_elements() {
        let (facts, store) = fixture();
        let spec = flow_chart(&facts, &store, &FlowOptions::default()).expect("gate-clean");
        assert!(
            spec.edges.iter().any(|e| e.edge_kind == "calls"
                && e.source == "p1"
                && e.target == "p2"),
            "run → compute calls edge"
        );
        assert!(
            spec.edges.iter().any(|e| e.edge_kind == "writes"
                && e.source == "p3"
                && e.target.starts_with('s')),
            "save → store writes edge"
        );
    }

    #[test]
    fn module_level_decisions_become_ledger_rows_not_orphans() {
        let (mut facts, store) = fixture();
        facts[0].ifs.push(IfSite {
            symbol: cie_core::types::SymbolId(0), // the MODULE, not a callable
            line: 2,
            cond: "if __name__ == '__main__'".into(),
            cond_vars: vec![],
            loc: cie_core::types::SourceLocation::span_bytes(cie_core::types::FileId(0), 20, 50),
        });
        let spec = flow_chart(&facts, &store, &FlowOptions::default()).expect("gate-clean");
        assert!(
            !spec.elements.iter().any(|e| e.fields.get("cond").and_then(|v| v.as_str()) == Some("__name__ == '__main__'"))
                || spec.elements.iter().all(|e| e.parent_id.is_some()),
            "no orphan decision nodes"
        );
        let row = spec
            .hidden
            .iter()
            .find(|h| h.kind.as_deref() == Some("decision") && h.id.starts_with('d'))
            .expect("module-level decision is a ledger row");
        assert!(row.label.as_deref().unwrap_or_default().contains("__name__"));
    }

    #[test]
    fn unresolved_calls_never_become_edges() {
        let (mut facts, store) = fixture();
        facts[0].calls.push(CallSite {
            caller: cie_core::types::SymbolId(1),
            callee_text: "external_thing()".into(),
            callee: Callee::Unresolved("unresolved_name".into()),
            line: 30,
            in_loop: false,
            arg_vars: vec![],
        });
        let spec = flow_chart(&facts, &store, &FlowOptions::default()).expect("gate-clean");
        assert!(
            !spec.edges.iter().any(|e| e.edge_kind == "calls" && e.evidence.file.is_empty()),
            "every calls edge is evidenced"
        );
    }

    #[test]
    fn cap_drops_lowest_salience_with_ledger_rows() {
        let (facts, store) = fixture();
        let opts = FlowOptions { max_elements: 3, layout: None };
        let spec = flow_chart(&facts, &store, &opts).expect("gate-clean");
        assert!(spec.elements.len() <= 3);
        assert!(!spec.hidden.is_empty(), "dropped elements are disclosed");
        assert!(spec.hidden.iter().all(|h| h.label.is_some() && h.kind.is_some()));
        let ids: BTreeSet<&str> = spec.elements.iter().map(|e| e.id.as_str()).collect();
        for e in &spec.edges {
            assert!(ids.contains(e.source.as_str()) && ids.contains(e.target.as_str()));
        }
    }
}
