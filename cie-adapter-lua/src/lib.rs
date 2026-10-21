//! Lua deep adapter (v1.2.2): metatable dynamic dispatch, `error()`/`pcall`
//! raise semantics, implicit numeric coercion.
//!
//! Own tree walk over `tree-sitter-lua` (parse → extract contract per
//! §16.1): `function f` / `local function f` / `T.field = function ... end`
//! as symbols, `function_call` with builtin classification, `require` as
//! module imports, `error()` as raised, `pcall`/`xpcall` as caught,
//! `for`/`while`/`repeat` loops, `if` conditions, assignments, returns.
//!
//! Semantics tables (§16, vouched here — FR-3):
//! - dispatch: `Dynamic` — every field lookup can be redirected at runtime
//!   by metatables (`__index`/`__newindex`), so dotted calls stay unresolved
//! - numeric: Lua 5.3+ integer operations wrap on overflow
//!   (`OverflowPolicy::Wrapping`); pre-5.3 every number is a double and
//!   overflow cannot happen — the wrap row is vouched with that caveat in
//!   the framework conventions. String↔number coercion is implicit
//!   (`"10" + 1 == 11.0`), so `CoercionPolicy::ImplicitAny`
//! - exceptions: `error()` aborts; `pcall`/`xpcall` catch — an unchecked
//!   raise model (Raise). Returning `false, msg` is a convention, not the
//!   language contract, and stays in the conventions table
//! - async: none — `coroutine.create/resume/yield` are cooperative threads,
//!   NOT spawns, and are recorded as an honesty note (FR-4)

use cie_adapters::{
    effects, CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    Callee, CallSite, ExceptionSite, ImportFact, LangFacts, LoopKind, LoopSite, RhsKind,
    SinkKind, SourceKind, SymbolFact, SymbolKind,
};
use cie_semantic::LangId;
use std::collections::BTreeSet;
use tree_sitter::Parser;

pub struct LuaAdapter;

pub fn to_language() -> tree_sitter::Language {
    tree_sitter_lua::LANGUAGE.into()
}

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

struct LuaCtx<'a> {
    bytes: &'a [u8],
    facts: LangFacts,
    func: SymbolId,
    next_id: u32,
    fn_names: BTreeSet<String>,
    notes: BTreeSet<String>,
}

impl<'a> LuaCtx<'a> {
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

fn children_of<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = node.walk();
    node.children(&mut c).collect()
}

/// A name in a `variable_list`: the grammar emits the concrete node
/// directly (`identifier`, `dot_index_expression`, ...) — there is no
/// `variable` wrapper at runtime.
fn variable_name_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "dot_index_expression" | "method_index_expression"
        | "bracket_index_expression" => Some(text_of(node, bytes).trim().to_string()),
        _ => None,
    }
}

fn collect_variable_names(vl: tree_sitter::Node, bytes: &[u8], out: &mut Vec<String>) {
    for v in children_of(vl) {
        if let Some(name) = variable_name_text(v, bytes) {
            out.push(name);
        }
    }
}

