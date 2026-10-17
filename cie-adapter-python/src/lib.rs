//! Deep Python analyzer built on tree-sitter.
//!
//! Extracts the full fact set: symbols (modules/classes/functions/methods),
//! calls, returns (with constant normalization), exceptions and handler
//! bodies, branches, loops (with body ranges for N+1), assignments (for
//! taint propagation), taint sources/sinks, string literals, imports and
//! async facts. Unmodeled constructs are recorded as `unsupported`.

use cie_adapters::effects;
use cie_core::types::{FileId, SourceLocation, SymbolId};
use cie_semantic::facts::*;
use cie_semantic::LangId;
use tree_sitter::{Node, Parser};

pub struct PythonAdapter;

impl cie_adapters::LanguageAdapter for PythonAdapter {
    fn id(&self) -> LangId {
        LangId::Python
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyw"]
    }

    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:python"
    }

    // TASK-011 semantics tables (§16 #1): Python vouches for dynamic
    // dispatch, arbitrary-precision ints + binary64 floats, and its
    // swallow-friendly exception/async conventions.
    fn dispatch_kind(&self) -> cie_adapters::DispatchKind {
        cie_adapters::DispatchKind::Dynamic
    }

    fn numeric_semantics(&self) -> cie_adapters::NumericSemantics {
        cie_adapters::NumericSemantics {
            overflow: cie_adapters::OverflowPolicy::ArbitraryPrecision,
            coercion: cie_adapters::CoercionPolicy::ImplicitAny,
            precision: cie_adapters::Precision::Arbitrary,
        }
    }

    fn framework_conventions(&self) -> &'static [cie_adapters::FrameworkRule] {
        &[
            cie_adapters::FrameworkRule {
                name: "flask-route",
                convention: "decorated function is an HTTP entry point",
            },
            cie_adapters::FrameworkRule {
                name: "context-manager",
                convention: "with-statement calls __enter__/__exit__",
            },
            cie_adapters::FrameworkRule {
                name: "bare-except",
                convention: "bare except: swallows every exception",
            },
        ]
    }

    fn extract(&self, path: &str, src: &str) -> LangFacts {
        extract_python(path, src)
    }
}

pub fn to_language() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

pub fn extract_python(path: &str, src: &str) -> LangFacts {
    let mut parser = Parser::new();
    if parser.set_language(&to_language()).is_err() {
        let mut f = LangFacts::new(path, LangId::Python, "tree-sitter-deep:python");
        f.unsupported.push("parser_unavailable".into());
        return f;
    }
    let Some(tree) = parser.parse(src, None) else {
        let mut f = LangFacts::new(path, LangId::Python, "tree-sitter-deep:python");
        f.unsupported.push("parse_error".into());
        return f;
    };

    let bytes = src.as_bytes();
    let module_name = cie_adapters::stem_module(path);
    let mut facts = LangFacts::new(path, LangId::Python, "tree-sitter-deep:python");

    // Local symbol 0 = the module.
    facts.symbols.push(SymbolFact {
        id: SymbolId(0),
        kind: SymbolKind::Module,
        name: module_name.clone(),
        qual_name: module_name.clone(),
        loc: SourceLocation::span_bytes(FileId(0), 0, tree.root_node().end_byte() as u32),
        lang: LangId::Python,
        parent: None,
        rev: 0,
        doc: None,
    });

    let mut ctx = Ctx {
        path,
        bytes,
        module: SymbolId(0),
        class: None,
        func: SymbolId(0),
        loop_stack: vec![],
        handler_depth: 0,
        next_id: SymbolId(1),
    };

    let root = tree.root_node();
    walk(&mut facts, &mut ctx, root);
    facts
}

struct Ctx<'a> {
    path: &'a str,
    bytes: &'a [u8],
    module: SymbolId,
    class: Option<SymbolId>,
    func: SymbolId,
    loop_stack: Vec<(u32, u32)>,
    handler_depth: u32,
    next_id: SymbolId,
}

fn text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// §14.1 byte span of a node: `(start_byte, end_byte)` straight from
/// tree-sitter (TASK-006).
fn span_of(node: Node) -> (u32, u32) {
    let r = node.byte_range();
    (r.start as u32, r.end as u32)
}

