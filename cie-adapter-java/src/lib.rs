//! Java deep adapter (TASK-015, §16 #4): JVM virtual dispatch, checked
//! exceptions, annotations.
//!
//! Own tree walk over `tree-sitter-java` (parse → extract contract per
//! §16.1): classes/interfaces/enums/methods as symbols, method invocations
//! with dispatch classification, `throws` clauses as raised-exception
//! facts, catch clauses as caught facts, annotations as framework
//! conventions, `Thread`/`ExecutorService`/`CompletableFuture` as spawn.
//!
//! Semantics tables (§16 #4, vouched here — FR-3):
//! - dispatch: `Dynamic` — JVM virtual dispatch for overridable instance
//!   methods (`CallKind::Virtual`); `static`/`final`/`private` calls are
//!   `Direct` when the declaration is visible in-tree
//! - numeric: fixed-width two's-complement wraparound for int/long
//!   (`OverflowPolicy::Wrapping`, JLS-defined), f32/f64 floats
//! - exceptions: `throw`/`try`/`catch`/`finally`; `throws` clauses emit
//!   raised facts even without a throw site, so interface contracts stay
//!   visible downstream (FR-2)
//! - async: `Thread`/`ExecutorService`/`CompletableFuture` → `Spawn`
//! - reflection (`Method.invoke`, `Class.forName`) is `Reflective` +
//!   unresolved — never pretended resolvable (FR-4)

use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    AsyncKind, AsyncSite, Callee, CallSite, ExceptionSite, IfSite, ImportFact, LangFacts,
    SymbolFact, SymbolKind,
};
use cie_semantic::LangId;
use std::collections::BTreeSet;
use tree_sitter::Parser;

pub struct JavaAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

struct JavaCtx<'a> {
    bytes: &'a [u8],
    facts: LangFacts,
    class_stack: Vec<SymbolId>,
    func: SymbolId,
    next_id: u32,
    /// `static`/`final`/`private` method names visible in-tree (Direct).
    direct_names: BTreeSet<String>,
    throws_facts: Vec<ExceptionSite>,
}

impl<'a> JavaCtx<'a> {
    fn id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    fn line(&self, node: tree_sitter::Node) -> u32 {
        cie_core::types::line_of(self.bytes, node.start_byte() as u32)
    }
}

