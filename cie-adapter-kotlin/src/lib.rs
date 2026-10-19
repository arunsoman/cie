//! Kotlin deep adapter (v1.2.2, TASK-015 pattern): JVM raise semantics,
//! default-final dispatch, coroutines as spawn.
//!
//! Own tree walk over `tree-sitter-kotlin-ng` (parse → extract contract per
//! §16.1): classes/interfaces/objects as symbols, `fun` declarations as
//! methods/functions, call expressions with dispatch classification,
//! `throw`/`try`/`catch` as raised/caught facts, `import` headers as module
//! facts, `val`/`var` properties and assignments as assignment facts,
//! coroutines (`launch`/`async`) and JVM threads/executors as spawn.
//!
//! Semantics tables (§16, vouched here — FR-3):
//! - dispatch: `Dynamic` — same JVM virtual dispatch as Java; Kotlin
//!   methods are final by default and `open`/`override` re-enable runtime
//!   dispatch, so dotted instance calls stay unresolved (FR-4)
//! - numeric: JVM fixed-width two's-complement wraparound for Int/Long
//!   (`OverflowPolicy::Wrapping`); Kotlin has NO implicit widening
//!   conversions (`CoercionPolicy::ExplicitOnly`); mixed widths
//! - exceptions: `throw`/`try`/`catch`/`finally` — JVM raise model
//! - async: coroutines `launch`/`async` are structured-concurrency spawns;
//!   `Thread(...).start()`/executors/`CompletableFuture` match the Java row
//! - reflection (`Class.forName`, `kotlin.reflect.*`) is `Reflective` +
//!   unresolved — never pretended resolvable (FR-4)
//! - `.kts` (Gradle Kotlin DSL) parses as plain Kotlin; plugins/tasks are
//!   build logic, recorded as an honesty note, not silently skipped

use cie_adapters::{
    effects, CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    AsyncKind, AsyncSite, Callee, CallSite, ExceptionSite, ImportFact, LangFacts, LoopKind,
    LoopSite, RhsKind, SymbolFact, SymbolKind,
};
use cie_semantic::LangId;
use std::collections::BTreeSet;
use tree_sitter::Parser;

pub struct KotlinAdapter;

pub fn to_language() -> tree_sitter::Language {
    tree_sitter_kotlin_ng::LANGUAGE.into()
}

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

struct KotlinCtx<'a> {
    bytes: &'a [u8],
    facts: LangFacts,
    class_stack: Vec<SymbolId>,
    func: SymbolId,
    next_id: u32,
    /// Names of `fun` declarations visible in-tree — resolution.rs links
    /// same-name unqualified calls through these (the "pending" convention).
    fun_names: BTreeSet<String>,
    /// One-time honesty notes (deduplicated).
    notes: BTreeSet<String>,
}

impl<'a> KotlinCtx<'a> {
    fn id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    fn line(&self, node: tree_sitter::Node) -> u32 {
        cie_core::types::line_of(self.bytes, node.start_byte() as u32)
    }

    fn in_loop(&self, node: tree_sitter::Node) -> bool {
        let l = self.line(node);
        self.facts
            .loops
            .iter()
            .any(|lp| lp.line <= l && l <= lp.body_end)
    }
}

/// The class/interface/object a member lives in, as a qual-name prefix.
fn current_qual(ctx: &KotlinCtx) -> String {
    ctx.class_stack
        .last()
        .and_then(|p| ctx.facts.symbols.get(p.0 as usize))
        .map(|s| s.qual_name.clone())
        .unwrap_or_default()
}

fn children_of<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

fn has_interface_keyword(node: tree_sitter::Node, bytes: &[u8]) -> bool {
    children_of(node)
        .iter()
        .any(|c| !c.is_named() && text_of(*c, bytes) == "interface")
}

fn has_annotation(node: tree_sitter::Node, bytes: &[u8], ann: &str) -> bool {
    children_of(node)
        .iter()
        .any(|c| c.kind() == "modifiers" && text_of(*c, bytes).contains(ann))
}

