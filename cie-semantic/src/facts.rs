//! Typed facts emitted by language adapters (PRD §48) and the semantic
//! program model (FR-1). Adapters produce per-file `LangFacts`; the engine
//! merges them into a `SymbolStore` and resolves cross-file calls.

use crate::lang::LangId;
use cie_core::types::{FileId, FileTable, RevId, SourceLocation, SymbolId};
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
    /// SQL: `CREATE TABLE` (v1.2.1).
    Table,
    /// SQL: `CREATE VIEW` / `MATERIALIZED VIEW` (v1.2.1).
    View,
}

impl SymbolKind {
    pub fn is_function_like(self) -> bool {
        matches!(
            self,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Test
        )
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
    /// Annotations/decorators/attributes found directly on this symbol's
    /// declaration, exactly as written (`@Service`, `@app.route("/x")`,
    /// `#[tokio::main]`, `[Authorize]` ...). Never inferred — empty when the
    /// language has no such construct, or none were written. Adapter-local
    /// extraction; no cross-symbol resolution (a class's annotations don't
    /// propagate to its methods).
    #[serde(default)]
    pub annotations: Vec<String>,
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
            SymbolKind::Table => "table",
            SymbolKind::View => "view",
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
            || n.starts_with("test") && (self.lang == LangId::Rust || self.lang == LangId::Go)
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
    /// The receiver expression as written ("submissionService" in
    /// `submissionService.submit(req)`), when the call has one. Evidence for
    /// receiver-typed resolution downstream — never a resolved type by itself.
    #[serde(default)]
    pub receiver: Option<String>,
    /// Argument count at the call site (positional + named), when counted.
    #[serde(default)]
    pub arguments: Option<u32>,
    /// Byte span [start, end) of the call expression within the file — the
    /// per-occurrence identity that lets repeated calls stay distinct.
    #[serde(default)]
    pub span: (u32, u32),
}

/// Scope of a variable binding recorded by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BindingKind {
    /// Class-level field (`private final SubmissionService submissionService;`).
    Field,
    /// Function parameter with (possibly absent) declared type.
    Param,
    /// Local variable declaration/assignment inside a function body.
    Local,
    /// `self.X = ...` attribute assignment (class-scoped in spirit).
    SelfAttr,
}

/// One name → type/binding-evidence record. Adapters emit what the syntax
/// literally declares: a declared type text (Java fields/params/locals) or
/// the assigned right-hand-side expression (Python assignments). Downstream
/// resolution (cie-analysis) turns these into receiver-type evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingFact {
    /// Owner symbol: the enclosing function (Param/Local/SelfAttr) or the
    /// enclosing class (Field).
    pub owner: SymbolId,
    pub name: String,
    pub kind: BindingKind,
    /// Declared type text when the syntax states one ("SubmissionService").
    #[serde(default)]
    pub type_text: Option<String>,
    /// Right-hand-side expression text when the binding is an assignment
    /// ("self.conn.cursor()").
    #[serde(default)]
    pub rhs_text: Option<String>,
    pub line: u32,
}

