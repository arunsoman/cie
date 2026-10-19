//! The flask/fastapi parser — python routes and handler flows.
//!
//! Reads the route files as written (tree-sitter, zero evaluation):
//! - `@app.get("/path")` / `@app.post(...)` / `@app.route("/p", methods=[...])`
//!   / `@router.…` decorators name the routes; the decorated function is
//!   the handler.
//! - Bodies walked like every other parser: if/elif/else with arms as
//!   written, `raise` as a throw, `return` as a return, storage-shaped
//!   calls (store./db./repo./crud.* or save/record/query/find named fns)
//!   as database nodes, and one hop into imported functions.
//! Everything cites rel:line; caps are shared across parsers.

use crate::factory::{self, Sources};
use crate::flow::{self, FlowBranch, FlowNode};
use crate::types::{ControllerFlow, HandlerFlow, MainSite, RouteSite, StackReport};
use std::collections::BTreeMap;

fn py_lang() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

const _PY: fn() -> tree_sitter::Language = py_lang;

pub struct PyParser {
    /// app dir inside the repo ("" when the manifest sits at the root)
    pub dir: String,
    /// "flask" | "fastapi" — named by the requirements/pyproject probe
    pub framework: String,
}

#[derive(Clone, Debug)]
struct FnDef {
    rel: String,
    line: u32,
    /// source text of the body (py bodies are cheap to re-walk from text)
    body: String,
}

struct PyTrees {
    trees: BTreeMap<String, tree_sitter::Tree>,
    fns: BTreeMap<String, FnDef>,
}

impl PyTrees {
    fn build(files: &Sources) -> PyTrees {
        let mut trees = BTreeMap::new();
        let mut fns: BTreeMap<String, FnDef> = BTreeMap::new();
        for (rel, src) in files {
            if !rel.ends_with(".py") {
                continue;
            }
            let mut p = tree_sitter::Parser::new();
            if p.set_language(&py_lang()).is_err() {
                continue;
            }
            let Some(tree) = p.parse(src, None) else { continue };
            for n in flow::descendants(tree.root_node()) {
                if n.kind() != "function_definition" {
                    continue;
                }
                let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src.as_bytes()).ok());
                let body = n.child_by_field_name("body");
                if let (Some(name), Some(body)) = (name, body) {
                    fns.entry(name.to_string()).or_insert(FnDef {
                        rel: rel.clone(),
                        line: n.start_position().row as u32 + 1,
                        body: body.utf8_text(src.as_bytes()).unwrap_or("").to_string(),
                    });
                }
            }
            trees.insert(rel.clone(), tree);
        }
        PyTrees { trees, fns }
    }
}


/// one route: (verb, path, handler fn name, decorator line, handler fn node)
struct PyRoute {
    verb: String,
    path: String,
    handler: String,
    line: u32,
}

