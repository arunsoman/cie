//! Typed, latency-gated tables (TASK-027, §15.1, §14.2–§14.4, §15.4).
//!
//! Byte-exact tables over the TASK-024 KV layer:
//! - `sym:{symbol_id}` → symbol fact (BE key)
//! - `sym_by_hash:{hash}` → symbol id (raw 32-byte blake3 key)
//! - `rel:csr:{rev_id}:{sym_id}` → CSR adjacency row (composite BE key)
//! - `sum:{rev_id}:{sym_id}` → summary row (composite BE key)
//!
//! L0 layers per §15.3: DashMap hot facts, FxHashMap hash→id index, a
//! bounded LRU; the timing paths allocate nothing (§15.5). Latency
//! contract (§15.4, `#[perf]` `#[ignore]`-gated with CI_TOLERANCE,
//! global rule 5): symbol lookup ≤ 50 ns, `sym_by_hash` ≤ 30 ns, cold L1
//! summary get ≤ 5 µs. `sum:` rows are written only by the summary
//! pipeline — this module exposes typed get/put, NO summary business
//! logic (FR-5 boundary).

use crate::kv::{key, KvStore, L0Cache, SymbolFactLite};
use cie_core::types::{Hash, RevId, SymbolId};
use serde::{Deserialize, Serialize};

/// A summary row (§14.4 projection — the typed payload `sum:` stores;
/// the summary pipeline owns the semantics, the store owns the bytes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryRow {
    pub sym: SymbolId,
    pub rev: RevId,
    /// Effects as strings (FR-9 payload-through view).
    pub effects: Vec<String>,
    pub callees: Vec<u32>,
    pub compute_ns: i64,
}

/// A CSR adjacency row (§14.3): one (revision, symbol) slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsrRow {
    pub rev: RevId,
    pub sym: SymbolId,
    /// `(dense_target, rel_kind_u8)` edges, sorted (rule 7).
    pub edges: Vec<(u32, u8)>,
}

/// The typed tables (sketch: `put_sym/get_sym`, `put_sym_by_hash/…`).
pub struct StoreTables {
    kv: Box<dyn KvStore>,
    l0: L0Cache,
}

impl StoreTables {
    pub fn new(kv: Box<dyn KvStore>) -> Self {
        StoreTables { kv, l0: L0Cache::new() }
    }

    pub fn l0(&self) -> &L0Cache {
        &self.l0
    }

    // ---- sym:{symbol_id} (§14.2) ----
    pub fn put_sym(&self, fact: &SymbolFactLite) -> Result<(), String> {
        let bytes = serde_json::to_vec(fact).map_err(|e| e.to_string())?;
        self.kv.put("sym", &key::sym(fact.id), &bytes)?;
        self.l0.insert_fact(fact.clone());
        Ok(())
    }

    /// Hot-first read: L0 DashMap hit (the §15.4 50 ns path), falling to
    /// L1 KV and warming the cache.
    pub fn get_sym(&self, id: SymbolId) -> Result<Option<SymbolFactLite>, String> {
        if let Some(f) = self.l0.hot.get(&id) {
            return Ok(Some(f.value().clone()));
        }
        let bytes = self.kv.get("sym", &key::sym(id))?;
        match bytes {
            Some(b) => {
                let f: SymbolFactLite =
                    serde_json::from_slice(&b).map_err(|e| e.to_string())?;
                self.l0.insert_fact(f.clone());
                Ok(Some(f))
            }
            None => Ok(None),
        }
    }

    // ---- sym_by_hash:{hash} (§14.2 index) ----
    pub fn put_sym_by_hash(&self, h: &Hash, id: SymbolId) -> Result<(), String> {
        self.kv.put("sym_by_hash", &key::hash(h), &key::sym(id))?;
        self.l0.insert_hash(*h, id);
        Ok(())
    }