/// A type declaration's inheritance surface (extends/implements), so
/// receiver-type classification can recognize a DB API through what the
/// type itself declares (e.g. `extends JpaRepository<...>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDeclFact {
    pub symbol: SymbolId,
    pub name: String,
    /// The extends/implements clause as written (may be empty).
    pub supers: String,
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
pub struct ExceptionSite {
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
    /// §14.1 byte span of the branch (TASK-032: findings cite the branch).
    #[serde(default)]
    pub loc: cie_core::types::SourceLocation,
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
    /// §14.1 byte span of the loop header (TASK-032).
    #[serde(default)]
    pub loc: cie_core::types::SourceLocation,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AsyncKind {
    /// async fn / async function definition (TASK-012/013).
    AsyncFn,
    /// await expression.
    Await,
    /// Thread/executor/goroutine/task spawn (TASK-015..020 §16 tables).
    Spawn,
    /// Channel send/receive or select (TASK-015..020 §16 tables).
    Channel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncSite {
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
    pub exceptions: Vec<ExceptionSite>,
    pub assignments: Vec<AssignmentFact>,
    pub ifs: Vec<IfSite>,
    pub loops: Vec<LoopSite>,
    pub literals: Vec<LiteralFact>,
    pub sources: Vec<SourceSite>,
    pub sinks: Vec<SinkSite>,
    pub imports: Vec<ImportFact>,
    pub async_facts: Vec<AsyncSite>,
    /// Constructs this adapter could not model — surfaced in evidence.
    pub unsupported: Vec<String>,
    /// Variable bindings the syntax declares (fields/params/locals/self-attrs)
    /// — the receiver-type evidence table for downstream resolution.
    #[serde(default)]
    pub bindings: Vec<BindingFact>,
    /// Type declarations' inheritance surfaces (extends/implements).
    #[serde(default)]
    pub type_decls: Vec<TypeDeclFact>,
    /// Resource-ownership effects (TASK-016/017/020): malloc/free/realloc,
    /// borrows, moves — the §14.3 `Owns`/`Reads`/`Writes` raw material.
    #[serde(default)]
    pub resource_effects: Vec<ResourceEffect>,
}

/// One resource-ownership effect (§14.3 relations input): `kind_text` is
/// the adapter-vouched operation (`malloc`, `free`, `realloc`, `move`,
/// `borrow`, `drop`); `resource` is the expression text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEffect {
    pub symbol: SymbolId,
    pub kind_text: String,
    pub resource: String,
    pub line: u32,
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

/// §9 `AnalysisInput.facts` (TASK-010): every fact family plus the typed
/// relation graph — the exact payload of an analysis run. `dedup()` collapses
/// structurally identical facts deterministically (stable order by
/// `(loc, id)` per family) and is idempotent, so the same revision always
/// yields byte-stable facts (FR-4). `check_integrity()` rejects references
/// to absent symbols as `CieError::DanglingReference` — dangling ids are
/// errors, never silent edges (global rule 3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TypedFacts {
    pub symbols: Vec<SymbolFact>,
    pub calls: Vec<CallSite>,
    pub returns: Vec<ReturnSite>,
    pub exceptions: Vec<ExceptionSite>,
    pub assignments: Vec<AssignmentFact>,
    pub ifs: Vec<IfSite>,
    pub loops: Vec<LoopSite>,
    pub sources: Vec<SourceSite>,
    pub sinks: Vec<SinkSite>,
    pub imports: Vec<ImportFact>,
    pub async_facts: Vec<AsyncSite>,
    pub relations: cie_relations::RelationGraph,
}

impl TypedFacts {
    pub fn push_symbol(&mut self, fact: SymbolFact) {
        self.symbols.push(fact);
    }

    pub fn push_call(&mut self, site: CallSite) {
        self.calls.push(site);
    }

    pub fn push_exception(&mut self, fact: ExceptionSite) {
        self.exceptions.push(fact);
    }

    /// Deterministic, idempotent dedup: per family, sort by the stable
    /// `(loc, id)` key, then collapse adjacent structural duplicates.
    pub fn dedup(&mut self) {
        self.symbols.sort_by_key(|s| (s.loc.file, s.loc.start, s.loc.end, s.id, s.kind));
        self.symbols.dedup();

        self.calls
            .sort_by_key(|c| (c.caller, c.line, c.callee_text.clone()));
        self.calls.dedup();

        self.returns
            .sort_by_key(|r| (r.symbol, r.line, r.expr.clone()));
        self.returns.dedup();

        self.exceptions
            .sort_by_key(|e| (e.symbol, e.line, e.kind_text.clone()));
        self.exceptions.dedup();

        self.assignments
            .sort_by_key(|a| (a.symbol, a.line, a.var.clone()));
        self.assignments.dedup();

        self.ifs.sort_by_key(|i| (i.symbol, i.line, i.cond.clone()));
        self.ifs.dedup();

        self.loops.sort_by_key(|l| (l.symbol, l.line, l.body_start));
        self.loops.dedup();

        self.sources
            .sort_by_key(|s| (s.symbol, s.line, s.var.clone()));
        self.sources.dedup();

        self.sinks
            .sort_by_key(|s| (s.symbol, s.line, s.call_text.clone()));
        self.sinks.dedup();

        self.imports
            .sort_by_key(|i| (i.module_symbol, i.line, i.module.clone()));
        self.imports.dedup();

        self.async_facts.sort_by_key(|a| (a.symbol, a.line, a.kind));
        self.async_facts.dedup();
    }

