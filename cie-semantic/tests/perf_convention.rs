//! TASK-004: the reference `#[perf]` test convention (§15.4, global rule 5).
//!
//! Perf tests are `#[ignore]`-gated, run via `cargo test -- --ignored`, and
//! fail when `measured > target × CI_TOLERANCE` (CI_TOLERANCE = 3.0, §15.4).
//! This file is the canonical example every other perf test follows.

use cie_core::types::{SourceLocation, SymbolId};
use cie_core::{CieError, CI_TOLERANCE};
use cie_semantic::facts::SymbolFact;
use cie_semantic::facts::SymbolStore;
use cie_semantic::LangId;

fn measure_lookup_ns_50k_samples() -> f64 {
    // Build a 50k-symbol store, then measure `get` by id — the §15.4
    // "symbol lookup by id" row (target 50 ns).
    let mut facts = Vec::new();
    for i in 0..50_000u32 {
        facts.push(SymbolFact {
            id: SymbolId(i),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: format!("fn_{i}"),
            qual_name: format!("mod.fn_{i}"),
            loc: SourceLocation::point(0.into(), format!("f{i}.py"), i % 400),
            lang: LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        });
    }
    let store = SymbolStore::build(facts);

    // Warm up index/cache paths.
    for i in 0..1_000u32 {
        let _ = store.get(SymbolId(i));
    }

    let t0 = std::time::Instant::now();
    let samples = 50_000u32;
    let mut sink = 0u64;
    for i in 0..samples {
        let s = store.get(SymbolId(i % 50_000));
        sink ^= s.map(|f| f.id.0 as u64).unwrap_or(0);
    }
    let wall = t0.elapsed().as_nanos() as f64;
    std::hint::black_box(sink);
    wall / samples as f64
}

#[test]
#[ignore = "perf: run with `cargo test -- --ignored` (global rule 5)"]
fn task_004_perf_convention_reference() {
    let target_ns = 50.0; // §15.4: symbol lookup by id
    let measured = measure_lookup_ns_50k_samples();
    assert!(
        measured <= target_ns * CI_TOLERANCE,
        "§15.4 target {target_ns} ns × CI_TOLERANCE {CI_TOLERANCE} exceeded: {measured:.1} ns"
    );
}

#[test]
fn task_004_protocol_and_evidence_invariant() {
    // Sketch adaptation: `RevId` is the §14.1 alias (TASK-002 constraint),
    // so `for_revision(RevId(1))` reads `for_revision(1)` — the only edit.
    let input = cie_core::AnalysisInput::for_revision(1);
    let out =
        cie_core::AnalysisOutput::empty_with_evidence(input.revision, 1_700_000_000_123_456_789);
    assert_eq!(
        out.evidence.rev, input.revision,
        "FR-4: output always carries evidence"
    );
    assert!(out.conclusions.is_empty() && out.derived_records.is_empty());

    let err = CieError::Timeout { deadline_ns: 7 };
    assert!(
        err.is_timeout(),
        "FR-35: timeout is first-class, never mapped to a clean status"
    );
    let _ = cie_core::RevisionId::default(); // alias in scope
}
