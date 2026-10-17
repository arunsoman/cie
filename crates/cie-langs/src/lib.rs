//! Language adapters, registry and the project extraction pipeline.
//!
//! Extension path (PRD §67 / acceptance #9): adding a language =
//! one adapter + ONE registration line in `default_registry()` +
//! zero core changes.

pub mod effects;
pub mod light;
pub mod python;
pub mod resolution;

use cie_core::facts::{Callee, ImportFact, LangFacts, SymbolFact, SymbolStore};
use cie_core::types::{FileId, LangId, SymbolId};
use std::collections::BTreeMap;
use std::path::Path;

pub trait LanguageAdapter: Send + Sync {
    fn id(&self) -> LangId;
    fn extensions(&self) -> &'static [&'static str];
    fn analyzer(&self) -> &'static str;
    fn extract(&self, path: &str, src: &str) -> LangFacts;

    fn matches_extension(&self, path: &str) -> bool {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        self.extensions().contains(&ext)
    }
}

/// One-line-per-language registration (acceptance #9).
pub struct LanguageRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl LanguageRegistry {
    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn adapter_for(&self, path: &str) -> Option<&dyn LanguageAdapter> {
        self.adapters.iter().find(|a| a.matches_extension(path)).map(|a| a.as_ref())
    }

    pub fn adapters(&self) -> &[Box<dyn LanguageAdapter>] {
        &self.adapters
    }
}

/// The v1 registry: Python deep + JS/TS/Go/Rust light.
pub fn default_registry() -> LanguageRegistry {
    let mut reg = LanguageRegistry { adapters: vec![] };
    reg.register(Box::new(python::PythonAdapter));
    reg.register(Box::new(light::JavaScriptAdapter));
    reg.register(Box::new(light::TypeScriptAdapter));
    reg.register(Box::new(light::GoAdapter));
    reg.register(Box::new(light::RustAdapter));
    // New language goes HERE — one line, e.g.:
    // reg.register(Box::new(ruby::RubyAdapter));
    reg
}

/// `pkg/util.py` -> `pkg.util`; `app.py` -> `app`.
pub fn stem_module(path: &str) -> String {
    let p = path.trim_end_matches(".py").trim_end_matches(".rs");
    let p = Path::new(p);
    let mut comps: Vec<String> = p
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if let Some(last) = comps.last_mut() {
        let l = last.clone();
        *last = l
            .trim_end_matches(".go")
            .trim_end_matches(".ts")
            .trim_end_matches(".tsx")
            .trim_end_matches(".js")
            .to_string();
    }
    comps.retain(|c| c != "src" && c != "lib");
    comps.join(".")
}

/// Extract all files, assign global symbol ids, rewrite references and
/// build the global symbol store. Local id -> global id remapping happens
/// here so adapters stay stateless.
pub fn extract_project(
    files: &[(String, LangId, String)],
    registry: &LanguageRegistry,
) -> ExtractedProject {
    let mut all_facts: Vec<LangFacts> = vec![];
    let mut symbols: Vec<SymbolFact> = vec![];
    let mut next_id = SymbolId(0);

    for (path, _lang, src) in files {
        let Some(adapter) = registry.adapter_for(path) else {
            continue;
        };
        let mut facts = adapter.extract(path, src);

        // Assign global ids in file order and rewrite references.
        let mut map: BTreeMap<SymbolId, SymbolId> = BTreeMap::new();
        for sym in &mut facts.symbols {
            let local = sym.id;
            sym.id = next_id;
            sym.loc.file = FileId(all_facts.len() as u32);
            map.insert(local, next_id);
            next_id = SymbolId(next_id.0 + 1);
        }
        rewrite(&mut facts, &map);
        symbols.append(&mut facts.symbols);
        all_facts.push(facts);
    }

    let store = SymbolStore::build(symbols);
    resolution::resolve_project(&store, &mut all_facts);

    ExtractedProject {
        store,
        per_file: all_facts,
    }
}

pub struct ExtractedProject {
    pub store: SymbolStore,
    pub per_file: Vec<LangFacts>,
}

/// Rewrite local symbol references to global ids.
fn rewrite(facts: &mut LangFacts, map: &BTreeMap<SymbolId, SymbolId>) {
    let m = |local: SymbolId| -> SymbolId { map.get(&local).copied().unwrap_or(local) };

    for c in &mut facts.calls {
        c.caller = m(c.caller);
        if let Callee::Resolved(id) = c.callee {
            c.callee = Callee::Resolved(m(id));
        }
    }
    for r in &mut facts.returns {
        r.symbol = m(r.symbol);
    }
    for e in &mut facts.exceptions {
        e.symbol = m(e.symbol);
    }
    for a in &mut facts.assignments {
        a.symbol = m(a.symbol);
    }
    for i in &mut facts.ifs {
        i.symbol = m(i.symbol);
    }
    for l in &mut facts.loops {
        l.symbol = m(l.symbol);
    }
    for l in &mut facts.literals {
        l.symbol = m(l.symbol);
    }
    for s in &mut facts.sources {
        s.symbol = m(s.symbol);
    }
    for s in &mut facts.sinks {
        s.symbol = m(s.symbol);
    }
    for im in &mut facts.imports {
        im.module_symbol = m(im.module_symbol);
    }
    for a in &mut facts.async_facts {
        a.symbol = m(a.symbol);
    }
}

/// Group `ImportFact`s per module path for resolution.
pub fn imports_by_path(per_file: &[LangFacts]) -> BTreeMap<String, Vec<ImportFact>> {
    let mut out: BTreeMap<String, Vec<ImportFact>> = BTreeMap::new();
    for f in per_file {
        out.entry(f.path.clone()).or_default().extend(f.imports.iter().cloned());
    }
    out
}