    /// FR-2/FR-4 integrity gate: every `SymbolId` referenced by any family
    /// must exist in `symbols`; resolved callees must exist too.
    pub fn check_integrity(&self) -> Result<(), cie_core::CieError> {
        let known: std::collections::BTreeSet<SymbolId> =
            self.symbols.iter().map(|s| s.id).collect();
        let dangler = |what: String, id: SymbolId, out: &mut Result<(), cie_core::CieError>| {
            if !known.contains(&id)
                && out.is_ok() {
                    *out = Err(cie_core::CieError::DanglingReference {
                        what: format!("{what} {id:?}"),
                    });
                }
        };
        let mut out = Ok(());
        for s in &self.symbols {
            if let Some(p) = s.parent {
                dangler("parent of".into(), p, &mut out);
            }
        }
        for c in &self.calls {
            dangler("caller of call".into(), c.caller, &mut out);
            if let Callee::Resolved(id) = c.callee {
                dangler("callee of call".into(), id, &mut out);
            }
        }
        for r in &self.returns {
            dangler("return in".into(), r.symbol, &mut out);
        }
        for e in &self.exceptions {
            dangler("exception in".into(), e.symbol, &mut out);
        }
        for a in &self.assignments {
            dangler("assignment in".into(), a.symbol, &mut out);
        }
        for i in &self.ifs {
            dangler("if in".into(), i.symbol, &mut out);
        }
        for l in &self.loops {
            dangler("loop in".into(), l.symbol, &mut out);
        }
        for s in &self.sources {
            dangler("source in".into(), s.symbol, &mut out);
        }
        for s in &self.sinks {
            dangler("sink in".into(), s.symbol, &mut out);
        }
        for i in &self.imports {
            dangler("import in".into(), i.module_symbol, &mut out);
        }
        for a in &self.async_facts {
            dangler("async in".into(), a.symbol, &mut out);
        }
        out
    }
}

/// Global symbol table with lookup indices (PRD §16 semantic model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolStore {
    pub symbols: Vec<SymbolFact>,
    /// File-id → path table for the extraction pass that produced the
    /// symbols (TASK-006: byte spans carry no path; rendering resolves
    /// through this table).
    pub files: FileTable,
    index_qual: BTreeMap<String, SymbolId>,
    index_name: BTreeMap<String, Vec<SymbolId>>,
    index_path_by_file: BTreeMap<FileId, Vec<SymbolId>>,
    index_parent: BTreeMap<SymbolId, Vec<SymbolId>>,
    /// Symbols whose call resolution is still pending (cross-file).
    pub unresolved: BTreeMap<SymbolId, Vec<String>>,
}

impl SymbolStore {
    pub fn build(mut symbols: Vec<SymbolFact>) -> SymbolStore {
        Self::build_with_files(symbols, FileTable::new())
    }

    /// Build with the extraction pass's file table (TASK-006).
    pub fn build_with_files(mut symbols: Vec<SymbolFact>, files: FileTable) -> SymbolStore {
        symbols.sort_by_key(|s| s.id);
        let mut store = SymbolStore {
            files,
            index_parent: BTreeMap::new(),
            ..Default::default()
        };
        for s in &symbols {
            let id = s.id;
            store.index_qual.insert(s.qual_name.clone(), id);
            store.index_name.entry(s.name.clone()).or_default().push(id);
            store.index_path_by_file.entry(s.loc.file).or_default().push(id);
            if let Some(p) = s.parent {
                store.index_parent.entry(p).or_default().push(id);
            }
        }
        store.symbols = symbols;
        store
    }

    pub fn get(&self, id: SymbolId) -> Option<&SymbolFact> {
        self.symbols.get(id.0 as usize)
    }

    pub fn resolve_qual(&self, q: &str) -> Option<SymbolId> {
        self.index_qual.get(q).copied()
    }

    pub fn by_name(&self, n: &str) -> &[SymbolId] {
        self.index_name.get(n).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn in_path(&self, path: &str) -> Vec<SymbolId> {
        let fid = self
            .files
            .paths
            .iter()
            .position(|p| p == path)
            .map(|i| FileId(i as u32));
        match fid {
            Some(f) => self.index_path_by_file.get(&f).cloned().unwrap_or_default(),
            None => Vec::new(),
        }
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

    /// Files that declare at least one symbol (path via the file table).
    pub fn files(&self) -> BTreeSet<String> {
        let mut used: BTreeSet<FileId> = BTreeSet::new();
        for s in &self.symbols {
            used.insert(s.loc.file);
        }
        used.into_iter().map(|f| self.files.path(f).to_string()).collect()
    }

    pub fn file_of(&self, id: SymbolId) -> Option<&str> {
        let f = self.get(id).map(|s| s.loc.file)?;
        Some(self.files.path(f))
    }

    /// Allocate a placeholder id — adapters use `u32::MAX` and the engine
    /// rewrites; this helper exists for tests building stores directly.
    pub fn push(&mut self, mut fact: SymbolFact) -> SymbolId {
        fact.id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(fact);
        SymbolId((self.symbols.len() - 1) as u32)
    }

    pub fn file_id_of(&self, path: &str) -> FileId {
        // Resolve through the file table (TASK-006: spans carry no path).
        FileId(
            self.files
                .paths
                .iter()
                .position(|p| p == path)
                .unwrap_or(0) as u32,
        )
    }
}
