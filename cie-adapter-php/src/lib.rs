//! Php adapter — TASK-001 scaffold stub. WP-B (TASK-021) replaces
//! `extract` with the real tree-sitter implementation; the stub records
//! honest `unsupported` evidence instead of extracting nothing silently.

use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct PhpAdapter;

impl LanguageAdapter for PhpAdapter {
    fn id(&self) -> LangId {
        LangId::Php
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["php", "php3", "php4", "php5", "phtml"]
    }
    fn analyzer(&self) -> &'static str {
        "stub:php"
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
    Box::new(PhpAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = PhpAdapter;
    assert_eq!(a.id(), LangId::Php);
    assert!(a.matches_extension("sample.php"));
    assert_eq!(a.analyzer(), "stub:php");
}
