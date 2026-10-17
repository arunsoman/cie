//! Rust adapter — thin shell over the shared light engine in
//! `cie-adapters::light_engine` (TASK-001 scaffold; TASK-013/019/020 add
//! deep semantics on top of this base).

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn id(&self) -> LangId {
        LangId::Rust
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-light:rust"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        extract_light(path, src, LightLang::Rust)
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(RustAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = RustAdapter;
    assert_eq!(a.id(), LangId::Rust);
    assert!(a.matches_extension("sample.rs"));
}