/// The Lua 5.4 standard library, one row per name that changes what a call
/// DOES (Lua 5.4 Reference Manual §6). Checked BEFORE the shared
/// classifier, whose Python-centric table would misroute `os.execute`
/// (shell, not SQL) and has no rows for `pairs`/`setmetatable`/...
fn lua_builtin_kind(callee: &str) -> Option<cie_semantic::facts::BuiltinKind> {
    use cie_semantic::facts::BuiltinKind;
    let t = callee.trim();
    // Dotted stdlib prefixes first.
    for (prefix, kind) in [
        ("os.execute", BuiltinKind::ProcessExec),
        ("io.popen", BuiltinKind::ProcessExec),
        ("os.getenv", BuiltinKind::Env),
        ("os.time", BuiltinKind::Time),
        ("os.clock", BuiltinKind::Time),
        ("os.date", BuiltinKind::Time),
        ("os.rename", BuiltinKind::FileWrite),
        ("os.remove", BuiltinKind::FileWrite),
        ("os.tmpname", BuiltinKind::FileWrite),
        ("io.open", BuiltinKind::FileRead),
        ("io.lines", BuiltinKind::FileRead),
        ("io.read", BuiltinKind::FileRead),
        ("io.input", BuiltinKind::FileRead),
        ("io.write", BuiltinKind::Other),
        ("io.close", BuiltinKind::Other),
        ("math.random", BuiltinKind::Random),
        ("string.format", BuiltinKind::Other),
        ("table.insert", BuiltinKind::Other),
        ("table.remove", BuiltinKind::Other),
        ("table.concat", BuiltinKind::Other),
        ("table.sort", BuiltinKind::Other),
        ("coroutine.", BuiltinKind::Other),
        ("string.", BuiltinKind::Other),
        ("math.", BuiltinKind::Other),
        ("utf8.", BuiltinKind::Other),
    ] {
        if t.starts_with(prefix) {
            return Some(kind);
        }
    }
    // Bare global builtins.
    let kind = match t {
        "print" => BuiltinKind::Print,
        "assert" => BuiltinKind::Assert,
        "load" | "loadstring" => BuiltinKind::Eval,
        "dofile" | "loadfile" => BuiltinKind::FileRead,
        "error" | "pcall" | "xpcall" | "require" | "setmetatable" | "getmetatable"
        | "type" | "tostring" | "tonumber" | "pairs" | "ipairs" | "next" | "select"
        | "rawget" | "rawset" | "rawequal" | "rawlen" | "unpack" | "collectgarbage"
        | "_G" => BuiltinKind::Other,
        _ => return None,
    };
    Some(kind)
}

/// Lua sinks the shared classifier would misroute: `os.execute` is a shell,
/// `load` compiles a chunk (the shared table only knows eval/exec names).
fn lua_sink_kind(callee: &str) -> Option<SinkKind> {
    let t = callee.trim();
    if t == "os.execute" || t == "io.popen" {
        return Some(SinkKind::ShellExec);
    }
    if t == "load" || t == "loadstring" || t == "dofile" {
        return Some(SinkKind::CodeEval);
    }
    None
}

fn walk_children(ctx: &mut LuaCtx, node: tree_sitter::Node) {
    for child in children_of(node) {
        walk_lua(ctx, child);
    }
}

/// First string-literal argument, unquoted — for `require("mod")`.
fn first_string_arg(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;
    for e in children_of(args) {
        if e.kind() == "string" {
            let raw = text_of(e, bytes).trim();
            let unquoted = raw
                .trim_start_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('"')
                .trim_end_matches('\'');
            return Some(unquoted.to_string());
        }
        if e.kind() == "parenthesized_expression" {
            for inner in children_of(e) {
                if inner.kind() == "string" {
                    let raw = text_of(inner, bytes).trim();
                    return Some(raw.trim_matches(|c| c == '"' || c == '\'').to_string());
                }
            }
        }
    }
    None
}

fn arg_vars_of_call(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    if let Some(args) = node.child_by_field_name("arguments") {
        for e in children_of(args) {
            if e.kind() == "identifier" {
                out.push(text_of(e, bytes).to_string());
            }
        }
    }
    out
}

/// Last dotted segment of a name: `M.test_foo` → `test_foo`.
fn last_segment(name: &str) -> &str {
    name.rsplit(['.', ':'])
        .next()
        .unwrap_or(name)
}

