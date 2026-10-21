//! The Spring parser — the factory's first full product.
//!
//! One tree-sitter walk per file (java grammar for .java, kotlin-ng for
//! .kt), producing a small statement LANGUAGE of its own:
//!
//!   If / When(switch/when) / Throw / Return / Call / Other
//!
//! …so flows assemble later against a whole-project class registry WITHOUT
//! keeping tree-sitter nodes alive. Java and Kotlin share one Spring
//! context here — both syntaxes are read and merged into one JvmStack.
//!
//! Flows are branch-capable: if/else, else-if chains, switch/when cases,
//! throw and return statements in a handler (and, one recursion in, the
//! service/repository the handler calls) become real nodes. Conditions are
//! QUOTED, never evaluated — CIE doesn't run the code.

use crate::types::{
    ControllerFlow, FlowBranch, FlowNode, HandlerFlow, Injection, JvmConfigCls, JvmEntity, JvmRepository, JvmScheduled,
    JvmService, JvmStack, JvmListener, JvmOutbound, MainSite, Prop, RouteSite,
};
use regex::Regex;
use std::collections::BTreeMap;
use tree_sitter::{Node, Parser};

pub const MAX_FLOW_NODES: usize = 48;
pub const MAX_FLOW_DEPTH: usize = 4;

