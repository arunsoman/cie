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

/// Source location. Lines are 1-based, columns 0-based. `path` is the
/// repo-relative path (kept alongside `file` id for ergonomic rendering).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: FileId,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
}

impl SourceLocation {
    pub fn point(file: FileId, path: impl Into<String>, line: u32) -> Self {
        SourceLocation {
            file,
            path: path.into(),
            start_line: line,
            end_line: line,
            start_col: 0,
            end_col: 0,
        }
    }

    pub fn span(
        file: FileId,
        path: impl Into<String>,
        start_line: u32,
        end_line: u32,
    ) -> Self {
        SourceLocation {
            file,
            path: path.into(),
            start_line,
            end_line,
            start_col: 0,
            end_col: 0,
        }
    }
}

/// Languages supported by the adapter set (PRD §65, §16 v1 table).
/// Shipping a `LangId` does NOT imply a shipped adapter (FR-17/FR-18):
/// ids exist so facts can name their language before adapters land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub enum LangId {
    #[default]
    Unknown,
    Python,
    JavaScript,
    TypeScript,
    Java,
    C,
    Cpp,
    CSharp,
    Go,
    Rust,
    Php,
}

impl LangId {
    pub fn as_str(&self) -> &'static str {
        match self {
            LangId::Unknown => "unknown",
            LangId::Python => "python",
            LangId::JavaScript => "javascript",
            LangId::TypeScript => "typescript",
            LangId::Java => "java",
            LangId::C => "c",
            LangId::Cpp => "cpp",
            LangId::CSharp => "csharp",
            LangId::Go => "go",
            LangId::Rust => "rust",
            LangId::Php => "php",
        }
    }

    pub fn parse(s: &str) -> LangId {
        match s.to_ascii_lowercase().as_str() {
            "py" | "python" => LangId::Python,
            "js" | "javascript" => LangId::JavaScript,
            "ts" | "typescript" => LangId::TypeScript,
            "java" => LangId::Java,
            "c" => LangId::C,
            "cpp" | "c++" => LangId::Cpp,
            "cs" | "csharp" | "c#" => LangId::CSharp,
            "go" | "golang" => LangId::Go,
            "rs" | "rust" => LangId::Rust,
            "php" => LangId::Php,
            _ => LangId::Unknown,
        }
    }

    pub fn from_extension(ext: &str) -> LangId {
        match ext.to_ascii_lowercase().as_str() {
            "py" | "pyw" => LangId::Python,
            "js" | "jsx" | "mjs" | "cjs" => LangId::JavaScript,
            "ts" | "tsx" => LangId::TypeScript,
            "java" => LangId::Java,
            "c" | "h" => LangId::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => LangId::Cpp,
            "cs" => LangId::CSharp,
            "go" => LangId::Go,
            "rs" => LangId::Rust,
            "php" | "php3" | "php4" | "php5" | "phtml" => LangId::Php,
            _ => LangId::Unknown,
        }
    }
}

/// The §16 v1 language table (TASK-005): exactly the ten v1 languages plus
/// `UNKNOWN`. No v1.1 languages (Ruby, Kotlin, Swift, Scala, SQL) receive
/// ids yet (FR-17 — the table is closed).
pub struct LanguageTable;

impl LanguageTable {
    /// The ten v1 language ids of §16, in table order. Ids only — no
    /// adapter is implied; the registry (`cie-langs`) decides what extracts.
    pub fn v1() -> &'static [LangId] {
        &[
            LangId::Python,
            LangId::JavaScript,
            LangId::TypeScript,
            LangId::Java,
            LangId::C,
            LangId::Cpp,
            LangId::CSharp,
            LangId::Go,
            LangId::Rust,
            LangId::Php,
        ]
    }
}

/// Extension → language detection used by the folder walker.
pub fn lang_for_path(path: &str) -> LangId {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    LangId::from_extension(ext)
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
        assert_eq!(FixedClock(1_700_000_000_123_456_789).now_ns(), 1_700_000_000_123_456_789);
        let _: &dyn ClockSource = &SystemClock; // trait object usable
    }

    #[test]
    fn lang_detection() {
        assert_eq!(lang_for_path("a/b/c.py"), LangId::Python);
        assert_eq!(lang_for_path("x.ts"), LangId::TypeScript);
        assert_eq!(lang_for_path("y.go"), LangId::Go);
        assert_eq!(lang_for_path("z.rs"), LangId::Rust);
        assert_eq!(lang_for_path("w.md"), LangId::Unknown);
    }
}
