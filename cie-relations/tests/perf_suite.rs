//! TASK-027/061 + acc #15: §15.4 CSR adjacency scan gate (L0 relations row)
//! — `#[ignore]`-gated `#[perf]` test (perf convention, TASK-004): fail when
//! `measured > target × CI_TOLERANCE`. Gates bind on release builds.
//! The 1M-edge fixture gate lives in `task_008_csr.rs`.

use cie_core::{now_ns, SymbolId, CI_TOLERANCE};
use cie_relations::{CsrGraph, RelKind, RelationGraph};
use petgraph::graph::NodeIndex;

/// §15.4: CSR adjacency scan ≤ 20 ns/edge (suite fixture: 100k edges).
#[test]
#[ignore = "perf: cargo test --release -- --ignored"]
fn perf_csr_scan() {
    if cfg!(debug_assertions) {
        eprintln!("perf gate skipped on debug build");
        return;
    }
    let nodes = 10_000u32;
    let mut g = RelationGraph::new();
    for i in 0..nodes {
        g.add_node(SymbolId(i));
    }
    for i in 0..nodes {
        for j in 0..10u32 {
            g.add_edge(
                NodeIndex::new(i as usize),
                NodeIndex::new(((i * 7 + j * 13 + 1) % nodes) as usize),
                RelKind::Calls,
            );
        }
    }
    let csr = CsrGraph::build_from(&g);

    // Warm pass.
    let mut warm = 0u64;
    for n in 0..csr.node_len() as u32 {
        for _ in csr.neighbors(n) {
            warm += 1;
        }
    }
    let t0 = now_ns();
    let mut seen = 0u64;
    for n in 0..csr.node_len() as u32 {
        for _ in csr.neighbors(n) {
            seen += 1;
        }
    }
    let per_edge = (now_ns() - t0) as f64 / seen.max(1) as f64;
    std::hint::black_box(warm);
    assert!(
        per_edge <= 20.0 * CI_TOLERANCE,
        "§15.4: CSR scan ≤ 20 ns/edge × {CI_TOLERANCE}, measured {per_edge:.1} ns"
    );
}
