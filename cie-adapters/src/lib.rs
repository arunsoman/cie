//! Language adapters, registry and the project extraction pipeline.
//!
//! Extension path (PRD §67 / acceptance #9): adding a language =
//! one adapter + ONE registration line in `default_registry()` +
//! zero core changes.

pub mod effects;
pub mod light_engine;
pub mod resolution;

use cie_core::types::{FileId, FileTable, SymbolId};
use cie_semantic::facts::{Callee, ImportFact, LangFacts, SymbolFact, SymbolStore};
use cie_semantic::LangId;
use std::collections::BTreeMap;
use std::path::Path;

pub trait LanguageAdapter: Send + Sync {
    fn id(&self) -> LangId;
    fn extensions(&self) -> &'static [&'static str];
    fn analyzer(&self) -> &'static str;
    fn extract(&self, path: &str, src: &str) -> LangFacts;

    // ---- TASK-011 semantics tables (§16.1) — data on the adapter, so rules
    // consult tables instead of hardcoding language knowledge (FR-3, global
    // rule 6). Defaults describe an unspecified hybrid language; each
    // adapter vouches for its own rows (Python overrides below).

    /// Dynamic vs static dispatch (§16.1): rules that assume a resolved
    /// callee must refuse to fire where `dispatch_kind()` vouches dynamic.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Hybrid
    }

    /// Numeric semantics row: overflow, coercion, precision.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics::default()
    }

    /// Framework conventions the adapter knows about (decorators, ORM
    /// query sets, test frameworks, ...).
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[]
    }

    /// Error/exception model row (§16, TASK-022): how THIS language
    /// signals failure. The SemanticsGuard consults this to admit or
    /// refuse cross-language rules (FR-3) — one table row per adapter,
    /// never a global constant.
    fn error_model(&self) -> ErrorModel {
        ErrorModel::Unspecified
    }

    /// TASK-067: per-language parse diagnostics (FR-3: each adapter owns
    /// its own syntax judgment). Empty = clean; non-empty entries become
    /// `FileError::Parse` for that path only — the batch never aborts.
    fn parse_diagnostics(&self, _src: &str) -> Vec<String> {
        Vec::new()
    }

    fn matches_extension(&self, path: &str) -> bool {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        self.extensions().contains(&ext)
    }
}

/// Error/exception model (TASK-022, §16): the table row the guard reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum ErrorModel {
    /// raise/throw/try/catch — a completed call ESTABLISHES its effect
    /// (Python, JS/TS, Java, C++, C#, PHP).
    Raise,
    /// Errors are values — a bare call does NOT establish success; the
    /// error must be checked (Go `if err != nil`, Rust `Result`/`?`).
    ErrorValue,
    /// Error-code conventions (-1/NULL/errno) — the caller must check the
    /// returned code (C).
    ErrorCode,
    #[default]
    Unspecified,
}

/// TASK-022: one admission per registered adapter — the cross-language
/// FR-3 verdict with its FR-4 evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    pub lang: LangId,
    pub fired: bool,
    pub evidence: cie_core::evidence::Evidence,
}

/// Cross-language semantics guard (FR-3 over FR-18): matrix-driven —
/// iterates `registry.adapters()` (no hardcoded language list), so adding
/// an adapter automatically extends the matrix. A candidate rule fires for
/// a language only where that language's adapter TABLES vouch for it;
/// every refusal is recorded with evidence (FR-4, global rule 6), never
/// silently dropped and never converted into a clean verdict (global rule 3).
pub struct SemanticsGuard;

/// A candidate rule under evaluation (Houdini-style candidate, cf. §7):
/// "a call to `target` establishes `effect`".
pub struct CandidateRule {
    /// Callee name the rule is about (e.g. "authorize").
    pub call_target: String,
    /// Effect the rule claims the call establishes.
    pub effect: String,
}

