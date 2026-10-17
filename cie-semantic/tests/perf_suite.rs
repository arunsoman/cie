//! TASK-027/061 + acc #15: §15.4 symbol lookup gate (row 1) —
//! `#[ignore]`-gated `#[perf]` test (perf convention, TASK-004): fail when
//! `measured > target × CI_TOLERANCE`. Gates bind on release builds.

use cie_core::{now_ns, SymbolId, CI_TOLERANCE};
use cie_semantic::facts::SymbolStore;

fn build_store(n: u32) -> SymbolStore {
    let mut facts = Vec::new();
    for i in 0..n {
        facts.push(cie_semantic::facts::SymbolFact {
            id: SymbolId(i),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: format!("fn_{i}"),
            qual_name: format!("mod_{i}.fn_{i}"),
            loc: cie_core::types::SourceLocation::point((i % 400).into(), 0),
            lang: cie_semantic::LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        });
    }
    SymbolStore::build(facts)
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
        sink ^= store
            .get(SymbolId(i % 50_000))
            .map(|s| s.id.0 as u64)
            .unwrap_or(0);
    }
    let per_lookup = (now_ns() - t0) as f64 / samples as f64;
    std::hint::black_box(sink);
    assert!(
        per_lookup <= 50.0 * CI_TOLERANCE,
        "§15.4: symbol lookup ≤ 50 ns × {CI_TOLERANCE}, measured {per_lookup:.1} ns"
    );
}
