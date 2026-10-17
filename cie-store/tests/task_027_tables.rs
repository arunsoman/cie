//! TASK-027 validation: symbol, hash-index, CSR and summary tables with
//! latency gates (§15.1 / §15.4).
//!
//! Sketch deviations (documented, single-point):
//! - `blake3_hash(b"pay")` → `cie_core::types::hash` (the crate's blake3
//!   wrapper).
//! - `CsrGraph` rows store the (rev, sym) SLICE (`CsrRow`) — the full
//!   graph freeze is TASK-008's `CsrGraph`; this is the §15.1 table view.
//! - The bench harness reports batched p99 (vDSO-noise-proof timing).

use cie_store::kv::{key, kv_suite, MemKv};
use cie_store::tables::{bench, CsrRow, StoreTables, SummaryRow};
use cie_store::kv::{L0Cache, SymbolFactLite};
use cie_core::types::{hash, RevId, SymbolId};
use cie_core::CI_TOLERANCE;

const REV: RevId = 42;

fn fact(id: SymbolId, name: &str) -> SymbolFactLite {
    SymbolFactLite { id, name: name.into(), rev: REV }
}

/// The sketch's `task_027_symbol_index_csr_and_summary_tables`.
#[test]
fn task_027_symbol_index_csr_and_summary_tables() {
    let tables = StoreTables::new(Box::new(MemKv::new()));
    let sym = SymbolId(7);
    let f = fact(sym, "pay");
    let h = hash(b"pay");
    let csr = CsrRow {
        rev: REV,
        sym,
        edges: vec![(9, 0), (11, 3)], // Calls, DataFlow discriminants
    };
    let sum = SummaryRow {
        sym,
        rev: REV,
        effects: vec!["db_read".into()],
        callees: vec![9, 11],
        compute_ns: 1_000,
    };

    // write through the four §15.1 tables at exact keys
    tables.put_sym(&f).expect("sym:{symbol_id}");
    tables.put_sym_by_hash(&h, sym).expect("sym_by_hash:{hash}");
    tables.put_csr(&csr).expect("rel:csr:{rev}:{sym}");
    tables.put_sum(&sum).expect("sum:{rev}:{sym}");

    // typed reads round-trip
    assert_eq!(tables.get_sym(sym).unwrap().unwrap().name, "pay");
    assert_eq!(tables.get_sym_by_hash(&h).unwrap().unwrap(), sym);
    assert_eq!(tables.get_csr(REV, sym).unwrap().unwrap(), csr);
    assert_eq!(tables.get_sum_cold(REV, sym).unwrap().unwrap(), sum);

    // CsrGraph round-trip preserves offsets/edges/kind across a
    // 3-revision fixture (each revision stores its own adjacency slice)
    for rev in [1u64, 2, 3] {
        let row = CsrRow { rev, sym, edges: vec![(rev as u32, 0)] };
        tables.put_csr(&row).unwrap();
        assert_eq!(tables.get_csr(rev, sym).unwrap().unwrap(), row);
    }
}

/// The KV-layer suite still passes under the tables (TASK-024 base).
#[test]
fn task_027_kv_suite_still_green() {
    kv_suite::run_suite(&MemKv::new());
    // and the BE codec keeps numeric order (range-scan correctness)
    assert!(key::u64_be(9) < key::u64_be(10));
}

/// §15.4 latency gates: sym ≤ 50 ns, sym_by_hash ≤ 30 ns (hot paths),
/// cold L1 summary get ≤ 5 µs — `#[perf]` `#[ignore]`-gated (rule 5).
#[test]
#[ignore = "perf: run with `cargo test --release -- --ignored` (global rule 5)"]
fn task_027_perf_latency_gates() {
    let sled = cie_store::kv::sled_backend::SledStore::open_temporary().unwrap();
    let tables = StoreTables::new(Box::new(sled));
    let n = 20_000u32;
    let ids: Vec<SymbolId> = (0..n).map(SymbolId).collect();
    let hashes: Vec<cie_core::types::Hash> = (0..n)
        .map(|i| cie_core::types::hash(format!("sym-{i}").as_bytes()))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        tables.put_sym(&fact(*id, &format!("fn_{i}"))).unwrap();
        tables.put_sym_by_hash(&hashes[i], *id).unwrap();
        tables
            .put_sum(&SummaryRow {
                sym: *id,
                rev: REV,
                effects: vec![],
                callees: vec![],
                compute_ns: 1,
            })
            .unwrap();
    }

    let b = bench::measure(&tables, &ids, REV, &hashes);
    assert!(
        b.sym_p99_ns <= 50.0 * CI_TOLERANCE,
        "§15.4 symbol lookup p99 {:.1} ns > 50 × {}",
        b.sym_p99_ns,
        CI_TOLERANCE
    );
    assert!(
        b.hash_p99_ns <= 30.0 * CI_TOLERANCE,
        "§15.4 sym_by_hash p99 {:.1} ns > 30 × {}",
        b.hash_p99_ns,
        CI_TOLERANCE
    );
    assert!(
        b.cold_sum_p99_ns <= 5_000.0 * CI_TOLERANCE,
        "§15.4 cold summary get p99 {:.1} ns > 5 µs × {}",
        b.cold_sum_p99_ns,
        CI_TOLERANCE
    );
}
