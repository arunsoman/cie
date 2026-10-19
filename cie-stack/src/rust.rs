//! The axum parser — Rust routes and handler flows.
//!
//! Reads the handler files as written (tree-sitter, zero evaluation):
//! - axum: `Router::new().route("/path", get(handler).post(h2))` chains.
//! - actix/rocket: `#[get("/path")]` attributes directly above the handler.
//! - Handler bodies walked like every other parser: `if`/`match` decisions
//!   with arms as written, `return`/`panic!` nodes, storage-shaped calls
//!   (save/find/query/pool/store::/repo::) as database nodes — and plain-fn
//!   hops within the crate, depth-capped. `?` propagation is quoted in the
//!   honest line, never drawn as a fake arrow.

use crate::factory::{self, Sources};
use crate::flow::{self, FlowNode};
use crate::types::{ControllerFlow, HandlerFlow, MainSite, RouteSite, StackReport};
use std::collections::BTreeMap;

pub struct RustParser {
    /// app dir inside the repo ("" when the manifest sits at the root)
    pub dir: String,
    /// "axum" | "actix-web" | "rocket" — named by the Cargo.toml probe
    pub framework: String,
}

fn rust_lang() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

/// a rust fn: name, body byte range in its file, decl line, declared return
#[derive(Clone, Debug)]
struct FnDef {
    rel: String,
    body_start: usize,
    body_end: usize,
    line: u32,
    returns: String,
}

struct Trees {
    trees: BTreeMap<String, tree_sitter::Tree>,
    fns: BTreeMap<String, FnDef>,
}

impl Trees {
    fn build(files: &Sources) -> Trees {
        let mut trees = BTreeMap::new();
        let mut fns: BTreeMap<String, FnDef> = BTreeMap::new();
        for (rel, src) in files {
            if !rel.ends_with(".rs") {
                continue;
            }
            let mut p = tree_sitter::Parser::new();
            if p.set_language(&rust_lang()).is_err() {
                continue;
            }
            let Some(tree) = p.parse(src, None) else { continue };
            for n in flow::descendants(tree.root_node()) {
                if n.kind() == "function_item" {
                    let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src.as_bytes()).ok());
                    let body = n.child_by_field_name("body");
                    if let (Some(name), Some(body)) = (name, body) {
                        let returns = n
                            .child_by_field_name("return_type")
                            .and_then(|t| t.utf8_text(src.as_bytes()).ok())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        fns.entry(name.to_string()).or_insert(FnDef {
                            rel: rel.clone(),
                            body_start: body.start_byte(),
                            body_end: body.end_byte(),
                            line: n.start_position().row as u32 + 1,
                            returns,
                        });
                    }
                }
            }
            trees.insert(rel.clone(), tree);
        }
        Trees { trees, fns }
    }
}

/// node whose utf8 byte range matches exactly (walking down)
fn node_at_byte(mut node: tree_sitter::Node, start: usize, end: usize) -> Option<tree_sitter::Node> {
    loop {
        if node.start_byte() == start && node.end_byte() == end {
            return Some(node);
        }
        if !(node.start_byte() <= start && node.end_byte() >= end && end > start) {
            return None;
        }
        let mut found: Option<tree_sitter::Node> = None;
        for i in 0..node.child_count() {
            if let Some(k) = node.child(i) {
                if k.start_byte() <= start && k.end_byte() >= end {
                    found = Some(k);
                    break;
                }
            }
        }
        if let Some(k) = found {
            node = k;
            continue;
        }
        return None;
    }
}

