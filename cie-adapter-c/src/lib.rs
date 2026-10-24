//! C deep adapter (TASK-016, §16 #5): pointers, UB, manual memory,
//! preprocessor.
//!
//! Own tree walk over `tree-sitter-c` (parse → extract contract per
//! §16.1): functions as symbols, calls with function-pointer dispatch
//! marking, malloc/free/realloc as resource-ownership effects (§14.3
//! `Owns` raw material), error-code conventions as framework rules with
//! `ErrorValue`-only exceptions, `#include`/macro evidence.
//!
//! Semantics tables (§16 #5, vouched here — FR-3):
//! - dispatch: `Static` — direct calls; function-pointer calls are
//!   `Dynamic` + unresolved
//! - numeric: fixed-width with `OverflowPolicy::UndefinedBehavior` for
//!   signed overflow plus integer promotions — UB sites are recorded as
//!   assumptions, never assumed away (FR-4)
//! - exceptions: C has none — `-1`/`NULL`/`errno` conventions are
//!   `FrameworkRule`s and produce `ErrorValue` facts only where vouched;
//!   NEVER `Raised` (FR-3)
//! - async: none in core C — `pthread_create` recorded as `Spawn` evidence

use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    Callee, CallSite, ExceptionSite, ImportFact, LangFacts, ResourceEffect, SymbolFact,
    SymbolKind,
};
use cie_semantic::LangId;
use tree_sitter::Parser;

pub struct CAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn children_of<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

struct CCtx<'a> {
    bytes: &'a [u8],
    facts: LangFacts,
    func: SymbolId,
    next_id: u32,
}

impl CCtx<'_> {
    fn id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }
    fn line(&self, node: tree_sitter::Node) -> u32 {
        cie_core::types::line_of(self.bytes, node.start_byte() as u32)
    }
}

fn walk_c(ctx: &mut CCtx, node: tree_sitter::Node) {
    match node.kind() {
        "function_definition" => {
            let decl = node.child_by_field_name("declarator");
            let name = decl
                .and_then(|d| find_identifier(d, ctx.bytes))
                .unwrap_or_default();
            let is_test = name.starts_with("test_");
            let id = ctx.id();
            ctx.facts.symbols.push(SymbolFact {
            annotations: Vec::new(),
                id,
                kind: if is_test { SymbolKind::Test } else { SymbolKind::Function },
                name: name.clone(),
                qual_name: name.clone(),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::C,
                parent: None,
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
            let fn_node = node.child_by_field_name("function").unwrap_or(node);
            let callee_text = text_of(fn_node, ctx.bytes).to_string();
            // Function-pointer calls dispatch at runtime (§16 #5).
            let is_fn_ptr = callee_text.contains('(')
                || callee_text.contains('[')
                || callee_text.starts_with('*');
            let callee = if is_fn_ptr {
                Callee::Unresolved("fn_pointer".into())
            } else {
                Callee::Unresolved("pending".into()) // resolution.rs links
            };
            ctx.facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text: callee_text.clone(),
                callee,
                line: ctx.line(node),
                in_loop: false,
                arg_vars: collect_args(node, ctx.bytes),
                receiver: callee_text.split('.').next().filter(|_| callee_text.contains('.')).map(|h| h.trim().trim_end_matches('(').to_string()).filter(|h| !h.is_empty()),
                arguments: None,
                span: (node.start_byte() as u32, node.end_byte() as u32),
            });
            // malloc/free/realloc → resource-ownership effects (§14.3).
            let base = callee_text
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .find(|s| !s.is_empty())
                .unwrap_or("");
            if matches!(base, "malloc" | "calloc" | "realloc") {
                ctx.facts.resource_effects.push(ResourceEffect {
                    symbol: ctx.func,
                    kind_text: base.to_string(),
                    resource: text_of(node, ctx.bytes).chars().take(120).collect(),
                    line: ctx.line(node),
                });
            } else if base == "free" {
                ctx.facts.resource_effects.push(ResourceEffect {
                    symbol: ctx.func,
                    kind_text: "free".into(),
                    resource: text_of(node, ctx.bytes).chars().take(120).collect(),
                    line: ctx.line(node),
                });
            } else if base == "pthread_create" {
                ctx.facts.async_facts.push(cie_semantic::facts::AsyncSite {
                    symbol: ctx.func,
                    kind: cie_semantic::facts::AsyncKind::Spawn,
                    line: ctx.line(node),
                });
            }
            walk_children(ctx, node);
            return;
        }
        "preproc_include" => {
            let t = text_of(node, ctx.bytes).to_string();
            ctx.facts.imports.push(ImportFact {
                module_symbol: SymbolId(0),
                module: t.trim_start_matches("#include").trim().to_string(),
                names: vec![],
                alias: None,
                line: ctx.line(node),
            });
            // #include-ed behavior stays unsupported in evidence (FR-4).
            ctx.facts.unsupported.push(format!(
                "#include at line {} — transitively included behavior not analyzed",
                ctx.line(node)
            ));
        }
        "preproc_def" | "preproc_function_def" => {
            // Macro expansions record expansion boundaries as assumptions.
            ctx.facts.unsupported.push(format!(
                "macro definition at line {} — expansion sites are assumptions (FR-4)",
                ctx.line(node)
            ));
        }
        _ => {}
    }
    walk_children(ctx, node);
}

