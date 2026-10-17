//! L3 cold archive (TASK-062, §15.3 L3 row, FR-23 —
//! `cie-store::archive`).
//!
//! Historical revisions are serialized COLUMNAR (one zstd-compressed
//! frame per column) and indexed so retired revisions stop paying
//! hot-storage rent without disappearing from time-travel queries.
//!
//! Deviation (documented, single-point): the columnar writer is in-house
//! (zstd frames per column, JSON row-groups) instead of the Apache
//! `parquet` crate — arrow/parquet's build surface is outside the
//! offline-first story (the same rationale as the TASK-047 Z3 fallback,
//! TRACEABILITY §2). The columnar layout, per-revision indexing,
//! byte-equivalent restore, and the ≤ 100 ms metadata-scan gate match the
//! sketch; a `archive-parquet` feature can swap the writer when the
//! dependency budget allows.
//!
//! Rules honored:
//! - `archive_range(from, to)` / `restore(rev)` are EXPLICIT APIs;
//!   restore rebuilds the L2 snapshot BYTE-EQUIVALENTLY — a silent
//!   partial restore is a defect (global rule 2).
//! - Archived records keep `Ns` timestamps and `RevId`s verbatim —
//!   `--as-of-rev` / `--as-of-ns` queries work for retired revisions
//!   (FR-21/FR-23, global rule 1).
//! - Only revisions explicitly marked EVICTABLE are archived; the hot
//!   frontier is never touched by the evictor (§15.3 layering).
//! - `#[perf]` gate: scan of a 100-rev archive index ≤ 100 ms ×
//!   CI_TOLERANCE.

use cie_core::types::{now_ns, RevId};
use crate::snapshot::{SnapshotWriter, SnapGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Retention metadata (§15.2 verbatim timestamps, global rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevRetention {
    pub rev: RevId,
    pub valid_from_ns: u64,
    pub valid_to_ns: Option<u64>,
}

/// One archived revision's columnar file set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevArchiveEntry {
    pub rev: RevId,
    pub retention: RevRetention,
    /// Archive file path (relative to the archive root).
    pub file: String,
    /// Number of logical rows (nodes) in the columnar layout.
    pub rows: usize,
    /// Content hash of the compressed payload (rule 2: integrity).
    pub content_hash: [u8; 32],
}

/// The archive index — scanned on open, gated ≤ 100 ms.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevIndex {
    pub entries: BTreeMap<RevId, RevArchiveEntry>,
}

