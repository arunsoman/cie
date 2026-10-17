//! C adapter — TASK-001 scaffold stub. WP-B (TASK-016) replaces
//! `extract` with the real tree-sitter implementation; the stub records
//! honest `unsupported` evidence instead of extracting nothing silently.

use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct CAdapter;

impl LanguageAdapter for CAdapter {
    fn id(&self) -> LangId {
        LangId::C
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }
    fn analyzer(&self) -> &'static str {
        "stub:c"
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
    Box::new(CAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = CAdapter;
    assert_eq!(a.id(), LangId::C);
    assert!(a.matches_extension("sample.c"));
    assert_eq!(a.analyzer(), "stub:c");
}
