//! Zero-copy revisioned graph snapshots (TASK-060, FR-30, §15.3 L2,
//! §13 item 16 — `cie-store::snapshot`).
//!
//! Each revision's graph is serialized with rkyv into a `.cie-snap`
//! archive that is MEMORY-MAPPED on open: the read path performs ZERO
//! deserialization — access goes through `Archived` accessors over the
//! mmap, and any `to_owned()` on hot structs is a review-blocking
//! violation (§15.5). Warm opens are pointer arithmetic plus validation.
//!
//! - Snapshots are versioned per `RevId` and content-addressed (blake3
//!   of the archive bytes); writing the same rev is IDEMPOTENT (global
//!   rules 1 and 7).
//! - Corrupt, short, or mismatched mmap → explicit
//!   `SnapshotError::Corrupt`, never a silently empty graph (rule 2).
//! - `open_latest_at(rev)` picks the newest snapshot ≤ rev — the
//!   `--as-of-rev` support (FR-23) is a file-selection problem.
//! - Layering (TASK-001): the payload shape is the plain [`SnapGraph`]
//!   defined HERE (cie-store stays core-only); the CSR graph converts at
//!   the caller and the archived views compare back — the DoD round-trip.
//! - §13 item 16 gate: warm open of a 1M-symbol graph ≤ 10 µs ×
//!   CI_TOLERANCE (`#[perf]` `#[ignore]`).

use cie_core::types::{hash, Hash, RevId};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The archived payload shape (§15.5: hot fields are plain vecs of
/// scalars — no nested allocation-heavy structures on the timing path).
#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SnapGraph {
    /// CSR offsets, `len == nodes + 1` (the §14.3 invariant).
    pub offsets: Vec<u32>,
    /// `(dense_target, rel_kind_u8)` edges, sorted per node (rule 7).
    pub edges: Vec<(u32, u8)>,
    /// Dense node weights: `nodes[i]` is the SymbolId of dense node i.
    pub nodes: Vec<u32>,
}