// regexes compiled once — several run inside per-class/per-method loops
static CTOR_PARAM_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"([A-Z]\w*(?:<[^>]*>)?)\s+(\w+)").unwrap());
static TABLE_NAME_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"name\s*=\s*"([^"]+)""#).unwrap());
static QUOTED_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());
static REPO_SUPERTYPE_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"(?:JpaRepository|CrudRepository|PagingAndSortingRepository|ReactiveCrudRepository|MongoRepository)\s*<\s*([^,>]+?)\s*,\s*([^>]+?)>",
    )
    .unwrap()
});
static REQUEST_METHOD_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"method\s*=\s*RequestMethod\.(\w+)").unwrap());
static BARE_PATH_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"^\s*"([^"]*)""#).unwrap());
static VALUE_PATH_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"value\s*=\s*"([^"]*)""#).unwrap());
static PATH_PATH_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"path\s*=\s*"([^"]*)""#).unwrap());


// ================= statement model =================

/// one parsed statement of a method body — the raw material of flows
#[derive(Debug, Clone)]
pub enum Stmt {
    If {
        cond: String,
        line: u32,
        if_arm: Vec<Stmt>,
        else_arm: Option<Vec<Stmt>>,
    },
    When {
        label: String, // "switch (x)" / "when (x)"
        cases: Vec<(String, Vec<Stmt>)>,
        line: u32,
    },
    Throw {
        text: String,
        line: u32,
    },
    Return {
        text: String,
        line: u32,
    },
    Call {
        recv: Option<String>, // receiver identifier before the dot
        method: String,
        line: u32,
    },
    Other {
        text: String,
        line: u32,
    },
}

#[derive(Debug, Clone)]
pub struct Annot {
    pub name: String,
    pub args: String,
    pub line: u32,
}

/// Reconstruct `@Name` / `@Name(args)` from a parsed `Annot` — the same
/// shape the source wrote, from the name/args this file already split it
/// into (see `split_ann`).
fn render_ann(a: &Annot) -> String {
    if a.args.is_empty() {
        format!("@{}", a.name)
    } else {
        format!("@{}({})", a.name, a.args)
    }
}

fn render_anns(anns: &[Annot]) -> Vec<String> {
    anns.iter().map(render_ann).collect()
}

#[derive(Debug, Clone, Default)]
pub struct ClassNode {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub lang: String,
    pub anns: Vec<Annot>,
    pub injections: Vec<Injection>,
    pub is_interface: bool,
    pub repo_extends: Option<String>,
    pub repo_entity: Option<String>,
    pub props: Vec<Prop>,
    pub methods: Vec<MethodNode>,
}

#[derive(Debug, Clone)]
pub struct MethodNode {
    pub name: String,
    pub line: u32,
    pub params: Vec<String>,
    pub returns: String,
    pub anns: Vec<Annot>,
    pub body: Vec<Stmt>,
}

// ================= per-file extraction =================

pub fn parse_file(lang: &'static str, rel: &str, src: &str) -> Vec<ClassNode> {
    let Some(mut parser) = parser_for(lang) else { return vec![] };
    let Some(tree) = parser.parse(src, None) else { return vec![] };
    let bytes = src.as_bytes();
    let mut out: Vec<ClassNode> = Vec::new();

    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "class_declaration" | "interface_declaration" | "record_declaration" | "object_declaration" | "enum_declaration"
        ) {
            if let Some(c) = class_info(node, node.kind(), lang, rel, bytes) {
                out.push(c);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    out.sort_by_key(|c| c.line);
    // dedupe: kotlin object + companion declarations can repeat a name
    out.dedup_by(|a, b| a.name == b.name && a.line == b.line);
    out
}

fn parser_for(lang: &'static str) -> Option<Parser> {
    let mut p = Parser::new();
    let ok = match lang {
        "java" => p.set_language(&tree_sitter_java::LANGUAGE.into()).is_ok(),
        "kotlin" => p.set_language(&tree_sitter_kotlin_ng::LANGUAGE.into()).is_ok(),
        _ => false,
    };
    ok.then_some(p)
}

fn text_of(node: Node, bytes: &[u8]) -> String {
    node.utf8_text(bytes).unwrap_or("").replace('\n', " ").trim().to_string()
}

fn class_info(cls: Node, kind: &str, lang: &'static str, rel: &str, bytes: &[u8]) -> Option<ClassNode> {
    let name = cls.child_by_field_name("name").map(|n| text_of(n, bytes))?;
    let anns = annotations_of(cls, bytes);
    // java interface = interface_declaration node; kotlin interface =
    // class_declaration with a bare `interface` keyword child
    let mut has_interface_kw = false;
    if lang == "kotlin" {
        let mut cursor = cls.walk();
        for c in cls.children(&mut cursor) {
            if c.kind() == "interface" {
                has_interface_kw = true;
            }
        }
    }
    let is_interface = kind == "interface_declaration" || has_interface_kw;
    let mut injections = Vec::new();
    let mut props = Vec::new();
    let mut repo_extends = None;
    let mut repo_entity = None;

    collect_injections(cls, lang, bytes, &mut injections);
    // repository supertypes — kotlin: delegation_specifiers child; java:
    // the header text (interfaces only — safe because interface bodies
    // rarely mention the type twice)
    {
        let mut cursor = cls.walk();
        let sup = cls
            .children(&mut cursor)
            .find(|c| matches!(c.kind(), "delegation_specifiers" | "superclass" | "superinterface"))
            .map(|c| text_of(c, bytes))
            .unwrap_or_default();
        if let Some(m) = REPO_SUPERTYPE_RE.captures(&sup) {
            repo_extends = Some(m.get(0).unwrap().as_str().replace('\n', " "));
            repo_entity = Some(m.get(1).unwrap().as_str().trim().to_string());
        }
    }
    if is_interface && repo_extends.is_none() {
        let full = cls.utf8_text(bytes).unwrap_or("");
        if let Some(m) = REPO_SUPERTYPE_RE.captures(full) {
            repo_extends = Some(m.get(0).unwrap().as_str().replace('\n', " "));
            repo_entity = Some(m.get(1).unwrap().as_str().trim().to_string());
        }
    }
    collect_props(cls, lang, bytes, &injections, &mut props);
    let methods = methods_of(cls, lang, bytes);

    Some(ClassNode {
        name,
        rel: rel.to_string(),
        line: cls.start_position().row as u32 + 1,
        lang: lang.to_string(),
        anns,
        injections,
        is_interface,
        repo_extends,
        repo_entity,
        props,
        methods,
    })
}

fn annotations_of(decl: Node, bytes: &[u8]) -> Vec<Annot> {
    let mut out = Vec::new();
    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        match child.kind() {
            // java: every annotation is its own direct child
            "annotation" | "marker_annotation" => push_ann(child, bytes, &mut out),
            // kotlin-ng: annotations live under `modifiers`
            "modifiers" => {
                let mut mc = child.walk();
                for m in child.children(&mut mc) {
                    if matches!(m.kind(), "annotation" | "marker_annotation") {
                        push_ann(m, bytes, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_ann(node: Node, bytes: &[u8], out: &mut Vec<Annot>) {
    let span = node.utf8_text(bytes).unwrap_or("");
    let (name, args) = split_ann(span);
    if name.is_empty() {
        return;
    }
    out.push(Annot {
        name,
        args,
        line: node.start_position().row as u32 + 1,
    });
}

/// "@RequestMapping(\"/wallet\")" → ("RequestMapping", "\"/wallet\"")
fn split_ann(span: &str) -> (String, String) {
    let s = span.trim().trim_start_matches('@');
    match s.find('(') {
        Some(i) => {
            let name = s[..i].trim().to_string();
            let args = s[i..]
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim()
                .to_string();
            (name, args)
        }
        None => {
            let name = s.trim().to_string();
            (name, String::new())
        }
    }
}

fn collect_injections(cls: Node, lang: &'static str, bytes: &[u8], out: &mut Vec<Injection>) {
    let field_re = Regex::new(r#"(?:\b(?:private|internal|public|protected|override)\s+)*\b(?:val|var)\s+(\w+)\s*:\s*([\w<>,.?\s]+?)(?:\s*=.*)?$"#).unwrap();
    // annotations that make a FIELD an injected bean (constructor params
    // are injections by construction; plain entity fields are NOT —
    // they're props)
    let field_inject_anns: &[&str] = &["Autowired", "Value", "Resource", "Inject", "Qualifier"];
    let mut stack = vec![cls];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "field_declaration" => {
                // java — ONLY annotated fields are injections; plain fields
                // (entity state) stay out of the injection list
                let anns = annotations_of(node, bytes);
                if !anns.iter().any(|a| field_inject_anns.contains(&a.name.as_str())) {
                    continue;
                }
                let mut ty = node
                    .child_by_field_name("type")
                    .map(|t| text_of(t, bytes))
                    .unwrap_or_default();
                if ty.is_empty() {
                    let mut cursor = node.walk();
                    ty = node
                        .children(&mut cursor)
                        .find(|c| matches!(c.kind(), "type_identifier" | "generic_type" | "scoped_type_identifier"))
                        .map(|c| text_of(c, bytes))
                        .unwrap_or_default();
                }
                let mut cursor = node.walk();
                for c in node.children(&mut cursor) {
                    if c.kind() == "variable_declarator" {
                        if let Some(nm) = c.child_by_field_name("name").map(|n| text_of(n, bytes)) {
                            if !nm.is_empty() && !out.iter().any(|i| i.field == nm) {
                                out.push(Injection {
                                    field: nm,
                                    r#type: ty.clone(),
                                });
                            }
                        }
                    }
                }
            }
            "property_declaration" if lang == "kotlin" => {
                // member property WITHOUT initializer (`val x: UserService`)
                let t = text_of(node, bytes);
                if !t.contains('=') {
                    if let Some(m) = field_re.captures(&t) {
                        let nm = m.get(1).unwrap().as_str();
                        if !out.iter().any(|i| i.field == nm) {
                            out.push(Injection {
                                field: nm.to_string(),
                                r#type: m.get(2).unwrap().as_str().trim().to_string(),
                            });
                        }
                    }
                }
            }
            "class_parameter" if lang == "kotlin" => {
                // primary-constructor parameters (the Kotlin injection idiom).
                // `val x: Type` WITHOUT an initializer = injection; WITH `=`
                // it's plain state (an entity field), not a bean
                let t = text_of(node, bytes);
                if t.contains('=') {
                    continue;
                }
                if let Some(m) = field_re.captures(&t) {
                    let nm = m.get(1).unwrap().as_str();
                    if !out.iter().any(|i| i.field == nm) {
                        out.push(Injection {
                            field: nm.to_string(),
                            r#type: m.get(2).unwrap().as_str().trim().to_string(),
                        });
                    }
                }
            }
            "constructor_declaration" => {
                // java constructor params: `Type name`
                if let Some(params) = node.child_by_field_name("parameters").map(|p| text_of(p, bytes)) {
                    for pm in CTOR_PARAM_RE.captures_iter(params.as_str()) {
                        if let (Some(t), Some(n)) = (pm.get(1), pm.get(2)) {
                            if !out.iter().any(|i| i.field == n.as_str()) {
                                out.push(Injection {
                                    field: n.as_str().to_string(),
                                    r#type: t.as_str().to_string(),
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn collect_props(cls: Node, lang: &'static str, bytes: &[u8], injections: &[Injection], out: &mut Vec<Prop>) {
    let field_re = Regex::new(r#"(?:\b(?:private|internal|public|protected|open|lateinit|override)\s+)*\b(?:val|var)\s+(\w+)\s*:\s*([\w<>,.?\s]+?)(?:\s*=.*)?$"#).unwrap();
    let mut stack = vec![cls];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "field_declaration" => {
                let ty = node
                    .child_by_field_name("type")
                    .map(|t| text_of(t, bytes))
                    .unwrap_or_default();
                let mut cursor = node.walk();
                for c in node.children(&mut cursor) {
                    if c.kind() == "variable_declarator" {
                        if let Some(nm) = c.child_by_field_name("name").map(|n| text_of(n, bytes)) {
                            if !out.iter().any(|p| p.name == nm) && !injections.iter().any(|i| i.field == nm) {
                                out.push(Prop {
                                    name: nm,
                                    r#type: ty.clone(),
                                });
                            }
                        }
                    }
                }
            }
            "property_declaration" if lang == "kotlin" => {
                let t = text_of(node, bytes);
                if let Some(m) = field_re.captures(&t) {
                    let nm = m.get(1).unwrap().as_str();
                    if !out.iter().any(|p| p.name == nm) && !injections.iter().any(|i| i.field == nm) {
                        out.push(Prop {
                            name: nm.to_string(),
                            r#type: m.get(2).unwrap().as_str().trim().to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
        if matches!(node.kind(), "property_declaration" | "field_declaration") {
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn methods_of(cls: Node, lang: &'static str, bytes: &[u8]) -> Vec<MethodNode> {
    let mut out = Vec::new();
    let mut stack = vec![cls];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "method_declaration" | "function_declaration") {
            let name = node.child_by_field_name("name").map(|n| text_of(n, bytes)).unwrap_or_default();
            if name.is_empty() {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    stack.push(child);
                }
                continue;
            }
            // params: java `formal_parameters`; kotlin `function_value_parameters`
            // (its `parameter` children carry the real text)
            let params: Vec<String> = {
                let mut cursor = node.walk();
                let params_node = node
                    .children(&mut cursor)
                    .find(|c| matches!(c.kind(), "formal_parameters" | "function_value_parameters"));
                match params_node {
                    None => vec![],
                    Some(p) if p.kind() == "function_value_parameters" => {
                        let mut pc = p.walk();
                        p.children(&mut pc)
                            .filter(|c| c.kind() == "parameter")
                            .map(|c| text_of(c, bytes))
                            .collect()
                    }
                    Some(p) => split_params(&text_of(p, bytes)),
                }
            };
            let mut ret = String::new();
            let mut body_node = None;
            if lang == "java" {
                if let Some(rt) = node.child_by_field_name("type") {
                    ret = text_of(rt, bytes);
                }
                body_node = node.child_by_field_name("body");
            } else {
                // kotlin: function_body OR a single-expression after ':'; the
                // return type is a `user_type`-ish child when the colon form is used
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "function_body" {
                        body_node = Some(child);
                    }
                    if matches!(child.kind(), "user_type" | "nullable_type") && Some(child) != body_node {
                        // the LAST such type-looking child wins (colon-return form)
                        ret = text_of(child, bytes);
                    }
                }
            }
            let anns = annotations_of(node, bytes);
            let body = body_node.map(|b| body_to_stmts(b, bytes)).unwrap_or_default();
            out.push(MethodNode {
                name,
                line: node.start_position().row as u32 + 1,
                params,
                returns: ret,
                anns,
                body,
            });
            continue; // don't walk inside method bodies
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // nested class declarations' methods belong to the nested class —
            // don't take them here
            stack.push(child);
        }
    }
    out
}

fn split_params(s: &str) -> Vec<String> {
    if s.trim().is_empty() || s.trim() == "()" {
        return vec![];
    }
    s.trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .map(|p| p.trim().replace(|c: char| c.is_whitespace(), " "))
        .filter(|p| !p.is_empty() && !p.starts_with(')'))
        .collect()
}

// ================= statement walking =================

fn body_to_stmts(block: Node, bytes: &[u8]) -> Vec<Stmt> {
    let mut out = Vec::new();
    walk_statements(block, bytes, &mut out);
    // drop `Other` noise that's just a lone block or parenthesis
    out.retain(|s| match s {
        Stmt::Other { text, .. } => !text.is_empty() && text != "{",
        _ => true,
    });
    out
}

fn walk_statements(container: Node, bytes: &[u8], out: &mut Vec<Stmt>) {
    let mut cursor = container.walk();
    for child in container.children(&mut cursor) {
        match child.kind() {
            // ---------- java if ----------
            "if_statement" => {
                out.push(java_if(child, bytes));
            }
            // ---------- kotlin if ----------
            "if_expression" => {
                let cond = child
                    .child_by_field_name("condition")
                    .map(|c| text_of(c, bytes))
                    .unwrap_or_default();
                let (if_arm, else_arm) = kotlin_if_arms(child, bytes);
                out.push(Stmt::If {
                    cond,
                    line: child.start_position().row as u32 + 1,
                    if_arm,
                    else_arm,
                });
            }
            // ---------- java switch (both styles) ----------
            "switch_expression" => {
                out.push(Stmt::When {
                    label: "switch".into(),
                    cases: java_switch_cases(child, bytes),
                    line: child.start_position().row as u32 + 1,
                });
            }
            "switch_statement" => {
                let disc = child
                    .child_by_field_name("condition")
                    .map(|c| text_of(c, bytes))
                    .unwrap_or_default();
                out.push(Stmt::When {
                    label: format!("switch ({disc})"),
                    cases: java_switch_cases(child, bytes),
                    line: child.start_position().row as u32 + 1,
                });
            }
            // ---------- kotlin when ----------
            "when_expression" => {
                // kotlin-ng has no named fields here — the subject is a
                // when_subject child; entries are when_entry children
                let mut subject = String::new();
                let mut cases: Vec<(String, Vec<Stmt>)> = Vec::new();
                let mut cursor = child.walk();
                for c in child.children(&mut cursor) {
                    if c.kind() == "when_subject" {
                        subject = text_of(c, bytes).trim_start_matches('(').trim_end_matches(')').trim().to_string();
                    }
                    if c.kind() == "when_entry" {
                        let entry_txt = text_of(c, bytes);
                        let label = entry_txt.split("->").next().unwrap_or("").trim().to_string();
                        // the body is the LAST child (return_expression,
                        // control_structure_body, block...)
                        let mut arm = Vec::new();
                        let mut ec = c.walk();
                        let kids: Vec<Node> = c.children(&mut ec).collect();
                        if let Some(last) = kids.last() {
                            let is_body = !matches!(last.kind(), "when_condition" | "when_conditions");
                            if is_body && last.kind() == "block" {
                                walk_statements(*last, bytes, &mut arm);
                            } else if is_body && callish(last.utf8_text(bytes).unwrap_or("")) {
                                let span = text_of(*last, bytes);
                                let (recv, method) = split_callish(&span);
                                arm.push(Stmt::Call {
                                    recv,
                                    method,
                                    line: last.start_position().row as u32 + 1,
                                });
                            } else if is_body {
                                let span = text_of(*last, bytes).trim_start_matches("return ").trim().to_string();
                                if !span.is_empty() {
                                    arm.push(Stmt::Return {
                                        text: span,
                                        line: last.start_position().row as u32 + 1,
                                    });
                                }
                            }
                        }
                        cases.push((label, arm));
                    }
                }
                out.push(Stmt::When {
                    label: format!("when ({subject})"),
                    cases,
                    line: child.start_position().row as u32 + 1,
                });
            }
            "throw_statement" | "throw_expression" => {
                let span = text_of(child, bytes);
                let inner = span.split_once("throw ").map(|x| x.1.trim().trim_end_matches(';').to_string()).unwrap_or(span);
                out.push(Stmt::Throw {
                    text: inner,
                    line: child.start_position().row as u32 + 1,
                });
            }
            "return_statement" => {
                let span = text_of(child, bytes).trim_end_matches(';').to_string();
                out.push(Stmt::Return {
                    text: span.trim_start_matches("return ").to_string(),
                    line: child.start_position().row as u32 + 1,
                });
            }
            "jump_expression" if lang_kotlin(child, bytes) => {
                // kotlin return
                let span = text_of(child, bytes);
                if span.contains("return") {
                    out.push(Stmt::Return {
                        text: span.trim_start_matches("return").trim().to_string(),
                        line: child.start_position().row as u32 + 1,
                    });
                }
            }
            "method_invocation" => {
                // java call with optional receiver
                let recv = child.child_by_field_name("object").map(|o| text_of(o, bytes));
                let name = child.child_by_field_name("name").map(|n| text_of(n, bytes)).unwrap_or_default();
                let call_txt = text_of(child, bytes);
                let recv = recv.or_else(|| {
                    // chained calls: receiver text up to the last dot
                    let head = call_txt.split('(').next().unwrap_or("");
                    if let Some(i) = head.rfind('.') {
                        let r = head[..i].trim().to_string();
                        return Some(r);
                    }
                    None
                });
                let recv = recv.filter(|r| !r.is_empty() && r.chars().all(|c| c.is_alphanumeric() || c == '_'));
                out.push(Stmt::Call {
                    recv,
                    method: name,
                    line: child.start_position().row as u32 + 1,
                });
            }
            _ => {
                // catch calls in kotlin / other statement shapes: text that
                // looks like `recv.method(...)` ends in a call
                let span = text_of(child, bytes);
                if callish(&span) {
                    let (recv, method) = split_callish(&span);
                    out.push(Stmt::Call {
                        recv,
                        method,
                        line: child.start_position().row as u32 + 1,
                    });
                } else if matches!(child.kind(), "expression_statement" | "assignment" | "local_function_declaration" | "property_declaration") {
                    // keep short assignments (response building) as Other
                    if span.len() <= 90 && !span.is_empty() {
                        out.push(Stmt::Other {
                            text: span,
                            line: child.start_position().row as u32 + 1,
                        });
                    }
                    let mut cc = child.walk();
                    for c in child.children(&mut cc) {
                        if matches!(c.kind(), "call_expression") {
                            if let Some(span2) = Some(text_of(c, bytes)) {
                                if callish(&span2) {
                                    let (recv, method) = split_callish(&span2);
                                    out.push(Stmt::Call {
                                        recv,
                                        method,
                                        line: c.start_position().row as u32 + 1,
                                    });
                                }
                            }
                        }
                    }
                } else if !matches!(child.kind(), "comment") {
                    // blocks (kotlin function_body→block), expression
                    // wrappers, loops — descend so nested ifs/when/calls ARE
                    // found; the arms above already consumed callish + Other
                    // for the statement kinds they own, so no double-counting
                    walk_statements(child, bytes, out);
                }
            }
        }
    }
}

fn lang_kotlin(node: Node, bytes: &[u8]) -> bool {
    // heuristic: kotlin return is written without `;` — a jump_expression
    // node only exists in the kotlin grammar anyway
    !node.utf8_text(bytes).unwrap_or("").ends_with(';')
}

/// `service.topUp(req)` → (Some("service"), "topUp")
fn split_callish(span: &str) -> (Option<String>, String) {
    let head = span.split('(').next().unwrap_or("").trim();
    match head.rfind('.') {
        Some(i) => {
            let recv = head[..i].trim().to_string();
            let method = head[i + 1..].trim().to_string();
            (
                (!recv.is_empty() && recv.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(recv),
                method,
            )
        }
        None => (None, head.to_string()),
    }
}

/// java if — with else-if chains rendered as a nested If in the else arm
fn java_if(node: Node, bytes: &[u8]) -> Stmt {
    // tree-sitter-java has NO else_clause wrapper — children are: `if`,
    // condition, if-arm block, `else` keyword, else-arm block (or a nested
    // if_statement for else-if chains)
    let cond = node
        .child_by_field_name("condition")
        .map(|c| text_of(c, bytes))
        .unwrap_or_default();
    let mut if_arm = Vec::new();
    let mut else_arm: Option<Vec<Stmt>> = None;
    let mut saw_else = false;
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "block" if !saw_else => {
                walk_statements(c, bytes, &mut if_arm);
            }
            "else" => saw_else = true,
            "block" if saw_else => {
                let mut arm = Vec::new();
                walk_statements(c, bytes, &mut arm);
                else_arm = Some(arm);
            }
            "if_statement" if saw_else => {
                // else-if — the nested if becomes the else arm
                else_arm = Some(vec![java_if(c, bytes)]);
            }
            _ => {}
        }
    }
    Stmt::If {
        cond,
        line: node.start_position().row as u32 + 1,
        if_arm,
        else_arm,
    }
}

/// kotlin if arms: everything after the condition — first body = if-arm,
/// second = else-arm; nested if in the else arm = else-if chain
fn kotlin_if_arms(node: Node, bytes: &[u8]) -> (Vec<Stmt>, Option<Vec<Stmt>>) {
    let mut arms: Vec<Vec<Stmt>> = Vec::new();
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        if c.kind() == "control_structure_body" || c.kind() == "block" {
            let mut arm = Vec::new();
            walk_statements(c, bytes, &mut arm);
            arms.push(arm);
        } else if c.kind() == "if_expression" && !arms.is_empty() {
            // `else if` — the whole if_expression is the else arm
            let cond = c
                .child_by_field_name("condition")
                .map(|cc| text_of(cc, bytes))
                .unwrap_or_default();
            let (arm, else_arm2) = kotlin_if_arms(c, bytes);
            arms.push(vec![Stmt::If {
                cond,
                line: c.start_position().row as u32 + 1,
                if_arm: arm,
                else_arm: else_arm2,
            }]);
        }
    }
    let if_arm = arms.first().cloned().unwrap_or_default();
    let else_arm = arms.get(1).cloned();
    (if_arm, else_arm)
}

/// java switch — both classic groups and arrow rules
fn java_switch_cases(node: Node, bytes: &[u8]) -> Vec<(String, Vec<Stmt>)> {
    let mut cases = Vec::new();
    let mut cursor = node.walk();
    for group in node.children(&mut cursor) {
        match group.kind() {
            "switch_block" => cases.extend(java_switch_cases(group, bytes)),
            "switch_block_statement_group" => {
                // label text before the first colon; statements after it
                let txt = group.utf8_text(bytes).unwrap_or("");
                let label = txt.split(':').next().unwrap_or("").trim().to_string();
                let mut arm = Vec::new();
                let mut gc = group.walk();
                let kids: Vec<Node> = group.children(&mut gc).collect();
                for c in kids.iter().skip(1) {
                    walk_statements(*c, bytes, &mut arm);
                }
                cases.push((label, arm));
            }
            "switch_rule" => {
                let txt = group.utf8_text(bytes).unwrap_or("");
                let label = txt.split("->").next().unwrap_or("").trim().to_string();
                let mut arm = Vec::new();
                let mut rc = group.walk();
                let kids: Vec<Node> = group.children(&mut rc).collect();
                if let Some(last) = kids.last() {
                    if kids.len() >= 2 {
                        if last.kind() == "block" {
                            walk_statements(*last, bytes, &mut arm);
                        } else {
                            // expression body — keep it as a statement
                            let span = text_of(*last, bytes);
                            if callish(&span) {
                                let (recv, method) = split_callish(&span);
                                arm.push(Stmt::Call {
                                    recv,
                                    method,
                                    line: last.start_position().row as u32 + 1,
                                });
                            } else {
                                arm.push(Stmt::Other {
                                    text: span,
                                    line: last.start_position().row as u32 + 1,
                                });
                            }
                        }
                    }
                }
                cases.push((label, arm));
            }
            _ => {}
        }
    }
    cases
}

/// public for the dump2 debug example: parse one block into statements
pub fn parse_body(block: tree_sitter::Node, src: &str) -> Vec<Stmt> {
    body_to_stmts(block, src.as_bytes())
}

// ================= assembly =================

pub struct Assembled {
    pub jvm: JvmStack,
    pub routes: Vec<RouteSite>,
    pub mains: Vec<MainSite>,
    pub flows: Vec<ControllerFlow>,
}

fn layer_of_class(c: &ClassNode) -> &'static str {
    let names: Vec<&str> = c.anns.iter().map(|a| a.name.as_str()).collect();
    if names.contains(&"RestController") || names.contains(&"Controller") {
        "controller"
    } else if names.contains(&"Service") {
        "service"
    } else if names.contains(&"Repository") || c.repo_extends.is_some() {
        "repository"
    } else if names.contains(&"Entity") {
        "entity"
    } else if c.is_interface && c.name.ends_with("Repository") {
        // a DDD PORT: the interface the domain speaks, infrastructure satisfyies
        // (billing's InvoiceRepository is the shape) — honest name-based read
        "repository"
    } else {
        "other"
    }
}

fn mapping_ann(name: &str) -> bool {
    matches!(name, "GetMapping" | "PostMapping" | "PutMapping" | "PatchMapping" | "DeleteMapping" | "RequestMapping")
}

fn verb_of(ann_name: &str) -> Option<&'static str> {
    match ann_name {
        "GetMapping" => Some("GET"),
        "PostMapping" => Some("POST"),
        "PutMapping" => Some("PUT"),
        "PatchMapping" => Some("PATCH"),
        "DeleteMapping" => Some("DELETE"),
        "RequestMapping" => Some("ALL"),
        _ => None,
    }
}

/// inside a @RequestMapping's args, `method = RequestMethod.GET` names the
/// verb — when present it wins over ALL
fn verb_from_method_clause(args: &str) -> Option<String> {
REQUEST_METHOD_RE.captures(args).map(|m| m.get(1).unwrap().as_str().to_uppercase())
}

fn annotation_path(args: &str) -> String {
    let args = args.trim();
    if args.is_empty() {
        return String::new();
    }
    if let Some(m) = BARE_PATH_RE.captures(args) {
        return m.get(1).unwrap().as_str().to_string();
    }
    if let Some(m) = VALUE_PATH_RE.captures(args) {
        return m.get(1).unwrap().as_str().to_string();
    }
    if let Some(m) = PATH_PATH_RE.captures(args) {
        return m.get(1).unwrap().as_str().to_string();
    }
    String::new()
}

fn paren_args(args: &str) -> String {
    if args.is_empty() {
        String::new()
    } else {
        format!("({args})")
    }
}

fn join_path(base: &str, path: &str) -> String {
    let mut base = base.trim().to_string();
    let mut path = path.trim().to_string();
    if !base.is_empty() && !base.starts_with('/') {
        base = format!("/{base}");
    }
    if !path.is_empty() && !path.starts_with('/') {
        path = format!("/{path}");
    }
    if base.is_empty() {
        return if path.is_empty() { "/".into() } else { path };
    }
    if path.is_empty() || path == "/" {
        return base;
    }
    if base.ends_with('/') {
        format!("{}{}", base, path.trim_start_matches('/'))
    } else {
        format!("{base}{path}")
    }
}

/// assemble the whole JVM stack from per-file classes, cross-referencing
/// the class registry (which types are services/repos/entities)
pub fn assemble(all: Vec<ClassNode>, dir: &str) -> Assembled {
    let mut layers: BTreeMap<String, &'static str> = BTreeMap::new();
    for c in &all {
        layers.insert(c.name.clone(), layer_of_class(c));
    }
    let entity_of_repo: BTreeMap<String, String> = all
        .iter()
        .filter(|c| c.repo_entity.is_some())
        .map(|c| {
            let ent = c.repo_entity.clone().unwrap_or_default();
            let bare = ent.split('.').next_back().unwrap_or(&ent).to_string();
            (c.name.clone(), bare)
        })
        .collect();

    let mut jvm = JvmStack {
        config_classes: vec![],
        services: vec![],
        repositories: vec![],
        entities: vec![],
        scheduled_in: vec![],
        listeners: vec![],
        outbound: vec![],
        scanned: 0,
        java_files: 0,
        kotlin_files: 0,
    };
    let mut routes: Vec<RouteSite> = Vec::new();
    let mut mains: Vec<MainSite> = Vec::new();
    let mut all_flows: Vec<ControllerFlow> = Vec::new();

    let registry: BTreeMap<String, ClassNode> = all
        .iter()
        .map(|c| (c.name.clone(), c.clone()))
        .collect();

    for c in &all {
        jvm.java_files += u32::from(c.lang == "java");
        jvm.kotlin_files += u32::from(c.lang == "kotlin");
        let names: Vec<&str> = c.anns.iter().map(|a| a.name.as_str()).collect();

        if names.contains(&"SpringBootApplication") || c.methods.iter().any(|m| m.name == "main") {
            mains.push(MainSite {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                kind: "spring-boot".into(),
                lang: c.lang.clone(),
            });
        }

        let is_rest = names.contains(&"RestController");
        let is_web = !is_rest && names.contains(&"Controller");
        if is_rest || is_web {
            let base_ann = c.anns.iter().find(|a| a.name == "RequestMapping");
            let base_path = base_ann.map(|a| annotation_path(&a.args)).unwrap_or_default();
            let mut flows: Vec<HandlerFlow> = Vec::new();
            for m in c.methods.iter().filter(|m| m.anns.iter().any(|a| mapping_ann(&a.name))) {
                let verb = {
                    let ann = m.anns.iter().find(|a| mapping_ann(&a.name));
                    match ann {
                        Some(a) => {
                            let v = verb_of(&a.name).unwrap_or("ALL");
                            if v == "ALL" {
                                verb_from_method_clause(&a.args).unwrap_or_else(|| "ALL".to_string())
                            } else {
                                v.to_string()
                            }
                        }
                        None => "ALL".to_string(),
                    }
                };
                let end_ann = m.anns.iter().find(|a| mapping_ann(&a.name) && a.name != "RequestMapping");
                let raw_path = end_ann.map(|a| annotation_path(&a.args)).unwrap_or_default();
                let full = join_path(&base_path, &raw_path);
                let evidence = end_ann
                    .map(|a| format!("@{}{}", a.name, paren_args(&a.args)))
                    .unwrap_or_else(|| {
                        base_ann
                            .map(|a| format!("@RequestMapping{}", paren_args(&a.args)))
                            .unwrap_or_default()
                    });
                routes.push(RouteSite {
                    verb: verb.clone(),
                    path: full.clone(),
                    owner: c.name.clone(),
                    method: m.name.clone(),
                    service: dir.to_string(),
                    rel: c.rel.clone(),
                    line: m.line,
                    params: m.params.clone(),
                    returns: m.returns.clone(),
                    evidence,
                });

                let mut budget = 0usize;
                let mut truncated = false;
                let mut counter = 0usize;
                let body_nodes = stmts_to_flow(&m.body, c, &registry, &layers, &entity_of_repo, 0, &mut budget, &mut truncated, &mut counter);
                flows.push(HandlerFlow {
                    method: m.name.clone(),
                    verb: verb.clone(),
                    path: full.clone(),
                    root: FlowNode {
                        id: "root".into(),
                        kind: "request".into(),
                        label: format!("{verb} {full}"),
                        layer: "client".into(),
                        r#type: None,
                        rel: None,
                        line: Some(m.line),
                        children: vec![FlowBranch {
                            label: "handled by".into(),
                            nodes: vec![FlowNode {
                                id: "c0".into(),
                                kind: "action".into(),
                                label: format!("{}.{}(…)", c.name, m.name),
                                layer: "controller".into(),
                                r#type: None,
                                rel: Some(c.rel.clone()),
                                line: Some(m.line),
                                children: vec![FlowBranch {
                                    label: "body".into(),
                                    nodes: body_nodes,
                                }],
                            }],
                        }],
                    },
                    honest: if truncated {
                        format!(
                            "deeper calls exist beyond the {MAX_FLOW_NODES}-node / {MAX_FLOW_DEPTH}-depth cap — every branch is the method's own if/else/switch as written; conditions quoted, never evaluated (CIE doesn't run the code)."
                        )
                    } else {
                        "every branch is the method's own if/else/switch as written; conditions quoted, never evaluated (CIE doesn't run the code).".into()
                    },
                });
            }
            if !flows.is_empty() {
                all_flows.push(ControllerFlow {
                    name: if dir.is_empty() { c.name.clone() } else { format!("{dir}/{}", c.name) },
                    rel: c.rel.clone(),
                    base_path: if base_path.is_empty() { "/".into() } else { base_path },
                    flows,
                });
            }
            continue;
        }

        if names.contains(&"Service") {
            jvm.services.push(JvmService {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                injections: c.injections.clone(),
                annotations: render_anns(&c.anns),
            });
        } else if names.contains(&"Repository")
            || c.repo_extends.is_some()
            || (c.is_interface && c.name.ends_with("Repository"))
        {
            jvm.repositories.push(JvmRepository {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                extends: c.repo_extends.clone().unwrap_or_default(),
                entity: c.repo_entity.clone(),
                annotations: render_anns(&c.anns),
            });
        } else if names.contains(&"Entity") {
            jvm.entities.push(JvmEntity {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                table: c
                    .anns
                    .iter()
                    .find(|a| a.name == "Table")
                    .and_then(|a| {
TABLE_NAME_RE.captures(&a.args)
                            .and_then(|m| m.get(1).map(|g| g.as_str().to_string()))
                    }),
                props: c.props.clone(),
                annotations: render_anns(&c.anns),
            });
        } else if names.contains(&"Configuration") || names.contains(&"SpringBootConfiguration") {
            jvm.config_classes.push(JvmConfigCls {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                kind: "configuration".into(),
                annotations: render_anns(&c.anns),
            });
        } else if names.contains(&"EnableWebSecurity") {
            jvm.config_classes.push(JvmConfigCls {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                kind: "security".into(),
                annotations: render_anns(&c.anns),
            });
        } else if names.contains(&"FeignClient") {
            jvm.config_classes.push(JvmConfigCls {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                kind: "feign".into(),
                annotations: render_anns(&c.anns),
            });
        } else if names.iter().any(|n| matches!(*n, "EnableFeignClients" | "EnableKafka" | "EnableScheduling" | "EnableJpaRepositories")) {
            jvm.config_classes.push(JvmConfigCls {
                name: c.name.clone(),
                rel: c.rel.clone(),
                line: c.line,
                lang: c.lang.clone(),
                kind: "messaging".into(),
                annotations: render_anns(&c.anns),
            });
        }

        for m in &c.methods {
            if let Some(s) = m.anns.iter().find(|a| a.name == "Scheduled") {
                jvm.scheduled_in.push(JvmScheduled {
                    cls: c.name.clone(),
                    method: m.name.clone(),
                    trigger: s.args.clone(),
                    rel: c.rel.clone(),
                    line: m.line,
                    annotations: render_anns(&m.anns),
                });
            }
            for a in &m.anns {
                if matches!(
                    a.name.as_str(),
                    "KafkaListener" | "RabbitListener" | "JmsListener" | "StreamListener" | "EventListener"
                ) {
                    jvm.listeners.push(JvmListener {
                        cls: c.name.clone(),
                        method: m.name.clone(),
                        target: {
QUOTED_RE.captures(&a.args)
                                .and_then(|x| x.get(1).map(|g| g.as_str().to_string()))
                                .unwrap_or_default()
                        },
                        rel: c.rel.clone(),
                        line: m.line,
                    });
                }
            }
        }

        if let Some(ann) = c.anns.iter().find(|a| a.name == "FeignClient") {
            jvm.outbound.push(JvmOutbound {
                kind: "feign".into(),
                line: ann.line,
                target: feign_target(&ann.args).map(|(_,v)|v),
                target_kind: feign_target(&ann.args).map(|(k,_)|k).unwrap_or_default(),
                requests: c.methods.iter().flat_map(|m|m.anns.iter()).filter_map(|a| {
                    let verb = match a.name.as_str() { "GetMapping"=>"GET", "PostMapping"=>"POST", "PutMapping"=>"PUT", "DeleteMapping"=>"DELETE", "PatchMapping"=>"PATCH", _=>return None };
                    let base=PATH_PATH_RE.captures(&ann.args).and_then(|c|c.get(1)).map(|m|m.as_str()).unwrap_or("");
                    Some(crate::types::OutboundRequest{verb:verb.into(),path:join_path(base,&annotation_path(&a.args))})
                }).collect(),
                cls: c.name.clone(),
                rel: c.rel.clone(),
            });
        }
        if c.injections.iter().any(|i| i.r#type.contains("RestTemplate")) {
            jvm.outbound.push(JvmOutbound {
                kind: "rest-template".into(),
                line: c.line, target: None, target_kind: String::new(), requests: vec![],
                cls: c.name.clone(),
                rel: c.rel.clone(),
            });
        } else if c.injections.iter().any(|i| i.r#type.contains("WebClient")) {
            jvm.outbound.push(JvmOutbound {
                kind: "web-client".into(),
                line: c.line, target: None, target_kind: String::new(), requests: vec![],
                cls: c.name.clone(),
                rel: c.rel.clone(),
            });
        } else if c.injections.iter().any(|i| i.r#type.contains("KafkaTemplate")) {
            jvm.outbound.push(JvmOutbound {
                kind: "kafka-producer".into(),
                line: c.line, target: None, target_kind: String::new(), requests: vec![],
                cls: c.name.clone(),
                rel: c.rel.clone(),
            });
        }
    }

    jvm.scanned = jvm.java_files + jvm.kotlin_files;
    Assembled { jvm, routes, mains, flows: all_flows }
}

// ============ the flow builder (Stmt → FlowNode) ============

#[allow(clippy::too_many_arguments)]
fn stmts_to_flow(
    stmts: &[Stmt],
    owner: &ClassNode,
    registry: &BTreeMap<String, ClassNode>,
    layers: &BTreeMap<String, &'static str>,
    entity_of_repo: &BTreeMap<String, String>,
    depth: usize,
    budget: &mut usize,
    truncated: &mut bool,
    counter: &mut usize,
) -> Vec<FlowNode> {
    let fields: BTreeMap<String, String> = owner
        .injections
        .iter()
        .map(|i| (i.field.clone(), i.r#type.clone()))
        .collect();
    let mut out = Vec::new();
    for s in stmts {
        if *budget >= MAX_FLOW_NODES || depth > MAX_FLOW_DEPTH {
            *truncated = true;
            break;
        }
        match s {
            Stmt::If { cond, line, if_arm, else_arm } => {
                let if_nodes = stmts_to_flow(if_arm, owner, registry, layers, entity_of_repo, depth + 1, budget, truncated, counter);
                let mut children = vec![FlowBranch {
                    label: format!("if ({cond})"),
                    nodes: if_nodes,
                }];
                if let Some(else_stmts) = else_arm {
                    let else_nodes = stmts_to_flow(else_stmts, owner, registry, layers, entity_of_repo, depth + 1, budget, truncated, counter);
                    children.push(FlowBranch {
                        label: "else".into(),
                        nodes: else_nodes,
                    });
                }
                *budget += 1;
                *counter += 1;
                out.push(FlowNode {
                    id: format!("if-{counter}"),
                    kind: "decision".into(),
                    label: format!("if ({cond})"),
                    layer: "decision".into(),
                    r#type: None,
                    rel: None,
                    line: Some(*line),
                    children,
                });
            }
            Stmt::When { label, cases, line } => {
                let children: Vec<FlowBranch> = if cases.is_empty() {
                    vec![FlowBranch {
                        label: "(cases not resolved)".into(),
                        nodes: vec![],
                    }]
                } else {
                    cases
                        .iter()
                        .map(|(case_label, arm)| FlowBranch {
                            label: case_label.clone(),
                            nodes: stmts_to_flow(arm, owner, registry, layers, entity_of_repo, depth + 1, budget, truncated, counter),
                        })
                        .collect()
                };
                *budget += 1;
                *counter += 1;
                out.push(FlowNode {
                    id: format!("sw-{counter}"),
                    kind: "decision".into(),
                    label: label.clone(),
                    layer: "decision".into(),
                    r#type: None,
                    rel: None,
                    line: Some(*line),
                    children,
                });
            }
            Stmt::Throw { text, line } => {
                *budget += 1;
                *counter += 1;
                out.push(FlowNode {
                    id: format!("th-{counter}"),
                    kind: "throw".into(),
                    label: format!("throws {text}"),
                    layer: "response".into(),
                    r#type: None,
                    rel: None,
                    line: Some(*line),
                    children: vec![],
                });
            }
            Stmt::Return { text, line } => {
                *budget += 1;
                *counter += 1;
                out.push(FlowNode {
                    id: format!("re-{counter}"),
                    kind: "return".into(),
                    label: format!("responds {text}"),
                    layer: "response".into(),
                    r#type: None,
                    rel: None,
                    line: Some(*line),
                    children: vec![],
                });
            }
            Stmt::Call { recv, method, line } => {
                let recv_txt = recv.clone().unwrap_or_default();
                let ty = fields.get(&recv_txt).cloned();
                let layer_from_ty = ty.as_ref().and_then(|t| {
                    let bare = t.split('<').next().unwrap_or(t).split('.').next_back().unwrap_or(t);
                    layers.get(bare).copied()
                });
                // resolved callee class → recurse into the called method
                let mut callee_nodes: Vec<FlowNode> = Vec::new();
                let mut callee_rel: Option<String> = None;
                let mut callee_line: Option<u32> = None;
                if depth < MAX_FLOW_DEPTH {
                    if let Some(ty) = &ty {
                        let bare = ty.split('<').next().unwrap_or(ty).split('.').next_back().unwrap_or(ty).to_string();
                        let layer = layers.get(&bare).copied();
                        if layer == Some("service") || layer == Some("repository") {
                            if let Some(callee_cls) = registry.get(&bare) {
                                if let Some(callee_m) = callee_cls.methods.iter().find(|mm| mm.name == method.as_str()) {
                                    callee_rel = Some(callee_cls.rel.clone());
                                    callee_line = Some(callee_m.line);
                                    callee_nodes = stmts_to_flow(&callee_m.body, callee_cls, registry, layers, entity_of_repo, depth + 1, budget, truncated, counter);
                                }
                                // repository calls name the database — even
                                // INHERITED methods (save/findX on
                                // JpaRepository) that aren't declared here
                                if layer == Some("repository") {
                                    let ent = entity_of_repo.get(&bare).cloned().unwrap_or_default();
                                    let op = if method.starts_with("save") || method.starts_with("delete") || method.starts_with("update") || method.starts_with("deleteAll") || method.starts_with("saveAll") {
                                        "write"
                                    } else {
                                        "read"
                                    };
                                    callee_nodes.push(FlowNode {
                                        id: format!("db-{counter}"),
                                        kind: "action".into(),
                                        label: format!(
                                            "database {} via {}{}",
                                            op,
                                            bare,
                                            if ent.is_empty() { String::new() } else { format!(" (entity {ent})") }
                                        ),
                                        layer: "database".into(),
                                        r#type: if ent.is_empty() { None } else { Some(ent) },
                                        rel: callee_cls.rel.clone().into(),
                                        line: callee_line.or(Some(callee_cls.line)),
                                        children: vec![],
                                    });
                                    *budget += 1;
                                }
                            }
                        }
                    }
                }
                *budget += 1;
                *counter += 1;
                let shown = if recv_txt.is_empty() {
                    format!("…{method}(…)") // chained or bare call
                } else {
                    format!("{recv_txt}.{method}(…)")
                };
                out.push(FlowNode {
                    id: format!("c-{counter}"),
                    kind: "action".into(),
                    label: shown,
                    layer: match layer_from_ty {
                        Some("service") => "service".to_string(),
                        Some("repository") => "repository".to_string(),
                        Some("controller") => "controller".into(),
                        _ => "other".into(),
                    },
                    r#type: ty.clone(),
                    rel: callee_rel.clone(),
                    line: callee_line.or(Some(*line)),
                    children: if callee_nodes.is_empty() {
                        vec![]
                    } else {
                        vec![FlowBranch {
                            label: format!("into {}", ty.clone().unwrap_or_default()),
                            nodes: callee_nodes,
                        }]
                    },
                });
            }
            Stmt::Other { text, line } => {
                *budget += 1;
                *counter += 1;
                out.push(FlowNode {
                    id: format!("o-{counter}"),
                    kind: "action".into(),
                    label: text.clone(),
                    layer: "other".into(),
                    r#type: None,
                    rel: None,
                    line: Some(*line),
                    children: vec![],
                });
            }
        }
    }
    out
}

/// `x.a(...)` / `a(...)` — display-ish detection. NOTE the parenthesis
/// discipline: every clause after `first.is_alphabetic() || first == '_'`
/// belongs together (operator precedence bites otherwise).
fn callish(span: &str) -> bool {
    if span.len() < 3 || span.len() > 140 {
        return false;
    }
    let head = span.split('(').next().unwrap_or("").trim();
    if head.is_empty() {
        return false;
    }
    let first = head.chars().next().unwrap_or(' ');
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    span.contains('(')
        && span.contains(')')
        && !span.contains(" = ")
        && !span.starts_with("return")
        && !span.contains("//")
        && !span.contains("new ")
}


/// Parse only Feign routing attributes; contextId/qualifiers are not destinations.
fn feign_target(args: &str) -> Option<(String,String)> {
    for key in ["url", "name", "value"] {
        let re=Regex::new(&format!(r#"(?:^|,)\s*{}\s*=\s*"([^"]+)"\s*(?:,|$)"#,key)).ok()?;
        if let Some(c)=re.captures(args) { return Some((if key=="url"{"url"}else{"name"}.into(),c[1].into())); }
    }
    BARE_PATH_RE.captures(args).map(|c|("name".into(),c[1].into()))
}

#[cfg(test)]
mod feign_destination_tests {
    use super::*;
    #[test]
    fn attributes_are_named_and_explicit_url_takes_precedence() {
        assert_eq!(feign_target(r#"contextId = "not-a-service", name = "billing""#),Some(("name".into(),"billing".into())));
        assert_eq!(feign_target(r#"name = "billing", url = "https://other/invoices""#),Some(("url".into(),"https://other/invoices".into())));
        assert_eq!(feign_target(r#"contextId = "not-a-service""#),None);
        assert_eq!(feign_target(r#""billing""#),Some(("name".into(),"billing".into())));
    }
}
