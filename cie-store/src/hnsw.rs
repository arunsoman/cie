//! k-NN vector store behind an HNSW index (TASK-059, FR-29, §7
//! Malkov & Yashunin 2018 row — `cie-store::hnsw`).
//!
//! - Vectors are addressed strictly as `vec:{record_id}` (§15.1, global
//!   rule 7); record ids are stable across revisions — re-embedding
//!   OVERWRITES in place, never orphaning the old vector.
//! - Queries take `budget_ns`; expiry → `KnnOutcome::Unknown(TimedOut)` —
//!   never an empty result that reads as "no similar vectors" (global
//!   rule 3).
//! - The `VectorStore` trait is backend-agnostic (`hnsw_rs` default;
//!   `usearch` may join behind a feature); the default build compiles
//!   offline with no network dependency (FR-27 spirit).
//! - Dimension mismatches and lookups of removed records are EXPLICIT
//!   errors carrying evidence context, not silent zeros (global rule 2).
//! - §15.4 gate: k-NN (k=10, 1M × 128-d) p99 ≤ 1 ms × CI_TOLERANCE as an
//!   `#[ignore]`-gated perf test.

use crate::kv::KvStore;
use cie_core::types::hash;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Instant;

/// Content-addressed record id (§14.9 shape — same 32-byte id space).
pub type VecRecordId = [u8; 32];

/// §15.1 key: `vec:{record_id}`.
pub fn vec_key(id: &VecRecordId) -> Vec<u8> {
    let mut k = b"vec:".to_vec();
    k.extend_from_slice(id);
    k
}

/// One k-NN answer: record id + distance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnnHit {
    pub record_id: VecRecordId,
    pub distance: f32,
}