/// Normalize a return expression into a constant class.
/// "true"/"false"/"none"/"num"/"str"/"empty" — or None when not constant.
pub fn normalize_constant(expr: &str) -> Option<String> {
    let t = expr.trim();
    if t.is_empty() {
        return None;
    }
    match t {
        "True" | "true" => Some("true".into()),
        "False" | "false" => Some("false".into()),
        "None" | "null" | "nil" | "undefined" => Some("none".into()),
        "{}" | "[]" | "()" | "{,}" => Some("empty".into()),
        _ => {
            if t.parse::<f64>().is_ok() {
                Some("num".into())
            } else if (t.starts_with('"') && t.ends_with('"'))
                || (t.starts_with('\'') && t.ends_with('\''))
            {
                if t.len() == 2 {
                    Some("empty".into())
                } else {
                    Some("str".into())
                }
            } else if t.starts_with("f\"") || t.starts_with("f'") {
                Some("fstr".into())
            } else {
                None
            }
        }
    }
}

pub fn is_success_const(c: &str) -> bool {
    matches!(c, "true" | "num" | "str" | "empty")
}

fn push_symbol(
    facts: &mut LangFacts,
    ctx: &mut Ctx,
    kind: SymbolKind,
    name: &str,
    node: Node,
    doc: Option<String>,
) -> SymbolId {
    let id = ctx.next_id;
    ctx.next_id = SymbolId(id.0 + 1);
    let qual = match (kind, ctx.class) {
        (SymbolKind::Method, Some(cls)) => {
            format!(
                "{}.{}.{}",
                facts.symbols[0].name, facts.symbols[cls.0 as usize].name, name
            )
        }
        (SymbolKind::Method, None) => {
            format!("{}.{}", facts.symbols[0].name, name)
        }
        _ => {
            if let Some(class_id) = ctx.class {
                format!(
                    "{}.{}.{}",
                    facts.symbols[0].name,
                    facts.symbols[class_id.0 as usize].name,
                    name
                )
            } else {
                format!("{}.{}", facts.symbols[0].name, name)
            }
        }
    };
    let (sl, be) = span_of(node);
    facts.symbols.push(SymbolFact {
        id,
        kind,
        name: name.to_string(),
        qual_name: qual,
        loc: SourceLocation::span_bytes(FileId(0), sl, be),
        lang: LangId::Python,
        parent: match kind {
            SymbolKind::Method => ctx.class,
            _ => Some(ctx.module),
        },
        rev: 0,
        doc,
    });
    id
}

fn docstring_of(body: Node, bytes: &[u8]) -> Option<String> {
    let mut c = body.walk();
    let first = body.children(&mut c).next()?;
    if first.kind() == "expression_statement" {
        let mut c2 = first.walk();
        let inner = first.children(&mut c2).next()?;
        if inner.kind() == "string" {
            let t = text(inner, bytes);
            let t = t.trim_matches(|ch| ch == '"' || ch == '\'');
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.chars().take(400).collect());
            }
        }
    }
    None
}

