//! CSharp adapter — TASK-001 scaffold stub. WP-B (TASK-018) replaces
//! `extract` with the real tree-sitter implementation; the stub records
//! honest `unsupported` evidence instead of extracting nothing silently.

use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct CSharpAdapter;

impl LanguageAdapter for CSharpAdapter {
    fn id(&self) -> LangId {
        LangId::CSharp
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["cs"]
    }
    fn analyzer(&self) -> &'static str {
        "stub:csharp"
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
    Box::new(CSharpAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = CSharpAdapter;
    assert_eq!(a.id(), LangId::CSharp);
    assert!(a.matches_extension("sample.cs"));
    assert_eq!(a.analyzer(), "stub:csharp");
}
