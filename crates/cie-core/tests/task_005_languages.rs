//! TASK-005 validation sketch — instructor contract, adapted per
//! docs/TRACEABILITY.md §2 (`cie_semantic` → `cie_core::facts`; `LanguageId`
//! → `LangId`; `SymbolFact` keeps its rich v0.1 shape with `rev: 0` literal
//! since `RevId` is the §14.1 alias).

use cie_core::facts::{SymbolFact, SymbolKind};
use cie_core::types::{LanguageTable, SourceLocation, SymbolId};
use cie_core::types::{FileId, LangId};

#[test]
fn task_005_symbol_facts_and_v1_languages() {
    let fact = SymbolFact {
        id: SymbolId(1),
        kind: SymbolKind::Function,
        name: "authorize".to_string(),
        qual_name: "auth.authorize".to_string(),
        loc: SourceLocation::span(FileId(0), "auth.py", 0, 128),
        lang: LangId::Python,
        parent: None,
        rev: 1,
        doc: None,
    };
    assert_eq!(fact.kind, SymbolKind::Function);
    assert_eq!(fact.lang, LangId::Python);

    let v1 = LanguageTable::v1();
    assert_eq!(v1.len(), 10, "FR-17: ten v1 languages (§16)");
    let ids: Vec<_> = v1.iter().copied().collect();
    for expected in [
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
    ] {
        assert!(ids.contains(&expected), "missing §16 language: {expected:?}");
    }
    // Closed table: no v1.1 languages receive ids (FR-17).
    assert_eq!(v1.iter().filter(|l| **l == LangId::Go).count(), 1);
}

/// §16 extension detection maps suffixes → language deterministically; the
/// unknown suffix reports `Unknown`, never silently guesses (FR-4).
#[test]
fn task_005_extension_detection_full_table() {
    for (ext, want) in [
        ("java", LangId::Java),
        ("c", LangId::C),
        ("h", LangId::C),
        ("cpp", LangId::Cpp),
        ("hpp", LangId::Cpp),
        ("cs", LangId::CSharp),
        ("php", LangId::Php),
        ("py", LangId::Python),
        ("ts", LangId::TypeScript),
        ("go", LangId::Go),
        ("rs", LangId::Rust),
        ("rb", LangId::Unknown), // v1.1 language: no id yet
    ] {
        assert_eq!(cie_core::types::LangId::from_extension(ext), want, "ext .{ext}");
    }
}
