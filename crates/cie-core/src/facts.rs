//! Typed facts emitted by language adapters (PRD §48) and the semantic
//! program model (FR-1). Adapters produce per-file `LangFacts`; the engine
//! merges them into a `SymbolStore` and resolves cross-file calls.

use crate::types::{FileId, LangId, RevId, SourceLocation, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Module,
    Class,
    Interface,
    Struct,
    Trait,
    Enum,
    Function,
    Method,
    Test,
    Constant,
}

impl SymbolKind {
    pub fn is_function_like(self) -> bool {
        matches!(self, SymbolKind::Function | SymbolKind::Method | SymbolKind::Test)
    }

    pub fn is_type_like(self) -> bool {
        matches!(
            self,
            SymbolKind::Class
                | SymbolKind::Interface
                | SymbolKind::Struct
                | SymbolKind::Trait
                | SymbolKind::Enum
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolFact {
    pub id: SymbolId,
    pub kind: SymbolKind,
    pub name: String,
    /// Dotted qualified name, e.g. `auth.authorize` or `Cart.add_item`.
    pub qual_name: String,
    pub loc: SourceLocation,
    pub lang: LangId,
    pub parent: Option<SymbolId>,
    pub rev: RevId,
    pub doc: Option<String>,
}

impl SymbolFact {
    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            SymbolKind::Module => "module",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Struct => "struct",
            SymbolKind::Trait => "trait",
            SymbolKind::Enum => "enum",
            SymbolKind::Function => "fn",
            SymbolKind::Method => "method",
            SymbolKind::Test => "test",
            SymbolKind::Constant => "const",
        }
    }

    /// Heuristic test detection across the five shipped languages.
    pub fn is_test(&self) -> bool {
        if self.kind == SymbolKind::Test {
            return true;
        }
        let n = self.name.to_ascii_lowercase();
        n.starts_with("test_")
            || n.ends_with("_test")
            || n.starts_with("test")
            && (self.lang == LangId::Rust || self.lang == LangId::Go)
            || n == "setup"
            || n.starts_with("should_")
    }

    pub fn is_function_like(&self) -> bool {
        self.kind.is_function_like()
    }
}

/// Classification of well-known callee names/effects detected at the
/// adapter layer. Adapters resolve project-internal callees to `Resolved`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BuiltinKind {
    Print,
    Log,
    DbExecute,
    DbQuery,
    Network,
    FileRead,
    FileWrite,
    Crypto,
    Random,
    Env,
    ProcessExec,
    Eval,
    Assert,
    Sleep,
    Lock,
    Time,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Callee {
    Resolved(SymbolId),
    Builtin(BuiltinKind),
    /// Unresolved with explicit reason (acceptance #6): dynamic_attribute,
    /// unresolved_name, external_module, generated_code, ...
    Unresolved(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub caller: SymbolId,
    pub callee_text: String,
    pub callee: Callee,
    pub line: u32,
    /// True when the call lexically occurs inside a loop body (N+1 hint).
    pub in_loop: bool,
    /// Names of variables passed as arguments (taint analysis inputs).
    pub arg_vars: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnSite {
    pub symbol: SymbolId,
    pub line: u32,
    pub expr: String,
    /// `Some("true")` for `return True`, etc.
    pub constant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionFact {
    pub symbol: SymbolId,
    pub kind_text: String,
    pub is_bare: bool,
    pub line: u32,
    /// True when raised inside an except handler (re-raise / wrap).
    pub in_handler: bool,
    /// For except-clause records: last line of the handler body (0 if N/A).
    pub handler_end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RhsKind {
    Call,
    Input,
    FString,
    Vars,
    Literal,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentFact {
    pub symbol: SymbolId,
    pub var: String,
    pub line: u32,
    pub rhs_kind: RhsKind,
    /// For `Call`: callee text. For `FString`/`Vars`: referenced variables.
    pub rhs_data: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfSite {
    pub symbol: SymbolId,
    pub line: u32,
    pub cond: String,
    pub cond_vars: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LoopKind {
    For,
    While,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopSite {
    pub symbol: SymbolId,
    pub kind: LoopKind,
    pub line: u32,
    pub body_start: u32,
    pub body_end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    RequestInput,
    EnvVar,
    Stdin,
    Argv,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSite {
    pub symbol: SymbolId,
    pub var: String,
    pub line: u32,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SinkKind {
    SqlExecute,
    ShellExec,
    CodeEval,
    FileWrite,
    HtmlRender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinkSite {
    pub symbol: SymbolId,
    pub call_text: String,
    pub arg_vars: Vec<String>,
    pub line: u32,
    pub kind: SinkKind,
    /// True when the sink uses parameterization (e.g. SQL `?`/`%s` placeholders).
    pub parameterized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiteralFact {
    pub symbol: SymbolId,
    pub line: u32,
    pub value: String,
    pub is_string: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFact {
    pub module_symbol: SymbolId,
    pub module: String,
    pub names: Vec<String>,
    /// `import numpy as np` -> Some("np")
    pub alias: Option<String>,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncKind {
    AsyncFn,
    Await,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncFact {
    pub symbol: SymbolId,
    pub kind: AsyncKind,
    pub line: u32,
}

/// Everything one adapter extraction produced for one file. All symbol ids
/// are placeholders (`u32::MAX`) until the engine assigns global ids.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LangFacts {
    pub path: String,
    pub lang: LangId,
    pub analyzer: String,
    pub symbols: Vec<SymbolFact>,
    pub calls: Vec<CallSite>,
    pub returns: Vec<ReturnSite>,
    pub exceptions: Vec<ExceptionFact>,
    pub assignments: Vec<AssignmentFact>,
    pub ifs: Vec<IfSite>,
    pub loops: Vec<LoopSite>,
    pub literals: Vec<LiteralFact>,
    pub sources: Vec<SourceSite>,
    pub sinks: Vec<SinkSite>,
    pub imports: Vec<ImportFact>,
    pub async_facts: Vec<AsyncFact>,
    /// Constructs this adapter could not model — surfaced in evidence.
    pub unsupported: Vec<String>,
}

impl LangFacts {
    pub fn new(path: impl Into<String>, lang: LangId, analyzer: impl Into<String>) -> Self {
        LangFacts {
            path: path.into(),
            lang,
            analyzer: analyzer.into(),
            ..Default::default()
        }
    }
}

/// Global symbol table with lookup indices (PRD §16 semantic model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolStore {
    pub symbols: Vec<SymbolFact>,
    index_qual: BTreeMap<String, SymbolId>,
    index_name: BTreeMap<String, Vec<SymbolId>>,
    index_path: BTreeMap<String, Vec<SymbolId>>,
    index_parent: BTreeMap<u32, Vec<SymbolId>>,
    /// Symbols whose call resolution is still pending (cross-file).
    pub unresolved: BTreeMap<SymbolId, Vec<String>>,
}

impl SymbolStore {
    pub fn build(mut symbols: Vec<SymbolFact>) -> SymbolStore {
        symbols.sort_by_key(|s| s.id);
        let mut store = SymbolStore {
            index_parent: BTreeMap::new(),
            ..Default::default()
        };
        for s in &symbols {
            let id = s.id;
            store.index_qual.insert(s.qual_name.clone(), id);
            store
                .index_name
                .entry(s.name.clone())
                .or_default()
                .push(id);
            store
                .index_path
                .entry(s.loc.path.clone())
                .or_default()
                .push(id);
            if let Some(p) = s.parent {
                store.index_parent.entry(p).or_default().push(id);
            }
        }
        store.symbols = symbols;
        store
    }

    pub fn get(&self, id: SymbolId) -> Option<&SymbolFact> {
        self.symbols.get(id as usize)
    }

    pub fn resolve_qual(&self, q: &str) -> Option<SymbolId> {
        self.index_qual.get(q).copied()
    }

    pub fn by_name(&self, n: &str) -> &[SymbolId] {
        self.index_name.get(n).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn in_path(&self, path: &str) -> &[SymbolId] {
        self.index_path.get(path).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn children_of(&self, id: SymbolId) -> &[SymbolId] {
        self.index_parent
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Unique bare-name match across the whole store.
    pub fn resolve_unique_name(&self, name: &str) -> Option<SymbolId> {
        match self.by_name(name) {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Function-like symbol ids (functions, methods, tests).
    pub fn function_ids(&self) -> Vec<SymbolId> {
        self.symbols
            .iter()
            .filter(|s| s.is_function_like())
            .map(|s| s.id)
            .collect()
    }

    /// Descendants of a class (methods).
    pub fn methods_of(&self, class: SymbolId) -> Vec<SymbolId> {
        self.children_of(class).to_vec()
    }

    pub fn module_ids(&self) -> Vec<SymbolId> {
        self.symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .map(|s| s.id)
            .collect()
    }

    /// Files that declare at least one symbol.
    pub fn files(&self) -> BTreeSet<String> {
        self.symbols.iter().map(|s| s.loc.path.clone()).collect()
    }

    pub fn file_of(&self, id: SymbolId) -> Option<&str> {
        self.get(id).map(|s| s.loc.path.as_str())
    }

    /// Allocate a placeholder id — adapters use `u32::MAX` and the engine
    /// rewrites; this helper exists for tests building stores directly.
    pub fn push(&mut self, mut fact: SymbolFact) -> SymbolId {
        fact.id = self.symbols.len() as SymbolId;
        self.symbols.push(fact);
        (self.symbols.len() - 1) as SymbolId
    }

    pub fn file_id_of(&self, path: &str) -> FileId {
        // Stable small file ids: hash of the path modulo 2^31 is overkill;
        // derive from module symbol location.
        self.symbols
            .iter()
            .find(|s| s.loc.path == path)
            .map(|s| s.loc.file)
            .unwrap_or(0)
    }
}
