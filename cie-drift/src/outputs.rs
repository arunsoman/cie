//! Human/markdown/dot/mermaid renderers for findings, concepts and drift.

use cie_claims::model::DriftReport;
use cie_concepts::ConceptDag;
use cie_contract::findings::Finding;
use cie_core::types::SymbolId;
use cie_semantic::facts::SymbolStore;
use std::collections::BTreeMap;

pub fn findings_markdown(findings: &[Finding]) -> String {
    let mut out = String::from("# Analysis Findings\n\n");
    let mut by_kind: BTreeMap<String, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        by_kind
            .entry(f.kind.as_str().to_string())
            .or_default()
            .push(f);
    }
    out.push_str("| Kind | Severity | Symbol | Message |\n|---|---|---|---|\n");
    for (kind, fs) in &by_kind {
        for f in fs {
            out.push_str(&format!(
                "| {} | {} | `{}` | {} |\n",
                kind,
                f.severity.as_str(),
                f.symbol_name,
                f.message.replace('|', "\\|")
            ));
        }
    }
    out
}

/// Nested concept tree in the PRD §27 shape (acceptance #27).
/// TASK-083: depth-limited render view — keeps nodes within `max_depth`
/// levels from the root (root = level 1), remapping ids into a dense DAG.
/// Multi-parent edges pointing outside the view are elided with the node.
pub fn depth_limited_view(dag: &ConceptDag, max_depth: u32) -> ConceptDag {
    let mut view = ConceptDag::default();
    if dag.nodes.is_empty() {
        return view;
    }
    let root_id = dag.nodes[0].id;
    // BFS collecting (old id, depth).
    use std::collections::{BTreeMap, VecDeque};
    let mut depth_of: BTreeMap<u32, u32> = BTreeMap::new();
    let mut queue = VecDeque::new();
    depth_of.insert(root_id, 1);
    queue.push_back(root_id);
    while let Some(id) = queue.pop_front() {
        let d = depth_of[&id];
        if d >= max_depth {
            continue;
        }
        if let Some(node) = dag.get(id) {
            for &c in &node.children {
                if let std::collections::btree_map::Entry::Vacant(e) = depth_of.entry(c) {
                    e.insert(d + 1);
                    queue.push_back(c);
                }
            }
        }
    }
    let mut new_id: BTreeMap<u32, u32> = BTreeMap::new();
    for old in depth_of.keys() {
        let node = dag.get(*old).unwrap();
        let mut copy = node.clone();
        copy.id = 0;
        let nid = view.push(copy);
        new_id.insert(*old, nid);
    }
    for old in depth_of.keys() {
        let node = dag.get(*old).unwrap();
        let nid = new_id[old];
        if let Some(v) = view.get_mut(nid) {
            v.parent = node.parent.and_then(|p| new_id.get(&p).copied());
            v.children = node
                .children
                .iter()
                .filter_map(|c| new_id.get(c).copied())
                .collect();
            v.extra_edges = node
                .extra_edges
                .iter()
                .filter_map(|(p, r)| new_id.get(p).map(|np| (*np, *r)))
                .collect();
        }
    }
    view.unattached_behaviors = dag.unattached_behaviors.clone();
    view.unattached_rules = dag.unattached_rules.clone();
    view
}

