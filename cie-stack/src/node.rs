//! The express/fastify parser — Node + TypeScript routes and handler flows.
//!
//! Reads the handler files as written (tree-sitter over the file text, zero
//! evaluation): `app.get("/path", handler)` calls are routes; named functions
//! and const-arrow functions are handler bodies; bodies are walked like the
//! spring parser walks java — decisions carry their arms as written,
//! throws/returns are nodes, storage-shaped calls (obj.save/query/update…)
//! become database nodes, and one call hop resolves into a sibling module's
//! function. Everything cites rel:line; caps are shared (flow.rs).

use crate::factory::{self, Sources};
use crate::flow::{self, FlowNode};
use crate::types::{ControllerFlow, HandlerFlow, MainSite, RouteSite, StackReport};
use std::collections::BTreeMap;

const VERBS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("delete", "DELETE"),
    ("patch", "PATCH"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
    ("all", "ALL"),
];

fn js_ts_lang(rel: &str) -> Option<tree_sitter::Language> {
    if rel.ends_with(".ts") {
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
    } else if rel.ends_with(".js") || rel.ends_with(".mjs") {
        Some(tree_sitter_javascript::LANGUAGE.into())
    } else {
        None
    }
}

fn module_owner(rel: &str) -> String {
    let base = rel.rsplit('/').next_back().unwrap_or(rel);
    base.trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".mjs")
        .trim_end_matches(".js")
        .to_string()
}

pub struct NodeParser {
    /// app dir inside the repo ("" when the manifest sits at the root)
    pub dir: String,
    /// "express" | "fastify" — named by the package.json probe
    pub framework: String,
}

/// a named callable in some file (function decl or const-arrow)
#[derive(Clone, Debug)]
struct FnDef {
    rel: String,
    body_start: usize,
    body_end: usize,
    line: u32,
}

/// the parsed trees of one app dir + the named-function registry
struct Trees {
    trees: BTreeMap<String, tree_sitter::Tree>,
    fns: BTreeMap<String, FnDef>,
}

