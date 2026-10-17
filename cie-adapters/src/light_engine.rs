//! Light adapters for JavaScript, TypeScript, Go and Rust.
//!
//! These extract symbols, calls, imports, string literals, loops (N+1
//! ranges), simple assignments, taint sources/sinks, tests and async facts.
//! Deep behavioral modeling (exception handler semantics, f-string data
//! flow, constant-return profiles) is Python-only in v1 — each adapter
//! records what it cannot model as explicit `unsupported` notes that flow
//! into coverage/evidence (never silently dropped).

use crate::effects;
use cie_core::types::{FileId, SourceLocation, SymbolId};
use cie_semantic::facts::*;
use cie_semantic::LangId;
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightLang {
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    Rust,
}

impl LightLang {
    fn ts_language(&self) -> tree_sitter::Language {
        match self {
            LightLang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            LightLang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            LightLang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            LightLang::Go => tree_sitter_go::LANGUAGE.into(),
            LightLang::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    fn lang_id(&self) -> LangId {
        match self {
            LightLang::JavaScript => LangId::JavaScript,
            LightLang::TypeScript | LightLang::Tsx => LangId::TypeScript,
            LightLang::Go => LangId::Go,
            LightLang::Rust => LangId::Rust,
        }
    }

    fn analyzer(&self) -> &'static str {
        match self {
            LightLang::JavaScript => "tree-sitter-light:javascript",
            LightLang::TypeScript => "tree-sitter-light:typescript",
            LightLang::Tsx => "tree-sitter-light:tsx",
            LightLang::Go => "tree-sitter-light:go",
            LightLang::Rust => "tree-sitter-light:rust",
        }
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => &[
                "function_declaration",
                "generator_function_declaration",
                "method_definition",
            ],
            LightLang::Go => &["function_declaration", "method_declaration"],
            LightLang::Rust => &["function_item"],
        }
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => {
                &["class_declaration", "class", "interface_declaration"]
            }
            LightLang::Go => &["type_spec"],
            LightLang::Rust => &["struct_item", "enum_item", "trait_item"],
        }
    }

    fn call_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => {
                &["call_expression", "new_expression"]
            }
            LightLang::Go => &["call_expression"],
            LightLang::Rust => &["call_expression", "macro_invocation"],
        }
    }

    fn string_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => {
                &["string", "template_string"]
            }
            LightLang::Go => &["interpreted_string_literal", "raw_string_literal"],
            LightLang::Rust => &["string_literal", "raw_string_literal"],
        }
    }

    fn loop_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => &[
                "for_statement",
                "for_in_statement",
                "while_statement",
                "do_statement",
            ],
            LightLang::Go => &["for_statement"],
            LightLang::Rust => &["for_expression", "while_expression", "loop_expression"],
        }
    }

    fn assignment_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => &[
                "variable_declarator",
                "assignment_expression",
                "lexical_declaration",
            ],
            LightLang::Go => &["short_var_declaration", "assignment_statement"],
            LightLang::Rust => &["let_declaration"],
        }
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        match self {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => &["import_statement"],
            LightLang::Go => &["import_declaration"],
            LightLang::Rust => &["use_declaration"],
        }
    }

    fn unsupported(&self) -> Vec<String> {
        let mut u = vec![
            "deep_dataflow_unavailable".into(),
            "interprocedural_branch_semantics_light".into(),
        ];
        match self {
            LightLang::Go => u.push("panic_recovery_not_modeled".into()),
            LightLang::Rust => u.push("result_propagation_not_modeled".into()),
            _ => u.push("exception_handler_semantics_light".into()),
        }
        u
    }
}

pub fn to_language(lang: LightLang) -> tree_sitter::Language {
    lang.ts_language()
}

fn text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// §14.1 byte span of a node: `(start_byte, end_byte)` straight from
/// tree-sitter — no position arithmetic needed (TASK-006).
fn span_of(node: Node) -> (u32, u32) {
    let r = node.byte_range();
    (r.start as u32, r.end as u32)
}

/// Name of a function-like node across the four grammars.
fn function_name(node: Node, lang: LightLang, bytes: &[u8]) -> Option<String> {
    match lang {
        LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => node
            .child_by_field_name("name")
            .map(|n| text(n, bytes).to_string()),
        LightLang::Go => node
            .child_by_field_name("name")
            .map(|n| text(n, bytes).to_string()),
        LightLang::Rust => node
            .child_by_field_name("name")
            .map(|n| text(n, bytes).to_string()),
    }
}