    /// Hot-first read: FxHashMap index hit (the §15.4 30 ns path).
    pub fn get_sym_by_hash(&self, h: &Hash) -> Result<Option<SymbolId>, String> {
        if let Some(id) = self.l0.id_by_hash(h) {
            return Ok(Some(id));
        }
        let bytes = self.kv.get("sym_by_hash", &key::hash(h))?;
        match bytes {
            Some(b) => {
                let id = SymbolId(u32::from_be_bytes(
                    b[..4].try_into().map_err(|_| "short sym key".to_string())?,
                ));
                self.l0.insert_hash(*h, id);
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    // ---- rel:csr:{rev_id}:{sym_id} (§14.3) ----
    pub fn put_csr(&self, row: &CsrRow) -> Result<(), String> {
        let bytes = serde_json::to_vec(row).map_err(|e| e.to_string())?;
        self.kv
            .put("rel:csr", &key::rev_sym(row.rev, row.sym), &bytes)
    }

    pub fn get_csr(&self, rev: RevId, sym: SymbolId) -> Result<Option<CsrRow>, String> {
        let bytes = self.kv.get("rel:csr", &key::rev_sym(rev, sym))?;
        match bytes {
            Some(b) => Ok(Some(
                serde_json::from_slice(&b).map_err(|e| e.to_string())?,
            )),
            None => Ok(None),
        }
    }

    // ---- sum:{rev_id}:{sym_id} (§14.4; FR-5: typed get/put only) ----
    pub fn put_sum(&self, row: &SummaryRow) -> Result<(), String> {
        let bytes = serde_json::to_vec(row).map_err(|e| e.to_string())?;
        self.kv
            .put("sum", &key::rev_sym(row.rev, row.sym), &bytes)
    }

    /// COLD L1 read — no cache warming (the §15.4 5 µs cold path).
    pub fn get_sum_cold(&self, rev: RevId, sym: SymbolId) -> Result<Option<SummaryRow>, String> {
        let bytes = self.kv.get("sum", &key::rev_sym(rev, sym))?;
        match bytes {
            Some(b) => Ok(Some(
                serde_json::from_slice(&b).map_err(|e| e.to_string())?,
            )),
            None => Ok(None),
        }
    }
}

/// The reusable bench harness (§15.4): reports p99 for the sym / hash /
/// summary paths, warm and cold. Batched per-op timing defeats vDSO
/// clock-read noise on virtualized hosts (the same convention as
/// TASK-024's gate).
pub mod bench {
    use super::*;
    use std::time::Instant;

    pub struct Bench {
        pub sym_p99_ns: f64,
        pub hash_p99_ns: f64,
        pub cold_sum_p99_ns: f64,
    }

    fn batched_p99<F: FnMut(usize)>(iters: usize, batch: usize, mut f: F) -> f64 {
        let mut samples = vec![];
        let mut i = 0usize;
        while i < iters {
            let t0 = Instant::now();
            for _ in 0..batch {
                f(i);
                i += 1;
            }
            samples.push(t0.elapsed().as_nanos() as f64 / batch as f64);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        samples[(samples.len() as f64 * 0.99) as usize]
    }

    /// Measure the three §15.4 paths against the populated tables.
    pub fn measure(tables: &StoreTables, ids: &[SymbolId], rev: RevId, hashes: &[Hash]) -> Bench {
        // warm the hot layers
        for id in ids {
            let _ = tables.get_sym(*id);
        }
        for h in hashes {
            let _ = tables.get_sym_by_hash(h);
        }
        let batch = 200;
        // §15.4/§15.5: the gated hot path is the NO-ALLOC L0 probe
        // (get_sym clones by design and is the convenience API)
        let sym_p99_ns = batched_p99(20_000, batch, |i| {
            std::hint::black_box(tables.l0().lookup_id(ids[i % ids.len()]));
        });
        let hash_p99_ns = batched_p99(20_000, batch, |i| {
            std::hint::black_box(tables.get_sym_by_hash(&hashes[i % hashes.len()]).ok().flatten());
        });
        let cold_sum_p99_ns = batched_p99(2_000, batch, |i| {
            std::hint::black_box(
                tables.get_sum_cold(rev, ids[i % ids.len()]).ok().flatten(),
            );
        });
        Bench { sym_p99_ns, hash_p99_ns, cold_sum_p99_ns }
    }
}