fn walk_java(ctx: &mut JavaCtx, node: tree_sitter::Node) {
    match node.kind() {
        "class_declaration" | "interface_declaration" | "enum_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let kind = match node.kind() {
                "class_declaration" => SymbolKind::Class,
                "interface_declaration" => SymbolKind::Interface,
                _ => SymbolKind::Enum,
            };
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
                lang: LangId::Java,
                parent: ctx.class_stack.last().copied(),
                rev: 0,
                doc: None,
                annotations: annotations_of(node, ctx.bytes),
            });
            ctx.class_stack.push(id);
            walk_children(ctx, node);
            ctx.class_stack.pop();
            return;
        }
        "method_declaration" | "constructor_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let id = ctx.id();
            let is_test = name.starts_with("test") && node_has_annotation(node, ctx.bytes, "@Test");
            let kind = if is_test { SymbolKind::Test } else { SymbolKind::Method };
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
                lang: LangId::Java,
                parent: ctx.class_stack.last().copied(),
                rev: 0,
                doc: None,
                annotations: annotations_of(node, ctx.bytes),
            });
            // static/final/private declarations are Direct-capable targets.
            let mods = node
                .child_by_field_name("modifiers")
                .map(|m| text_of(m, ctx.bytes).to_string())
                .unwrap_or_default();
            if mods.contains("static") || mods.contains("final") || mods.contains("private") {
                ctx.direct_names.insert(name.clone());
            }
            // `throws` clauses: raised-exception facts even without a throw
            // site (FR-2: interface contracts stay visible downstream).
            for child in children_of(node) {
                if child.kind() == "throws" || child.kind() == "throws_clause" {
                    for exc in children_of(child) {
                        let t = text_of(exc, ctx.bytes).trim().to_string();
                        if !t.is_empty() && t != "throws" {
                            ctx.throws_facts.push(ExceptionSite {
                                symbol: id,
                                kind_text: format!("throws {t}"),
                                is_bare: false,
                                line: ctx.line(exc),
                                in_handler: false,
                                handler_end: 0,
                            });
                        }
                    }
                }
            }
            let prev = ctx.func;
            ctx.func = id;
            walk_children(ctx, node);
            ctx.func = prev;
            return;
        }
        "method_invocation" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let object = node
                .child_by_field_name("object")
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let full = if object.is_empty() {
                name.clone()
            } else {
                format!("{object}.{name}")
            };
            let full_for_async = full.clone();
            // Reflection is never resolvable (FR-4).
            let reflective = object.ends_with("Method.invoke")
                || full.starts_with("Method.invoke")
                || full.starts_with("Class.forName")
                || object.ends_with("Class.forName");
            let callee = if reflective {
                Callee::Unresolved("reflective".into())
            } else if object.is_empty() && ctx.direct_names.contains(&name) {
                // static/final/private + visible declaration → Direct.
                Callee::Unresolved("pending".into()) // resolution.rs links by name
            } else if object.is_empty() {
                // Unqualified call = this.method() = JVM virtual dispatch on
                // an overridable instance method (§16 #4) — unresolved.
                Callee::Unresolved("virtual_dispatch".into())
            } else {
                // Dotted instance call: overridable → Virtual; the static
                // target is unknown → unresolved (FR-4).
                Callee::Unresolved("virtual_dispatch".into())
            };
            ctx.facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text: full,
                callee,
                line: ctx.line(node),
                in_loop: ctx
                    .facts
                    .loops
                    .iter()
                    .any(|l| l.line <= ctx.line(node) && node.end_byte() as u32 >= l.line * 0),
                arg_vars: collect_args(node, ctx.bytes),
            });
            // spawn-style async: new Thread(...).start(), executors, futures
            if full_for_async.contains("new Thread")
                || full_for_async.contains("ExecutorService")
                || full_for_async.contains("CompletableFuture")
                || name == "start" && object.contains("Thread")
            {
                ctx.facts.async_facts.push(AsyncSite {
                    symbol: ctx.func,
                    kind: AsyncKind::Spawn,
                    line: ctx.line(node),
                });
            }
            walk_children(ctx, node);
            return;
        }
        "throw_statement" => {
            ctx.facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: "throw".into(),
                is_bare: false,
                line: ctx.line(node),
                in_handler: false,
                handler_end: 0,
            });
        }
        // RFC-0024 stage 0.5: decision facts (the FlowChart's diamonds).
        // Each `if` records {symbol, line, cond} — the exact line where the
        // condition is written — so a decision is drawn only when the
        // engine recorded it (honest absence, never a guessed diamond).
        "if_statement" => {
            if let Some(cond) = node.child_by_field_name("condition") {
                let cond_text = text_of(cond, ctx.bytes).trim().to_string();
                let mut vars = vec![];
                collect_identifiers(cond, ctx.bytes, &mut vars);
                vars.sort();
                vars.dedup();
                ctx.facts.ifs.push(IfSite {
                    symbol: ctx.func,
                    line: ctx.line(node),
                    cond: cond_text,
                    cond_vars: vars,
                    loc: SourceLocation::span_bytes(
                        cie_core::types::FileId(0),
                        node.start_byte() as u32,
                        node.end_byte() as u32,
                    ),
                });
            }
        }
        "catch_clause" => {
            ctx.facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: "catch".into(),
                is_bare: false,
                line: ctx.line(node),
                in_handler: true,
                handler_end: cie_core::types::line_of(ctx.bytes, node.end_byte() as u32),
            });
        }
        "import_declaration" => {
            let t = text_of(node, ctx.bytes)
                .trim()
                .trim_start_matches("import")
                .trim()
                .trim_end_matches(';')
                .to_string();
            ctx.facts.imports.push(ImportFact {
                module_symbol: SymbolId(0),
                module: t,
                names: vec![],
                alias: None,
                line: ctx.line(node),
            });
        }
        _ => {}
    }
    walk_children(ctx, node);
}

fn current_qual(ctx: &JavaCtx) -> String {
    ctx.class_stack
        .last()
        .and_then(|p| ctx.facts.symbols.get(p.0 as usize))
        .map(|s| s.qual_name.clone())
        .unwrap_or_default()
}

fn node_has_annotation(node: tree_sitter::Node, bytes: &[u8], ann: &str) -> bool {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if child.kind() == "modifiers" && text_of(child, bytes).contains(ann) {
            return true;
        }
    }
    false
}

