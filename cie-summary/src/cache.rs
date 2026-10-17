//! Summary-reuse cache types (TASK-030): per-file (blake3(file),
//! blake3(dep set)) keys and reuse counters. Persisted inside
//! `RevSnapshot` (see `cie-incremental::snapshot`).

use serde::{Deserialize, Serialize};

/// One file's summary-reuse cache key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryCacheEntry {
    pub path: String,
    pub file_hash: [u8; 32],
    pub deps_hash: [u8; 32],
    /// TASK-056: per-symbol (qual_name -> local-facts blake3) for the
    /// symbol-level dirty worklist.
    #[serde(default)]
    pub symbol_keys: std::collections::BTreeMap<String, [u8; 32]>,
}

/// Summary reuse counters (TASK-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReuseStats {
    pub reused_files: usize,
    pub recomputed_files: usize,
    #[serde(default)]
    pub reused_symbols: usize,
    #[serde(default)]
    pub recomputed_symbols: usize,
}
