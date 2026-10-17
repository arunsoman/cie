//! JavaScript adapter — thin shell over the shared light engine in
//! `cie-adapters::light_engine` (TASK-001 scaffold; TASK-013/019/020 add
//! deep semantics on top of this base).

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct JavaScriptAdapter;

impl LanguageAdapter for JavaScriptAdapter {
    fn id(&self) -> LangId {
        LangId::JavaScript
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["js", "jsx", "mjs", "cjs"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-light:javascript"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        extract_light(path, src, LightLang::JavaScript)
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(JavaScriptAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = JavaScriptAdapter;
    assert_eq!(a.id(), LangId::JavaScript);
    assert!(a.matches_extension("sample.js"));
}
