//! Go adapter — thin shell over the shared light engine in
//! `cie-adapters::light_engine` (TASK-001 scaffold; TASK-013/019/020 add
//! deep semantics on top of this base).

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct GoAdapter;

impl LanguageAdapter for GoAdapter {
    fn id(&self) -> LangId {
        LangId::Go
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-light:go"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        extract_light(path, src, LightLang::Go)
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(GoAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = GoAdapter;
    assert_eq!(a.id(), LangId::Go);
    assert!(a.matches_extension("sample.go"));
}