/// Errors (rule 2: explicit, never silently empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    Corrupt(String),
    NotFound { rev: RevId },
    Io(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Corrupt(d) => write!(f, "snapshot corrupt: {d}"),
            SnapshotError::NotFound { rev } => write!(f, "no snapshot at or before rev {rev}"),
            SnapshotError::Io(e) => write!(f, "snapshot io: {e}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// The on-disk header: magic, version, revision, content hash. Validated
/// before ANY archive access (rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapHeader {
    pub magic: [u8; 8], // b"CIESNAP\0"
    pub version: u32,
    pub rev: RevId,
    /// blake3 of the archive bytes (content-addressed, idempotent writes).
    pub content_hash: Hash,
}

const MAGIC: [u8; 8] = *b"CIESNAP\0";
const VERSION: u32 = 1;

fn snap_path(dir: &Path, rev: RevId) -> PathBuf {
    dir.join(format!("rev{rev:020}.cie-snap"))
}

/// The writer (sketch: `SnapshotWriter::write(dir, rev, graph)`).
pub struct SnapshotWriter;

impl SnapshotWriter {
    /// Emit the revisioned `.cie-snap` file. Writing the same rev twice
    /// is idempotent — byte-identical output (content-addressed).
    pub fn write(dir: &Path, rev: RevId, graph: &SnapGraph) -> Result<PathBuf, SnapshotError> {
        let archive = rkyv::to_bytes::<rkyv::rancor::Error>(graph)
            .map_err(|e| SnapshotError::Io(format!("rkyv serialize: {e}")))?;
        let header = SnapHeader {
            magic: MAGIC,
            version: VERSION,
            rev,
            content_hash: hash(&archive),
        };
        let mut header_bytes =
            serde_json::to_vec(&header).map_err(|e| SnapshotError::Io(e.to_string()))?;
        // §15.5: the archive must start at an 8-byte-aligned offset for
        // zero-copy access — pad the JSON header with whitespace (serde
        // tolerates it; the content hash covers the archive only).
        while (8 + header_bytes.len()) % 8 != 0 {
            header_bytes.push(b' ');
        }
        let len_bytes = (header_bytes.len() as u64).to_le_bytes();
        let mut out = Vec::with_capacity(8 + header_bytes.len() + archive.len());
        out.extend_from_slice(&len_bytes);
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(&archive);
        fs::create_dir_all(dir).map_err(|e| SnapshotError::Io(e.to_string()))?;
        let path = snap_path(dir, rev);
        // idempotent: same rev → byte-identical file (rule 7)
        if path.exists() {
            let existing = fs::read(&path).map_err(|e| SnapshotError::Io(e.to_string()))?;
            if existing == out {
                return Ok(path);
            }
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &out).map_err(|e| SnapshotError::Io(e.to_string()))?;
        fs::rename(&tmp, &path).map_err(|e| SnapshotError::Io(e.to_string()))?;
        Ok(path)
    }
}

/// The reader over a memory-mapped archive (zero deserialization on the
/// read path — `Archived` accessors only).
pub struct SnapshotReader {
    mmap: memmap2::Mmap,
    header: SnapHeader,
    archive_start: usize,
}

impl SnapshotReader {
    /// Map + validate (sketch: `SnapshotReader::open(dir, rev)`).
    pub fn open(dir: &Path, rev: RevId) -> Result<SnapshotReader, SnapshotError> {
        let path = snap_path(dir, rev);
        let file = fs::File::open(&path).map_err(|e| SnapshotError::Io(e.to_string()))?;
        // SAFETY: the file is read-only and not mutated while mapped.
        let mmap =
            unsafe { memmap2::Mmap::map(&file).map_err(|e| SnapshotError::Io(e.to_string()))? };
        Self::from_bytes(rev, mmap)
    }

    fn from_bytes(rev: RevId, mmap: memmap2::Mmap) -> Result<SnapshotReader, SnapshotError> {
        if mmap.len() < 8 {
            return Err(SnapshotError::Corrupt("shorter than the length prefix".into()));
        }
        let hlen = u64::from_le_bytes(mmap[..8].try_into().unwrap()) as usize;
        if mmap.len() < 8 + hlen {
            return Err(SnapshotError::Corrupt("shorter than the header".into()));
        }
        let header: SnapHeader = serde_json::from_slice(&mmap[8..8 + hlen])
            .map_err(|e| SnapshotError::Corrupt(format!("header: {e}")))?;
        if header.magic != MAGIC {
            return Err(SnapshotError::Corrupt("bad magic".into()));
        }
        if header.version != VERSION {
            return Err(SnapshotError::Corrupt(format!("version {}", header.version)));
        }
        if header.rev != rev {
            return Err(SnapshotError::Corrupt(format!("rev mismatch {} ≠ {}", header.rev, rev)));
        }
        let archive_start = 8 + hlen;
        // content-addressed: the blake3 of the archive must match (rule 2)
        if hash(&mmap[archive_start..]) != header.content_hash {
            return Err(SnapshotError::Corrupt("content hash mismatch".into()));
        }
        Ok(SnapshotReader { mmap, header, archive_start })
    }

    pub fn header(&self) -> &SnapHeader {
        &self.header
    }

    /// Zero-copy access: validate + return the Archived view over the
    /// mmap. NO deserialization, NO to_owned (FR-30/§15.5).
    pub fn graph(&self) -> Result<&<SnapGraph as rkyv::Archive>::Archived, SnapshotError> {
        let bytes = &self.mmap[self.archive_start..];
        rkyv::access::<<SnapGraph as rkyv::Archive>::Archived, rkyv::rancor::Error>(bytes)
            .map_err(|e| SnapshotError::Corrupt(format!("archive access: {e}")))
    }

    /// Round-trip fidelity check materialization (allocates BY DESIGN —
    /// used only by tests/equivalence checks, never on the hot path).
    pub fn graph_for_check(&self) -> Result<SnapGraph, SnapshotError> {
        let a = self.graph()?;
        Ok(SnapGraph {
            offsets: a.offsets.iter().map(|x| x.to_native()).collect(),
            edges: a.edges.iter().map(|e| (e.0.to_native(), e.1)).collect(),
            nodes: a.nodes.iter().map(|x| x.to_native()).collect(),
        })
    }
}

/// `open_latest_at(rev)`: the newest snapshot ≤ rev (FR-23).
pub fn open_latest_at(dir: &Path, rev: RevId) -> Result<(RevId, SnapshotReader), SnapshotError> {
    let mut best: Option<RevId> = None;
    let entries = fs::read_dir(dir).map_err(|e| SnapshotError::Io(e.to_string()))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(stripped) = name.strip_suffix(".cie-snap") {
            if let Some(r) = stripped.strip_prefix("rev").and_then(|s| s.parse::<u64>().ok()) {
                if r <= rev && best.map(|b| r > b).unwrap_or(true) {
                    best = Some(r);
                }
            }
        }
    }
    let Some(r) = best else {
        return Err(SnapshotError::NotFound { rev });
    };
    Ok((r, SnapshotReader::open(dir, r)?))
}

/// Fixture helper: a deterministic graph with `n` nodes (2 out-edges
/// each), CSR-encoded.
pub fn fixture_csr_graph(n: usize) -> SnapGraph {
    let mut offsets = Vec::with_capacity(n + 1);
    let mut edges = Vec::new();
    let nodes: Vec<u32> = (0..n as u32).collect();
    let mut cursor = 0u32;
    for i in 0..n as u32 {
        offsets.push(cursor);
        for d in 1..=2u32 {
            edges.push(((i + d) % (n as u32), cie_relations_kind_calls()));
            cursor += 1;
        }
    }
    offsets.push(cursor);
    SnapGraph { offsets, edges, nodes }
}

fn cie_relations_kind_calls() -> u8 {
    0 // RelKind::Calls discriminant in the snapshot payload encoding
}
