//! TASK-027/061 + acc #15: the §15.4 latency-target subset that applies to
//! SHIPPED subsystems, as `#[ignore]`-gated `#[perf]` tests (perf
//! convention, TASK-004): fail when `measured > target × CI_TOLERANCE`.
//! Gates bind on release builds (`cargo test --release -- --ignored`).
//!
//! Shipped subset: symbol lookup ≤ 50 ns (§15.4 row 1), CSR adjacency scan
//! ≤ 20 ns (§15.4 row for L0 relations — the 1M-edge fixture lives in
//! `task_008_csr.rs`), interner lookup ≤ 50 ns (§15.5 no-alloc hot path).
//! The full latency-gated table suite (TASK-027 complete form: hash-index,
//! summary tables) lands with the L1 store in v1.0.

use cie_core::relations::{CsrGraph, RelKind, RelationGraph};
use cie_core::{now_ns, CI_TOLERANCE, SymbolId};
use petgraph::graph::NodeIndex;

fn build_store(n: u32) -> cie_core::facts::SymbolStore {
    let mut facts = Vec::new();
    for i in 0..n {
        facts.push(cie_core::facts::SymbolFact {
            id: SymbolId(i),
            kind: cie_core::facts::SymbolKind::Function,
            name: format!("fn_{i}"),
            qual_name: format!("mod_{i}.fn_{i}"),
            loc: cie_core::types::SourceLocation::point(
                0.into(),
                format!("f{i}.py"),
                i % 400,
            ),
            lang: cie_core::LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        });
    }
    cie_core::facts::SymbolStore::build(facts)
}

/// §15.4: symbol lookup by id ≤ 50 ns.
#[test]
#[ignore = "perf: cargo test --release -- --ignored"]
fn perf_symbol_lookup_by_id() {
    if cfg!(debug_assertions) {
        eprintln!("perf gate skipped on debug build");
        return;
    }
    let store = build_store(50_000);
    for i in 0..1_000u32 {
        let _ = store.get(SymbolId(i));
    }
    let samples = 200_000u32;
    let t0 = now_ns();
    let mut sink = 0u64;
    for i in 0..samples {
        sink ^= store.get(SymbolId(i % 50_000)).map(|s| s.id.0 as u64).unwrap_or(0);
    }
    let per_lookup = (now_ns() - t0) as f64 / samples as f64;
    std::hint::black_box(sink);
    assert!(
        per_lookup <= 50.0 * CI_TOLERANCE,
        "§15.4: symbol lookup ≤ 50 ns × {CI_TOLERANCE}, measured {per_lookup:.1} ns"
    );
}

/// §15.4: CSR adjacency scan ≤ 20 ns/edge (suite fixture: 100k edges; the
/// 1M-edge gate lives in task_008_csr.rs).
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

/// §15.5/§15.4: interner lookup — no allocation, sub-50 ns.
#[test]
#[ignore = "perf: cargo test --release -- --ignored"]
fn perf_interner_lookup() {
    if cfg!(debug_assertions) {
        eprintln!("perf gate skipped on debug build");
        return;
    }
    let mut names = cie_core::StrInterner::new();
    for i in 0..10_000u32 {
        names.intern(&format!("sym_{i}"));
    }
    // Warm.
    for i in 0..1_000u32 {
        let _ = names.lookup(&format!("sym_{i}"));
    }
    let samples = 200_000u32;
    let t0 = now_ns();
    let mut sink = 0u32;
    for i in 0..samples {
        sink ^= names
            .lookup(&format!("sym_{}", i % 10_000))
            .map(|h| h.raw())
            .unwrap_or(u32::MAX);
    }
    let per_lookup = (now_ns() - t0) as f64 / samples as f64;
    std::hint::black_box(sink);
    assert!(
        per_lookup <= 50.0 * CI_TOLERANCE,
        "§15.4/§15.5: interner lookup ≤ 50 ns × {CI_TOLERANCE}, measured {per_lookup:.1} ns"
    );
}
