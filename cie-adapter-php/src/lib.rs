//! PHP deep adapter (TASK-021, §16 #10): dynamic types, magic methods,
//! exceptions.
//!
//! Own tree walk over `tree-sitter-php`: classes/interfaces/methods as
//! symbols, method calls with dynamic-dispatch marking, throw/try/catch
//! pairing with the `Error` vs `Exception` hierarchy as two distinct
//! raised families, magic-method/call_user_func reflective marking.
//!
//! Semantics tables (§16 #10, vouched here — FR-3):
//! - dispatch: `Dynamic` — dynamic types, trait reuse, property-based
//!   method calls
//! - numeric: `CoercionPolicy::IntToFloatOnOverflow` (ints promote to f64
//!   on overflow) with f64 precision
//! - exceptions: `throw`/`try`/`catch`/`finally` — `\Error` and
//!   `\Exception` hierarchies are TWO DISTINCT raised families (kind_text
//!   records which)
//! - async: NO native v1 model — async extension calls are recorded as
//!   `unsupported` assumptions, never fabricated `AsyncFact`s (FR-4)
//! - magic methods (`__call`, `__get`, `__invoke`) and `call_user_func`
//!   are `Reflective` + unresolved (FR-4)
//! - property-based calls (`$auth->authorize($u)`) are `Dynamic` +
//!   unresolved unless the class is visible in-tree
//! - variable variables (`$$x`) and `extract()` land in `unsupported`
//!   evidence (FR-4)

use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    Callee, CallSite, ExceptionSite, ImportFact, LangFacts, SymbolFact, SymbolKind,
};
use cie_semantic::LangId;
use tree_sitter::Parser;

pub struct PhpAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn children_of<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

struct PhpCtx<'a> {
    bytes: &'a [u8],
    facts: LangFacts,
    class_stack: Vec<SymbolId>,
    func: SymbolId,
    next_id: u32,
    /// Classes visible in-tree — property calls on these CAN resolve.
    known_classes: Vec<String>,
}

impl PhpCtx<'_> {
    fn id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }
    fn line(&self, node: tree_sitter::Node) -> u32 {
        cie_core::types::line_of(self.bytes, node.start_byte() as u32)
    }
}

fn walk_php(ctx: &mut PhpCtx, node: tree_sitter::Node) {
    match node.kind() {
        "class_declaration" | "interface_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).trim_start_matches('$').to_string())
                .unwrap_or_default();
            let kind = if node.kind() == "interface_declaration" {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            };
            ctx.known_classes.push(name.clone());
            let id = ctx.id();
            ctx.facts.symbols.push(SymbolFact {
                id,
                kind,
                name: name.clone(),
                qual_name: name,
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::Php,
                parent: ctx.class_stack.last().copied(),
                rev: 0,
                doc: None,
            });
            ctx.class_stack.push(id);
            walk_children(ctx, node);
            ctx.class_stack.pop();
            return;
        }
        "method_declaration" | "function_definition" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).trim_start_matches('$').to_string())
                .unwrap_or_default();
            let id = ctx.id();
            let is_test = name.starts_with("test");
            ctx.facts.symbols.push(SymbolFact {
                id,
                kind: if is_test {
                    SymbolKind::Test
                } else if ctx.class_stack.last().is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                name: name.clone(),
                qual_name: name.clone(),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::Php,
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
        "method_call_expression" | "scoped_call_expression" | "member_call_expression"
        | "function_call_expression" => {
            let callee_text = text_of(node, ctx.bytes)
                .split('(')
                .next()
                .unwrap_or("")
                .to_string();
            // Magic methods + call_user_func → Reflective (FR-4).
            let reflective = callee_text.contains("__call")
                || callee_text.contains("__get")
                || callee_text.contains("__invoke")
                || callee_text.contains("call_user_func");
            // `new ClassChains` / `$obj->method` — dynamic unless the class
            // is visible in-tree (FR-4).
            let object_known = ctx
                .known_classes
                .iter()
                .any(|c| callee_text.starts_with(&format!("{c}::")));
            let callee = if reflective {
                Callee::Unresolved("reflective".into())
            } else if object_known {
                Callee::Unresolved("pending".into()) // resolution links
            } else {
                Callee::Unresolved("dynamic_dispatch".into())
            };
            ctx.facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text,
                callee,
                line: ctx.line(node),
                in_loop: false,
                arg_vars: vec![],
            });
            // No native async in v1 — async extension calls are unsupported
            // assumptions, NEVER fabricated AsyncFacts (FR-4).
            let txt = text_of(node, ctx.bytes);
            if txt.contains("async") || txt.contains("await") || txt.contains("Amp\\") || txt.contains("React\\") {
                ctx.facts.unsupported.push(format!(
                    "async extension usage at line {} — no native PHP async model in v1 (FR-4)",
                    ctx.line(node)
                ));
            }
            walk_children(ctx, node);
            return;
        }
        "throw_statement" | "throw_expression" => {
            let txt = text_of(node, ctx.bytes);
            // Error vs Exception: two DISTINCT raised families (§16 #10).
            let family = if txt.contains("Error") && !txt.contains("Exception") {
                "throw_error"
            } else {
                "throw_exception"
            };
            ctx.facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: family.into(),
                is_bare: false,
                line: ctx.line(node),
                in_handler: false,
                handler_end: 0,
            });
        }
        "catch_clause" => {
            let txt = text_of(node, ctx.bytes);
            let family = if txt.contains("Error") && !txt.contains("Exception") {
                "catch_error"
            } else {
                "catch_exception"
            };
            ctx.facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: family.into(),
                is_bare: false,
                line: ctx.line(node),
                in_handler: true,
                handler_end: cie_core::types::line_of(ctx.bytes, node.end_byte() as u32),
            });
        }
        _ => {
            // Variable variables (`$$x`) and extract() → unsupported (FR-4).
            let txt = text_of(node, ctx.bytes);
            if txt.starts_with("$$") || (txt.contains("extract(") && node.kind() == "expression_statement") {
                ctx.facts.unsupported.push(format!(
                    "dynamic scoping at line {} — variable variables/extract() are dynamic (FR-4)",
                    ctx.line(node)
                ));
            }
        }
    }
    walk_children(ctx, node);
}