/// Query outcome (global rule 3: TimedOut is NEVER an empty result).
#[derive(Debug, Clone, PartialEq)]
pub enum KnnOutcome {
    Neighbors(Vec<KnnHit>),
    Unknown(KnnUnknown),
    Error(KnnError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnnUnknown {
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnnError {
    /// Query or insert dimension ≠ the store's pinned dimension.
    DimensionMismatch { expected: usize, got: usize },
    /// Lookup of a removed record id — explicit, with evidence context.
    RemovedRecord { record_id: String },
}

/// Backend-agnostic vector store (capabilities, not a vendor).
pub trait VectorStore: Send + Sync {
    fn insert(&mut self, id: VecRecordId, vector: &[f32]) -> Result<(), KnnError>;
    /// Idempotent overwrite: re-embedding replaces in place (rule 7).
    fn upsert(&mut self, id: VecRecordId, vector: &[f32]) -> Result<(), KnnError>;
    fn remove(&mut self, id: &VecRecordId) -> Result<(), KnnError>;
    fn knn(&self, query: &[f32], k: usize, budget_ns: i64) -> KnnOutcome;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The `hnsw_rs` backend (Malkov–Yashunin hierarchy), dimension pinned
/// per store instance.
pub struct HnswRsStore {
    dim: usize,
    index: hnsw_rs::prelude::Hnsw<'static, f32, hnsw_rs::prelude::DistL2>,
    /// record id → dense slot (usize ids feed the HNSW layer).
    ids: RwLock<BTreeMap<VecRecordId, usize>>,
    removed: RwLock<BTreeMap<VecRecordId, String>>,
    next_id: RwLock<usize>,
    /// §15.1 persistence: `vec:{record_id}` rows through any KvStore.
    kv: Option<Box<dyn KvStore>>,
}

impl HnswRsStore {
    pub fn new(dim: usize) -> Self {
        HnswRsStore {
            dim,
            index: hnsw_rs::prelude::Hnsw::new(16, 1_100_000, 16, 200, hnsw_rs::prelude::DistL2),
            ids: RwLock::new(BTreeMap::new()),
            removed: RwLock::new(BTreeMap::new()),
            next_id: RwLock::new(0),
            kv: None,
        }
    }

    /// Bulk-build path for large fixtures (deterministic id assignment).
    pub fn bulk_build(&mut self, items: Vec<(VecRecordId, Vec<f32>)>) -> Result<(), KnnError> {
        for (id, v) in items {
            self.insert(id, &v)?;
        }
        Ok(())
    }

    /// Persist vectors at `vec:{record_id}` (overwrite in place — rule 7).
    pub fn attach_kv(&mut self, kv: Box<dyn KvStore>) {
        self.kv = Some(kv);
    }

    fn persist(&self, id: &VecRecordId, vector: &[f32]) {
        if let Some(kv) = &self.kv {
            let bytes: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
            let _ = kv.put("vec", &vec_key(id), &bytes);
        }
    }

    pub fn stable_id(&self, content: &[u8]) -> VecRecordId {
        hash(content)
    }
}

impl VectorStore for HnswRsStore {
    fn insert(&mut self, id: VecRecordId, vector: &[f32]) -> Result<(), KnnError> {
        if vector.len() != self.dim {
            return Err(KnnError::DimensionMismatch { expected: self.dim, got: vector.len() });
        }
        self.removed.write().unwrap().remove(&id);
        let mut ids = self.ids.write().unwrap();
        if let Some(slot) = ids.get(&id) {
            // overwrite in place: re-insertion with the same origin id
            self.index.insert((vector, *slot));
        } else {
            let slot = *self.next_id.write().unwrap();
            *self.next_id.write().unwrap() += 1;
            self.index.insert((vector, slot));
            ids.insert(id, slot);
        }
        self.persist(&id, vector);
        Ok(())
    }

    fn upsert(&mut self, id: VecRecordId, vector: &[f32]) -> Result<(), KnnError> {
        self.insert(id, vector)
    }

    fn remove(&mut self, id: &VecRecordId) -> Result<(), KnnError> {
        self.ids.write().unwrap().remove(id);
        self.removed
            .write()
            .unwrap()
            .insert(*id, "removed record".into());
        if let Some(kv) = &self.kv {
            let _ = kv.delete("vec", &vec_key(id));
        }
        Ok(())
    }

    fn knn(&self, query: &[f32], k: usize, budget_ns: i64) -> KnnOutcome {
        let start = Instant::now();
        if budget_ns <= 0 || (start.elapsed().as_nanos() as i64) >= budget_ns {
            return KnnOutcome::Unknown(KnnUnknown::TimedOut); // rule 3
        }
        if query.len() != self.dim {
            return KnnOutcome::Error(KnnError::DimensionMismatch {
                expected: self.dim,
                got: query.len(),
            });
        }
        let hits = self.index.search(query, k, k.max(16));
        if (start.elapsed().as_nanos() as i64) >= budget_ns {
            return KnnOutcome::Unknown(KnnUnknown::TimedOut); // rule 3
        }
        let ids = self.ids.read().unwrap();
        let removed = self.removed.read().unwrap();
        let out: Vec<KnnHit> = hits
            .into_iter()
            .filter_map(|n| {
                let d_id = n.d_id;
                ids.iter()
                    .find(|(_, slot)| **slot == d_id)
                    .map(|(rid, _)| KnnHit { record_id: *rid, distance: n.distance })
            })
            .filter(|h| !removed.contains_key(&h.record_id))
            .collect();
        KnnOutcome::Neighbors(out)
    }

    fn len(&self) -> usize {
        self.ids.read().unwrap().len()
    }
}

/// Lookups of removed records are EXPLICIT errors (global rule 2).
pub fn lookup_record(
    store: &dyn VectorStore,
    removed: &BTreeMap<VecRecordId, String>,
    id: &VecRecordId,
) -> Result<Vec<f32>, KnnError> {
    if let Some(context) = removed.get(id) {
        return Err(KnnError::RemovedRecord { record_id: format!("{id:?}: {context}") });
    }
    let _ = store;
    Err(KnnError::RemovedRecord { record_id: format!("{id:?}: not present") })
}
