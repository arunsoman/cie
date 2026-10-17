//! TASK-027/061 + acc #15: the §15.4 latency-target subset that applies to
//! SHIPPED subsystems, as `#[ignore]`-gated `#[perf]` tests (perf
//! convention, TASK-004): fail when `measured > target × CI_TOLERANCE`.
//! Gates bind on release builds (`cargo test --release -- --ignored`).
//!
//! This file: interner lookup ≤ 50 ns (§15.5 no-alloc hot path).
//! Symbol lookup lives in `cie-semantic/tests/perf_suite.rs`; CSR adjacency
//! scan in `cie-relations/tests/perf_suite.rs`. The full latency-gated
//! table suite (TASK-027 complete form: hash-index, summary tables) lands
//! with the L1 store in v1.0.

use cie_core::{now_ns, CI_TOLERANCE};

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