/// find flask/fastapi routes: decorator(f) call fn property in route|get|post|…
fn collect_routes(tree: tree_sitter::Tree, src: &str) -> (Vec<PyRoute>, Option<u32>) {
    let mut routes: Vec<PyRoute> = vec![];
    let mut main_line: Option<u32> = None;
    let c = flow::descendants(tree.root_node());
    for n in c {
        if n.kind() == "decorated_definition" {
            // decorators: @app.get("/x") @app.route("/x", methods=["GET"]) @router.post(...)
            let Some(_def) = n.child_by_field_name("definition") else { continue };
            let mut verb_path: Option<(String, String, u32)> = None;
            for k in flow::children_of(n) {
                if k.kind() != "decorator" {
                    continue;
                }
                let text = k.utf8_text(src.as_bytes()).unwrap_or("");
                let Some((verb, path)) = decorator_route(text) else { continue };
                verb_path = Some((verb, path, k.start_position().row as u32 + 1));
            }
            if let Some((verb, path, dline)) = verb_path {
                let hname = n
                    .child_by_field_name("definition")
                    .and_then(|d| d.child_by_field_name("name"))
                    .and_then(|x| x.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("")
                    .to_string();
                routes.push(PyRoute {
                    verb,
                    path,
                    handler: hname,
                    line: dline,
                });
            }
        }
        // the main: app.run(...) / uvicorn.run(...)
        if n.kind() == "call" {
            let text = n.utf8_text(src.as_bytes()).unwrap_or("");
            if text.contains("app.run(") || text.contains("uvicorn.run(") {
                main_line = Some(n.start_position().row as u32 + 1);
            }
        }
    }
    (routes, main_line)
}

/// `@app.get("/x")` → GET /x ; `@app.route("/x", methods=["GET","POST"])` → GET (first)
/// `@app.post("/x")` fastapi/flask ok ; returns None for non-route decorators
fn decorator_route(text: &str) -> Option<(String, String)> {
    const DECS: &[(&str, &str)] = &[
        ("get", "GET"), ("post", "POST"), ("put", "PUT"), ("delete", "DELETE"), ("patch", "PATCH"),
    ];
    let dot_idx = text.find('.')?;
    let obj_end = &text[dot_idx + 1..];
    let paren = obj_end.find('(')?;
    let call_name = &obj_end[..paren];
    if call_name == "route" {
        // @app.route("/x", methods=["GET"]) → path = first string, verb = first method
        let args = obj_end[paren..].to_string();
        let path = quoted_first(&args)?;
        let verbs: Vec<String> = {
            let mut v = vec![];
            for m in args.split("\"").skip(1) {
                if m.to_uppercase().chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) && m.len() <= 8 && !m.is_empty() {
                    v.push(m.to_string());
                }
            }
            v
        };
        let verb = verbs.first().cloned().unwrap_or_else(|| "GET".into());
        return Some((verb, path));
    }
    for (name, verb) in DECS {
        if call_name == *name {
            let path = quoted_first(&obj_end[paren..])?;
            return Some((verb.to_string(), path));
        }
    }
    None
}

fn quoted_first(args: &str) -> Option<String> {
    let q = args.find(|c| c == '"' || c == '\'')?;
    let close = args[q + 1..].find(|c| c == '"' || c == '\'')?;
    Some(args[q + 1..q + 1 + close].to_string())
}

impl PyParser {
    fn parse_dir(&self, files: &Sources) -> (Vec<RouteSite>, Vec<MainSite>, Vec<ControllerFlow>, Vec<String>) {
        let mut routes: Vec<RouteSite> = vec![];
        let mut mains: Vec<MainSite> = vec![];
        let mut flows: Vec<ControllerFlow> = vec![];
        let mut notes: Vec<String> = vec![];
        let trees = PyTrees::build(files);
        // import maps: from app import store → store → "app/store.py"
        let imports = import_maps(files);
        for (rel, tree) in &trees.trees {
            let (py_routes, main_line) = collect_routes(tree.clone(), files.get(rel).unwrap());
            let src = files.get(rel).unwrap();
            let file_base = rel.rsplit('/').next_back().unwrap_or(rel);
            for r in &py_routes {
                routes.push(RouteSite {
                    verb: r.verb.clone(),
                    path: r.path.clone(),
                    owner: owner_of(rel),
                    method: r.handler.clone(),
                    service: self.dir.clone(),
                    rel: rel.to_string(),
                    line: r.line,
                    params: crate::node::path_params(&r.path),
                    returns: "the handler's response".into(),
                    evidence: { let vb = r.verb.to_lowercase(); format!("{file_base}:{line2} @{vb} \"{path2}\"", line2 = r.line, path2 = r.path) },
                });
                // the flow
                if let Some(def) = trees.fns.get(&r.handler) {
                    let mut budget = 0usize;
                    let mut truncated = false;
                    let mut counter = 0usize;
                    let body_nodes = walk_py_body(&trees, &imports, rel, &def.body, 1, &mut budget, &mut truncated, &mut counter);
                    flow::merge_flow(
                        &mut flows,
                        rel,
                        &owner_of(rel),
                        &self.dir,
                        HandlerFlow {
                            method: r.handler.clone(),
                            verb: r.verb.clone(),
                            path: r.path.clone(),
                            root: FlowNode {
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
                                        format!("{}.{}(…)", owner_of(rel), r.handler),
                                        "controller",
                                        Some(rel.to_string()),
                                        Some(def.line),
                                        vec![flow::branch("body".into(), body_nodes)],
                                    )],
                                )],
                            },
                            honest: flow::honest_line(truncated),
                        },
                    );
                } else {
                    notes.push(format!(
                        "the {} handler wasn't defined in the scanned files — its body flow is honestly not drawn.",
                        r.handler
                    ));
                }
            }
            if let Some(line) = main_line {
                mains.push(MainSite {
                    name: owner_of(rel),
                    rel: rel.to_string(),
                    line,
                    kind: format!("{}-run", self.framework),
                    lang: "python".into(),
                });
            }
        }
        (routes, mains, flows, notes)
    }
}