impl SemanticsGuard {
    /// Evaluate `rule` over every registered adapter against the fixture
    /// sources (one file per language, same `authorize()` intent).
    pub fn evaluate(
        rule: &CandidateRule,
        registry: &LanguageRegistry,
        fixtures: &std::collections::BTreeMap<String, String>,
    ) -> Vec<Admission> {
        let mut out = vec![];
        for adapter in registry.adapters() {
            let lang = adapter.id();
            let mut ev = cie_core::evidence::Evidence::new(0, "semantics-guard", 0);
            // Locate this language's fixture (extension match).
            let ext = adapter.extensions().first().copied().unwrap_or("");
            let Some((path, src)) = fixtures
                .iter()
                .find(|(p, _)| p.ends_with(&format!(".{ext}")))
                .map(|(p, s)| (p.clone(), s.clone()))
            else {
                ev.assumptions
                    .push(format!("no fixture for {ext} — admission still recorded (FR-4)"));
                out.push(Admission { lang, fired: false, evidence: ev });
                continue;
            };
            let mut facts = adapter.extract(&path, &src);
            // Same-file resolution: the guard evaluates one fixture file per
            // language, so run the cross-file resolver over this single file
            // (extract_project does this for multi-file ingests).
            {
                let mut table = cie_core::types::FileTable::new();
                table.intern_with_source(&path, src.as_bytes());
                let store =
                    cie_semantic::facts::SymbolStore::build_with_files(facts.symbols.clone(), table);
                resolution::resolve_project(&store, std::slice::from_mut(&mut facts));
            }
            let model = adapter.error_model();
            // The rule can only fire where BOTH hold (§16.1 vouches):
            //  1. the language's error model is Raise — a completed call
            //     establishes its effect (Go/Rust/C vouch AGAINST);
            //  2. the `rule.call_target` call resolves to a symbol in-tree.
            let resolved = facts
                .calls
                .iter()
                .any(|c| c.callee_text == rule.call_target && c.callee.is_resolved());
            match model {
                ErrorModel::Raise if resolved => {
                    ev.assumptions.push(format!(
                        "rule admitted: Raise model + resolved call ({} vouches)",
                        adapter.analyzer()
                    ));
                    out.push(Admission { lang, fired: true, evidence: ev });
                }
                ErrorModel::Raise => {
                    ev.assumptions.push(format!(
                        "refused: Raise model but the {:?} call did not resolve in-tree (FR-4)",
                        rule.call_target
                    ));
                    for c in &facts.calls {
                        if let Callee::Unresolved(r) = &c.callee {
                            ev.unresolved
                                .push(format!("{}: {}", c.callee_text, r.clone()));
                        }
                    }
                    out.push(Admission { lang, fired: false, evidence: ev });
                }
                model => {
                    ev.assumptions.push(format!(
                        "refused: {:?} error model — a bare {:?} call does not establish {:?} (FR-3: {} vouches against)",
                        model, rule.call_target, rule.effect, adapter.analyzer()
                    ));
                    for c in &facts.calls {
                        if let Callee::Unresolved(r) = &c.callee {
                            ev.unresolved
                                .push(format!("{}: {}", c.callee_text, r.clone()));
                        }
                    }
                    out.push(Admission { lang, fired: false, evidence: ev });
                }
            }
        }
        out
    }
}

/// Local resolvedness check used by the guard.
trait ResolutionMark {
    fn is_resolved(&self) -> bool;
}
impl ResolutionMark for Callee {
    fn is_resolved(&self) -> bool {
        matches!(self, Callee::Resolved(_))
    }
}

/// Dispatch model of a language (TASK-011, §16.1 comment).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum DispatchKind {
    /// Calls resolve to one implementation at compile time.
    Static,
    /// Calls resolve at runtime (attribute lookup, monkey-patching).
    Dynamic,
    /// Mix (e.g. C#: static by default, `dynamic`/reflection where used).
    #[default]
    Hybrid,
}

