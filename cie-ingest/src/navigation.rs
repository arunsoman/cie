//! Revision-local navigation artifacts and compatibility repair. A cache hit
//! never reparses; missing artifacts are rebuilt only from verified sources.
use crate::{default_registry, IngestOutcome, RelationGraphStoreExt};
use cie_core::types::{content_hash, RevId, SymbolId};
use cie_incremental::snapshot::{RevSnapshot, SnapshotStoreExt};
use cie_relations::RelationGraph;
use cie_semantic::facts::SymbolStore;
use cie_store::Store;
use std::{collections::BTreeMap, io, path::Path};

type Unresolved = BTreeMap<SymbolId, Vec<String>>;
fn unresolved_path(store: &Store, rev: RevId) -> std::path::PathBuf {
    store
        .root()
        .join("revs")
        .join(rev.to_string())
        .join("unresolved.json")
}
pub fn save_unresolved(store: &Store, rev: RevId, calls: &Unresolved) -> io::Result<()> {
    store.write_json(&unresolved_path(store, rev), calls)
}
pub fn load_unresolved(store: &Store, rev: RevId) -> io::Result<Option<Unresolved>> {
    store.read_json(&unresolved_path(store, rev))
}
fn id_map(old: &SymbolStore, snap: &RevSnapshot) -> BTreeMap<SymbolId, SymbolId> {
    let targets: BTreeMap<_, _> = snap
        .symbols
        .iter()
        .map(|s| {
            (
                (
                    snap.files
                        .get(s.loc.file.0 as usize)
                        .map(String::as_str)
                        .unwrap_or(""),
                    s.qual_name.as_str(),
                    s.loc.start,
                ),
                s.id,
            )
        })
        .collect();
    old.symbols
        .iter()
        .filter_map(|s| {
            targets
                .get(&(
                    old.files.path(s.loc.file),
                    s.qual_name.as_str(),
                    s.loc.start,
                ))
                .map(|id| (s.id, *id))
        })
        .collect()
}
pub fn remap_relations(
    graph: &RelationGraph,
    old: &SymbolStore,
    snap: &RevSnapshot,
) -> RelationGraph {
    let ids = id_map(old, snap);
    let mut out = RelationGraph::new();
    let nodes: BTreeMap<_, _> = snap
        .symbols
        .iter()
        .map(|s| (s.id, out.add_node(s.id)))
        .collect();
    for (a, b, kind) in graph.edge_triples() {
        if let (Some(a), Some(b)) = (
            graph.node_weight(a).and_then(|s| ids.get(&s)),
            graph.node_weight(b).and_then(|s| ids.get(&s)),
        ) {
            out.add_edge(nodes[a], nodes[b], kind);
        }
    }
    out
}
pub fn remap_unresolved(calls: &Unresolved, old: &SymbolStore, snap: &RevSnapshot) -> Unresolved {
    let ids = id_map(old, snap);
    calls
        .iter()
        .filter_map(|(id, names)| ids.get(id).map(|id| (*id, names.clone())))
        .collect()
}
/// Old revisions may lack either artifact. Never reinterpret today's changed
/// files as yesterday's graph: compare each source with the revision's index.
pub fn ensure_navigation(project: &Path, outcome: &mut IngestOutcome) -> io::Result<RelationGraph> {
    let store = Store::open(project)?;
    let graph = store.load_relations(outcome.rev)?;
    let calls = load_unresolved(&store, outcome.rev)?;
    if let (Some(graph), Some(calls)) = (&graph, &calls) {
        outcome.store.unresolved = calls.clone();
        return Ok(graph.clone());
    }
    let index = store
        .load_index()?
        .ok_or_else(|| io::Error::other("navigation unavailable: no revision file index"))?;
    if index.last_rev != outcome.rev {
        return Err(io::Error::other(
            "navigation unavailable: source index belongs to another revision",
        ));
    }
    let mut files = Vec::new();
    for path in &outcome.snapshot.files {
        let rec = index
            .files
            .get(path)
            .ok_or_else(|| io::Error::other("navigation unavailable: missing source hash"))?;
        let src = std::fs::read_to_string(project.join(path))?;
        if content_hash(src.as_bytes()) != rec.hash {
            return Err(io::Error::other(
                "navigation unavailable: source changed since revision",
            ));
        }
        files.push((path.clone(), rec.lang, src));
    }
    let extracted = cie_adapters::extract_project(&files, &default_registry());
    let graph = {
        remap_relations(
            &cie_semantic::graph::relation_graph_from_facts(&extracted.per_file, &extracted.store),
            &extracted.store,
            &outcome.snapshot,
        )
    };
    let calls = calls.unwrap_or_else(|| {
        let mut calls: Unresolved = BTreeMap::new();
        for facts in &extracted.per_file {
            for call in &facts.calls {
                if matches!(call.callee, cie_semantic::facts::Callee::Unresolved(_)) {
                    calls
                        .entry(call.caller)
                        .or_default()
                        .push(call.callee_text.clone());
                }
            }
        }
        remap_unresolved(&calls, &extracted.store, &outcome.snapshot)
    });
    store.save_relations(outcome.rev, &graph)?;
    save_unresolved(&store, outcome.rev, &calls)?;
    outcome.store.unresolved = calls;
    Ok(graph)
}