fn is_rust_test(node: Node, bytes: &[u8]) -> bool {
    // #[test] attribute precedes the function inside the same item list.
    let mut sibling = node;
    for _ in 0..3 {
        sibling = match sibling.prev_sibling() {
            Some(s) => s,
            None => return false,
        };
        let t = text(sibling, bytes);
        if t.contains("#[test]") || t.contains("#[tokio::test]") {
            return true;
        }
        if !t.trim_start().starts_with("#") {
            return false;
        }
    }
    false
}

fn callee_text_of_call(node: Node, lang: LightLang, bytes: &[u8]) -> String {
    match lang {
        LightLang::Rust if node.kind() == "macro_invocation" => {
            let t = text(node, bytes).to_string();
            t
        }
        _ => node
            .child_by_field_name("function")
            .map(|n| text(n, bytes).to_string())
            .unwrap_or_default(),
    }
}

struct LightCtx<'a> {
    path: &'a str,
    bytes: &'a [u8],
    module: SymbolId,
    class: Option<SymbolId>,
    func: SymbolId,
    loop_stack: Vec<(u32, u32)>,
    next_id: SymbolId,
    lang: LightLang,
    in_test_file: bool,
}

pub fn extract_light(path: &str, src: &str, lang: LightLang) -> LangFacts {
    let mut parser = Parser::new();
    if parser.set_language(&lang.ts_language()).is_err() {
        let mut f = LangFacts::new(path, lang.lang_id(), lang.analyzer());
        f.unsupported.push("parser_unavailable".into());
        return f;
    }
    let Some(tree) = parser.parse(src, None) else {
        let mut f = LangFacts::new(path, lang.lang_id(), lang.analyzer());
        f.unsupported.push("parse_error".into());
        return f;
    };

    let bytes = src.as_bytes();
    let module_name = super::stem_module(path);
    let mut facts = LangFacts::new(path, lang.lang_id(), lang.analyzer());
    facts.unsupported = lang.unsupported();
    facts.symbols.push(SymbolFact {
        id: SymbolId(0),
        kind: SymbolKind::Module,
        name: module_name.clone(),
        qual_name: module_name,
        loc: SourceLocation::span_bytes(
            FileId(0),
            0,
            tree.root_node().end_byte() as u32,
        ),
        lang: lang.lang_id(),
        parent: None,
        rev: 0,
        doc: None,
    });

    let mut ctx = LightCtx {
        path,
        bytes,
        module: SymbolId(0),
        class: None,
        func: SymbolId(0),
        loop_stack: vec![],
        next_id: SymbolId(1),
        lang,
        in_test_file: path.contains(".test.")
            || path.contains("_test.go")
            || path.contains("tests/")
            || path.contains("/tests"),
    };

    let root = tree.root_node();
    walk_light(&mut facts, &mut ctx, root);
    facts
}

fn push_light_symbol(
    facts: &mut LangFacts,
    ctx: &mut LightCtx,
    kind: SymbolKind,
    name: &str,
    node: Node,
) -> SymbolId {
    let id = ctx.next_id;
    ctx.next_id = SymbolId(id.0 + 1);
    let module_name = facts.symbols[0].name.clone();
    let qual = if kind == SymbolKind::Method {
        match ctx.class {
            Some(c) => format!("{}.{}", facts.symbols[c.0 as usize].qual_name, name),
            None => format!("{}.{}", module_name, name),
        }
    } else {
        format!("{}.{}", module_name, name)
    };
    let (sl, be) = span_of(node);
    facts.symbols.push(SymbolFact {
        id,
        kind,
        name: name.to_string(),
        qual_name: qual,
        loc: SourceLocation::span_bytes(FileId(0), sl, be),
        lang: ctx.lang.lang_id(),
        parent: if kind == SymbolKind::Method {
            ctx.class
        } else {
            Some(ctx.module)
        },
        rev: 0,
        doc: None,
    });
    id
}

fn collect_arg_vars(node: Node, bytes: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" | "shorthand_property_identifier" => {
            out.push(text(node, bytes).to_string());
        }
        "member_expression" | "attribute" | "selector_expression" => {
            if let Some(attr) = node
                .child_by_field_name("attribute")
                .or_else(|| node.child_by_field_name("property"))
            {
                out.push(text(attr, bytes).to_string());
            }
            if let Some(obj) = node.child_by_field_name("object") {
                collect_arg_vars(obj, bytes, out);
            }
        }
        "string" | "string_literal" | "interpreted_string_literal" | "comment" => {}
        _ => {
            let mut c = node.walk();
            let children: Vec<Node> = node.children(&mut c).collect();
            for child in children {
                collect_arg_vars(child, bytes, out);
            }
        }
    }
}