/// What happens when an integer computation exceeds its range (§16.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum OverflowPolicy {
    /// Arbitrary-precision integers — overflow cannot happen (Python).
    ArbitraryPrecision,
    /// Two's-complement wraparound, DEFINED behavior (Java int/long, Go,
    /// C unsigned; §16 calls this `WrapAround`).
    Wrapping,
    /// Signed overflow is undefined behavior (C/C++ signed, §16.5/16.6):
    /// UB sites must be recorded as assumptions, never assumed away (FR-4).
    UndefinedBehavior,
    /// Panics on overflow in debug builds; release wrap is recorded as an
    /// assumption (Rust, §16.9 `PanicInDebug`).
    PanicInDebug,
    /// Overflow is a runtime fault / must be checked (C# `checked{}`, §16.7).
    Checked,
    #[default]
    Unspecified,
}

/// How values of different numeric types mix (§16.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum CoercionPolicy {
    /// Conversions must be written down (Rust, Go).
    ExplicitOnly,
    /// Numeric types mix implicitly (C promotions, JS `+`).
    ImplicitNumeric,
    /// Ints promote to f64 on overflow (PHP, §16.10).
    IntToFloatOnOverflow,
    /// Anything coerces to anything (dynamic languages).
    #[default]
    ImplicitAny,
}

/// Floating-point / integer precision guarantees (§16.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum Precision {
    /// IEEE-754 binary64 only (JS numbers, Python floats).
    #[default]
    Binary64,
    Binary32,
    /// Multiple integer widths (C, Go, Java, C#).
    MixedWidths,
    /// Arbitrary-precision integers plus binary64 floats (Python).
    Arbitrary,
}

/// Numeric semantics table row (TASK-011): one struct, three vouched cells.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct NumericSemantics {
    pub overflow: OverflowPolicy,
    pub coercion: CoercionPolicy,
    pub precision: Precision,
}

/// A framework convention an adapter vouches for (TASK-011): e.g.
/// `FrameworkRule { name: "flask-route", convention: "decorated function is
/// an HTTP entry point" }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FrameworkRule {
    pub name: &'static str,
    pub convention: &'static str,
}

/// One-line-per-language registration (acceptance #9).
pub struct LanguageRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    /// An empty registry (TASK-011 sketch: `LanguageRegistry::new()`).
    pub fn new() -> Self {
        LanguageRegistry { adapters: vec![] }
    }

    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn adapter_for(&self, path: &str) -> Option<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .find(|a| a.matches_extension(path))
            .map(|a| a.as_ref())
    }

    /// TASK-011 lookup helper: resolve by bare extension ("tl", not ".tl").
    pub fn by_extension(&self, ext: &str) -> Option<&dyn LanguageAdapter> {
        let ext = ext.trim_start_matches('.');
        self.adapters
            .iter()
            .find(|a| a.extensions().contains(&ext))
            .map(|a| a.as_ref())
    }

    pub fn adapters(&self) -> &[Box<dyn LanguageAdapter>] {
        &self.adapters
    }
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
    let mut table = FileTable::new();

    for (path, _lang, src) in files {
        let Some(adapter) = registry.adapter_for(path) else {
            continue;
        };
        let mut facts = adapter.extract(path, src);

        // Intern path + source (line index) in file order so FileId(i)
        // indexes table entry i (TASK-006: spans carry no path).
        table.intern_with_source(path, src.as_bytes());

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

    let store = SymbolStore::build_with_files(symbols, table.clone());
    resolution::resolve_project(&store, &mut all_facts);

    ExtractedProject {
        store,
        per_file: all_facts,
        files: table,
    }
}

pub struct ExtractedProject {
    pub store: SymbolStore,
    pub per_file: Vec<LangFacts>,
    /// File table for the pass (path + line index per FileId).
    pub files: FileTable,
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
        out.entry(f.path.clone())
            .or_default()
            .extend(f.imports.iter().cloned());
    }
    out
}