fn walk(facts: &mut LangFacts, ctx: &mut Ctx, node: Node) {
    let kind = node.kind();

    match kind {
        "function_definition" => {
            let is_async = node.children(&mut node.walk()).any(|c| c.kind() == "async");
            let name_node = node.child_by_field_name("name").unwrap_or(node);
            let name = text(name_node, ctx.bytes).to_string();
            let s_kind = if ctx.class.is_some() {
                if name.starts_with("test_") {
                    SymbolKind::Test
                } else {
                    SymbolKind::Method
                }
            } else if name.starts_with("test_") {
                SymbolKind::Test
            } else {
                SymbolKind::Function
            };
            let id = push_symbol(facts, ctx, s_kind, &name, node, None);
            if is_async {
                facts.async_facts.push(AsyncSite {
                    symbol: id,
                    kind: AsyncKind::AsyncFn,
                    line: line_of(node),
                });
            }
            if let Some(body) = node.child_by_field_name("body") {
                let doc = docstring_of(body, ctx.bytes);
                if doc.is_some() {
                    facts.symbols[id.0 as usize].doc = doc;
                }
                let prev_fn = ctx.func;
                ctx.func = id;
                walk_children(facts, ctx, body);
                ctx.func = prev_fn;
            }
        }
        "class_definition" => {
            let name_node = node.child_by_field_name("name").unwrap_or(node);
            let name = text(name_node, ctx.bytes).to_string();
            let id = push_symbol(facts, ctx, SymbolKind::Class, &name, node, None);
            let prev_class = ctx.class;
            ctx.class = Some(id);
            if let Some(body) = node.child_by_field_name("body") {
                walk_children(facts, ctx, body);
            }
            ctx.class = prev_class;
        }
        "decorated_definition" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                match child.kind() {
                    "decorator" => {
                        // Record decorator as a call (e.g. @app.route(...)).
                        let t = text(child, ctx.bytes).trim_start_matches('@').to_string();
                        if !t.is_empty() {
                            facts.calls.push(CallSite {
                                caller: ctx.func,
                                callee_text: t,
                                callee: Callee::Unresolved("pending".into()),
                                line: line_of(child),
                                in_loop: false,
                                arg_vars: vec![],
                            });
                        }
                    }
                    _ => walk(facts, ctx, child),
                }
            }
        }
        "call" => {
            let fn_node = node.child_by_field_name("function").unwrap_or(node);
            let callee_text = text(fn_node, ctx.bytes).to_string();
            let line = line_of(node);
            let (cb, ce) = span_of(node);
            let in_loop = ctx
                .loop_stack
                .iter()
                // loop_stack entries are §14.1 byte spans (TASK-006)
                .any(|(s, e)| cb >= *s && ce <= *e);

            // Argument variables + whole-args text (for sink/source checks).
            let mut arg_vars = vec![];
            let mut args_text = String::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                args_text = text(args, ctx.bytes).to_string();
                collect_identifiers(args, ctx.bytes, &mut arg_vars);
            }

            let callee = if let Some(b) = effects::classify_builtin(&callee_text) {
                Callee::Builtin(b)
            } else {
                Callee::Unresolved("pending".into())
            };
            facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text,
                callee,
                line,
                in_loop,
                arg_vars: dedup(arg_vars),
            });

            // Sink detection.
            if let Some(sink_kind) =
                effects::classify_sink(&facts.calls.last().unwrap().callee_text, &args_text)
            {
                let parameterized = args_text.contains('?')
                    || args_text.contains("%s")
                    || args_text.contains("%(")
                    || args_text.contains("${");
                facts.sinks.push(SinkSite {
                    symbol: ctx.func,
                    call_text: facts.calls.last().unwrap().callee_text.clone(),
                    arg_vars: facts.calls.last().unwrap().arg_vars.clone(),
                    line,
                    kind: sink_kind,
                    parameterized,
                });
            }

            // Recurse into arguments for nested calls.
            if let Some(args) = node.child_by_field_name("arguments") {
                walk_children(facts, ctx, args);
            }
        }
        "return_statement" => {
            let mut parts = vec![];
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() != "return" {
                    parts.push(text(child, ctx.bytes).to_string());
                }
            }
            let expr = parts.join(" ").trim().to_string();
            let constant = normalize_constant(&expr);
            facts.returns.push(ReturnSite {
                symbol: ctx.func,
                line: line_of(node),
                expr,
                constant,
            });
            let mut c = node.walk();
            for child in node.children(&mut c) {
                walk(facts, ctx, child);
            }
        }
        "raise_statement" => {
            let t = text(node, ctx.bytes)
                .trim_start_matches("raise ")
                .to_string();
            let kind_text = t
                .split(['(', ' ', '\n'])
                .next()
                .unwrap_or("")
                .to_string();
            facts.exceptions.push(ExceptionSite {
                symbol: ctx.func,
                kind_text,
                is_bare: false,
                line: line_of(node),
                in_handler: ctx.handler_depth > 0,
                handler_end: 0,
            });
        }
        "try_statement" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                match child.kind() {
                    "except_clause" => {
                        // type child may be absent -> bare except.
                        // Children between the `except` keyword and `as`/block:
                        let mut type_text = String::new();
                        let mut after_except = false;
                        let mut cc = child.walk();
                        let children: Vec<Node> = child.children(&mut cc).collect();
                        for gc in children {
                            match gc.kind() {
                                "except" => after_except = true,
                                "as" | "block" | "comment" => after_except = false,
                                k if after_except
                                    && matches!(
                                        k,
                                        "identifier"
                                            | "attribute"
                                            | "tuple"
                                            | "string"
                                            | "dotted_name"
                                    ) =>
                                {
                                    type_text = text(gc, ctx.bytes).to_string();
                                    after_except = false;
                                }
                                _ => {}
                            }
                        }
                        let handler_end = child
                            .children(&mut child.walk())
                            .filter(|c| c.kind() == "block")
                            .last()
                            .map(|b| b.end_position().row as u32 + 1)
                            .unwrap_or(line_of(child));
                        facts.exceptions.push(ExceptionSite {
                            symbol: ctx.func,
                            is_bare: type_text.is_empty(),
                            kind_text: type_text,
                            line: line_of(child),
                            in_handler: false,
                            handler_end,
                        });
                        if let Some(block) = child
                            .children(&mut child.walk())
                            .filter(|c| c.kind() == "block")
                            .last()
                        {
                            let prev = ctx.handler_depth;
                            ctx.handler_depth += 1;
                            walk_children(facts, ctx, block);
                            ctx.handler_depth = prev;
                        }
                    }
                    "else_clause" | "finally_clause" => walk_children(facts, ctx, child),
                    "block" => walk_children(facts, ctx, child),
                    _ => {}
                }
            }
        }
        "if_statement" | "elif_clause" => {
            if let Some(cond) = node.child_by_field_name("condition") {
                let cond_text = text(cond, ctx.bytes).to_string();
                let mut vars = vec![];
                collect_identifiers(cond, ctx.bytes, &mut vars);
                facts.ifs.push(IfSite {
                    symbol: ctx.func,
                    line: line_of(node),
                    cond: cond_text,
                    cond_vars: dedup(vars),
                });
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                walk(facts, ctx, child);
            }
        }
        "for_statement" | "while_statement" => {
            let (bs, be) = node
                .child_by_field_name("body")
                .map(span_of)
                .unwrap_or((line_of(node), line_of(node)));
            facts.loops.push(LoopSite {
                symbol: ctx.func,
                kind: if kind == "for_statement" {
                    LoopKind::For
                } else {
                    LoopKind::While
                },
                line: line_of(node),
                body_start: bs,
                body_end: be,
            });
            ctx.loop_stack.push((bs, be));
            walk_children(facts, ctx, node);
            ctx.loop_stack.pop();
        }
        "assignment" | "augmented_assignment" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if let (Some(left), Some(right)) = (left, right) {
                if left.kind() == "identifier" {
                    let var = text(left, ctx.bytes).to_string();
                    let rhs_text = text(right, ctx.bytes).to_string();
                    let (rhs_kind, rhs_data) = classify_rhs(right, ctx.bytes, &rhs_text);
                    facts.assignments.push(AssignmentFact {
                        symbol: ctx.func,
                        var: var.clone(),
                        line: line_of(node),
                        rhs_kind,
                        rhs_data: rhs_data.clone(),
                    });
                    if let Some(sk) = effects::classify_source(&rhs_text) {
                        facts.sources.push(SourceSite {
                            symbol: ctx.func,
                            var,
                            line: line_of(node),
                            kind: sk,
                        });
                    }
                }
            }
            walk_children(facts, ctx, node);
        }
        "string" => {
            let t = text(node, ctx.bytes).to_string();
            let is_fstring = t.starts_with("f\"")
                || t.starts_with("f'")
                || node
                    .children(&mut node.walk())
                    .any(|c| c.kind() == "interpolation");
            if !is_fstring {
                facts.literals.push(LiteralFact {
                    symbol: ctx.func,
                    line: line_of(node),
                    value: t.chars().take(240).collect(),
                    is_string: true,
                });
            } else {
                facts.literals.push(LiteralFact {
                    symbol: ctx.func,
                    line: line_of(node),
                    value: t.chars().take(240).collect(),
                    is_string: false,
                });
            }
            walk_children(facts, ctx, node);
        }
        "import_statement" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                match child.kind() {
                    "dotted_name" | "identifier" => {
                        facts.imports.push(ImportFact {
                            module_symbol: ctx.module,
                            module: text(child, ctx.bytes).to_string(),
                            names: vec![],
                            alias: None,
                            line: line_of(node),
                        });
                    }
                    "aliased_import" => {
                        let name = child
                            .child_by_field_name("name")
                            .map(|n| text(n, ctx.bytes).to_string())
                            .unwrap_or_default();
                        let alias = child
                            .child_by_field_name("alias")
                            .map(|n| text(n, ctx.bytes).to_string());
                        facts.imports.push(ImportFact {
                            module_symbol: ctx.module,
                            module: name,
                            names: vec![],
                            alias,
                            line: line_of(node),
                        });
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            let module = node
                .child_by_field_name("module_name")
                .map(|n| text(n, ctx.bytes).to_string())
                .unwrap_or_default();
            let mut names = vec![];
            let mut c = node.walk();
            for child in node.children(&mut c) {
                match child.kind() {
                    "dotted_name" | "identifier" => names.push(text(child, ctx.bytes).to_string()),
                    "aliased_import" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            names.push(text(n, ctx.bytes).to_string());
                        }
                    }
                    "wildcard_import" => names.push("*".into()),
                    _ => {}
                }
            }
            // "from . import x" — the relative import node is not module_name
            let module = if module.is_empty() {
                ".".into()
            } else {
                module
            };
            facts.imports.push(ImportFact {
                module_symbol: ctx.module,
                module,
                names,
                alias: None,
                line: line_of(node),
            });
        }
        "await" => {
            facts.async_facts.push(AsyncSite {
                symbol: ctx.func,
                kind: AsyncKind::Await,
                line: line_of(node),
            });
            walk_children(facts, ctx, node);
        }
        "pass_statement" | "break_statement" | "continue_statement" | "comment"
        | "line_continuation" => {
        }
        _ => {
            walk_children(facts, ctx, node);
        }
    }
}