fn walk_light(facts: &mut LangFacts, ctx: &mut LightCtx, node: Node) {
    let kind = node.kind();
    let lang = ctx.lang;

    if lang.function_kinds().contains(&kind) {
        let Some(name) = function_name(node, lang, ctx.bytes) else {
            return;
        };
        let is_test = match lang {
            LightLang::Go => name.starts_with("Test"),
            LightLang::Rust => is_rust_test(node, ctx.bytes),
            _ => ctx.in_test_file,
        };
        let parent_is_impl = matches!(lang, LightLang::Rust | LightLang::Go) && ctx.class.is_some();
        let s_kind = if is_test {
            SymbolKind::Test
        } else if ctx.class.is_some()
            && (parent_is_impl || kind == "method_definition" || kind == "method_declaration")
        {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let id = push_light_symbol(facts, ctx, s_kind, &name, node);
        // async detection
        let node_text = text(node, ctx.bytes);
        if (lang == LightLang::Rust && node_text.starts_with("async fn"))
            || matches!(
                lang,
                LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx
            ) && node.children(&mut node.walk()).any(|c| c.kind() == "async")
        {
            facts.async_facts.push(AsyncSite {
                symbol: id,
                kind: AsyncKind::AsyncFn,
                line: line_of(node),
            });
        }
        let prev = ctx.func;
        ctx.func = id;
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for child in children {
            walk_light(facts, ctx, child);
        }
        ctx.func = prev;
        return;
    }

    if lang.type_kinds().contains(&kind) {
        let name = match lang {
            LightLang::Go => {
                // type_spec: name field; struct_type/interface_type inside
                let n = node
                    .child_by_field_name("name")
                    .map(|n| text(n, ctx.bytes).to_string());
                match n {
                    Some(n) => n,
                    None => return,
                }
            }
            _ => function_name(node, lang, ctx.bytes).unwrap_or_default(),
        };
        if name.is_empty() {
            return;
        }
        let t_kind = match kind {
            "interface_declaration" | "interface_type" => SymbolKind::Interface,
            "struct_item" | "struct_type" => SymbolKind::Struct,
            "trait_item" => SymbolKind::Trait,
            "enum_item" => SymbolKind::Enum,
            _ => SymbolKind::Class,
        };
        let id = push_light_symbol(facts, ctx, t_kind, &name, node);
        let prev = ctx.class;
        ctx.class = Some(id);
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for child in children {
            walk_light(facts, ctx, child);
        }
        ctx.class = prev;
        return;
    }

    if lang.call_kinds().contains(&kind) {
        let callee_text = callee_text_of_call(node, lang, ctx.bytes);
        if callee_text.is_empty() {
            return;
        }
        let line = line_of(node);
        let (cb, ce) = span_of(node);
            let in_loop = ctx
                .loop_stack
                .iter()
                // loop_stack entries are §14.1 byte spans (TASK-006)
                .any(|(s, e)| cb >= *s && ce <= *e);
        let mut arg_vars = vec![];
        if let Some(args) = node.child_by_field_name("arguments") {
            collect_arg_vars(args, ctx.bytes, &mut arg_vars);
        }
        arg_vars.sort();
        arg_vars.dedup();
        let callee = if let Some(b) = effects::classify_builtin(&callee_text) {
            Callee::Builtin(b)
        } else {
            Callee::Unresolved("pending".into())
        };
        facts.calls.push(CallSite {
            caller: ctx.func,
            callee_text: callee_text.clone(),
            callee,
            line,
            in_loop,
            arg_vars,
        });
        if let Some(sink_kind) = effects::classify_sink(&callee_text, text(node, ctx.bytes)) {
            let args_text = node
                .child_by_field_name("arguments")
                .map(|a| text(a, ctx.bytes).to_string())
                .unwrap_or_default();
            let parameterized = args_text.contains('?') || args_text.contains("%s");
            facts.sinks.push(SinkSite {
                symbol: ctx.func,
                call_text: callee_text.clone(),
                arg_vars: facts.calls.last().unwrap().arg_vars.clone(),
                line,
                kind: sink_kind,
                parameterized,
            });
        }
        // test framework calls
        if matches!(callee_text.as_str(), "test" | "it" | "describe") {
            // marked below via test tag; nothing structural needed
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for child in children {
            if child.kind() != "function" && !lang.function_kinds().contains(&child.kind()) {
                walk_light(facts, ctx, child);
            }
        }
        return;
    }

    if lang.loop_kinds().contains(&kind) {
        let line = line_of(node);
        let (bs, be) = span_of(node);
        facts.loops.push(LoopSite {
            symbol: ctx.func,
            kind: if kind.contains("for") {
                LoopKind::For
            } else {
                LoopKind::While
            },
            line,
            body_start: bs,
            body_end: be,
        });
        ctx.loop_stack.push((bs, be));
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for child in children {
            walk_light(facts, ctx, child);
        }
        ctx.loop_stack.pop();
        return;
    }

    if lang.assignment_kinds().contains(&kind) {
        let (var_node, value_node) = match lang {
            LightLang::JavaScript | LightLang::TypeScript | LightLang::Tsx => match kind {
                "variable_declarator" => (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("value"),
                ),
                "assignment_expression" => (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ),
                _ => {
                    // lexical_declaration: first variable_declarator child
                    let mut c = node.walk();
                    let decl = node
                        .children(&mut c)
                        .find(|ch| ch.kind() == "variable_declarator");
                    match decl {
                        Some(d) => (
                            d.child_by_field_name("name"),
                            d.child_by_field_name("value"),
                        ),
                        None => (None, None),
                    }
                }
            },
            LightLang::Go => (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ),
            LightLang::Rust => (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ),
        };
        if let (Some(vn), Some(vnode)) = (var_node, value_node) {
            let var = text(vn, ctx.bytes)
                .trim_start_matches('&')
                .trim_end_matches(':')
                .to_string();
            let rhs_text = text(vnode, ctx.bytes).to_string();
            let rhs_kind = if vnode.kind().contains("call") || vnode.kind() == "macro_invocation" {
                RhsKind::Call
            } else if vnode.kind().contains("string") {
                RhsKind::Literal
            } else if matches!(
                vnode.kind(),
                "identifier"
                    | "member_expression"
                    | "attribute"
                    | "selector_expression"
                    | "subscript"
                    | "index_expression"
            ) {
                RhsKind::Vars
            } else {
                RhsKind::Other
            };
            let mut rhs_data = vec![];
            collect_arg_vars(vnode, ctx.bytes, &mut rhs_data);
            rhs_data.sort();
            rhs_data.dedup();
            facts.assignments.push(AssignmentFact {
                symbol: ctx.func,
                var,
                line: line_of(node),
                rhs_kind,
                rhs_data,
            });
            if let Some(sk) = effects::classify_source(&rhs_text) {
                facts.sources.push(SourceSite {
                    symbol: ctx.func,
                    var: text(vn, ctx.bytes).trim_start_matches('&').to_string(),
                    line: line_of(node),
                    kind: sk,
                });
            }
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for child in children {
            walk_light(facts, ctx, child);
        }
        return;
    }

    if lang.string_kinds().contains(&kind) {
        let t = text(node, ctx.bytes).to_string();
        if t.len() > 2 {
            facts.literals.push(LiteralFact {
                symbol: ctx.func,
                line: line_of(node),
                value: t.chars().take(240).collect(),
                is_string: kind != "template_string",
            });
        }
        return;
    }

    if lang.import_kinds().contains(&kind) {
        facts.imports.push(ImportFact {
            module_symbol: ctx.module,
            module: text(node, ctx.bytes).chars().take(200).collect(),
            names: vec![],
            alias: None,
            line: line_of(node),
        });
        return;
    }

    // async / exceptions / throw light facts
    match kind {
        "await_expression" => {
            facts.async_facts.push(AsyncSite {
                symbol: ctx.func,
                kind: AsyncKind::Await,
                line: line_of(node),
            });
        }
        "throw_statement" => {
            facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: "throw".into(),
                is_bare: false,
                line: line_of(node),
                in_handler: false,
                handler_end: 0,
            });
        }
        "panic_expression" | "panic_statement" => {
            facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text: "panic".into(),
                is_bare: false,
                line: line_of(node),
                in_handler: false,
                handler_end: 0,
            });
        }
        "go_statement" => {
            facts.async_facts.push(AsyncSite {
                symbol: ctx.func,
                kind: AsyncKind::Await,
                line: line_of(node),
            });
        }
        _ => {}
    }

    let mut c = node.walk();
    let children: Vec<Node> = node.children(&mut c).collect();
    for child in children {
        walk_light(facts, ctx, child);
    }
}
