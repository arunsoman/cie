//! Shared flow-building helpers for the stack parsers.
//!
//! Every parser (spring, express, axum, flask) turns a handler body into the
//! same branch-capable FlowNode tree: decisions with labelled arms as written,
//! throws, returns, and database-shaped calls. Conditions are QUOTED, never
//! evaluated — CIE doesn't run the code. The caps are shared so no parser
//! can silently lie about depth.

pub use crate::types::{ControllerFlow, FlowBranch, FlowNode, HandlerFlow};

pub const MAX_FLOW_NODES: usize = 48;
pub const MAX_FLOW_DEPTH: usize = 4;

/// one honest sentence, shared by every parser (the "never evaluated" line)
pub fn honest_line(truncated: bool) -> String {
    if truncated {
        format!(
            "deeper calls exist beyond the {MAX_FLOW_NODES}-node / {MAX_FLOW_DEPTH}-depth cap — every branch is the handler's own if/else/switch as written; conditions quoted, never evaluated (CIE doesn't run the code)."
        )
    } else {
        "every branch is the handler's own if/else/switch as written; conditions quoted, never evaluated (CIE doesn't run the code).".into()
    }
}

/// text_of: collapse newlines/extra spaces, truncate for a node label
pub fn text_of(src: &str, max: usize) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for c in src.chars() {
        let is_space = c.is_whitespace();
        if is_space {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            out.push(c);
            if out.len() >= max {
                out.truncate(max);
                out.push('…');
                break;
            }
        }
    }
    out.trim_end().to_string()
}

/// a callee looks like it speaks to storage when its name/path says so —
/// one shared heuristic across languages (as written in the call text)
pub fn db_like(call_text: &str) -> Option<(&'static str, &'static str)> {
    let t = call_text.to_lowercase();
    const WRITE: &[&str] = &["save", "insert", "update", "delete", "upsert", "record", "set_"];
    const READ: &[&str] = &["find", "query", "select", "get", "by_id", "load", "fetch", "status"];
    for w in WRITE {
        if t.contains(w) {
            return Some(("database", "write"));
        }
    }
    for w in READ {
        if t.contains(w) {
            return Some(("database", "read"));
        }
    }
    None
}

/// classify a callee into a layer + a plain label: "store.save(…)" →
/// database node; a service-shaped callee (catalog./events./service.) →
/// service node; plain calls stay actions.
pub fn callee_is_db(call_text: &str) -> bool {
    db_like(call_text).is_some()
}

pub fn node(id: String, kind: &str, label: String, layer: &str, rel: Option<String>, line: Option<u32>, children: Vec<FlowBranch>) -> FlowNode {
    FlowNode {
        id,
        kind: kind.into(),
        label,
        layer: layer.into(),
        r#type: None,
        rel,
        line,
        children,
    }
}

pub fn branch(label: String, nodes: Vec<FlowNode>) -> FlowBranch {
    FlowBranch { label, nodes }
}

/// strip one leading path segment (the service dir) — helpers for name maps
pub fn strip_dir<'a>(rel: &'a str, dir: &str) -> &'a str {
    if dir.is_empty() {
        rel
    } else if let Some(stripped) = rel.strip_prefix(dir) {
        stripped.trim_start_matches('/')
    } else {
        rel
    }
}
/// ALL descendants (pre-order) of a node — the shared walk helper. tree-sitter
///'s cursors borrow; Node is Copy, so materializing keeps every parser simple.
pub fn descendants(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut out = vec![node];
    let mut i = 0;
    while i < out.len() {
        let n = out[i];
        let len = n.child_count();
        let mut push: Vec<tree_sitter::Node> = Vec::with_capacity(len);
        {
            let mut c = n.walk();
            for k in n.children(&mut c) {
                push.push(k);
            }
        }
        out.extend(push);
        i += 1;
    }
    out
}

/// the DIRECT children of a node (collected — same reason as descendants)
pub fn children_of(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut c = node.walk();
    node.children(&mut c).collect ()
}

/// descendants WITHOUT the node itself (used by the parsers' tree scans)
pub fn only_descendants(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut all = descendants(node);
    all.remove(0);
    all
}

/// one ControllerFlow per FILE: append/merge a handler flow into `flows`
pub fn merge_flow(flows: &mut Vec<ControllerFlow>, rel: &str, owner: &str, dir: &str, hf: HandlerFlow) {
    let name = if dir.is_empty() {
        owner.to_string()
    } else {
        format!("{dir}/{owner}")
    };
    if let Some(existing) = flows.iter_mut().find(|f| f.rel == rel) {
        existing.flows.push(hf);
    } else {
        flows.push(ControllerFlow {
            name,
            rel: rel.to_string(),
            base_path: "/".into(),
            flows: vec![hf],
        });
    }
}