//! LangId, the §16 v1 language table (TASK-005) and extension detection.

use serde::{Deserialize, Serialize};

/// Languages supported by the adapter set (PRD §65, §16 v1 table).
/// Shipping a `LangId` does NOT imply a shipped adapter (FR-17/FR-18):
/// ids exist so facts can name their language before adapters land.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
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
    fn lang_detection() {
        assert_eq!(lang_for_path("a/b/c.py"), LangId::Python);
        assert_eq!(lang_for_path("x.ts"), LangId::TypeScript);
        assert_eq!(lang_for_path("y.go"), LangId::Go);
        assert_eq!(lang_for_path("z.rs"), LangId::Rust);
        assert_eq!(lang_for_path("w.md"), LangId::Unknown);
    }
}