impl Trees {
    fn build(files: &Sources) -> Trees {
        let mut trees = BTreeMap::new();
        let mut fns: BTreeMap<String, FnDef> = BTreeMap::new();
        for (rel, src) in files {
            let Some(lang) = js_ts_lang(rel) else { continue };
            let mut p = tree_sitter::Parser::new();
            if p.set_language(&lang).is_err() {
                continue;
            }
            let Some(tree) = p.parse(src, None) else { continue };
            for n in flow::descendants(tree.root_node()) {
                match n.kind() {
                    "function_declaration" => {
                        let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src.as_bytes()).ok());
                        let body = n.child_by_field_name("body");
                        if let (Some(name), Some(body)) = (name, body) {
                            fns.entry(name.to_string()).or_insert(FnDef {
                                rel: rel.clone(),
                                body_start: body.start_byte(),
                                body_end: body.end_byte(),
                                line: n.start_position().row as u32 + 1,
                            });
                        }
                    }
                    "variable_declarator" => {
                        let Some(val) = n.child_by_field_name("value") else { continue };
                        if val.kind() == "arrow_function" {
                            let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src.as_bytes()).ok());
                            let body = val.child_by_field_name("body");
                            if let (Some(name), Some(body)) = (name, body) {
                                fns.entry(name.to_string()).or_insert(FnDef {
                                    rel: rel.clone(),
                                    body_start: body.start_byte(),
                                    body_end: body.end_byte(),
                                    line: n.start_position().row as u32 + 1,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            trees.insert(rel.clone(), tree);
        }
        Trees { trees, fns }
    }
}

/// import { a, b } from "./mod" / import d from "./mod" → name → module path
fn import_maps(files: &Sources) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (rel, src) in files {
        let Some(lang) = js_ts_lang(rel) else { continue };
        let mut p = tree_sitter::Parser::new();
        if p.set_language(&lang).is_err() {
            continue;
        }
        let Some(tree) = p.parse(src, None) else { continue };
        let mut im: BTreeMap<String, String> = BTreeMap::new();
        for n in flow::descendants(tree.root_node()) {
            if n.kind() == "import_statement" {
                let text = n.utf8_text(src.as_bytes()).unwrap_or("");
                let module = text.split(|q| q == '"' || q == '\'').nth(1).unwrap_or("").to_string();
                if let Some(brace) = text.find('{') {
                    if let Some(close) = text[brace..].find('}') {
                        for piece in text[brace + 1..brace + close].split(',') {
                            let piece = piece.trim().to_string();
                            let name = piece.split(" as ").last().map(|x| x.trim().to_string()).unwrap_or(piece);
                            if !name.is_empty() {
                                im.insert(name, module.clone());
                            }
                        }
                    }
                } else {
                    let name = text.trim_start_matches("import").trim_start().split_whitespace().next().unwrap_or("").to_string();
                    if !name.is_empty() {
                        im.insert(name, module);
                    }
                }
            }
        }
        out.insert(rel.clone(), im);
    }
    out
}

impl NodeParser {
    fn parse_dir(&self, files: &Sources) -> (Vec<RouteSite>, Vec<MainSite>, Vec<ControllerFlow>, Vec<String>) {
        let mut routes: Vec<RouteSite> = vec![];
        let mut mains: Vec<MainSite> = vec![];
        let mut flows: Vec<ControllerFlow> = vec![];
        let mut notes: Vec<String> = vec![];
        let trees = Trees::build(files);
        let imports = import_maps(files);

        for (rel, tree) in &trees.trees {
            let src = files.get(rel).unwrap();
            let file_base = rel.rsplit('/').next_back().unwrap_or(rel);
            let owner = module_owner(rel);

            // ---- routes: app.get("/path", handler) call expressions ----
            let mut file_routes: Vec<(String, String, tree_sitter::Node, u32)> = vec![];
            for n in flow::descendants(tree.root_node()) {
                if n.kind() != "call_expression" {
                    continue;
                }
                let Some(fnn) = n.child_by_field_name("function") else { continue };
                if fnn.kind() != "member_expression" {
                    continue;
                }
                let method = fnn
                    .child_by_field_name("property")
                    .and_then(|p| p.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("");
                let Some((_name, verb)) = VERBS.iter().find(|(name, _)| *name == method) else { continue };
                let Some(args) = n.child_by_field_name("arguments") else { continue };
                let strings: Vec<tree_sitter::Node> = flow::children_of(args)
                    .into_iter()
                    .filter(|k| k.kind() == "string")
                    .collect();
                if strings.is_empty() {
                    continue; // literal path only — computed paths stay honest/unread
                }
                let path = strings[0]
                    .utf8_text(src.as_bytes())
                    .unwrap_or("")
                    .trim_matches(|q| q == '"' || q == '\'' || q == '`')
                    .to_string();
                // handler = the LAST DIRECT fn-shaped child of the arguments
                // (arguments nodes carry their own parens/commas as children —
                // direct children are `( string , arrow )` — never the arrow's
                // innards, which pre-order descendants would mispick)
                let mut handler: Option<tree_sitter::Node> = None;
                for k in flow::children_of(args) {
                    let is_fn = k.kind() == "arrow_function" || k.kind() == "function_expression" || k.kind() == "identifier";
                    if is_fn && k.start_byte() > strings[strings.len() - 1].end_byte() {
                        handler = Some(k);
                    }
                }
                if std::env::var("CIE_STACK_DEBUG").is_ok() {
                    eprintln!("route? {verb} {path} handler={:?}", handler.as_ref().map(|h| h.utf8_text(src.as_bytes()).unwrap_or("").chars().take(30).collect::<String>()));
                }
                if let Some(h) = handler {
                    file_routes.push((verb.to_string(), path, h, n.start_position().row as u32 + 1));
                }
            }

            for (verb, path, handler, line) in file_routes {
                let inline = matches!(handler.kind(), "arrow_function" | "function_expression");
                let hname = if inline {
                    "handler".to_string()
                } else {
                    handler
                        .utf8_text(src.as_bytes())
                        .unwrap_or("")
                        .rsplit('.')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                };
                routes.push(RouteSite {
                    verb: verb.clone(),
                    path: path.clone(),
                    owner: owner.clone(),
                    method: hname.clone(),
                    service: self.dir.clone(),
                    rel: rel.to_string(),
                    line,
                    params: path_params(&path),
                    returns: "the handler's response".into(),
                    evidence: format!("{file_base}:{line} {verb} \"{path}\""),
                });

                // the handler body: inline node, named def in-file, or via imports
                let mut budget = 0usize;
                let mut truncated = false;
                let mut counter = 0usize;
                let mut body_nodes: Vec<FlowNode> = vec![];
                if inline {
                    if let Some(body_node) = handler.child_by_field_name("body") {
                        self.walk_stmts(
                            files, &trees, &imports, rel, src, body_node, 1, &mut budget, &mut truncated, &mut counter, &mut body_nodes,
                        );
                    }
                } else if let Some(def) = trees.fns.get(&hname) {
                    if let Some(tree2) = trees.trees.get(&def.rel) {
                        if let Some(src2) = files.get(&def.rel) {
                            if let Some(body_node) = node_at_byte(tree2.root_node(), def.body_start, def.body_end) {
                                self.walk_stmts(
                                    files, &trees, &imports, &def.rel, src2, body_node, 1, &mut budget, &mut truncated,
                                    &mut counter, &mut body_nodes,
                                );
                            }
                        }
                    }
                }

                let root_node = FlowNode {
                    id: "root".into(),
                    kind: "request".into(),
                    label: format!("{verb} {path}"),
                    layer: "client".into(),
                    r#type: None,
                    rel: None,
                    line: Some(line),
                    children: vec![flow::branch(
                        "handled by".into(),
                        vec![flow::node(
                            "c0".into(),
                            "action",
                            format!("{owner}.{hname}(…)"),
                            "controller",
                            Some(rel.to_string()),
                            Some(line),
                            vec![flow::branch("body".into(), body_nodes)],
                        )],
                    )],
                };
                flow::merge_flow(&mut flows, rel, &owner, &self.dir, HandlerFlow {
                    method: hname.clone(),
                    verb,
                    path,
                    root: root_node,
                    honest: flow::honest_line(truncated),
                });
            }

            // ---- the main: app.listen(...) ----
            for n in flow::descendants(tree.root_node()) {
                if n.kind() != "call_expression" {
                    continue;
                }
                let Some(fnn) = n.child_by_field_name("function") else { continue };
                if fnn.kind() != "member_expression" {
                    continue;
                }
                let m = fnn.child_by_field_name("property").and_then(|p| p.utf8_text(src.as_bytes()).ok()).unwrap_or("");
                if m == "listen" {
                    mains.push(MainSite {
                        name: owner.clone(),
                        rel: rel.to_string(),
                        line: n.start_position().row as u32 + 1,
                        kind: format!("{}-listen", self.framework),
                        lang: "javascript".into(),
                    });
                    break;
                }
            }
        }
        (routes, mains, flows, notes)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_stmts(
        &self,
        files: &Sources,
        trees: &Trees,
        imports: &BTreeMap<String, BTreeMap<String, String>>,
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
        match node.kind() {
            "if_statement" => {
                let cond = node
                    .child_by_field_name("condition")
                    .and_then(|c| c.utf8_text(src.as_bytes()).ok())
                    .map(|t| flow::text_of(t, 60))
                    .unwrap_or_else(|| "?".into());
                let mut arms = vec![];
                if let Some(cons) = node.child_by_field_name("consequence") {
                    let mut nodes = vec![];
                    self.walk_stmts(files, trees, imports, rel, src, cons, depth + 1, budget, truncated, counter, &mut nodes);
                    arms.push(flow::branch(format!("if ({cond})"), nodes));
                }
                if let Some(alt) = node.child_by_field_name("alternative") {
                    let mut nodes = vec![];
                    self.walk_stmts(files, trees, imports, rel, src, alt, depth + 1, budget, truncated, counter, &mut nodes);
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
                    format!("if ({cond})"),
                    "decision",
                    Some(rel.to_string()),
                    Some(node.start_position().row as u32 + 1),
                    arms,
                ));
            }
            "else_clause" | "statement_block" => {
                for k in flow::children_of(node) {
                    self.walk_stmts(files, trees, imports, rel, src, k, depth, budget, truncated, counter, out);
                }
            }
            "return_statement" | "throw_statement" => {
                let kind = if node.kind() == "return_statement" { "return" } else { "throw" };
                let text = node.utf8_text(src.as_bytes()).unwrap_or("");
                *budget += 1;
                if *budget > flow::MAX_FLOW_NODES {
                    *truncated = true;
                    return;
                }
                *counter += 1;
                out.push(flow::node(
                    format!("n{counter}"),
                    kind,
                    flow::text_of(text, 80),
                    "response",
                    Some(rel.to_string()),
                    Some(node.start_position().row as u32 + 1),
                    vec![],
                ));
            }
            "expression_statement" | "variable_declaration" | "lexical_declaration" => {
                let text = node.utf8_text(src.as_bytes()).unwrap_or("");
                if !text.contains('(') {
                    return; // plain declarations — the walk follows CALLS
                }
                let Some(call) = outer_call(node) else { return };
                let callee = call
                    .child_by_field_name("function")
                    .and_then(|f| f.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                // res.json(...) / res.status(...).json(...) = writing the response.
                // When the response carries a call (res.json(bookByCode(...))),
                // the inner call resolves FIRST — the feature service hop.
                let inner_callee: Option<String> = flow::descendants(call)
                    .iter()
                    .filter(|k| k.kind() == "call_expression" && k.id() != call.id())
                    .filter_map(|k| k.child_by_field_name("function"))
                    .filter_map(|f| f.utf8_text(src.as_bytes()).ok().map(|t| t.trim().to_string()))
                    .find(|t| !t.is_empty() && !t.contains('.'));
                if let Some(bare) = &inner_callee {
                    if depth <= flow::MAX_FLOW_DEPTH {
                        if let Some(def) = self.hop(files, trees, imports, rel, bare) {
                            let mut nodes = vec![];
                            if let Some(tree2) = trees.trees.get(&def.rel) {
                                if let Some(src2) = files.get(&def.rel) {
                                    if let Some(body_node) = node_at_byte(tree2.root_node(), def.body_start, def.body_end) {
                                        self.walk_stmts(files, trees, imports, &def.rel, src2, body_node, depth + 1, budget, truncated, counter, &mut nodes);
                                    }
                                }
                            }
                            if !nodes.is_empty() {
                                *counter += 1;
                                out.push(flow::node(
                                    format!("n{counter}"),
                                    "action",
                                    format!("{bare}()"),
                                    "service",
                                    Some(def.rel.clone()),
                                    Some(def.line),
                                    vec![flow::branch("body".into(), nodes)],
                                ));
                            }
                        }
                    }
                }
                if callee.starts_with("res.json") || callee.starts_with("res.status") {
                    *budget += 1;
                    if *budget > flow::MAX_FLOW_NODES {
                        *truncated = true;
                        return;
                    }
                    *counter += 1;
                    out.push(flow::node(
                        format!("n{counter}"),
                        "return",
                        flow::text_of(&format!("responds {}", flow::text_of(text, 60)), 90),
                        "response",
                        Some(rel.to_string()),
                        Some(node.start_position().row as u32 + 1),
                        vec![],
                    ));
                    return;
                }
                if flow::callee_is_db(&callee) {
                    *budget += 1;
                    if *budget > flow::MAX_FLOW_NODES {
                        *truncated = true;
                        return;
                    }
                    *counter += 1;
                    out.push(flow::node(
                        format!("n{counter}"),
                        "action",
                        flow::text_of(&callee, 60),
                        "database",
                        Some(rel.to_string()),
                        Some(node.start_position().row as u32 + 1),
                        vec![],
                    ));
                    return;
                }
                // one hop into a plain-function callee (service-shaped)
                let bare = callee.rsplit('(').next_back().unwrap_or("").trim().to_string();
                if !callee.contains('.') && !bare.is_empty() && depth <= flow::MAX_FLOW_DEPTH {
                    if let Some(def) = self.hop(files, trees, imports, rel, &bare) {
                        let mut nodes = vec![];
                        if let Some(tree2) = trees.trees.get(&def.rel) {
                            if let Some(src2) = files.get(&def.rel) {
                                if let Some(body_node) = node_at_byte(tree2.root_node(), def.body_start, def.body_end) {
                                    self.walk_stmts(
                                        files, trees, imports, &def.rel, src2, body_node, depth + 1, budget, truncated,
                                        counter, &mut nodes,
                                    );
                                }
                            }
                        }
                        if !nodes.is_empty() {
                            *counter += 1;
                            out.push(flow::node(
                                format!("n{counter}"),
                                "action",
                                format!("{bare}()"),
                                "service",
                                Some(def.rel.clone()),
                                Some(def.line),
                                vec![flow::branch("body".into(), nodes)],
                            ));
                        }
                    }
                }
            }
            "comment" | "import_statement" => {}
            _ => {
                for k in flow::children_of(node) {
                    self.walk_stmts(files, trees, imports, rel, src, k, depth, budget, truncated, counter, out);
                }
            }
        }
    }

    /// where does this bare callee name live? (imports first, then registry)
    fn hop(
        &self,
        _files: &Sources,
        trees: &Trees,
        imports: &BTreeMap<String, BTreeMap<String, String>>,
        rel: &str,
        name: &str,
    ) -> Option<FnDef> {
        if let Some(def) = trees.fns.get(name) {
            if def.rel == rel {
                return Some(def.clone());
            }
        }
        if let Some(im) = imports.get(rel) {
            if let Some(module) = im.get(name) {
                let dir = rel.rfind('/').map(|i| &rel[..i]).unwrap_or("");
                let wanted = if let Some(stripped) = module.strip_prefix("./") {
                    if dir.is_empty() {
                        stripped.to_string()
                    } else {
                        format!("{dir}/{stripped}")
                    }
                } else if dir.is_empty() {
                    module.clone()
                } else {
                    format!("{dir}/{module}")
                };
                let base_name = name_file_module(&wanted);
                for (other, tree) in &trees.trees {
                    if file_module_of(other) == base_name {
                        trees.fns.get(name).filter(|d| d.rel == *other)?;
                        return Some(d2(other, name, &trees.fns));
                    }
                }
            }
        }
        // same-name fallback in the registry
        trees.fns.get(name).filter(|d| d.rel != rel).cloned()
    }
}

fn d2(other: &str, name: &str, fns: &BTreeMap<String, FnDef>) -> FnDef {
    fns.get(name).cloned().unwrap_or(FnDef {
        rel: other.to_string(),
        body_start: 0,
        body_end: 0,
        line: 0,
    })
}

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

/// the FIRST call_expression walking down from a statement (the outermost)
fn outer_call<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut probe = node;
    loop {
        if probe.kind() == "call_expression" {
            return Some(probe);
        }
        let mut found: Option<tree_sitter::Node> = None;
        for i in 0..probe.child_count() {
            if let Some(k) = probe.child(i) {
                if k.kind() == "call_expression" {
                    found = Some(k);
                    break;
                }
            }
        }
        match found {
            Some(n) => probe = n,
            None => return None,
        }
    }
}

fn file_module_of(rel: &str) -> String {
    rel.trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".mjs")
        .trim_end_matches(".js")
        .to_string()
}

fn name_file_module(wanted: &str) -> String {
    wanted.to_string()
}

/// `/berths/search` → []; `/webhook/<event>` → event; `:code` → code
pub fn path_params(path: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let b: Vec<char> = path.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == ':' {
            let mut name = String::new();
            let mut j = i + 1;
            while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
                name.push(b[j]);
                j += 1;
            }
            if !name.is_empty() {
                out.push(name.clone());
            }
            i = j;
        } else if c == '<' {
            let mut name = String::new();
            let mut j = i + 1;
            while j < b.len() && b[j] != '>' {
                name.push(b[j]);
                j += 1;
            }
            if !name.is_empty() {
                out.push(name);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

impl factory::StackParser for NodeParser {
    fn id(&self) -> &'static str {
        "node-express"
    }
    fn describes(&self) -> String {
        format!("{} over node (javascript/typescript read as one family)", self.framework)
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        let scoped = factory::scope(files, &self.dir);
        let (routes, mains, flows, notes) = self.parse_dir(&scoped);
        report.routes.extend(routes);
        report.mains.extend(mains);
        report.flows.extend(flows);
        report
            .notes
            .push("[node] handler flows follow the handler's module plus one hop into sibling-module functions; deeper hops are capped honestly.".into());
        for n in notes {
            report.notes.push(n);
        }
    }
}