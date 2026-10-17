//! Core scalar types, time and hashing primitives (PRD Part VII §47).
//!
//! Every timestamp, duration, deadline and budget in CIE is nanoseconds
//! (`u64` since epoch for points, `i64` for deltas). Content hashes are
//! blake3 32-byte digests.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Ns = u64; // ns since epoch
pub type NsDelta = i64; // signed ns
pub type RevId = u64; // monotonic revision
pub type ConceptNodeId = u32;
pub type ClaimId = u32;
pub type ContractId = u32;
pub type RepoId = u16;
pub type Hash = [u8; 32]; // blake3

/// §14.1 identity newtypes — `Copy + Hash + Eq + Ord`, no String fields
/// (§15.5). Serde derives render these transparently (plain `u32`/`u64` on
/// the wire), so persisted snapshots keep their v0.1 layout.
macro_rules! identity_newtype {
    ($(#[$doc:meta])* $name:ident($inner:ty)) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
        pub struct $name(pub $inner);

        impl $name {
            /// Sentinel id used by adapters for not-yet-assigned identities.
            pub const PLACEHOLDER: $name = $name(<$inner>::MAX);
            /// Raw inner value (hot-path escape hatch; keep uses rare).
            pub const fn raw(self) -> $inner {
                self.0
            }
        }

        impl From<$inner> for $name {
            fn from(v: $inner) -> Self {
                $name(v)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

identity_newtype!(
    /// Stable identity of one code symbol (§14.1, FR-1).
    SymbolId(u32)
);
identity_newtype!(
    /// Stable identity of one operation/effect (§14.1).
    OperationId(u32)
);
identity_newtype!(
    /// Stable identity of one SSA value (§14.1).
    ValueId(u32)
);
identity_newtype!(
    /// Stable identity of one external resource (§14.1).
    ResourceId(u32)
);
identity_newtype!(
    /// Stable identity of one source file (§14.1).
    FileId(u32)
);

/// blake3 digest of the empty input — the TASK-002 known vector. Every
/// content-addressed table (§15.1 `sym_by_hash:{hash}`, LLM cache FR-25)
/// keys on `hash()`; this constant pins the empty-input identity.
pub const BLAKE3_EMPTY: Hash = [
    0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9, 0x49,
    0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f, 0x32, 0x62,
];

/// TASK-002 name for `content_hash` — the only hash function allowed in hot
/// paths (FR-8, FR-25). All content addressing builds on blake3.
pub fn hash(data: &[u8]) -> Hash {
    content_hash(data)
}

/// Lock-free, strictly monotonic revision allocator (TASK-002 / FR-21).
///
/// `next()` is a single `AtomicU64::fetch_add`, so concurrent callers get
/// unique, strictly increasing `RevId`s with no lock and no rollback. The
/// last handed-out value is recoverable after process restart via the
/// storage layer's `rev:{rev_id}:meta` head (§15.1 global rule 7): seed with
/// `RevisionAllocator::starting_at(last_rev)` (`Store::allocator()`).
pub struct RevisionAllocator {
    next: AtomicU64,
}

impl RevisionAllocator {
    /// Continue the revision chain after `last` — the first `next()` call
    /// returns `last + 1`.
    pub fn starting_at(last: RevId) -> Self {
        RevisionAllocator {
            next: AtomicU64::new(last),
        }
    }

    /// Hand out the next revision id (strictly monotonic, thread-safe).
    pub fn next(&self) -> RevId {
        self.next.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Current head without consuming a revision (for diagnostics/tests).
    pub fn peek(&self) -> RevId {
        self.next.load(Ordering::SeqCst)
    }
}

impl Default for RevisionAllocator {
    fn default() -> Self {
        RevisionAllocator::starting_at(0)
    }
}

/// Injectable clock source (TASK-002 / FR-21). Production code uses the
/// system wall clock via the `now_ns()` free function; tests inject
/// deterministic sources instead of sleeping. `chrono::DateTime` never
/// appears in cie-core or any hot path (global rule 1).
pub trait ClockSource: Send + Sync {
    fn now_ns(&self) -> Ns;
}

/// The production clock: system wall time as ns since the Unix epoch.
pub struct SystemClock;

impl ClockSource for SystemClock {
    fn now_ns(&self) -> Ns {
        system_now_ns()
    }
}

/// A deterministic clock for tests — always returns the fixed instant.
pub struct FixedClock(pub Ns);

impl ClockSource for FixedClock {
    fn now_ns(&self) -> Ns {
        self.0
    }
}

/// Wall-clock nanoseconds since the Unix epoch (the default `ClockSource`).
pub fn now_ns() -> Ns {
    system_now_ns()
}

fn system_now_ns() -> Ns {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// blake3 content hash (PRD FR-8 incremental engine).
pub fn content_hash(data: &[u8]) -> Hash {
    *blake3::hash(data).as_bytes()
}

/// Lowercase hex encoding of a hash.
pub fn hash_hex(h: &Hash) -> String {
    let mut s = String::with_capacity(64);
    for b in h {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Parse a lowercase hex string back into a `Hash`.
pub fn hash_from_hex(s: &str) -> Option<Hash> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)? as u8;
        let lo = (chunk[1] as char).to_digit(16)? as u8;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// §14.1 location: byte offsets into the file identified by `FileId` —
/// shape EXACTLY `{ file, start, end }` (TASK-006 alignment; the v0.1
/// path/line/col rendering moved to [`FileTable`], see TRACEABILITY §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SourceLocation {
    pub file: FileId,
    /// Byte offset of the first byte of the span.
    pub start: u32,
    /// Byte offset one past the last byte of the span.
    pub end: u32,
}

impl SourceLocation {
    /// The §14.1 byte-span constructor.
    pub fn span_bytes(file: FileId, start: u32, end: u32) -> Self {
        SourceLocation { file, start, end }
    }

    /// A zero-width byte span (degenerate facts and tests).
    pub fn point(file: FileId, at: u32) -> Self {
        SourceLocation { file, start: at, end: at }
    }

    /// Length in bytes.
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// File-id -> path table (§15.1 `files` list). One per extraction pass /
/// snapshot; `FileId(0)` indexes `paths[0]`. Rendering layers resolve
/// locations through this table instead of carrying paths inside spans.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FileTable {
    pub paths: Vec<String>,
    /// Byte offset of each line start per file (1-based line =
    /// position + 1). Built by [`FileTable::intern_with_source`]; a plain
    /// [`FileTable::intern`] entry has an empty index (line_of -> 0).
    #[serde(default)]
    pub line_starts: Vec<Vec<u32>>,
}

impl FileTable {
    pub fn new() -> Self {
        FileTable { paths: Vec::new(), line_starts: Vec::new() }
    }

    /// Intern a path, returning its stable `FileId`.
    pub fn intern(&mut self, path: impl Into<String>) -> FileId {
        let path = path.into();
        if let Some(i) = self.paths.iter().position(|p| p == &path) {
            FileId(i as u32)
        } else {
            self.paths.push(path);
            self.line_starts.push(Vec::new());
            FileId((self.paths.len() - 1) as u32)
        }
    }

    /// Intern a path together with its source bytes, building the line
    /// index so reports can render byte spans as 1-based lines later.
    pub fn intern_with_source(&mut self, path: impl Into<String>, src: &[u8]) -> FileId {
        let fid = self.intern(path);
        let idx = fid.0 as usize;
        if self.line_starts[idx].is_empty() {
            let mut starts = vec![0u32];
            for (i, &b) in src.iter().enumerate() {
                if b == b'\n' {
                    starts.push((i + 1) as u32);
                }
            }
            self.line_starts[idx] = starts;
        }
        fid
    }

    /// 1-based line of a byte offset (0 when no line index exists).
    pub fn line_of(&self, file: FileId, offset: u32) -> u32 {
        let idx = match self.line_starts.get(file.0 as usize) {
            Some(v) if !v.is_empty() => v,
            _ => return 0,
        };
        match idx.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(0) => 1,
            Err(i) => i as u32,
        }
    }

    /// 1-based line of a location's start (convenience).
    pub fn line(&self, loc: SourceLocation) -> u32 {
        self.line_of(loc.file, loc.start)
    }

    /// Path for a file id (empty string when out of range — never panics).
    pub fn path(&self, file: FileId) -> &str {
        self.paths.get(file.0 as usize).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// 1-based line number of a byte offset within `src` (extraction-time
/// helper: adapters hold the source bytes, reports do not).
pub fn line_of(src: &[u8], offset: u32) -> u32 {
    let off = offset as usize;
    1 + src[..off.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let h = content_hash(b"authorize");
        let hex = hash_hex(&h);
        assert_eq!(hash_from_hex(&hex), Some(h));
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn blake3_known_vectors() {
        assert_eq!(content_hash(b""), BLAKE3_EMPTY, "TASK-002 known vector");
        assert_ne!(content_hash(b"authorize"), BLAKE3_EMPTY);
    }

    #[test]
    fn allocator_strictly_monotonic_8_threads_100k() {
        let alloc = std::sync::Arc::new(RevisionAllocator::starting_at(0));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let a = alloc.clone();
                std::thread::spawn(move || (0..100_000).map(|_| a.next()).collect::<Vec<RevId>>())
            })
            .collect();
        let mut all: Vec<RevId> = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .flatten()
            .collect();
        assert_eq!(all.len(), 800_000);
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 800_000, "no duplicate RevIds across 8 threads");
        assert_eq!(all[0], 1);
        assert_eq!(all[799_999], 800_000, "strictly monotonic, no rollback");
    }

    #[test]
    fn injectable_clock() {
        assert!(SystemClock.now_ns() > 0);
        assert_eq!(
            FixedClock(1_700_000_000_123_456_789).now_ns(),
            1_700_000_000_123_456_789
        );
        let _: &dyn ClockSource = &SystemClock; // trait object usable
    }
}