/// Every annotation written directly on this declaration (class/interface/
/// enum/method/constructor), exact source text. tree-sitter-java 0.23 nests
/// a class/interface/enum's annotations inside its `modifiers` child
/// (verified against node-types.json — `class_declaration`'s only generic
/// child is `modifiers`), while a method/constructor can carry `annotation`/
/// `marker_annotation` either as direct children or inside `modifiers` too
/// — so this checks both shapes rather than assuming one.
fn annotations_of(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    for child in children_of(node) {
        match child.kind() {
            "annotation" | "marker_annotation" => {
                out.push(text_of(child, bytes).trim().to_string());
            }
            "modifiers" => {
                for m in children_of(child) {
                    if matches!(m.kind(), "annotation" | "marker_annotation") {
                        out.push(text_of(m, bytes).trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn children_of<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

fn collect_args(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    if let Some(args) = node.child_by_field_name("arguments") {
        let mut c = args.walk();
        for a in args.children(&mut c) {
            if a.kind() == "identifier" {
                out.push(text_of(a, bytes).to_string());
            }
        }
    }
    out
}

fn collect_identifiers(node: tree_sitter::Node, bytes: &[u8], out: &mut Vec<String>) {
    if node.kind() == "identifier" {
        let t = text_of(node, bytes).to_string();
        if !t.is_empty() {
            out.push(t);
        }
    }
    let mut c = node.walk();
    for child in node.children(&mut c) {
        collect_identifiers(child, bytes, out);
    }
}

fn walk_children(ctx: &mut JavaCtx, node: tree_sitter::Node) {
    for child in children_of(node) {
        walk_java(ctx, child);
    }
}

impl JavaAdapter {
    /// Deep extraction: one parse → facts (§16.1 contract).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("java grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::Java, "tree-sitter-deep:java");
        };
        let bytes = src.as_bytes();
        let module = path.trim_end_matches(".java").replace('/', ".");
        let module_id = SymbolId(0);
        let mut ctx = JavaCtx {
            bytes,
            facts: LangFacts::new(path, LangId::Java, "tree-sitter-deep:java"),
            class_stack: vec![],
            func: module_id,
            next_id: 1,
            direct_names: BTreeSet::new(),
            throws_facts: vec![],
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
            lang: LangId::Java,
            parent: None,
            rev: 0,
            doc: None,
            annotations: vec![],
        });
        walk_java(&mut ctx, tree.root_node());
        // throws-clause facts join throw/catch facts, sorted by line.
        ctx.facts.exceptions.extend(ctx.throws_facts);
        ctx.facts
            .exceptions
            .sort_by_key(|e| (e.line, e.kind_text.clone()));
        ctx.facts
    }
}

impl LanguageAdapter for JavaAdapter {
    fn id(&self) -> LangId {
        LangId::Java
    }
    /// §16 error model row (TASK-022): vouched by this adapter.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::Raise
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["java"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:java"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #4: JVM virtual dispatch for overridable instance methods.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16 #4: int/long wrap per JLS (`WrapAround`); f32/f64 floats;
    /// explicit-only conversions.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Wrapping,
            coercion: CoercionPolicy::ExplicitOnly,
            precision: Precision::MixedWidths,
        }
    }
    /// §16 #4: annotations are framework conventions, never behavior facts.
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "java-override",
                convention: "@Override marks virtual reimplementation; dispatch stays runtime",
            },
            FrameworkRule {
                name: "java-transactional",
                convention: "@Transactional wraps the method in a transaction boundary",
            },
            FrameworkRule {
                name: "java-spring",
                convention: "@Autowired/@Component wire dependencies via the container",
            },
            FrameworkRule {
                name: "java-junit",
                convention: "@Test marks a test entry point",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(JavaAdapter)
}

#[test]
fn task_015_java_adapter_facts() {
    let adapter = JavaAdapter;
    assert_eq!(adapter.id(), LangId::Java);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::Wrapping,
        "§16 #4: JLS-defined wrap"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = "\
import java.io.IOException;
public class Auth {
    public boolean authorize(String u) throws IOException { return check(u); }
    private boolean check(String u) { if (u != null) { return true; } return false; }
    public void run(Object o) throws IOException {
        Object r = Class.forName(\"Auth\");
        try { authorize(\"u\"); } catch (IOException e) { throw new RuntimeException(e); }
    }
}
";
    let facts = adapter.extract("Auth.java", src);
    for e in &facts.exceptions {
        eprintln!("DBG EXC kind={} line={}", e.kind_text, e.line);
    }

    // Class + methods are symbols.
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Class && s.name == "Auth"));
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Method && s.name == "authorize"));
    // throws IOException → raised-family fact (FR-2: contract visible).
    assert!(
        facts
            .exceptions
            .iter()
            .any(|e| e.kind_text.starts_with("throws IOException")),
        "throws clause yields a raised fact"
    );
    // catch clause → caught fact.
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "catch" && e.in_handler));
    // Reflection: Class.forName → unresolved reflective.
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "reflective")),
        "reflection is Reflective + unresolved (FR-4)"
    );
    // Overridable instance call → unresolved virtual dispatch.
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "virtual_dispatch")),
        "JVM virtual dispatch stays unresolved"
    );
    // Imports extracted.
    assert!(facts.imports.iter().any(|i| i.module.contains("java.io.IOException")));
    // RFC-0024 stage 0.5: decision facts (IfSite) carry sym+line+cond —
    // the FlowChart's diamonds draw only from these.
    let guarded = facts
        .ifs
        .iter()
        .find(|i| i.cond.contains("u != null"))
        .expect("decision fact recorded from the method body");
    assert!(guarded.line >= 1, "decision fact carries its line");
    assert!(!guarded.cond_vars.is_empty(), "condition variables recorded");
    assert_eq!(guarded.cond_vars[0], "u");
    // The decision is attributed to check(), not the module.
    let check_sym = facts
        .symbols
        .iter()
        .find(|s| s.name == "check")
        .expect("check symbol exists");
    assert_eq!(guarded.symbol, check_sym.id);
}