impl RustParser {
    fn parse_dir(&self, files: &Sources) -> (Vec<RouteSite>, Vec<MainSite>, Vec<ControllerFlow>, Vec<String>) {
        let mut routes: Vec<RouteSite> = vec![];
        let mut mains: Vec<MainSite> = vec![];
        let mut flows: Vec<ControllerFlow> = vec![];
        let mut notes: Vec<String> = vec![];
        let trees = Trees::build(files);

        for (rel, tree) in &trees.trees {
            let src = files.get(rel).unwrap();
            let file_base = rel.rsplit('/').next_back().unwrap_or(rel).trim_end_matches(".rs").to_string();

            // ---- actix/rocket: #[get("/alerts")] right above the handler fn ----
            for n in flow::descendants(tree.root_node()) {
                if n.kind() != "attribute_item" {
                    continue;
                }
                let text = n.utf8_text(src.as_bytes()).unwrap_or("");
                let inner = text.trim_start_matches('#').trim().trim_start_matches('[').trim_end_matches(']');
                if let Some((verb, path)) = attr_route(inner) {
                    // the fn after this attribute (skipping more attributes)
                    let mut sib = n;
                    let mut hname: Option<String> = None;
                    loop {
                        match sib.next_sibling() {
                            Some(next) if next.kind() == "attribute_item" => sib = next,
                            Some(next) if next.kind() == "function_item" => {
                                hname = next
                                    .child_by_field_name("name")
                                    .and_then(|x| x.utf8_text(src.as_bytes()).ok())
                                    .map(|s| s.to_string());
                                break;
                            }
                            _ => break,
                        }
                    }
                    if let Some(hname) = hname {
                        routes.push(RouteSite {
                            verb,
                            path: path.clone(),
                            owner: file_base.clone(),
                            method: hname,
                            service: self.dir.clone(),
                            rel: rel.to_string(),
                            line: n.start_position().row as u32 + 1,
                            params: crate::node::path_params(&path),
                            returns: "the handler's response".into(),
                            evidence: format!("{}:{} {}", file_base, n.start_position().row as u32 + 1, text.trim()),
                        });
                    }
                }
            }

            // ---- axum: `.route("/path", get(h))` chains ----
            for n in flow::descendants(tree.root_node()) {
                if n.kind() != "call_expression" {
                    continue;
                }
                let Some(fnn) = n.child_by_field_name("function") else { continue };
                if fnn.kind() != "field_expression" {
                    continue;
                }
                let field = fnn.child_by_field_name("field").and_then(|x| x.utf8_text(src.as_bytes()).ok()).unwrap_or("");
                if field != "route" {
                    continue;
                }
                let Some(args) = n.child_by_field_name("arguments") else { continue };
                // the path = the first string argument
                let Some(path_node) = flow::children_of(args)
                    .into_iter()
                    .find(|k| k.kind() == "string_literal")
                else {
                    continue;
                };
                let path = path_node
                    .utf8_text(src.as_bytes())
                    .unwrap_or("")
                    .trim_matches(|q| q == '"' || q == '\'')
                    .to_string();
                // the method router `get(h).post(h2)` = the first call_expression child
                let Some(method_call) = flow::children_of(args)
                    .into_iter()
                    .find(|k| k.kind() == "call_expression")
                else {
                    continue;
                };
                let line = fnn
                    .child_by_field_name("field")
                    .map(|f| f.start_position().row as u32 + 1)
                    .unwrap_or_else(|| n.start_position().row as u32 + 1);
                for (verb, handler) in method_handlers(&method_call, src) {
                    routes.push(RouteSite {
                        verb: verb.clone(),
                        path: path.clone(),
                        owner: file_base.clone(),
                        method: handler,
                        service: self.dir.clone(),
                        rel: rel.to_string(),
                        line,
                        params: crate::node::path_params(&path),
                        returns: "the handler's response".into(),
                        evidence: format!("{file_base}:{line} .route(\"{path}\", {lower}(…))", lower = verb.to_lowercase()),
                    });
                }
            }

            // ---- the main: fn main + axum::serve / bind in its body ----
            if let Some(def) = trees.fns.get("main") {
                if def.rel == *rel {
                    if let Some(src2) = files.get(&def.rel) {
                        if def.body_end <= src2.len() {
                            let body = &src2[def.body_start..def.body_end];
                            if body.contains("axum::serve") || body.contains("TcpListener") || body.contains("HttpServer") {
                                mains.push(MainSite {
                                    name: file_base.clone(),
                                    rel: rel.to_string(),
                                    line: def.line,
                                    kind: format!("{}-serve", self.framework),
                                    lang: "rust".into(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // ---- flows: per route whose handler exists (draw-capped at 8) ----
        let mut drawn = 0usize;
        let mut route_sorted = routes.clone();
        route_sorted.sort_by_key(|r| r.line);
        for r in &route_sorted {
            let Some(def) = trees.fns.get(&r.method) else {
                notes.push(format!(
                    "the {} handler wasn't defined in the scanned files — its body flow is honestly not drawn.",
                    r.method
                ));
                continue;
            };
            if drawn >= 8 {
                notes.push("route-flow drawing is capped at 8 per service — the rest are parsed for the table, not drawn.".into());
                break;
            }
            let tree2 = trees.trees.get(&def.rel);
            if let (Some(tree2), Some(src2)) = (tree2, files.get(&def.rel)) {
                if let Some(body_node) = node_at_byte(tree2.root_node(), def.body_start, def.body_end) {
                    drawn += 1;
                    let mut budget = 0usize;
                    let mut truncated = false;
                    let mut counter = 0usize;
                    let mut body_nodes: Vec<FlowNode> = vec![];
                    walk_rust_body(files, &trees, &def.rel, src2, body_node, 1, &mut budget, &mut truncated, &mut counter, &mut body_nodes);

                    let root_node = FlowNode {
                        id: "root".into(),
                        kind: "request".into(),
                        label: format!("{} {}", r.verb, r.path),
                        layer: "client".into(),
                        r#type: None,
                        rel: None,
                        line: Some(r.line),
                        children: vec![flow::branch(
                            "handled by".into(),
                            vec![flow::node(
                                "c0".into(),
                                "action",
                                format!("{}.{}(…)", r.owner, r.method),
                                "controller",
                                Some(def.rel.clone()),
                                Some(def.line),
                                vec![flow::branch("body".into(), body_nodes)],
                            )],
                        )],
                    };
                    flow::merge_flow(
                        &mut flows,
                        &r.rel,
                        &r.owner,
                        &self.dir,
                        HandlerFlow {
                            method: r.method.clone(),
                            verb: r.verb.clone(),
                            path: r.path.clone(),
                            root: root_node,
                            honest: {
                                let mut h = flow::honest_line(truncated);
                                if def.returns.contains("Result") {
                                    h.push_str(" The handler returns a Result — the ? that propagates the error is noted here, not drawn.");
                                }
                                h
                            },
                        },
                    );
                }
            }
        }
        (routes, mains, flows, notes)
    }
}

/// `#[get("/alerts")]` → GET /alerts
fn attr_route(inner: &str) -> Option<(String, String)> {
    const ATTR_VERBS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];
    let inner = inner.trim();
    for v in ATTR_VERBS {
        if inner.starts_with(&format!("{v}(")) {
            let rest = inner.trim_start_matches(&format!("{v}("));
            if let Some(close) = rest.rfind(')') {
                let path = rest[..close].trim_matches(|c| c == '"' || c == '\'');
                if !path.is_empty() && path.starts_with('/') {
                    return Some((v.to_uppercase(), path.to_string()));
                }
            }
        }
    }
    None
}

/// inside `get(handler).post(h2)`: every (verb, handler-name), tree-read
fn method_handlers(expr: &tree_sitter::Node, src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    const METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options", "trace"];
    for n in flow::descendants(*expr) {
        if n.kind() != "call_expression" {
            continue;
        }
        let Some(fnn) = n.child_by_field_name("function") else { continue };
        let name = fnn.utf8_text(src.as_bytes()).unwrap_or("");
        let base = name.rsplit("::").last().unwrap_or(name);
        if METHODS.contains(&base) {
            if let Some(args) = n.child_by_field_name("arguments") {
                // the fn expression is the LAST identifier in the args —
                // `get(alert::current_alert)` → "current_alert"
                let handler = flow::descendants(args)
                    .into_iter()
                    .filter(|k| k.kind() == "identifier")
                    .last()
                    .and_then(|k| k.utf8_text(src.as_bytes()).ok().map(|s| s.trim().to_string()));
                if let Some(handler) = handler {
                    out.push((base.to_uppercase(), handler));
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn walk_rust_body(
    files: &Sources,
    trees: &Trees,
    rel: &str,
    src: &str,
    node: tree_sitter::Node,
    depth: usize,
    budget: &mut usize,
    truncated: &mut bool,
    counter: &mut usize,
    out: &mut Vec<FlowNode>,
) {
    if depth > flow::MAX_FLOW_DEPTH {
        *truncated = true;
        return;
    }
    if std::env::var("CIE_STACK_DEBUG_2").is_ok() {
        eprintln!("walk kind={} depth={} text={:?}", node.kind(), depth, node.utf8_text(src.as_bytes()).unwrap_or("").chars().take(40).collect::<String>());
    }
    match node.kind() {
        "block" => {
            for k in flow::children_of(node) {
                // tree-sitter-rust wraps a block-tail if/match inside an expression_statement
                if k.kind() == "expression_statement" {
                    let inner = flow::children_of(k)
                        .into_iter()
                        .find(|c| c.kind() == "if_expression" || c.kind() == "match_expression");
                    if let Some(inner) = inner {
                        walk_rust_body(files, trees, rel, src, inner, depth, budget, truncated, counter, out);
                        continue;
                    }
                }
                walk_rust_body(files, trees, rel, src, k, depth, budget, truncated, counter, out);
            }
        }
        "if_expression" => {
            if std::env::var("CIE_STACK_DEBUG_2").is_ok() {
                let kids: Vec<String> = flow::children_of(node).iter().map(|k| k.kind().to_string()).collect();
                eprintln!("IF expr kids: {:?} text={:?}", kids, node.utf8_text(src.as_bytes()).unwrap_or("").chars().take(60).collect::<String>());
            }
            // tree-sitter-rust if_expression: 'if' condition? block else-clause —
            // the condition is NOT always a named field: grab the first
            // expression child after the 'if' token
            let mut condition = "?".to_string();
            {
                let kids = flow::children_of(node);
                let mut started = false;
                for k in &kids {
                    if k.kind() == "if" {
                        started = true;
                        continue;
                    }
                    if started && k.kind() != "block" {
                        condition = k.utf8_text(src.as_bytes()).ok().map(|t| flow::text_of(t, 60)).unwrap_or(condition.clone());
                        break;
                    }
                }
            }
            let mut arms = vec![];
            let blocks: Vec<tree_sitter::Node> = flow::children_of(node).into_iter().filter(|k| k.kind() == "block").collect();
            let mut blocks_it = blocks.iter();
            if let Some(cons) = node.child_by_field_name("consequence") {
                let mut nodes = vec![];
                walk_rust_body(files, trees, rel, src, cons, depth + 1, budget, truncated, counter, &mut nodes);
                arms.push(flow::branch(format!("if ({condition})"), nodes));
            } else if let Some(first) = blocks_it.next() {
                let mut nodes = vec![];
                walk_rust_body(files, trees, rel, src, *first, depth + 1, budget, truncated, counter, &mut nodes);
                arms.push(flow::branch(format!("if ({condition})"), nodes));
            }
            if let Some(alt) = node.child_by_field_name("alternative") {
                let mut nodes = vec![];
                walk_rust_body(files, trees, rel, src, alt, depth + 1, budget, truncated, counter, &mut nodes);
                arms.push(flow::branch("else".into(), nodes));
            } else if blocks.len() == 2 {
                let mut nodes = vec![];
                walk_rust_body(files, trees, rel, src, blocks[1], depth + 1, budget, truncated, counter, &mut nodes);
                arms.push(flow::branch("else".into(), nodes));
            }
            *budget += 1;
            if *budget > flow::MAX_FLOW_NODES {
                *truncated = true;
                return;
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "decision",
                format!("if ({condition})"),
                "decision",
                Some(rel.to_string()),
                Some(node.start_position().row as u32 + 1),
                arms,
            ));
        }
        "return_expression" => {
            // return <expr|macro|call> — walk the carried expression; the return
            // wording comes from the inner macro/call arm
            for k in flow::children_of(node) {
                if k.kind() == "return" || k.is_extra() {
                    continue;
                }
                walk_rust_body(files, trees, rel, src, k, depth + 1, budget, truncated, counter, out);
            }
        }
        "macro_invocation" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            let head = text.split('!').next().unwrap_or("").trim().to_string();
            let kind = match head.as_str() {
                "panic" | "unreachable" | "todo" | "unimplemented" => ("throw", "response"),
                "println" | "format" | "warn" | "eprintln" | "info" | "debug" => ("return", "response"),
                _ => return, // build macros and the like are not behaviour
            };
            *budget += 1;
            if *budget > flow::MAX_FLOW_NODES {
                *truncated = true;
                return;
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                kind.0,
                flow::text_of(&if kind.0 == "throw" { format!("raises {text}") } else { format!("responds {text}") }, 80),
                kind.1,
                Some(rel.to_string()),
                Some(node.start_position().row as u32 + 1),
                vec![],
            ));
        }
        "match_expression" => {
            let subject = node
                .child_by_field_name("value")
                .and_then(|v| v.utf8_text(src.as_bytes()).ok())
                .map(|t| flow::text_of(t, 50))
                .unwrap_or_default();
            // arms: match_body { match_arm { pattern, value } }
            let mut arms: Vec<flow::FlowBranch> = vec![];
            let body = flow::children_of(node).into_iter().find(|k| k.kind() == "match_body");
            if let Some(body) = body {
                for arm in flow::children_of(body) {
                    if arm.kind() != "match_arm" {
                        continue;
                    }
                    let label = arm
                        .child_by_field_name("pattern")
                        .and_then(|p| p.utf8_text(src.as_bytes()).ok())
                        .map(|t| flow::text_of(t, 50))
                        .unwrap_or_else(|| "arm".into());
                    let mut nodes = vec![];
                    if let Some(v) = arm.child_by_field_name("value") {
                        walk_rust_body(files, trees, rel, src, v, depth + 1, budget, truncated, counter, &mut nodes);
                    }
                    arms.push(flow::branch(label, nodes));
                }
            }
            if arms.is_empty() {
                return;
            }
            *budget += 1;
            if *budget > flow::MAX_FLOW_NODES {
                *truncated = true;
                return;
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "decision",
                if subject.is_empty() { "match".into() } else { format!("match {subject}") },
                "decision",
                Some(rel.to_string()),
                Some(node.start_position().row as u32 + 1),
                arms,
            ));
        }
        "return_expression" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            *budget += 1;
            if *budget > flow::MAX_FLOW_NODES {
                *truncated = true;
                return;
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "return",
                flow::text_of(text, 80),
                "response",
                Some(rel.to_string()),
                Some(node.start_position().row as u32 + 1),
                vec![],
            ));
        }
        "panic_expression" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            *budget += 1;
            if *budget > flow::MAX_FLOW_NODES {
                *truncated = true;
                return;
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "throw",
                flow::text_of(text, 80),
                "response",
                Some(rel.to_string()),
                Some(node.start_position().row as u32 + 1),
                vec![],
            ));
        }
        "expression_statement" | "let_declaration" | "call_expression" | "parenthesized_expression" => {
            let text = node.utf8_text(src.as_bytes()).unwrap_or("");
            if !text.contains('(') {
                return;
            }
            let Some(call) = rust_call_text(text) else { return };
            if flow::callee_is_db(&call) {
                *budget += 1;
                if *budget > flow::MAX_FLOW_NODES {
                    *truncated = true;
                    return;
                }
                *counter += 1;
                out.push(flow::node(
                    format!("n{counter}"),
                    "action",
                    flow::text_of(&call, 60),
                    "database",
                    Some(rel.to_string()),
                    Some(node.start_position().row as u32 + 1),
                    vec![],
                ));
                return;
            }
            let fn_name = callee_for_call(&call);
            if fn_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) && trees.fns.get(&fn_name).is_none() {
                // a constructor call — the response VALUE being built (Json(…), Health {…})
                *counter += 1;
                let rendered = flow::text_of(
                    text.trim_start_matches("let ").trim_start_matches("mut ").trim(),
                    60,
                );
                out.push(flow::node(
                    format!("n{counter}"),
                    "return",
                    format!("responds {rendered}"),
                    "response",
                    Some(rel.to_string()),
                    Some(node.start_position().row as u32 + 1),
                    vec![],
                ));
                return;
            }
            if let Some(def) = trees.fns.get(&fn_name) {
                if depth <= flow::MAX_FLOW_DEPTH {
                    if let (Some(tree2), Some(src2)) = (trees.trees.get(&def.rel), files.get(&def.rel)) {
                        if let Some(body_node) = node_at_byte(tree2.root_node(), def.body_start, def.body_end) {
                            let mut nodes: Vec<FlowNode> = vec![];
                            walk_rust_body(files, trees, &def.rel, src2, body_node, depth + 1, budget, truncated, counter, &mut nodes);
                            if !nodes.is_empty() {
                                *counter += 1;
                                out.push(flow::node(
                                    format!("n{counter}"),
                                    "action",
                                    format!("{fn_name}()"),
                                    "service",
                                    Some(def.rel.clone()),
                                    Some(def.line),
                                    vec![flow::branch("body".into(), nodes)],
                                ));
                            }
                        }
                    }
                }
            }
        }
        "comment" | "line_comment" | "block_comment" | "attribute_item" => {}
        _ => {
            for k in flow::children_of(node) {
                walk_rust_body(files, trees, rel, src, k, depth, budget, truncated, counter, out);
            }
        }
    }
}

/// "store::save(&pool, &reading)" → save ; "level_for(&berth)" → level_for
fn callee_for_call(call: &str) -> String {
    let head = call.split('(').next().unwrap_or("");
    head.rsplit("::").last().unwrap_or(head).trim().to_string()
}

/// the callee expression of a statement's first call, as written
fn rust_call_text(text: &str) -> Option<String> {
    let after = match text.find(" = ") {
        Some(i) => &text[i + 3..],
        None => text,
    };
    let after = after.trim();
    let head_end = after.find('(')?;
    let head = after[..head_end].trim();
    if head.is_empty() {
        return None;
    }
    if !head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.') {
        return None;
    }
    Some(head.to_string())
}

impl factory::StackParser for RustParser {
    fn id(&self) -> &'static str {
        "rust-axum"
    }
    fn describes(&self) -> String {
        format!("{} over rust (axum/actix routes + handler flows, one hop across the crate's modules)", self.framework)
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        let scoped = factory::scope(files, &self.dir);
        let (routes, mains, flows, notes) = self.parse_dir(&scoped);
        report.routes.extend(routes);
        report.mains.extend(mains);
        report.flows.extend(flows);
        report
            .notes
            .push("[rust] handler flows follow the handler's module plus plain-fn hops inside the crate; `?` propagation is quoted, not drawn; deeper hops capped honestly.".into());
        for n in notes {
            report.notes.push(n);
        }
    }
}