fn classify_rhs(right: Node, bytes: &[u8], rhs_text: &str) -> (RhsKind, Vec<String>) {
    match right.kind() {
        "call" => {
            let fn_text = right
                .child_by_field_name("function")
                .map(|n| text(n, bytes).to_string())
                .unwrap_or_default();
            if effects::classify_source(rhs_text).is_some() {
                (RhsKind::Input, vec![fn_text])
            } else {
                (RhsKind::Call, vec![fn_text])
            }
        }
        "string" => {
            let t = rhs_text;
            if t.starts_with("f\"") || t.starts_with("f'") {
                let mut vars = vec![];
                collect_interpolations(right, bytes, &mut vars);
                (RhsKind::FString, dedup(vars))
            } else {
                (RhsKind::Literal, vec![])
            }
        }
        "identifier" | "attribute" | "subscript" => {
            let mut vars = vec![];
            collect_identifiers(right, bytes, &mut vars);
            (RhsKind::Vars, dedup(vars))
        }
        "binary_operator" | "boolean_operator" => {
            let mut vars = vec![];
            collect_identifiers(right, bytes, &mut vars);
            (RhsKind::Vars, dedup(vars))
        }
        "integer" | "float" | "true" | "false" | "none" | "list" | "dictionary" | "set"
        | "tuple" => (RhsKind::Literal, vec![]),
        _ => {
            let mut vars = vec![];
            collect_identifiers(right, bytes, &mut vars);
            if vars.is_empty() {
                (RhsKind::Other, vec![])
            } else {
                (RhsKind::Vars, dedup(vars))
            }
        }
    }
}