fn walk_children(ctx: &mut PhpCtx, node: tree_sitter::Node) {
    for child in children_of(node) {
        walk_php(ctx, child);
    }
}

impl PhpAdapter {
    /// Deep extraction: one parse → facts (§16.1 contract).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("php grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::Php, "tree-sitter-deep:php");
        };
        let module = path.trim_end_matches(".php").replace('/', ".");
        let module_id = SymbolId(0);
        let mut ctx = PhpCtx {
            bytes: src.as_bytes(),
            facts: LangFacts::new(path, LangId::Php, "tree-sitter-deep:php"),
            class_stack: vec![],
            func: module_id,
            next_id: 1,
            known_classes: vec![],
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
            lang: LangId::Php,
            parent: None,
            rev: 0,
            doc: None,
        });
        walk_php(&mut ctx, tree.root_node());
        ctx.facts
    }
}

impl LanguageAdapter for PhpAdapter {
    fn id(&self) -> LangId {
        LangId::Php
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["php", "php3", "php4", "php5", "phtml"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:php"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #10: dynamic types, trait reuse, property-based dispatch.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16 #10: ints promote to f64 on overflow (IntToFloatOnOverflow).
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Unspecified,
            coercion: CoercionPolicy::IntToFloatOnOverflow,
            precision: Precision::Binary64,
        }
    }
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "php-magic-methods",
                convention: "__call/__get/__invoke route through magic dispatch at runtime",
            },
            FrameworkRule {
                name: "php-error-exception-split",
                convention: "\\Error and \\Exception hierarchies are distinct raised families",
            },
            FrameworkRule {
                name: "php-trait-reuse",
                convention: "trait methods are copied into the class; dispatch stays dynamic",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(PhpAdapter)
}

#[test]
fn task_021_php_adapter_facts() {
    let adapter = PhpAdapter;
    assert_eq!(adapter.id(), LangId::Php);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic);
    assert_eq!(
        adapter.numeric_semantics().coercion,
        CoercionPolicy::IntToFloatOnOverflow,
        "§16 #10: ints promote to f64 on overflow"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = r#"<?php
class Authorizer {
    public function authorize($u) { return $u !== null; }
    public function __call($name, $args) { return false; }
}
function run($auth, $u) {
    try {
        return $auth->authorize($u);
    } catch (Exception $e) {
        throw new Error("wrapped");
    }
}
$x = call_user_func('boom');
"#;
    let facts = adapter.extract("auth.php", src);

    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Class && s.name == "Authorizer"));
    // Property-based call on an unknown-typed variable → Dynamic + unresolved.
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "dynamic_dispatch")),
        "property-based dispatch is Dynamic + unresolved (FR-4)"
    );
    // call_user_func → Reflective.
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "reflective")),
        "call_user_func is Reflective + unresolved (FR-4)"
    );
    // Error vs Exception: two distinct raised families (§16 #10 DoD).
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "catch_exception"));
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "throw_error"));
    // No fabricated AsyncFacts (FR-4).
    assert!(facts.async_facts.is_empty(), "PHP v1 has no async model");
}
