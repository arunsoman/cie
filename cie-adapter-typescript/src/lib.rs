//! TypeScript adapter — thin shell over the shared light engine in
//! `cie-adapters::light_engine` (TASK-001 scaffold; TASK-013/019/020 add
//! deep semantics on top of this base).

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::LanguageAdapter;
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn id(&self) -> LangId {
        LangId::TypeScript
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-light:typescript"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        let lang = if path.ends_with(".tsx") {
            LightLang::Tsx
        } else {
            LightLang::TypeScript
        };
        extract_light(path, src, lang)
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(TypeScriptAdapter)
}

#[test]
fn scaffold_placeholder() {
    let a = TypeScriptAdapter;
    assert_eq!(a.id(), LangId::TypeScript);
    assert!(a.matches_extension("sample.ts"));
}
