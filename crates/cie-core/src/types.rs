//! Core scalar types, time and hashing primitives (PRD Part VII §47).
//!
//! Every timestamp, duration, deadline and budget in CIE is nanoseconds
//! (`u64` since epoch for points, `i64` for deltas). Content hashes are
//! blake3 32-byte digests.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Ns = u64; // ns since epoch
pub type NsDelta = i64; // signed ns
pub type RevId = u64; // monotonic revision
pub type FileId = u32;
pub type SymbolId = u32;
pub type ConceptNodeId = u32;
pub type ClaimId = u32;
pub type ContractId = u32;
pub type RepoId = u16;
pub type Hash = [u8; 32]; // blake3

/// Wall-clock nanoseconds since the Unix epoch.
pub fn now_ns() -> Ns {
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

/// Languages supported by the v1 adapter set (PRD §65 — top-10, we ship 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub enum LangId {
    #[default]
    Unknown,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Rust,
}

impl LangId {
    pub fn as_str(&self) -> &'static str {
        match self {
            LangId::Unknown => "unknown",
            LangId::Python => "python",
            LangId::JavaScript => "javascript",
            LangId::TypeScript => "typescript",
            LangId::Go => "go",
            LangId::Rust => "rust",
        }
    }

    pub fn parse(s: &str) -> LangId {
        match s.to_ascii_lowercase().as_str() {
            "py" | "python" => LangId::Python,
            "js" | "javascript" => LangId::JavaScript,
            "ts" | "typescript" => LangId::TypeScript,
            "go" | "golang" => LangId::Go,
            "rs" | "rust" => LangId::Rust,
            _ => LangId::Unknown,
        }
    }

    pub fn from_extension(ext: &str) -> LangId {
        match ext.to_ascii_lowercase().as_str() {
            "py" | "pyw" => LangId::Python,
            "js" | "jsx" | "mjs" | "cjs" => LangId::JavaScript,
            "ts" | "tsx" => LangId::TypeScript,
            "go" => LangId::Go,
            "rs" => LangId::Rust,
            _ => LangId::Unknown,
        }
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
    fn lang_detection() {
        assert_eq!(lang_for_path("a/b/c.py"), LangId::Python);
        assert_eq!(lang_for_path("x.ts"), LangId::TypeScript);
        assert_eq!(lang_for_path("y.go"), LangId::Go);
        assert_eq!(lang_for_path("z.rs"), LangId::Rust);
        assert_eq!(lang_for_path("w.md"), LangId::Unknown);
    }
}