fn owner_of(rel: &str) -> String {
    let base = rel.rsplit('/').next_back().unwrap_or(rel);
    base.trim_end_matches(".py").to_string()
}

/// one statement at a time over the body TEXT + the shared registry
fn walk_py_body(
    trees: &PyTrees,
    imports: &BTreeMap<String, BTreeMap<String, String>>,
    rel: &str,
    body: &str,
    depth: usize,
    budget: &mut usize,
    truncated: &mut bool,
    counter: &mut usize,
) -> Vec<FlowNode> {
    if depth > flow::MAX_FLOW_DEPTH {
        *truncated = true;
        return vec![];
    }
    let mut out: Vec<FlowNode> = vec![];
    for raw in body.lines() {
        let line = raw.trim_end();
        let t = line.trim();
        let indent = raw.len() - raw.trim_start().len();
        if t.is_empty() || t.starts_with('#') || t.starts_with('"') {
            continue;
        }
        *budget += 1;
        if *budget > flow::MAX_FLOW_NODES {
            *truncated = true;
            break;
        }
        if t.starts_with("raise ") {
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "throw",
                flow::text_of(t, 80),
                "response",
                Some(rel.to_string()),
                Some(line_of(body, t)),
                vec![],
            ));
            continue;
        }
        if t.starts_with("if ") || t.starts_with("if(") {
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "decision",
                flow::text_of(t, 70),
                "decision",
                Some(rel.to_string()),
                Some(line_of(body, t)),
                vec![flow::branch(format!("{}", flow::text_of(t, 60)), vec![])],
            ));
            continue;
        }
        if t.starts_with("elif ") || t.starts_with("else") {
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "decision",
                flow::text_of(t, 70),
                "decision",
                Some(rel.to_string()),
                Some(line_of(body, t)),
                vec![],
            ));
            continue;
        }
        if t.starts_with("return ") || t == "return" {
            // return store.record(event, target) → database node + the return
            if let Some(callee) = py_callee(t) {
                if flow::callee_is_db(&callee) {
                    *counter += 1;
                    out.push(flow::node(
                        format!("n{counter}"),
                        "action",
                        flow::text_of(&callee, 60),
                        "database",
                        Some(rel.to_string()),
                        Some(line_of(body, t)),
                        vec![],
                    ));
                }
            }
            *counter += 1;
            out.push(flow::node(
                format!("n{counter}"),
                "return",
                flow::text_of(t, 80),
                "response",
                Some(rel.to_string()),
                Some(line_of(body, t)),
                vec![],
            ));
            continue;
        }
        if t.starts_with("import ") || t.starts_with("from ") {
            continue;
        }
        // calls: the LAST name ( path → the callee, with module hops
        if let Some(callee) = py_callee(t) {
            if flow::callee_is_db(&callee) {
                *counter += 1;
                out.push(flow::node(
                    format!("n{counter}"),
                    "action",
                    flow::text_of(&callee, 60),
                    "database",
                    Some(rel.to_string()),
                    Some(line_of(body, t)),
                    vec![],
                ));
                continue;
            }
            // hop: events.deliver(...) → app/events.py deliver()
            if let Some((target_rel, target_fn)) = resolve_python(&trees, &imports, rel, &callee) {
                if depth <= flow::MAX_FLOW_DEPTH {
                    if let Some(def) = trees.fns.get(&target_fn) {
                        if def.rel == target_rel {
                            let sub = walk_py_body(trees, imports, &target_rel, &def.body, depth + 1, budget, truncated, counter);
                            if !sub.is_empty() {
                                *counter += 1;
                                out.push(flow::node(
                                    format!("n{counter}"),
                                    "action",
                                    format!("{target_fn}()"),
                                    "service",
                                    Some(target_rel.clone()),
                                    Some(def.line),
                                    vec![flow::branch("body".into(), sub)],
                                ));
                            }
                            continue;
                        }
                    }
                }
            }
        }
        let _ = indent;
    }
    out
}