fn walk_children(ctx: &mut KotlinCtx, node: tree_sitter::Node) {
    for child in children_of(node) {
        walk_kotlin(ctx, child);
    }
}

/// The callee of a `call_expression` is the first named child that is not
/// the argument list / type arguments / annotated lambda.
fn callee_of_call(node: tree_sitter::Node, bytes: &[u8]) -> String {
    for child in children_of(node) {
        match child.kind() {
            "value_arguments" | "type_arguments" | "annotated_lambda" => continue,
            _ => return text_of(child, bytes).trim().to_string(),
        }
    }
    String::new()
}

fn arg_vars_of_call(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    for child in children_of(node) {
        if child.kind() != "value_arguments" {
            continue;
        }
        for va in children_of(child) {
            if va.kind() != "value_argument" {
                continue;
            }
            for e in children_of(va) {
                if e.kind() == "identifier" {
                    out.push(text_of(e, bytes).to_string());
                }
            }
        }
    }
    out
}

/// Coroutine/JVM spawn detection (§16: same row as the Java adapter, plus
/// structured-concurrency `launch`/`async`).
fn is_spawn_call(text: &str) -> bool {
    let t = text.trim();
    t.contains("launch") || t.contains("async") || t.contains("GlobalScope")
        || t.contains("new Thread") || t.contains("Thread(") && t.ends_with(".start")
        || t.contains("ExecutorService") || t.contains("CompletableFuture")
        || t.contains("Executors.")
}

/// Identifiers appearing anywhere in a subtree (condition reads).
fn collect_identifiers(node: tree_sitter::Node, bytes: &[u8], out: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        out.insert(text_of(node, bytes).to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, bytes, out);
    }
}