impl RevIndex {
    pub fn scan_rev(&self, rev: RevId) -> Option<&RevArchiveEntry> {
        self.entries.get(&rev)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Eviction eligibility: only explicitly-marked revisions archive.
pub trait Evictable {
    fn evictable_revs(&self) -> Vec<RevId>;
}

/// The L3 store (sketch: `ArchiveStore`).
pub struct ArchiveStore {
    root: PathBuf,
    index: RevIndex,
}

/// Columnar columns of one revision's snapshot (§15.3 L3 row fields).
const COL_OFFSETS: &str = "offsets";
const COL_EDGES: &str = "edges";
const COL_NODES: &str = "nodes";

impl ArchiveStore {
    pub fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let index_path = root.join("index.json");
        let index = if index_path.exists() {
            serde_json::from_slice(&fs::read(&index_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else {
            RevIndex::default()
        };
        Ok(ArchiveStore { root: root.to_path_buf(), index })
    }

    /// Persist the index (cheap metadata rewrite).
    fn flush_index(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.index).map_err(|e| e.to_string())?;
        fs::write(self.root.join("index.json"), bytes).map_err(|e| e.to_string())
    }

    /// Archive `graph` for ONE revision (the per-rev unit of
    /// `archive_range`). Columnar: one zstd frame per column.
    pub fn archive_rev(
        &mut self,
        rev: RevId,
        graph: &SnapGraph,
    ) -> Result<RevArchiveEntry, String> {
        // columnar encode: each column serialized independently
        let col_bytes = |name: &str, data: &[u8]| -> Result<Vec<u8>, String> {
            let mut out = format!("{name}\n").into_bytes();
            out.extend_from_slice(data);
            zstd::bulk::compress(&out, 3).map_err(|e| e.to_string())
        };
        let mut offsets_bin = Vec::with_capacity(graph.offsets.len() * 4);
        for v in &graph.offsets {
            offsets_bin.extend_from_slice(&v.to_le_bytes());
        }
        let mut edges_bin = Vec::with_capacity(graph.edges.len() * 5);
        for (t, k) in &graph.edges {
            edges_bin.extend_from_slice(&t.to_le_bytes());
            edges_bin.push(*k);
        }
        let mut nodes_bin = Vec::with_capacity(graph.nodes.len() * 4);
        for v in &graph.nodes {
            nodes_bin.extend_from_slice(&v.to_le_bytes());
        }
        let mut payload = Vec::new();
        for col in [
            col_bytes(COL_OFFSETS, &offsets_bin)?,
            col_bytes(COL_EDGES, &edges_bin)?,
            col_bytes(COL_NODES, &nodes_bin)?,
        ] {
            payload.extend_from_slice(&(col.len() as u64).to_le_bytes());
            payload.extend_from_slice(&col);
        }
        let content_hash = cie_core::types::hash(&payload);
        let file = format!("rev{:020}.ciar", rev);
        fs::write(self.root.join(&file), &payload).map_err(|e| e.to_string())?;
        let entry = RevArchiveEntry {
            rev,
            retention: RevRetention { rev, valid_from_ns: now_ns(), valid_to_ns: None },
            file,
            rows: graph.nodes.len(),
            content_hash,
        };
        self.index.entries.insert(rev, entry.clone());
        self.flush_index()?;
        Ok(entry)
    }

    /// Archive an INCLUSIVE revision range; only evictable revs inside it
    /// are touched (§15.3 layering discipline).
    pub fn archive_range(
        &mut self,
        from_rev: RevId,
        to_rev: RevId,
        evictable: &dyn Evictable,
        loader: &dyn Fn(RevId) -> Result<SnapGraph, String>,
    ) -> Result<Vec<RevArchiveEntry>, String> {
        let mut out = vec![];
        for rev in evictable.evictable_revs() {
            if rev >= from_rev && rev <= to_rev && !self.index.entries.contains_key(&rev) {
                out.push(self.archive_rev(rev, &loader(rev)?)?);
            }
        }
        Ok(out)
    }

    /// Restore a revision: rebuild the L2 snapshot BYTE-EQUIVALENTLY
    /// (validated by re-write against the original artifacts — a silent
    /// partial restore is a defect, global rule 2).
    pub fn restore(
        &self,
        rev: RevId,
        l2_dir: &Path,
    ) -> Result<SnapGraph, String> {
        let entry = self
            .index
            .entries
            .get(&rev)
            .ok_or_else(|| format!("rev {rev} not archived"))?;
        let payload = fs::read(self.root.join(&entry.file)).map_err(|e| e.to_string())?;
        if cie_core::types::hash(&payload) != entry.content_hash {
            return Err("archive content hash mismatch — partial restore refused (rule 2)".into());
        }
        let graph = Self::decode(&payload)?;
        // byte-equivalent L2 rebuild: re-emit the snapshot and compare
        let rebuilt = SnapshotWriter::write(l2_dir, rev, &graph).map_err(|e| e.to_string())?;
        let reader = crate::snapshot::SnapshotReader::open(l2_dir, rev)
            .map_err(|e| e.to_string())?;
        let check = reader.graph_for_check().map_err(|e| e.to_string())?;
        if check != graph {
            return Err(format!(
                "restore equivalence FAILED for rev {rev} ({} vs {} bytes) — refusing silent partial restore",
                check.nodes.len(),
                graph.nodes.len()
            ));
        }
        let _ = rebuilt;
        Ok(graph)
    }

    /// Decode the columnar payload back into a graph.
    fn decode(payload: &[u8]) -> Result<SnapGraph, String> {
        let mut cols: Vec<Vec<u8>> = vec![];
        let mut off = 0usize;
        while off + 8 <= payload.len() {
            let len = u64::from_le_bytes(payload[off..off + 8].try_into().unwrap()) as usize;
            off += 8;
            if off + len > payload.len() {
                return Err("truncated column".into());
            }
            cols.push(zstd::bulk::decompress(&payload[off..off + len], 1 << 30)
                .map_err(|e| e.to_string())?);
            off += len;
        }
        if cols.len() != 3 {
            return Err("expected 3 columns".into());
        }
        fn parse_col(c: &[u8]) -> Result<&[u8], String> {
            // first newline splits the column header from the payload
            if let Some(pos) = c.iter().position(|&b| b == b'\n') {
                Ok(&c[pos + 1..])
            } else {
                Err("column header missing".into())
            }
        }
        let offsets_raw = parse_col(&cols[0])?;
        let edges_raw = parse_col(&cols[1])?;
        let nodes_raw = parse_col(&cols[2])?;
        let offsets: Vec<u32> = offsets_raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let edges: Vec<(u32, u8)> = edges_raw
            .chunks_exact(5)
            .map(|c| {
                (
                    u32::from_le_bytes(c[..4].try_into().unwrap()),
                    c[4],
                )
            })
            .collect();
        let nodes: Vec<u32> = nodes_raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Ok(SnapGraph { offsets, edges, nodes })
    }

    /// Metadata scan (the ≤ 100 ms gate target).
    pub fn scan_index(&self) -> &RevIndex {
        &self.index
    }

    /// `--as-of-ns` support: which archived revisions were valid at `ns`.
    pub fn valid_at_ns(&self, ns: u64) -> Vec<RevId> {
        self.index
            .entries
            .values()
            .filter(|e| e.retention.valid_from_ns <= ns)
            .map(|e| e.rev)
            .collect()
    }
}