pub fn concepts_markbook(
    dag: &ConceptDag,
    with_behaviors: bool,
    with_rules: bool,
    symbol_names: &std::collections::BTreeMap<SymbolId, String>,
) -> String {
    let mut out = String::from("# Hierarchical Concept Behavior\n\n");
    if dag.nodes.is_empty() {
        out.push_str("(no concepts)\n");
        return out;
    }
    let root = &dag.nodes[0];

    #[allow(clippy::too_many_arguments)]
    fn walk(
        dag: &ConceptDag,
        id: usize,
        depth: usize,
        out: &mut String,
        with_behaviors: bool,
        with_rules: bool,
        symbol_names: &std::collections::BTreeMap<SymbolId, String>,
    ) {
        let Some(node) = dag.nodes.get(id) else {
            return;
        };
        let indent = "  ".repeat(depth);
        let extra = if node.extra_edges.is_empty() {
            String::new()
        } else {
            format!(
                " *(also under: {})*",
                node.extra_edges
                    .iter()
                    .filter_map(|(p, r)| dag.get(*p).map(|pn| format!(
                        "{} [{}]",
                        pn.label,
                        r.as_str()
                    )))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        out.push_str(&format!(
            "{}## {} ({}){}\n",
            indent,
            node.label,
            node.kind.as_str(),
            extra
        ));
        if !node.intent.is_empty() {
            out.push_str(&format!("{}- Intent: {}\n", indent, node.intent.join(", ")));
        }
        if !node.symbols.is_empty() {
            out.push_str(&format!(
                "{}- Symbols: {}\n",
                indent,
                node.symbols
                    .iter()
                    .map(|s| {
                        symbol_names
                            .get(s)
                            .cloned()
                            .unwrap_or_else(|| format!("sym{}", s))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if with_behaviors {
            for b in &node.behaviors {
                out.push_str(&format!("{}  - Behavior: {}\n", indent, b.kind));
            }
        }
        if with_rules {
            for r in &node.rules {
                out.push_str(&format!(
                    "{}  - Rule: {} ({}, {})\n",
                    indent,
                    dag.nodes.first()
                        .and(Some(format!("C-{:03}", r.contract)))
                        .unwrap_or_default(),
                    r.kind,
                    r.authority.as_str()
                ));
            }
        }
        out.push_str(&format!(
            "{}- Coverage: {:.0}% complete={} confidence={:.2}\n",
            indent,
            node.coverage_ratio * 100.0,
            node.coverage_complete,
            node.confidence
        ));
        for c in &node.children {
            walk(
                dag,
                *c as usize,
                depth + 1,
                out,
                with_behaviors,
                with_rules,
                symbol_names,
            );
        }
    }

    walk(
        dag,
        root.id as usize,
        0,
        &mut out,
        with_behaviors,
        with_rules,
        symbol_names,
    );

    if !dag.unattached_behaviors.is_empty() {
        out.push_str("\n## Unattached behaviors\n\n");
        for b in &dag.unattached_behaviors {
            out.push_str(&format!("- {} (summary {})\n", b.kind, b.summary));
        }
    }
    if !dag.unattached_rules.is_empty() {
        out.push_str("\n## Unattached rules\n\n");
        for r in &dag.unattached_rules {
            out.push_str(&format!(
                "- C-{:03} ({}, {})\n",
                r.contract,
                r.kind,
                r.authority.as_str()
            ));
        }
    }
    out
}

pub fn concepts_dot(dag: &ConceptDag) -> String {
    let mut out = String::from("digraph concepts {\n  rankdir=LR;\n  node [shape=box];\n");
    for n in &dag.nodes {
        out.push_str(&format!(
            "  n{} [label=\"{}\\n({})\"];\n",
            n.id,
            n.label.replace('"', "'"),
            n.kind.as_str()
        ));
    }
    for n in &dag.nodes {
        if let Some(p) = n.parent {
            out.push_str(&format!(
                "  n{} -> n{} [label=\"{}\"];\n",
                p,
                n.id,
                n.primary_edge.as_str()
            ));
        }
        for (p, r) in &n.extra_edges {
            out.push_str(&format!(
                "  n{} -> n{} [style=dashed label=\"{}\"];\n",
                p,
                n.id,
                r.as_str()
            ));
        }
    }
    out.push_str("}\n");
    out
}

pub fn concepts_mermaid(dag: &ConceptDag) -> String {
    let mut out = String::from("graph TD\n");
    for n in &dag.nodes {
        out.push_str(&format!(
            "  n{}[\"{} ({})\"]\n",
            n.id,
            n.label.replace('"', "'"),
            n.kind.as_str()
        ));
    }
    for n in &dag.nodes {
        if let Some(p) = n.parent {
            out.push_str(&format!("  n{} --> n{}\n", p, n.id));
        }
    }
    out
}

pub fn drift_markdown(report: &DriftReport, store: &SymbolStore) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Drift Report — rev {}\n\nDocs analyzed: {}\n\n",
        report.rev,
        report
            .docs
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str("## Aggregate\n\n| Status | Count |\n|---|---|\n");
    for (status, count) in &report.aggregate {
        out.push_str(&format!("| {status} | {count} |\n"));
    }
    if !report.complete {
        out.push_str("\n> **Incomplete verification** — budget/timeout reached; remaining claims are `Unknown`, never `Implemented`.\n");
    }

    out.push_str("\n## Per-claim\n\n| Doc:Line | Claim | Kind | Status | Evidence |\n|---|---|---|---|---|\n");
    for c in &report.claims {
        let Some(v) = report.verifications.iter().find(|v| v.claim == c.id) else {
            continue;
        };
        out.push_str(&format!(
            "| `{}:{}` | {} | {} | **{}** | {} |\n",
            c.doc,
            c.loc.line,
            c.text
                .chars()
                .take(80)
                .collect::<String>()
                .replace('|', "\\|"),
            c.kind.as_str(),
            v.status.as_str(),
            v.evidence
                .first()
                .map(|e| e.chars().take(100).collect::<String>())
                .unwrap_or_default()
                .replace('|', "\\|")
        ));
    }

    if !report.reverse_drift.is_empty() {
        out.push_str("\n## Reverse drift (code without claims)\n\n");
        for r in &report.reverse_drift {
            let path = store
                .get(r.symbol)
                .map(|s| s.loc.path.clone())
                .unwrap_or_default();
            out.push_str(&format!("- `{}` ({}) — {}\n", r.qual_name, path, r.reason));
        }
    }
    out
}
