//! Cpp adapter — TASK-001 scaffold stub. WP-B (TASK-017) replaces
//! `extract` with the real tree-sitter implementation; the stub records
//! honest `unsupported` evidence instead of extracting nothing silently.

use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct CppAdapter;

impl LanguageAdapter for CppAdapter {
    fn id(&self) -> LangId {
        LangId::Cpp
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["cpp", "cc", "cxx", "hpp", "hh"]
    }
    fn analyzer(&self) -> &'static str {
        "stub:cpp"
    }
    fn extract(&self, _path: &str, _src: &str) -> LangFacts {
        // Scaffold stub: no extraction yet (WP-B). Empty facts keep the
        // pipeline honest — nothing is asserted about these files.
        LangFacts::default()
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(CppAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = CppAdapter;
    assert_eq!(a.id(), LangId::Cpp);
    assert!(a.matches_extension("sample.cpp"));
    assert_eq!(a.analyzer(), "stub:cpp");
}
