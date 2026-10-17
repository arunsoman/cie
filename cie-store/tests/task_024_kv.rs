//! TASK-024 validation: trees, key codec, registry + backend swap.
//!
//! The identical `kv_suite` runs against every backend — MemKv (always),
//! SledStore (default `backend-sled`), RedbStore (`backend-redb` in CI) —
//! proving the swap needs zero call-site changes. The L0 hot-cache gate
//! (DashMap lookup p99 ≤ 50 ns × CI_TOLERANCE, §15.4) is `#[ignore]`-gated
//! per global rule 5.

use cie_store::kv::{key, kv_suite, L0Cache, MemKv, SymbolFactLite};
use cie_core::types::{hash, SymbolId};
use cie_core::CI_TOLERANCE;

#[test]
fn task_024_trees_key_codec_and_registry_memkv() {
    kv_suite::run_suite(&MemKv::new());
}

#[cfg(feature = "backend-sled")]
#[test]
fn task_024_trees_key_codec_and_registry_sled() {
    let store =
        cie_store::kv::sled_backend::SledStore::open_temporary().expect("open temp store");
    kv_suite::run_suite(&store);
    // typed put/get through the exact §15.1 namespace keys (sketch)
    let fact = SymbolFactLite { id: SymbolId(7), name: "pay".into(), rev: 41 };
    let l0 = L0Cache::new();
    l0.insert_fact(fact.clone());
    assert_eq!(l0.lookup(SymbolId(7)).unwrap().id, fact.id);
}

#[cfg(feature = "backend-redb")]
#[test]
fn task_024_trees_key_codec_and_registry_redb() {
    let store =
        cie_store::kv::redb_backend::RedbStore::open_temporary().expect("open temp redb store");
    kv_suite::run_suite(&store);
}

#[cfg(feature = "backend-sled")]
#[test]
fn task_024_backend_swap_zero_call_sites() {
    // the same call-site code drives both backends through `&dyn KvStore`
    fn roundtrip(store: &dyn cie_store::kv::KvStore) -> Option<Vec<u8>> {
        let h: cie_core::types::Hash = hash(b"swap-key");
        store.put("map", &key::hash(&h), b"v1").unwrap();
        store.get("map", &key::hash(&h)).unwrap()
    }
    let mem = roundtrip(&MemKv::new());
    let sled_store = cie_store::kv::sled_backend::SledStore::open_temporary().unwrap();
    let via_sled = roundtrip(&sled_store);
    assert_eq!(mem, via_sled);
}

#[cfg(feature = "backend-redb")]
#[test]
fn task_024_backend_swap_sled_redb_identical() {
    fn roundtrip(store: &dyn cie_store::kv::KvStore) -> Option<Vec<u8>> {
        let h: cie_core::types::Hash = hash(b"swap-key");
        store.put("map", &key::hash(&h), b"v1").unwrap();
        store.get("map", &key::hash(&h)).unwrap()
    }
    let sled_store = cie_store::kv::sled_backend::SledStore::open_temporary().unwrap();
    let redb_store = cie_store::kv::redb_backend::RedbStore::open_temporary().unwrap();
    assert_eq!(roundtrip(&sled_store), roundtrip(&redb_store));
}

/// §15.4 L0 gate: DashMap hot-fact lookup p99 ≤ 50 ns × CI_TOLERANCE.
/// `#[ignore]`-gated (global rule 5): `cargo test -- --ignored`.
#[test]
#[ignore = "perf: run with `cargo test -- --ignored` (global rule 5)"]
fn task_024_perf_l0_dashmap_lookup_p99() {
    let target_ns = 50.0; // §15.4 L0 lookup
    let l0 = L0Cache::new();
    for i in 0..50_000u32 {
        l0.insert_fact(SymbolFactLite { id: SymbolId(i), name: format!("f{i}"), rev: 1 });
    }
    // warm
    for i in 0..1_000u32 {
        let _ = l0.lookup(SymbolId(i));
    }
    // batched measurement: per-op latency amortizes the vDSO clock-read
    // overhead that dominates single samples on virtualized hosts
    let batch = 100usize;
    let mut samples: Vec<f64> = Vec::with_capacity(500);
    for b in 0..500 {
        let t0 = std::time::Instant::now();
        for i in 0..batch {
            let r = l0.lookup_id(SymbolId(((b * batch + i) % 50_000) as u32));
            std::hint::black_box(&r);
        }
        samples.push(t0.elapsed().as_nanos() as f64 / batch as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p99 = samples[(samples.len() as f64 * 0.99) as usize];
    assert!(
        p99 <= target_ns * CI_TOLERANCE,
        "§15.4 L0 lookup p99 {p99:.1} ns > target {target_ns} × CI_TOLERANCE {CI_TOLERANCE}"
    );
    // hash index path (§15.4: ≤ 30 ns)
    let h: cie_core::types::Hash = hash(b"hot-symbol");
    l0.insert_hash(h, SymbolId(1));
    let idx_target = 30.0;
    let mut idx_samples: Vec<f64> = Vec::with_capacity(500);
    for b in 0..500 {
        let t0 = std::time::Instant::now();
        for _ in 0..batch {
            let r = l0.id_by_hash(&h);
            std::hint::black_box(&r);
        }
        idx_samples.push(t0.elapsed().as_nanos() as f64 / batch as f64);
    }
    idx_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx_p99 = idx_samples[(idx_samples.len() as f64 * 0.99) as usize];
    assert!(
        idx_p99 <= idx_target * CI_TOLERANCE,
        "§15.4 hash→id p99 {idx_p99:.1} ns > target {idx_target} × CI_TOLERANCE"
    );
}
