//! TASK-011 validation sketch — FR-18 extension-cost guard (acceptance #9).
//! A hypothetical 11th language, defined ONLY inside this test file,
//! registers with ONE line and zero edits to cie-core/cie-langs internals.

use cie_adapters::{DispatchKind, LanguageAdapter, LanguageRegistry, NumericSemantics};

struct TestLangAdapter; // a hypothetical 11th language, defined ONLY in this test file (FR-18)

impl LanguageAdapter for TestLangAdapter {
    fn id(&self) -> cie_semantic::LangId {
        cie_semantic::LangId::Unknown // ids-only table: an 11th language has no §16 id yet
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["tl"]
    }
    fn analyzer(&self) -> &'static str {
        "test-lang:contract"
    }
    // The single `extract` method replaces the sketch's seven §16.1
    // extract_* stubs — each returned Vec::new() for this contract test.
    fn extract(&self, _path: &str, _src: &str) -> cie_semantic::facts::LangFacts {
        cie_semantic::facts::LangFacts::new(_path, cie_semantic::LangId::Unknown, self.analyzer())
    }
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Hybrid
    }
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics::default()
    }
    fn framework_conventions(&self) -> &'static [cie_adapters::FrameworkRule] {
        &[]
    }
}

#[test]
fn task_011_registry_extension_cost_fr18() {
    let mut registry = LanguageRegistry::new();
    registry.register(Box::new(TestLangAdapter)); // THE one registration line (§16.2)
    let adapter = registry
        .by_extension("tl")
        .expect("by_extension resolves the test adapter");
    assert_eq!(adapter.id(), cie_semantic::LangId::Unknown);
    assert_eq!(adapter.extensions(), &["tl"]);
    assert_eq!(
        registry.adapters().len(),
        1,
        "≤ 1 crate + ≤ 1 registry line + 0 core changes (FR-18)"
    );
    // No core edits: the adapter compiled against the existing trait only.
}

/// TASK-011 DoD: semantics tables are data on the adapter — Python vouches
/// for Dynamic dispatch + ArbitraryPrecision (§16 #1); rules consult the
/// tables (FR-3) instead of hardcoding language knowledge.
#[test]
fn task_011_python_semantics_tables() {
    let py = cie_adapter_python::PythonAdapter;
    assert_eq!(py.dispatch_kind(), DispatchKind::Dynamic, "§16 #1");
    assert_eq!(
        py.numeric_semantics().overflow,
        cie_adapters::OverflowPolicy::ArbitraryPrecision
    );
    assert!(py
        .framework_conventions()
        .iter()
        .any(|r| r.name == "bare-except"));
    // Default rows describe an unspecified hybrid language.
    assert_eq!(
        NumericSemantics::default().overflow,
        cie_adapters::OverflowPolicy::Unspecified
    );
}