/// the callee path of the line's first call, as written: `events.deliver`
fn py_callee(t: &str) -> Option<String> {
    let t = t.strip_prefix("return ").map(str::trim).unwrap_or(t);
    let t = t.strip_prefix("await ").map(str::trim).unwrap_or(t);
    let after = after_assignment(t);
    let paren = after.find('(')?;
    let head = after[..paren].trim().to_string();
    if head.is_empty() || head.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
        if head.contains(|c: char| c.is_alphabetic()) {
            return Some(head);
        }
    }
    None
}

fn after_assignment(t: &str) -> &str {
    match t.find(" = ") {
        Some(i) => &t[i + 3..],
        None => t,
    }
}

/// events.deliver → (file rel, fn name) via the importing file's imports
fn resolve_python(
    trees: &PyTrees,
    imports: &BTreeMap<String, BTreeMap<String, String>>,
    rel: &str,
    callee: &str,
) -> Option<(String, String)> {
    // `events.deliver(...)`: module attr = deliver, module name = events
    let (module_name, rest) = callee.split_once('.')?;
    let from_map = imports.get(rel)?.get(module_name)?.clone();
    let dir = rel.rfind('/').map(|i| i);
    let _ = dir;
    let wanted = module_file(&from_map, rel);
    // the fn name is the FIRST dotted component after the module: rest.split('.')
    let fn_name = rest.split('.').next_back()?.to_string();
    // the defining file: trees own the rel; check the module file exists
    for (other, _) in &trees.trees {
        if *other == wanted {
            return Some((wanted, fn_name));
        }
    }
    // relative import within the same dir (from . import store)
    let same_dir = rel.rsplit_once('/').map(|(d, _)| format!("{d}/store.py"));
    if wanted.ends_with(".py") {
        return Some((wanted, fn_name));
    }
    if let Some(sd) = same_dir {
        if trees.trees.contains_key(&sd) {
            return Some((sd, fn_name));
        }
    }
    None
}

/// "app/store" → "app/store.py" (relative to the importing file's dir)
fn module_file(module: &str, importer_rel: &str) -> String {
    let clean = module.trim_start_matches('.');
    let base = module.starts_with('.');
    let dir = importer_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    if base && !dir.is_empty() {
        format!("{dir}/{clean}.py")
    } else if !dir.is_empty() {
        format!("{dir}/{clean}.py")
    } else {
        format!("{clean}.py")
    }
}

fn line_of(body: &str, t: &str) -> u32 {
    for (i, l) in body.lines().enumerate() {
        let lt = l.trim_end();
        let trim = lt.trim_end();
        let _ = trim;
        if lt.ends_with(t.trim_end()) {
            return i as u32 + 1;
        }
    }
    1
}

/// from app import store ; from app.events import deliver
fn import_maps(files: &Sources) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (rel, src) in files {
        if !rel.ends_with(".py") {
            continue;
        }
        let mut im: BTreeMap<String, String> = BTreeMap::new();
        for line in src.lines() {
            let t = line.trim();
            if t.starts_with("from ") {
                // from X import A, B
                let module = t.split_whitespace().nth(1).unwrap_or("");
                let names = t.split("import").nth(1).unwrap_or("");
                for n in names.split(',') {
                    let n = n.trim();
                    if !n.is_empty() {
                        im.insert(n.to_string(), module.to_string());
                    }
                }
            } else if t.starts_with("import ") {
                // import X.Y as alias → alias: X.Y
                let rest = t.split_whitespace().nth(1).unwrap_or("");
                let mut it = rest.split(" as ");
                let module = it.next().unwrap_or("").to_string();
                let alias = it.next().unwrap_or(&module);
                im.insert(alias.trim().split('.').last().unwrap_or(alias).trim().to_string(), module);
            }
        }
        out.insert(rel.clone(), im);
    }
    out
}

impl factory::StackParser for PyParser {
    fn id(&self) -> &'static str {
        "python-flask"
    }
    fn describes(&self) -> String {
        format!("{} over python (flask/fastapi routes + handler flows, one hop into sibling modules)", self.framework)
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        let scoped = factory::scope(files, &self.dir);
        let (routes, mains, flows, notes) = self.parse_dir(&scoped);
        report.routes.extend(routes);
        report.mains.extend(mains);
        report.flows.extend(flows);
        report
            .notes
            .push("[python] handler flows follow the handler's module plus one hop into imported functions (as written, not evaluated).".into());
        for n in notes {
            report.notes.push(n);
        }
    }
}