/// Preserve source coordinates across reloads without reopening current files.
pub fn snapshot_files(
    snap: &RevSnapshot,
    source: &cie_core::types::FileTable,
) -> cie_core::types::FileTable {
    let mut files = cie_core::types::FileTable::new();
    for path in &snap.files {
        let id = files.intern(path.clone());
        if let Some(i) = source.paths.iter().position(|p| p == path) {
            files.line_starts[id.0 as usize] =
                source.line_starts.get(i).cloned().unwrap_or_default();
        }
    }
    files
}
pub fn load_files(
    store: &Store,
    snap: &RevSnapshot,
    project: &Path,
) -> io::Result<cie_core::types::FileTable> {
    let path = store
        .root()
        .join("revs")
        .join(snap.rev.to_string())
        .join("file-table.json");
    if let Some(files) = store.read_json::<cie_core::types::FileTable>(&path)? {
        return Ok(files);
    }
    let index = store.load_index()?;
    let mut files = cie_core::types::FileTable::new();
    for path in &snap.files {
        // Legacy stores can recover line indices only from byte-identical
        // sources. Missing/modified sources keep a known path and unknown line.
        let valid = index
            .as_ref()
            .filter(|i| i.last_rev == snap.rev)
            .and_then(|i| i.files.get(path));
        let bytes = std::fs::read(project.join(path)).ok();
        match (valid, bytes) {
            (Some(rec), Some(bytes)) if content_hash(&bytes) == rec.hash => {
                files.intern_with_source(path, &bytes);
            }
            _ => {
                files.intern(path);
            }
        }
    }
    Ok(files)
}

#[derive(Debug)]
pub struct TraceWalk {
    pub hops: Vec<Vec<SymbolId>>,
    pub truncated: bool,
    pub limit_reason: Option<&'static str>,
}
/// Shared bounded BFS for CLI and future protocol consumers. Budgets cover
/// graph traversal, not artifact loading. Never silently discard a frontier.
pub fn trace(
    graph: &RelationGraph,
    symbol: SymbolId,
    kind: Option<cie_relations::RelKind>,
    depth: usize,
    reverse: bool,
    budget: std::time::Duration,
) -> TraceWalk {
    let start_time = std::time::Instant::now();
    let mut out = TraceWalk {
        hops: Vec::new(),
        truncated: false,
        limit_reason: None,
    };
    let Some(start) = graph.find_node(symbol) else {
        return out;
    };
    let mut visited = std::collections::BTreeSet::from([symbol]);
    let mut frontier = vec![start];
    let mut work = 0usize;
    for _ in 0..depth {
        let mut next = Vec::new();
        let mut hop = Vec::new();
        for idx in &frontier {
            // At most the remaining edge budget plus one sentinel edge is
            // materialized, even for a pathological high-degree node.
            let remaining = 100_000usize.saturating_sub(work);
            let edges: Vec<_> = if reverse {
                graph.edges_to(*idx).take(remaining + 1).collect()
            } else {
                graph.edges_from(*idx).take(remaining + 1).collect()
            };
            for (other, relation) in edges {
                if start_time.elapsed() >= budget || work >= 100_000 || visited.len() >= 10_000 {
                    if !hop.is_empty() {
                        out.hops.push(hop);
                    }
                    out.truncated = true;
                    out.limit_reason = Some(if start_time.elapsed() >= budget {
                        "time_budget"
                    } else {
                        "work_budget"
                    });
                    return out;
                }
                work += 1;
                if kind.is_some_and(|k| k != relation) {
                    continue;
                }
                if let Some(id) = graph.node_weight(other) {
                    if visited.insert(id) {
                        next.push(other);
                        hop.push(id);
                    }
                }
            }
        }
        if next.is_empty() {
            frontier.clear();
            break;
        }
        out.hops.push(hop);
        frontier = next;
    }
    for idx in frontier {
        let remaining = 100_000usize.saturating_sub(work);
        let edges: Vec<_> = if reverse {
            graph.edges_to(idx).take(remaining + 1).collect()
        } else {
            graph.edges_from(idx).take(remaining + 1).collect()
        };
        for (other, relation) in edges {
            if start_time.elapsed() >= budget || work >= 100_000 {
                out.truncated = true;
                out.limit_reason = Some("time_or_work_budget");
                return out;
            }
            work += 1;
            if kind.is_none_or(|k| k == relation)
                && graph
                    .node_weight(other)
                    .is_some_and(|id| !visited.contains(&id))
            {
                out.truncated = true;
                out.limit_reason = Some("depth");
                return out;
            }
        }
    }
    out
}
