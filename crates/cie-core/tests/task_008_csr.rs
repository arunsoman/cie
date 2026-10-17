//! TASK-008 validation sketch — instructor contract, bodies intact.
//! Single documented mapping: `cie_relations` → `cie_core::relations`.

use cie_core::relations::{CsrGraph, RelationGraph, RelKind};
use cie_core::{now_ns, CI_TOLERANCE, SymbolId};

#[test]
fn task_008_csr_build_and_adjacency() {
    let mut g = RelationGraph::new();
    let a = g.add_node(SymbolId(0));
    let b = g.add_node(SymbolId(1));
    let c = g.add_node(SymbolId(2));
    g.add_edge(a, b, RelKind::Calls);
    g.add_edge(a, c, RelKind::DataFlow);
    g.add_edge(b, c, RelKind::Owns);

    let csr = CsrGraph::build_from(&g);
    assert_eq!(csr.offsets.len(), g.node_count() + 1, "§14.3: len = nodes + 1");
    let mut nbrs: Vec<_> = csr.neighbors(0).collect();
    nbrs.sort_by_key(|(t, _)| *t);
    assert_eq!(nbrs, vec![(1, RelKind::Calls), (2, RelKind::DataFlow)]);
    assert_eq!(csr.edge_len(), 3);
}

/// TASK-008 DoD: offsets invariant holds for an empty graph, a single-node
/// graph, and a 1M-edge fixture (fixture used by the perf gate below).
#[test]
fn task_008_csr_offsets_invariants() {
    // Empty graph.
    let g = RelationGraph::new();
    let csr = CsrGraph::build_from(&g);
    assert_eq!(csr.offsets, vec![0], "empty graph: offsets = [0]");
    assert_eq!(csr.node_len(), 0);

    // Single node.
    let mut g1 = RelationGraph::new();
    g1.add_node(SymbolId(0));
    let csr1 = CsrGraph::build_from(&g1);
    assert_eq!(csr1.offsets, vec![0, 0], "one node: offsets = [0, 0]");

    // Roundtrip: every (from, to, kind) of the source appears in the CSR view.
    let mut g2 = RelationGraph::new();
    let a = g2.add_node(SymbolId(7));
    let b = g2.add_node(SymbolId(9));
    g2.add_edge(a, b, RelKind::Throws);
    g2.add_edge(b, a, RelKind::Catches);
    let csr2 = CsrGraph::build_from(&g2);
    let csr_view: std::collections::BTreeSet<(u32, u32, RelKind)> = (0..csr2.node_len() as u32)
        .flat_map(|n| csr2.neighbors(n).map(move |(t, k)| (n, t, k)))
        .collect();
    assert!(csr_view.contains(&(0, 1, RelKind::Throws)));
    assert!(csr_view.contains(&(1, 0, RelKind::Catches)));
}

/// TASK-008 DoD: rkyv roundtrip of the CSR snapshot (FR-30 L2 snapshots).
#[test]
fn task_008_csr_rkyv_roundtrip() {
    let mut g = RelationGraph::new();
    let a = g.add_node(SymbolId(0));
    let b = g.add_node(SymbolId(1));
    g.add_edge(a, b, RelKind::Calls);
    g.add_edge(b, a, RelKind::DependsOn);
    let csr = CsrGraph::build_from(&g);
    let bytes = csr.to_rkyv().expect("rkyv serialize");
    let back = CsrGraph::from_rkyv(&bytes).expect("rkyv deserialize");
    assert_eq!(csr, back, "byte-level roundtrip preserves the freeze");
}

#[test]
#[ignore = "perf: cargo test -- --ignored"]
fn task_008_csr_scan_perf() {
    let csr = fixture_csr(1_000_000); // ~1M edges, built in-test
    let t0 = now_ns();
    let mut seen: u64 = 0;
    for n in 0..csr.node_len() as u32 {
        for _ in csr.neighbors(n) {
            seen += 1;
        }
    }
    let per_edge = (now_ns() - t0) as f64 / seen as f64;
    assert!(
        per_edge <= 20.0 * CI_TOLERANCE,
        "§15.4: ≤ 20 ns/edge × {CI_TOLERANCE}, measured {per_edge:.1} ns"
    );
}

/// ~1M-edge fixture: 100k nodes, each with 10 out-edges (mod targets).
fn fixture_csr(target_edges: usize) -> CsrGraph {
    let nodes = 100_000u32;
    let mut g = RelationGraph::new();
    for i in 0..nodes {
        g.add_node(SymbolId(i));
    }
    let per_node = target_edges / nodes as usize;
    for i in 0..nodes {
        for j in 0..per_node as u32 {
            let to = NodeIndex::new(((i * 7 + j * 13 + 1) % nodes) as usize);
            let from = NodeIndex::new(i as usize);
            g.add_edge(from, to, RelKind::Calls);
        }
    }
    CsrGraph::build_from(&g)
}

use petgraph::graph::NodeIndex;