fn collect_interpolations(node: Node, bytes: &[u8], out: &mut Vec<String>) {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if child.kind() == "interpolation" {
            collect_identifiers(child, bytes, out);
        } else if child.kind() == "string_content" {
            // plain content — nothing
        } else {
            collect_interpolations(child, bytes, out);
        }
    }
}

fn collect_identifiers(node: Node, bytes: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            let t = text(node, bytes).to_string();
            if !t.is_empty() && t != "self" && t != "cls" {
                out.push(t);
            }
        }
        "attribute" => {
            if let Some(attr) = node.child_by_field_name("attribute") {
                out.push(text(attr, bytes).to_string());
            }
            if let Some(obj) = node.child_by_field_name("object") {
                collect_identifiers(obj, bytes, out);
            }
        }
        "keyword_argument" => {
            if let Some(v) = node.child_by_field_name("value") {
                collect_identifiers(v, bytes, out);
            }
        }
        "comment" => (),
        "string" | "concatenated_string" => {
            // f-string interpolations carry taint
            let mut c = node.walk();
            let children: Vec<Node> = node.children(&mut c).collect();
            for child in children {
                if child.kind() == "interpolation" {
                    collect_identifiers(child, bytes, out);
                }
            }
        }
        _ => {
            // Document order (node.id() is not stable across parses — FR-4).
            let mut c = node.walk();
            for child in node.children(&mut c) {
                collect_identifiers(child, bytes, out);
            }
        }
    }
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

fn walk_children(facts: &mut LangFacts, ctx: &mut Ctx, node: Node) {
    // children() yields document order; sorting by node.id() (the v0.1
    // behavior) is NOT stable across parses and broke FR-4 byte-stable
    // evidence — same source must always yield same facts.
    let mut c = node.walk();
    for child in node.children(&mut c) {
        walk(facts, ctx, child);
    }
}
