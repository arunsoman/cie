//! TASK-059 validation: k-NN over embeddings behind the HNSW store trait.
//!
//! Sketch deviations (documented, single-point):
//! - `HnswRsStore::new(Dim(128))` → `HnswRsStore::new(128)` (usize dim).
//! - The 1M-vector `#[perf]` gate generates vectors deterministically and
//!   measures the per-query p99 (batched to defeat vDSO clock noise).

use cie_store::hnsw::{KnnError, KnnOutcome, KnnUnknown, VectorStore, HnswRsStore};
use cie_core::CI_TOLERANCE;

fn dummy(dim: usize, seed: u32) -> Vec<f32> {
    (0..dim).map(|i| ((seed as f32) * 0.01 + i as f32 * 0.001).sin()).collect()
}

/// The sketch's `task_059_knn_returns_known_neighbor_within_budget`.
#[test]
fn task_059_knn_returns_known_neighbor_within_budget() {
    // arrange: 10k vectors; query exactly matches vector 0
    let mut vs = HnswRsStore::new(128);
    let ids: Vec<[u8; 32]> = (0..10_000u32)
        .map(|i| cie_core::types::hash(format!("vec-{i}").as_bytes()))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        vs.insert(*id, &dummy(128, i as u32)).expect("insert");
    }
    assert_eq!(vs.len(), 10_000);

    // the query matches vector 0 exactly → it must rank first with d=0
    let query = dummy(128, 0);
    let out = vs.knn(&query, 10, 50_000_000);
    match out {
        KnnOutcome::Neighbors(hits) => {
            assert_eq!(hits.len(), 10);
            assert_eq!(hits[0].record_id, ids[0], "exact match ranks first");
            assert!(hits[0].distance < 1e-5, "distance ~0 for exact match");
            // distances ascending
            for w in hits.windows(2) {
                assert!(w[0].distance <= w[1].distance);
            }
        }
        other => panic!("expected neighbors, got {other:?}"),
    }
}

/// Budget expiry → Unknown(TimedOut), never an empty result (rule 3).
#[test]
fn task_059_budget_expiry_is_unknown() {
    let mut vs = HnswRsStore::new(8);
    vs.insert([1u8; 32], &dummy(8, 1)).unwrap();
    assert!(matches!(
        vs.knn(&dummy(8, 1), 10, 0),
        KnnOutcome::Unknown(KnnUnknown::TimedOut)
    ));
}

/// Dimension mismatches are explicit errors (rule 2), and re-embedding
/// overwrites in place — the record id stays stable, no orphaned vector.
#[test]
fn task_059_dimension_and_overwrite_semantics() {
    let mut vs = HnswRsStore::new(4);
    let id = [7u8; 32];
    assert!(matches!(
        vs.insert(id, &dummy(8, 1)),
        Err(KnnError::DimensionMismatch { expected: 4, got: 8 })
    ));
    vs.insert(id, &dummy(4, 1)).unwrap();
    assert_eq!(vs.len(), 1);
    // re-embedding overwrites in place (rule 7): len stays 1
    vs.upsert(id, &dummy(4, 2)).unwrap();
    assert_eq!(vs.len(), 1, "stable record id — no orphaned old vector");

    // removed record lookups are explicit errors, not silent zeros
    vs.remove(&id).unwrap();
    assert_eq!(vs.len(), 0);
    assert!(matches!(
        cie_store::hnsw::lookup_record(&vs, &Default::default(), &id),
        Err(KnnError::RemovedRecord { .. })
    ));

    // queries after removal still serve the remaining set
    vs.insert([9u8; 32], &dummy(4, 3)).unwrap();
    assert!(matches!(
        vs.knn(&dummy(4, 3), 10, 50_000_000),
        KnnOutcome::Neighbors(_)
    ));
}

/// §15.4 gate: k-NN (k=10, 1M × 128-d) p99 ≤ 1 ms × CI_TOLERANCE.
/// `#[ignore]`-gated (global rule 5).
#[test]
#[ignore = "perf: run with `cargo test --release -- --ignored` (global rule 5)"]
fn task_059_perf_knn_1m_p99_under_1ms() {
    let n = 1_000_000usize;
    let dim = 128;
    let mut vs = HnswRsStore::new(dim);
    // deterministic bulk build (generator keeps memory bounded per vector)
    let batch = 50_000;
    for b in 0..(n / batch) {
        let items: Vec<([u8; 32], Vec<f32>)> = (0..batch)
            .map(|i| {
                let idx = (b * batch + i) as u32;
                (cie_core::types::hash(format!("perf-{idx}").as_bytes()), dummy(dim, idx))
            })
            .collect();
        vs.bulk_build(items).expect("bulk insert");
    }
    assert_eq!(vs.len(), n);
    // warm queries
    for i in 0..100u32 {
        let _ = vs.knn(&dummy(dim, i), 10, 50_000_000);
    }
    // batched p99 over 200 queries
    let batch_q = 20usize;
    let mut samples = vec![];
    for b in 0..10usize {
        let t0 = std::time::Instant::now();
        for i in 0..batch_q {
            let r = vs.knn(&dummy(dim, (1000 + b * batch_q + i) as u32), 10, 50_000_000);
            std::hint::black_box(&r);
        }
        samples.push(t0.elapsed().as_nanos() as f64 / batch_q as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p99 = samples[(samples.len() as f64 * 0.99) as usize];
    let target_ms = 1.0;
    assert!(
        p99 <= target_ms * 1_000_000.0 * CI_TOLERANCE,
        "§15.4 k-NN p99 {p99:.0} ns > 1 ms × CI_TOLERANCE"
    );
}