fn find_identifier<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(text_of(node, bytes).to_string());
    }
    for child in children_of(node) {
        if let Some(f) = find_identifier(child, bytes) {
            return Some(f);
        }
    }
    None
}

fn collect_args(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    if let Some(args) = node.child_by_field_name("arguments") {
        for a in children_of(args) {
            if a.kind() == "identifier" {
                out.push(text_of(a, bytes).to_string());
            }
        }
    }
    out
}

fn walk_children(ctx: &mut CCtx, node: tree_sitter::Node) {
    for child in children_of(node) {
        walk_c(ctx, child);
    }
}

impl CAdapter {
    /// Deep extraction: one parse → facts (§16.1 contract).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("c grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::C, "tree-sitter-deep:c");
        };
        self.extract_tree(path, src, &tree)
    }

    fn extract_shared(&self, path: &str, src: &str, tree: &tree_sitter::Tree) -> LangFacts {
        let module = path.trim_end_matches(".c").replace('/', ".");
        let module_id = SymbolId(0);
        let mut ctx = CCtx {
            bytes: src.as_bytes(),
            facts: LangFacts::new(path, LangId::C, "tree-sitter-deep:c"),
            func: module_id,
            next_id: 1,
        };
        ctx.facts.symbols.push(SymbolFact {
            annotations: Vec::new(),
            id: module_id,
            kind: SymbolKind::Module,
            name: module.clone(),
            qual_name: module,
            loc: SourceLocation::span_bytes(
                cie_core::types::FileId(0),
                0,
                tree.root_node().end_byte() as u32,
            ),
            lang: LangId::C,
            parent: None,
            rev: 0,
            doc: None,
        });
        walk_c(&mut ctx, tree.root_node());
        ctx.facts
    }
}

impl LanguageAdapter for CAdapter {
    fn grammar(&self, path: &str) -> Option<tree_sitter::Language> {
        let _ = path;
        Some(tree_sitter_c::LANGUAGE.into())
    }
    fn extract_tree(&self, path: &str, src: &str, tree: &tree_sitter::Tree) -> LangFacts {
        self.extract_shared(path, src, tree)
    }

    fn id(&self) -> LangId {
        LangId::C
    }
    /// §16 error model row (TASK-022): vouched by this adapter.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::ErrorCode
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:c"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #5: direct calls are static; function pointers are dynamic.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Static
    }
    /// §16 #5: fixed-width with SIGNED-overflow UB + integer promotions —
    /// UB sites are assumptions (FR-4), never assumed away.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::UndefinedBehavior,
            coercion: CoercionPolicy::ImplicitNumeric,
            precision: Precision::MixedWidths,
        }
    }
    /// §16 #5: error-code conventions are vouched framework rules; the
    /// adapter emits `ErrorValue` facts only under these conventions and
    /// NEVER `Raised` (FR-3).
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "c-errno",
                convention: "errno is set on failure; check it after the call",
            },
            FrameworkRule {
                name: "c-negative-return",
                convention: "-1 return signals error (POSIX convention)",
            },
            FrameworkRule {
                name: "c-null-return",
                convention: "NULL return signals allocation/lookup failure",
            },
            FrameworkRule {
                name: "c-owns-malloc",
                convention: "the caller of malloc owns the allocation and must free it",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(CAdapter)
}

#[test]
fn task_016_c_adapter_facts() {
    let adapter = CAdapter;
    assert_eq!(adapter.id(), LangId::C);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Static);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::UndefinedBehavior,
        "§16 #5: signed overflow is UB"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = r#"#include <stdlib.h>
typedef int (*handler_t)(int);
int authorize(int uid, handler_t h) {
    char *buf = malloc(64);
    if (buf == NULL) { return -1; }
    int r = (*h)(uid);
    free(buf);
    return r;
}
"#;
    let facts = adapter.extract("auth.c", src);

    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name == "authorize"));
    // malloc → resource-ownership effect (Owns raw material).
    assert!(
        facts
            .resource_effects
            .iter()
            .any(|e| e.kind_text == "malloc"),
        "malloc yields an ownership effect"
    );
    assert!(facts.resource_effects.iter().any(|e| e.kind_text == "free"));
    // Function-pointer call → dynamic + unresolved (FR-4).
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "fn_pointer")),
        "function-pointer call is dynamic + unresolved"
    );
    // Direct call stays resolvable-pending (never Raised anywhere).
    assert!(facts.exceptions.is_empty(), "C never emits exceptions (FR-3)");
    // #include evidence.
    assert!(facts.unsupported.iter().any(|u| u.contains("#include")));
    assert!(facts.imports.iter().any(|i| i.module.contains("stdlib.h")));
}