fn walk_kotlin(ctx: &mut KotlinCtx, node: tree_sitter::Node) {
    match node.kind() {
        "class_declaration" | "object_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let is_interface = node.kind() == "class_declaration"
                && has_interface_keyword(node, ctx.bytes);
            let kind = if is_interface {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            };
            push_class(ctx, node, name, kind);
            return;
        }
        "companion_object" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_else(|| "Companion".to_string());
            push_class(ctx, node, name, SymbolKind::Class);
            return;
        }
        "function_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let in_class = !ctx.class_stack.is_empty();
            let is_test = name.starts_with("test")
                || has_annotation(node, ctx.bytes, "@Test");
            let kind = if is_test {
                SymbolKind::Test
            } else if in_class {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            let id = ctx.id();
            ctx.facts.symbols.push(SymbolFact {
                id,
                kind,
                name: name.clone(),
                qual_name: format!("{}.{name}", current_qual(ctx)),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::Kotlin,
                parent: ctx.class_stack.last().copied(),
                rev: 0,
                doc: None,
            });
            ctx.fun_names.insert(name);
            let prev = ctx.func;
            ctx.func = id;
            walk_children(ctx, node);
            ctx.func = prev;
            return;
        }
        "secondary_constructor" => {
            let id = ctx.id();
            ctx.facts.symbols.push(SymbolFact {
                id,
                kind: SymbolKind::Method,
                name: "init".into(),
                qual_name: format!("{}.init", current_qual(ctx)),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::Kotlin,
                parent: ctx.class_stack.last().copied(),
                rev: 0,
                doc: None,
            });
            let prev = ctx.func;
            ctx.func = id;
            walk_children(ctx, node);
            ctx.func = prev;
            return;
        }
        "call_expression" => {
            let callee_text = callee_of_call(node, ctx.bytes);
            let callee = if callee_text.starts_with("Class.forName")
                || callee_text.contains("kotlin.reflect")
                || callee_text.ends_with("::class.java.newInstance")
            {
                Callee::Unresolved("reflective".into())
            } else if !callee_text.contains('.')
                && ctx.fun_names.contains(&callee_text)
            {
                // Same-file fun, default-final → the resolver links by name.
                Callee::Unresolved("pending".into())
            } else if let Some(b) = effects::classify_builtin(&callee_text) {
                Callee::Builtin(b)
            } else if callee_text.contains('.') {
                Callee::Unresolved("virtual_dispatch".into())
            } else {
                Callee::Unresolved("unresolved_name".into())
            };
            ctx.facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text: callee_text.clone(),
                callee,
                line: ctx.line(node),
                in_loop: ctx.in_loop(node),
                arg_vars: arg_vars_of_call(node, ctx.bytes),
            });
            if is_spawn_call(&callee_text) {
                ctx.facts.async_facts.push(AsyncSite {
                    symbol: ctx.func,
                    kind: AsyncKind::Spawn,
                    line: ctx.line(node),
                });
            }
            walk_children(ctx, node);
            return;
        }
        "throw_expression" => {
            ctx.facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: "throw".into(),
                is_bare: false,
                line: ctx.line(node),
                in_handler: false,
                handler_end: 0,
            });
        }
        "try_expression" => {
            for child in children_of(node) {
                if child.kind() == "catch_block" {
                    ctx.facts.exceptions.push(ExceptionSite {
                        symbol: ctx.func,
                        kind_text: "catch".into(),
                        is_bare: false,
                        line: ctx.line(child),
                        in_handler: true,
                        handler_end: cie_core::types::line_of(
                            ctx.bytes,
                            child.end_byte() as u32,
                        ),
                    });
                }
            }
        }
        "import" => {
            let raw = text_of(node, ctx.bytes).trim();
            let body = raw
                .strip_prefix("import")
                .unwrap_or(raw)
                .trim();
            let (module, alias) = match body.split_once(" as ") {
                Some((m, a)) => (m.trim().to_string(), Some(a.trim().to_string())),
                None => (body.to_string(), None),
            };
            ctx.facts.imports.push(ImportFact {
                module_symbol: SymbolId(0),
                module,
                names: vec![],
                alias,
                line: ctx.line(node),
            });
        }
        "for_statement" => {
            let body = children_of(node)
                .into_iter()
                .find(|c| c.kind() == "block" || c.kind() == "statement");
            ctx.facts.loops.push(LoopSite {
                symbol: ctx.func,
                kind: LoopKind::For,
                line: ctx.line(node),
                body_start: body.map(|b| ctx.line(b)).unwrap_or(0),
                body_end: body
                    .map(|b| cie_core::types::line_of(ctx.bytes, b.end_byte() as u32))
                    .unwrap_or(0),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "while_statement" | "do_while_statement" => {
            let body = children_of(node)
                .into_iter()
                .find(|c| c.kind() == "block" || c.kind() == "statement");
            ctx.facts.loops.push(LoopSite {
                symbol: ctx.func,
                kind: LoopKind::While,
                line: ctx.line(node),
                body_start: body.map(|b| ctx.line(b)).unwrap_or(0),
                body_end: body
                    .map(|b| cie_core::types::line_of(ctx.bytes, b.end_byte() as u32))
                    .unwrap_or(0),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "if_expression" => {
            let cond = node
                .child_by_field_name("condition")
                .map(|c| text_of(c, ctx.bytes).to_string())
                .unwrap_or_default();
            let mut cond_vars_set = BTreeSet::new();
            if let Some(c) = node.child_by_field_name("condition") {
                collect_identifiers(c, ctx.bytes, &mut cond_vars_set);
            }
            let cond_vars: Vec<String> = cond_vars_set.into_iter().collect();
            ctx.facts.ifs.push(cie_semantic::facts::IfSite {
                symbol: ctx.func,
                line: ctx.line(node),
                cond,
                cond_vars,
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "property_declaration" => {
            emit_property_fact(ctx, node);
            walk_children(ctx, node);
            return;
        }
        "assignment" => {
            let left = node
                .child_by_field_name("left")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let right = node.child_by_field_name("right");
            let (rhs_kind, rhs_data) = right
                .map(|r| rhs_classification(r, ctx.bytes))
                .unwrap_or((RhsKind::Other, vec![]));
            ctx.facts.assignments.push(cie_semantic::facts::AssignmentFact {
                symbol: ctx.func,
                var: left,
                line: ctx.line(node),
                rhs_kind,
                rhs_data,
            });
        }
        "when_expression" => {
            ctx.notes.insert(
                "when-expressions are read but not emitted as branch facts".into(),
            );
        }
        _ => {}
    }
    walk_children(ctx, node);
}

fn push_class(ctx: &mut KotlinCtx, node: tree_sitter::Node, name: String, kind: SymbolKind) {
    let id = ctx.id();
    let qual = match ctx.class_stack.last() {
        Some(p) => {
            let p = ctx.facts.symbols.get(p.0 as usize).map(|s| s.qual_name.clone()).unwrap_or_default();
            format!("{p}.{name}")
        }
        None => name.clone(),
    };
    ctx.facts.symbols.push(SymbolFact {
        id,
        kind,
        name: name.clone(),
        qual_name: qual,
        loc: SourceLocation::span_bytes(
            cie_core::types::FileId(0),
            node.start_byte() as u32,
            node.end_byte() as u32,
        ),
        lang: LangId::Kotlin,
        parent: ctx.class_stack.last().copied(),
        rev: 0,
        doc: None,
    });
    ctx.class_stack.push(id);
    walk_children(ctx, node);
    ctx.class_stack.pop();
}

/// `val`/`var` with an initializer or delegate → one assignment fact.
fn emit_property_fact(ctx: &mut KotlinCtx, node: tree_sitter::Node) {
    let mut name = String::new();
    for child in children_of(node) {
        if child.kind() == "variable_declaration" {
            for vc in children_of(child) {
                if vc.kind() == "identifier" {
                    name = text_of(vc, ctx.bytes).to_string();
                }
            }
        }
    }
    if name.is_empty() {
        return;
    }
    // Initializer: the LAST child that is not a modifier/name/type/accessor.
    // (The grammar puts the initializer expression after the type; custom
    // accessors, when present, are skipped — they are not assignments.)
    let skipped = [
        "modifiers",
        "variable_declaration",
        "multi_variable_declaration",
        "nullable_type",
        "parenthesized_type",
        "user_type",
        "type_constraints",
        "type_modifiers",
        "type_parameters",
        "getter",
        "setter",
    ];
    let mut init: Option<tree_sitter::Node> = None;
    for child in children_of(node) {
        let k = child.kind();
        if skipped.contains(&k) {
            continue;
        }
        if k == "property_delegate" {
            // `val vm: Model by viewModels()` — the delegate call is the rhs.
            init = children_of(child).into_iter().find(|c| {
                c.kind() == "expression" || c.kind() == "call_expression"
            });
            break;
        }
        init = Some(child);
    }
    let (rhs_kind, rhs_data) = match init {
        Some(r) => rhs_classification(r, ctx.bytes),
        // `val`/`var` without initializer (constructor properties, delegated
        // accessors only) — nothing was assigned; emit nothing rather than
        // an empty fact.
        None => return,
    };
    ctx.facts.assignments.push(cie_semantic::facts::AssignmentFact {
        symbol: ctx.func,
        var: name,
        line: ctx.line(node),
        rhs_kind,
        rhs_data,
    });
}

/// RhsKind per the shared vocabulary (facts.rs): calls, literals, vars.
fn rhs_classification(rhs: tree_sitter::Node, bytes: &[u8]) -> (RhsKind, Vec<String>) {
    let text = text_of(rhs, bytes).trim().to_string();
    match rhs.kind() {
        "call_expression" => (RhsKind::Call, vec![callee_of_call(rhs, bytes)]),
        "string_literal" | "multiline_string_literal" | "character_literal"
        | "number_literal" | "float_literal" | "collection_literal" | "this" | "super" => {
            (RhsKind::Literal, vec![text])
        }
        "identifier" => (RhsKind::Vars, vec![text]),
        _ => {
            // Dotted reads (navigation) and mixed expressions.
            if text.contains('.') {
                (RhsKind::Vars, vec![text])
            } else {
                (RhsKind::Other, vec![text])
            }
        }
    }
}

impl KotlinAdapter {
    /// Deep extraction: one parse → facts (§16.1 contract).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser
            .set_language(&to_language())
            .expect("kotlin grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::Kotlin, "tree-sitter-deep:kotlin");
        };
        let bytes = src.as_bytes();
        let module = path
            .trim_end_matches(".kt")
            .trim_end_matches(".kts")
            .replace('/', ".");
        let module_id = SymbolId(0);
        let mut ctx = KotlinCtx {
            bytes,
            facts: LangFacts::new(path, LangId::Kotlin, "tree-sitter-deep:kotlin"),
            class_stack: vec![],
            func: module_id,
            next_id: 1,
            fun_names: BTreeSet::new(),
            notes: BTreeSet::new(),
        };
        ctx.facts.symbols.push(SymbolFact {
            id: module_id,
            kind: SymbolKind::Module,
            name: module.clone(),
            qual_name: module,
            loc: SourceLocation::span_bytes(
                cie_core::types::FileId(0),
                0,
                tree.root_node().end_byte() as u32,
            ),
            lang: LangId::Kotlin,
            parent: None,
            rev: 0,
            doc: None,
        });
        walk_kotlin(&mut ctx, tree.root_node());
        // Honesty notes: build-script shape + per-file syntax judgment.
        if path.ends_with(".kts") {
            ctx.notes.insert(
                "Gradle Kotlin DSL build script — parsed as plain Kotlin; plugins and task blocks are build logic".into(),
            );
        }
        if tree.root_node().has_error() {
            ctx.notes.insert("syntax errors present — extraction is partial".into());
        }
        ctx.facts.unsupported.extend(ctx.notes.into_iter());
        ctx.facts
            .exceptions
            .sort_by_key(|e| (e.line, e.kind_text.clone()));
        ctx.facts
    }
}

impl LanguageAdapter for KotlinAdapter {
    fn id(&self) -> LangId {
        LangId::Kotlin
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:kotlin"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16: same JVM virtual dispatch as Java for open/override members.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16: JVM int/long wrap (`Wrapping`); Kotlin forbids implicit
    /// widening (`ExplicitOnly`); mixed widths.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Wrapping,
            coercion: CoercionPolicy::ExplicitOnly,
            precision: Precision::MixedWidths,
        }
    }
    /// §16: JVM raise model — exceptions abort like Java's.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::Raise
    }
    /// TASK-067: real parse-based syntax judgment for THIS language only.
    fn parse_diagnostics(&self, src: &str) -> Vec<String> {
        let mut parser = Parser::new();
        if parser.set_language(&to_language()).is_err() {
            return vec!["parser_unavailable".into()];
        }
        match parser.parse(src, None) {
            Some(tree) if tree.root_node().has_error() => vec!["syntax error".into()],
            Some(_) => vec![],
            None => vec!["parse aborted".into()],
        }
    }
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "kotlin-default-final",
                convention: "methods are final unless marked open — override dispatch stays runtime",
            },
            FrameworkRule {
                name: "kotlin-coroutines",
                convention: "launch/async are structured-concurrency spawns; cancellation is cooperative",
            },
            FrameworkRule {
                name: "kotlin-test",
                convention: "@Test (kotlin.test/JUnit) marks a test entry point",
            },
            FrameworkRule {
                name: "kotlin-ktor",
                convention: "suspend handlers inside routing blocks are HTTP entry points",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(KotlinAdapter)
}

#[test]
fn task_023_kotlin_adapter_facts() {
    let adapter = KotlinAdapter;
    assert_eq!(adapter.id(), LangId::Kotlin);
    assert_eq!(adapter.extensions(), &["kt", "kts"]);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::Wrapping,
        "§16: JVM int/long wrap, same row as Java"
    );
    assert_eq!(
        adapter.numeric_semantics().coercion,
        CoercionPolicy::ExplicitOnly,
        "Kotlin has no implicit widening conversions"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = "\
import java.io.IOException
import kotlinx.coroutines.launch

interface Gate {
    fun authorize(user: String): Boolean
}

class Auth(private val minLen: Int) : Gate {
    private val realm = \"cie\"

    override fun authorize(user: String): Boolean {
        if (user.length < minLen) throw IOException(\"too short\")
        return check(user)
    }

    private fun check(user: String): Boolean = user != null

    @Test
    fun testAuthorize() {
        try {
            authorize(\"u\")
        } catch (e: IOException) {
            throw RuntimeException(e)
        }
    }

    fun poll() {
        for (i in 1..10) {
            println(\"tick $i\")
        }
        launch { work() }
    }

    private fun work() {
        val r = Class.forName(\"Auth\")
        while (r != null) { break }
    }
}
";
    let facts = adapter.extract("app/Auth.kt", src);

    // Class + interface + methods are symbols with proper kinds.
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Interface && s.name == "Gate"));
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Class && s.name == "Auth"));
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Method && s.name == "authorize"));
    assert!(
        facts.symbols.iter().any(|s| s.kind == SymbolKind::Test && s.name == "testAuthorize"),
        "@Test + test* prefix → Test kind"
    );
    // throw → raised fact; catch → caught fact.
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "throw"));
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "catch" && e.in_handler));
    // Reflection stays unresolved (FR-4).
    assert!(
        facts.calls.iter().any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "reflective")),
        "Class.forName is Reflective + unresolved"
    );
    // Default-final same-file fun call → pending (resolver links by name).
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "pending")),
        "same-file fun call is pending resolution"
    );
    // println → builtin Print via the shared classifier.
    assert!(facts.calls.iter().any(|c| matches!(c.callee, Callee::Builtin(b) if b == cie_semantic::facts::BuiltinKind::Print)));
    // Coroutine launch → Spawn.
    assert!(facts.async_facts.iter().any(|a| a.kind == AsyncKind::Spawn));
    // Imports extracted with alias normalization.
    assert!(facts.imports.iter().any(|i| i.module == "java.io.IOException"));
    assert!(facts.imports.iter().any(|i| i.module == "kotlinx.coroutines.launch"));
    // for-loop recorded with a real body span; println inside it → in_loop.
    let for_loop = facts.loops.iter().find(|l| l.kind == LoopKind::For).expect("for loop");
    assert!(for_loop.body_end > for_loop.line, "body span measured");
    assert!(
        facts.calls.iter().any(|c| c.callee_text.contains("println") && c.in_loop),
        "call inside for-loop flagged in_loop (N+1 hint)"
    );
    // val property → assignment fact.
    assert!(facts.assignments.iter().any(|a| a.var == "realm" && a.rhs_kind == RhsKind::Literal));
    // Clean syntax judgment for well-formed source.
    assert!(adapter.parse_diagnostics(src).is_empty());
    // Garbage source is judged honestly.
    assert_eq!(adapter.parse_diagnostics("fun {"), vec!["syntax error".to_string()]);
}

#[test]
fn task_023_kotlin_kts_and_errors() {
    let adapter = KotlinAdapter;
    let facts = adapter.extract(
        "build.gradle.kts",
        "plugins { kotlin(\"jvm\") }\ntasks.register(\"ci\") { }\n",
    );
    assert!(
        facts.unsupported.iter().any(|u| u.contains("Gradle Kotlin DSL")),
        ".kts build scripts carry an honesty note"
    );
    assert!(adapter.parse_diagnostics("val x: = 1").len() == 1);
}