fn walk_lua(ctx: &mut LuaCtx, node: tree_sitter::Node) {
    match node.kind() {
        "function_declaration" => {
            let name_node = node.child_by_field_name("name");
            let name = name_node
                .map(|n| text_of(n, ctx.bytes).to_string())
                .unwrap_or_default();
            if name.is_empty() {
                walk_children(ctx, node);
                return;
            }
            let is_method = name_node
                .map(|n| n.kind() == "method_index_expression")
                .unwrap_or(false);
            let is_test = last_segment(&name).starts_with("test");
            let kind = if is_test {
                SymbolKind::Test
            } else if is_method {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            let id = ctx.id();
            ctx.facts.symbols.push(SymbolFact {
            annotations: Vec::new(),
                id,
                kind,
                name: name.clone(),
                qual_name: name.clone(),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
                lang: LangId::Lua,
                parent: Some(ctx.func),
                rev: 0,
                doc: None,
            });
            ctx.fn_names.insert(name);
            let prev = ctx.func;
            ctx.func = id;
            walk_children(ctx, node);
            ctx.func = prev;
            return;
        }
        "function_call" => {
            let callee_text = node
                .child_by_field_name("name")
                .map(|n| text_of(n, ctx.bytes).trim().to_string())
                .unwrap_or_default();
            // require("mod") — the Lua import mechanism.
            if callee_text == "require" {
                if let Some(mod_name) = first_string_arg(node, ctx.bytes) {
                    ctx.facts.imports.push(ImportFact {
                        module_symbol: SymbolId(0),
                        module: mod_name,
                        names: vec![],
                        alias: None,
                        line: ctx.line(node),
                    });
                }
            }
            let callee = if let Some(b) = lua_builtin_kind(&callee_text) {
                Callee::Builtin(b)
            } else if let Some(b) = effects::classify_builtin(&callee_text) {
                Callee::Builtin(b)
            } else if callee_text.contains('.') || callee_text.contains(':') {
                Callee::Unresolved("external_module".into())
            } else {
                Callee::Unresolved("unresolved_name".into())
            };
            let args_text = node
                .child_by_field_name("arguments")
                .map(|a| text_of(a, ctx.bytes).to_string())
                .unwrap_or_default();
            ctx.facts.calls.push(CallSite {
                caller: ctx.func,
                callee_text: callee_text.clone(),
                callee,
                line: ctx.line(node),
                in_loop: ctx.in_loop(node),
                arg_vars: arg_vars_of_call(node, ctx.bytes),
            });
            // Lua-specific raises/catches + sinks (checked before the shared
            // classifier, which would misroute `os.execute` → SQL).
            if callee_text == "error" {
                ctx.facts.exceptions.push(ExceptionSite {
                    symbol: ctx.func,
                    kind_text: "error()".into(),
                    is_bare: false,
                    line: ctx.line(node),
                    in_handler: false,
                    handler_end: 0,
                });
            }
            if callee_text == "pcall" || callee_text == "xpcall" {
                ctx.facts.exceptions.push(ExceptionSite {
                    symbol: ctx.func,
                    kind_text: callee_text.clone(),
                    is_bare: false,
                    line: ctx.line(node),
                    in_handler: true,
                    handler_end: cie_core::types::line_of(ctx.bytes, node.end_byte() as u32),
                });
            }
            let sink = lua_sink_kind(&callee_text)
                .or_else(|| effects::classify_sink(&callee_text, &args_text));
            if let Some(kind) = sink {
                ctx.facts.sinks.push(cie_semantic::facts::SinkSite {
                    symbol: ctx.func,
                    call_text: callee_text.clone(),
                    arg_vars: arg_vars_of_call(node, ctx.bytes),
                    line: ctx.line(node),
                    kind,
                    parameterized: false,
                });
            }
            walk_children(ctx, node);
            return;
        }
        "variable_declaration" => {
            // `local x, y = f(), g` — names from the declaration's own
            // variable_list (or, when the grammar nests it, from the
            // assignment_statement's), values from the nested
            // assignment_statement's expression_list.
            let mut names: Vec<String> = vec![];
            let mut rhs_nodes: Vec<tree_sitter::Node> = vec![];
            for child in children_of(node) {
                if child.kind() == "variable_list" {
                    collect_variable_names(child, ctx.bytes, &mut names);
                } else if child.kind() == "assignment_statement" {
                    for c2 in children_of(child) {
                        if c2.kind() == "variable_list" && names.is_empty() {
                            collect_variable_names(c2, ctx.bytes, &mut names);
                        }
                        if c2.kind() == "expression_list" {
                            rhs_nodes = children_of(c2);
                        }
                    }
                }
            }
            emit_assignments(ctx, node, &names, &rhs_nodes);
            // `local h = function() end` — the closure is a real function.
            for rhs in &rhs_nodes {
                if rhs.kind() == "function_definition" {
                    let nm = assignment_lhs_name(node, ctx.bytes);
                    emit_anonymous_fn(ctx, *rhs, &nm);
                }
            }
            // The nested assignment_statement's own facts were emitted
            // above — but its VALUES still get walked (a `require` call or
            // any other call hides there), so imports/calls are not lost.
            for child in children_of(node) {
                if child.kind() == "assignment_statement" {
                    for c2 in children_of(child) {
                        if c2.kind() == "expression_list" {
                            walk_children(ctx, c2);
                        }
                    }
                } else {
                    walk_lua(ctx, child);
                }
            }
            return;
        }
        "assignment_statement" => {
            let mut names: Vec<String> = vec![];
            for child in children_of(node) {
                if child.kind() == "variable_list" {
                    collect_variable_names(child, ctx.bytes, &mut names);
                }
            }
            let mut rhs_nodes: Vec<tree_sitter::Node> = vec![];
            for c2 in children_of(node) {
                if c2.kind() == "expression_list" {
                    rhs_nodes = children_of(c2);
                }
            }
            emit_assignments(ctx, node, &names, &rhs_nodes);
            // `M.handler = function(ev) ... end` — the module pattern.
            for rhs in &rhs_nodes {
                if rhs.kind() == "function_definition" {
                    let nm = names.first().cloned().unwrap_or_default();
                    emit_anonymous_fn(ctx, *rhs, &nm);
                    // The closure body was walked under its own symbol —
                    // do not walk it again under the enclosing one.
                    return;
                }
            }
            walk_children(ctx, node);
            return;
        }
        "for_statement" => {
            ctx.facts.loops.push(LoopSite {
                symbol: ctx.func,
                kind: LoopKind::For,
                line: ctx.line(node),
                body_start: node
                    .child_by_field_name("body")
                    .map(|b| ctx.line(b))
                    .unwrap_or(0),
                body_end: node
                    .child_by_field_name("body")
                    .map(|b| cie_core::types::line_of(ctx.bytes, b.end_byte() as u32))
                    .unwrap_or(0),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "while_statement" | "repeat_statement" => {
            ctx.facts.loops.push(LoopSite {
                symbol: ctx.func,
                kind: LoopKind::While,
                line: ctx.line(node),
                body_start: node
                    .child_by_field_name("body")
                    .map(|b| ctx.line(b))
                    .unwrap_or(0),
                body_end: node
                    .child_by_field_name("body")
                    .map(|b| cie_core::types::line_of(ctx.bytes, b.end_byte() as u32))
                    .unwrap_or(0),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "if_statement" => {
            let cond = node
                .child_by_field_name("condition")
                .map(|c| text_of(c, ctx.bytes).to_string())
                .unwrap_or_default();
            let mut vars = BTreeSet::new();
            if let Some(c) = node.child_by_field_name("condition") {
                collect_identifiers(c, ctx.bytes, &mut vars);
            }
            ctx.facts.ifs.push(cie_semantic::facts::IfSite {
                symbol: ctx.func,
                line: ctx.line(node),
                cond,
                cond_vars: vars.into_iter().collect(),
                loc: SourceLocation::span_bytes(
                    cie_core::types::FileId(0),
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                ),
            });
        }
        "return_statement" => {
            let mut expr = String::new();
            let mut constant: Option<String> = None;
            for child in children_of(node) {
                if child.kind() == "expression_list" {
                    if let Some(first) = children_of(child).first() {
                        expr = text_of(*first, ctx.bytes).trim().to_string();
                        if matches!(
                            first.kind(),
                            "string" | "number" | "true" | "false" | "nil"
                        ) {
                            constant = Some(expr.clone());
                        }
                    }
                }
            }
            ctx.facts.returns.push(cie_semantic::facts::ReturnSite {
                symbol: ctx.func,
                line: ctx.line(node),
                expr,
                constant,
            });
        }
        _ => {}
    }
    walk_children(ctx, node);
}

fn collect_identifiers(node: tree_sitter::Node, bytes: &[u8], out: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        out.insert(text_of(node, bytes).to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, bytes, out);
    }
}

fn assignment_lhs_name(decl: tree_sitter::Node, bytes: &[u8]) -> String {
    for child in children_of(decl) {
        if child.kind() == "variable_list" {
            let mut out = vec![];
            collect_variable_names(child, bytes, &mut out);
            if let Some(first) = out.into_iter().next() {
                return first;
            }
        }
    }
    String::new()
}

/// A closure bound to a name (`local h = function() end`,
/// `M.handler = function(ev) end`) is a real function: symbol it, then
/// walk ONLY the function_definition body under its own symbol.
fn emit_anonymous_fn(ctx: &mut LuaCtx, fn_node: tree_sitter::Node, name: &str) {
    if name.is_empty() {
        return;
    }
    let is_test = last_segment(name).starts_with("test");
    let id = ctx.id();
    ctx.facts.symbols.push(SymbolFact {
            annotations: Vec::new(),
        id,
        kind: if is_test { SymbolKind::Test } else { SymbolKind::Function },
        name: name.to_string(),
        qual_name: name.to_string(),
        loc: SourceLocation::span_bytes(
            cie_core::types::FileId(0),
            fn_node.start_byte() as u32,
            fn_node.end_byte() as u32,
        ),
        lang: LangId::Lua,
        parent: Some(ctx.func),
        rev: 0,
        doc: None,
    });
    let prev = ctx.func;
    ctx.func = id;
    walk_children(ctx, fn_node);
    ctx.func = prev;
}

fn emit_assignments(
    ctx: &mut LuaCtx,
    node: tree_sitter::Node,
    names: &[String],
    rhs_nodes: &[tree_sitter::Node],
) {
    for (i, var) in names.iter().enumerate() {
        if var.is_empty() {
            continue;
        }
        // Lua pairs names and values positionally; missing values are nil.
        let rhs = rhs_nodes.get(i);
        let (rhs_kind, rhs_data) = match rhs {
            Some(r) => match r.kind() {
                "function_definition" => (RhsKind::Call, vec!["<function>".into()]),
                "function_call" => (
                    RhsKind::Call,
                    vec![r
                        .child_by_field_name("name")
                        .map(|n| text_of(n, ctx.bytes).trim().to_string())
                        .unwrap_or_default()],
                ),
                "string" | "number" | "true" | "false" | "nil" | "table_constructor" => {
                    (RhsKind::Literal, vec![text_of(*r, ctx.bytes).trim().to_string()])
                }
                "identifier" => (
                    RhsKind::Vars,
                    vec![text_of(*r, ctx.bytes).trim().to_string()],
                ),
                _ => {
                    let t = text_of(*r, ctx.bytes).trim().to_string();
                    if t.contains('.') || t.contains(':') || t.contains('[') {
                        (RhsKind::Vars, vec![t])
                    } else {
                        (RhsKind::Other, vec![t])
                    }
                }
            },
            None => (RhsKind::Other, vec![]),
        };
        ctx.facts.assignments.push(cie_semantic::facts::AssignmentFact {
            symbol: ctx.func,
            var: var.clone(),
            line: ctx.line(node),
            rhs_kind,
            rhs_data,
        });
        // Taint sources on the right-hand side (io.read / os.getenv).
        if let Some(r) = rhs {
            let rt = text_of(*r, ctx.bytes);
            if rt.contains("io.read") {
                ctx.facts.sources.push(cie_semantic::facts::SourceSite {
                    symbol: ctx.func,
                    var: var.clone(),
                    line: ctx.line(node),
                    kind: SourceKind::Stdin,
                });
            } else if rt.contains("os.getenv") {
                ctx.facts.sources.push(cie_semantic::facts::SourceSite {
                    symbol: ctx.func,
                    var: var.clone(),
                    line: ctx.line(node),
                    kind: SourceKind::EnvVar,
                });
            }
        }
    }
}

impl LuaAdapter {
    /// Deep extraction: one parse → facts (§16.1 contract).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser.set_language(&to_language()).expect("lua grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::Lua, "tree-sitter-deep:lua");
        };
        let bytes = src.as_bytes();
        let module = path.trim_end_matches(".lua").replace('/', ".");
        let module_id = SymbolId(0);
        let mut ctx = LuaCtx {
            bytes,
            facts: LangFacts::new(path, LangId::Lua, "tree-sitter-deep:lua"),
            func: module_id,
            next_id: 1,
            fn_names: BTreeSet::new(),
            notes: BTreeSet::new(),
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
            lang: LangId::Lua,
            parent: None,
            rev: 0,
            doc: None,
        });
        walk_lua(&mut ctx, tree.root_node());
        // Honesty notes — what this adapter deliberately does NOT model.
        ctx.notes.insert(
            "Lua has no classes — table + metatable OOP is read as plain functions and fields, not class hierarchies".into(),
        );
        ctx.notes.insert(
            "coroutine.create/resume/yield are cooperative threads — not modeled as spawn".into(),
        );
        ctx.notes.insert(
            "C modules and code generated at load() time are not traced".into(),
        );
        if tree.root_node().has_error() {
            ctx.notes
                .insert("syntax errors present — extraction is partial".into());
        }
        ctx.facts.unsupported.extend(ctx.notes);
        ctx.facts
            .exceptions
            .sort_by_key(|e| (e.line, e.kind_text.clone()));
        ctx.facts
    }
}

impl LanguageAdapter for LuaAdapter {
    fn id(&self) -> LangId {
        LangId::Lua
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["lua"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:lua"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16: metatables can redirect any lookup at runtime — Dynamic.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16: 5.3+ integers wrap (`Wrapping`); string↔number coercion is
    /// implicit (`ImplicitAny`); int64 + binary64 (`MixedWidths`).
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Wrapping,
            coercion: CoercionPolicy::ImplicitAny,
            precision: Precision::MixedWidths,
        }
    }
    /// §16: `error()` aborts, `pcall` catches — an unchecked raise model.
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
                name: "lua-metatable-oop",
                convention: "setmetatable(T, {__index = ...}) is class inheritance by convention",
            },
            FrameworkRule {
                name: "lua-false-msg",
                convention: "returning false, msg on failure is a convention — not checked by the language",
            },
            FrameworkRule {
                name: "lua-busted",
                convention: "describe/it/test callbacks are test entry points",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(LuaAdapter)
}

#[test]
fn task_024_lua_adapter_facts() {
    use cie_semantic::facts::BuiltinKind;
    let adapter = LuaAdapter;
    assert_eq!(adapter.id(), LangId::Lua);
    assert_eq!(adapter.extensions(), &["lua"]);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::Wrapping,
        "§16: 5.3+ integer wrap"
    );
    assert_eq!(
        adapter.numeric_semantics().coercion,
        CoercionPolicy::ImplicitAny,
        "Lua coerces strings to numbers in arithmetic"
    );
    assert_eq!(adapter.error_model(), cie_adapters::ErrorModel::Raise);

    let src = "\
local ledger = require(\"ledger\")
local M = {}

local MAX = 3

function M.authorize(user)
    if #user < MAX then
        error(\"too short\")
    end
    return ledger.check(user)
end

function M.retry(name)
    for i = 1, MAX do
        local line = io.read()
        print(name, line)
    end
    while true do
        break
    end
end

M.handler = function(ev)
    local ok, err = pcall(M.authorize, ev.user)
    if not ok then
        os.execute(\"logger \" .. err)
    end
    return ok
end

M.test_authorize = function()
    assert(M.authorize(\"ci\") == true)
end

return M
";
    let facts = adapter.extract("svc/gate.lua", src);

    // require → import fact.
    assert!(
        facts.imports.iter().any(|i| i.module == "ledger"),
        "require() is the import mechanism"
    );
    // Function + method-ish + table-assigned closures are symbols; the
    // test_* closure is a Test.
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name == "M.authorize"));
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name == "M.retry"));
    assert!(facts.symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name == "M.handler"));
    assert!(
        facts.symbols.iter().any(|s| s.kind == SymbolKind::Test && s.name == "M.test_authorize"),
        "test_* closures are tests (luaunit convention)"
    );
    // error() → raised; pcall → caught.
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "error()" && !e.in_handler));
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "pcall" && e.in_handler));
    // require/print are builtins.
    assert!(facts.calls.iter().any(|c| matches!(c.callee, Callee::Builtin(BuiltinKind::Print))));
    assert!(facts.calls.iter().any(|c| matches!(c.callee, Callee::Builtin(BuiltinKind::Other)) && c.callee_text == "require"));
    // os.execute stays a shell sink (never misrouted to SQL).
    assert!(
        facts.sinks.iter().any(|s| matches!(s.kind, SinkKind::ShellExec) && s.call_text == "os.execute"),
        "os.execute → ShellExec, not SqlExecute"
    );
    // io.read inside the for-loop is a stdin source, and print there is in_loop.
    assert!(facts.sources.iter().any(|s| matches!(s.kind, SourceKind::Stdin) && s.var == "line"));
    let for_loop = facts.loops.iter().find(|l| l.kind == LoopKind::For).expect("for loop");
    assert!(for_loop.body_end > for_loop.line);
    assert!(facts.calls.iter().any(|c| c.callee_text == "print" && c.in_loop));
    // local MAX = 3 → literal assignment.
    assert!(facts.assignments.iter().any(|a| a.var == "MAX" && a.rhs_kind == RhsKind::Literal));
    assert!(facts.assignments.iter().any(|a| a.var == "M" && a.rhs_kind == RhsKind::Literal));
    // return ok → return fact; return M → constant.
    assert!(facts.returns.iter().any(|r| r.expr == "ok"));
    // Honesty notes present (metatable OOP, coroutines, C modules).
    assert!(facts.unsupported.iter().any(|u| u.contains("metatable")));
    assert!(facts.unsupported.iter().any(|u| u.contains("coroutine")));
    // Clean syntax judgment for well-formed source.
    assert!(adapter.parse_diagnostics(src).is_empty());
    // Garbage source is judged honestly.
    assert_eq!(adapter.parse_diagnostics("function end"), vec!["syntax error".to_string()]);
}

#[test]
fn task_024_lua_numeric_and_hashbang() {
    let adapter = LuaAdapter;
    // String arithmetic coercion is real Lua semantics (5.4 manual §3.4.3).
    let facts = adapter.extract(
        "coerce.lua",
        "#!/usr/bin/lua\nlocal n = tonumber(\"10\") + 1\nreturn n\n",
    );
    assert!(facts.assignments.iter().any(|a| a.var == "n"));
    assert!(adapter.parse_diagnostics("local = 1").len() == 1);
}

