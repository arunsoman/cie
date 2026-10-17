//! MCP (Model Context Protocol) server seam (CIE-US US-134 engine close).
//!
//! A tool builder's coding agent asks impact / callers / tests /
//! explanation questions over JSON-RPC 2.0 and gets answers that state
//! how they are known. The story's obligations, enforced here:
//! * **read-only** — the tool registry contains no mutating tools;
//! * **budgeted** — every call goes through the engine budget
//!   (`BudgetNs`); an exhausted budget is an honest error, not a timeout;
//! * **declared scope** — tools refuse anything outside their declared
//!   input schema (unknown tools, malformed args);
//! * **claim-classes first** — every result leads with the claim class
//!   (measured/static/inferred), citations, gaps, the revision it
//!   describes, and what was withheld; staleness is stated, not hidden.
//!
//! Transport: MCP stdio framing (JSON-RPC 2.0, one message per line —
//! the newline-delimited variant agents embed most easily). The seam is
//! transport-agnostic: [`handle_message`] is pure and testable; a stdio
//! loop can wrap it without re-implementing protocol semantics.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol constants (MCP `initialize` handshake).
pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const SERVER_NAME: &str = "cie-evidence-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One declared read-only tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDecl {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON-schema-ish input contract (declared scope).
    pub input_keys: &'static [&'static str],
}

/// The read-only tool registry. Deliberately small and closed: the
/// engine's honest read surface over CLI capabilities.
pub const TOOLS: &[ToolDecl] = &[
    ToolDecl {
        name: "cie_report",
        description: "drift + findings report for a revision, with statuses and evidence citations",
        input_keys: &["rev"],
    },
    ToolDecl {
        name: "cie_impact",
        description: "reverse-dependency impact of a symbol with test coverage (how known: call graph from parsed sources)",
        input_keys: &["symbol"],
    },
    ToolDecl {
        name: "cie_concepts",
        description: "concept -> symbol mapping with confidence and provenance",
        input_keys: &["concept"],
    },
    ToolDecl {
        name: "cie_stories_coverage",
        description: "user-story disposition coverage (delivered/partial/planned counts, no percentages)",
        input_keys: &[],
    },
];

/// The per-call budget (ns of engine work); an exhausted budget is an
/// explicit error result, never a fabricated partial answer.
#[derive(Debug, Clone, Copy)]
pub struct McpBudget {
    pub ns: u64,
}

/// Engine-backed tool executor — the seam the CLI implements.
pub trait ToolExecutor {
    /// Execute one tool with declared-scope-checked args. Returns the
    /// structured result payload (already claim-class-first).
    fn execute(
        &self,
        tool: &str,
        args: &BTreeMap<String, Value>,
        budget: &McpBudget,
    ) -> Result<Value, ToolError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    UnknownTool(String),
    MissingArg(String),
    BudgetExceeded,
    Engine(String),
}

impl ToolError {
    pub fn code(&self) -> &'static str {
        match self {
            ToolError::UnknownTool(_) => "unknown_tool",
            ToolError::MissingArg(_) => "missing_argument",
            ToolError::BudgetExceeded => "budget_exceeded",
            ToolError::Engine(_) => "engine_error",
        }
    }
}

/// An incoming JSON-RPC request (parsed).
#[derive(Debug, Clone, Deserialize)]
pub struct RpcRequest {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// An outgoing JSON-RPC response.
#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Handle one newline-delimited JSON-RPC message (pure, transport-free).
/// Implements the MCP subset: `initialize`, `tools/list`, `tools/call`,
/// `ping`. Anything else is `method_not_found` — the declared scope is
/// the whole protocol, and refusals are explicit.
pub fn handle_message(
    line: &str,
    executor: &dyn ToolExecutor,
    budget: &McpBudget,
) -> Option<RpcResponse> {
    let req: RpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("parse error: {e}"),
                    data: None,
                }),
            });
        }
    };
    let id = req.id.clone();
    match req.method.as_str() {
        "initialize" => Some(ok(id, serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            "capabilities": { "tools": {} },
        }))),
        "ping" => Some(ok(id, serde_json::json!({}))),
        "tools/list" => {
            let tools: Vec<Value> = TOOLS
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": {
                            "type": "object",
                            "required": t.input_keys,
                            "properties": t.input_keys.iter()
                                .map(|k| (k.to_string(), serde_json::json!({"type": "string"})))
                                .collect::<BTreeMap<String, Value>>(),
                        },
                        "annotations": { "readOnlyHint": true },
                    })
                })
                .collect();
            Some(ok(id, serde_json::json!({ "tools": tools })))
        }
        "tools/call" => {
            let name = req.params.get("name").and_then(|v| v.as_str());
            let Some(name) = name else {
                return Some(err(id, -32602, "tools/call requires a tool name", None));
            };
            let decl = TOOLS.iter().find(|t| t.name == name);
            let Some(decl) = decl else {
                return Some(err(
                    id,
                    -32602,
                    "tool outside declared scope",
                    Some(serde_json::json!({ "code": ToolError::UnknownTool(name.into()).code() })),
                ));
            };
            let args_json = req.params.get("arguments").cloned().unwrap_or(Value::Null);
            let args: BTreeMap<String, Value> = args_json.as_object().map(|m| {
                m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }).unwrap_or_default();
            // declared scope: required keys must be present
            for k in decl.input_keys {
                if !args.contains_key(*k) {
                    return Some(err(
                        id,
                        -32602,
                        &format!("missing argument: {k}"),
                        Some(serde_json::json!({ "code": ToolError::MissingArg(k.to_string()).code() })),
                    ));
                }
            }
            match executor.execute(name, &args, budget) {
                Ok(v) => Some(ok(id, v)),
                Err(e @ ToolError::BudgetExceeded) => Some(err(
                    id,
                    -32000,
                    "engine budget exhausted — no partial answer fabricated",
                    Some(serde_json::json!({ "code": e.code() })),
                )),
                Err(e) => Some(err(
                    id,
                    -32000,
                    &match &e {
                        ToolError::Engine(m) => m.clone(),
                        other => format!("tool error: {}", other.code()),
                    },
                    Some(serde_json::json!({ "code": e.code() })),
                )),
            }
        }
        _ => Some(err(id, -32601, "method not found (declared scope: initialize, ping, tools/list, tools/call)", None)),
    }
}

/// Wrap a tool payload with the honesty envelope (US-134): results lead
/// with claim classes, citations, gaps, revision and withheld items.
pub fn honesty_envelope(
    revision: u64,
    claim_class: &str,
    citations: Vec<String>,
    gaps: Vec<String>,
    withheld: Vec<String>,
    stale: Option<u64>,
    payload: Value,
) -> Value {
    serde_json::json!({
        "revision": revision,
        "claim_class": claim_class,
        "citations": citations,
        "gaps": gaps,
        "withheld": withheld,
        "staleness": stale.map(|s| serde_json::json!({ "ns_since_ingest": s })),
        "result": payload,
    })
}

fn ok(id: Value, result: Value) -> RpcResponse {
    RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}

fn err(id: Value, code: i64, message: &str, data: Option<Value>) -> RpcResponse {
    RpcResponse { jsonrpc: "2.0", id, result: None, error: Some(RpcError { code, message: message.into(), data }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeExecutor;

    impl ToolExecutor for FakeExecutor {
        fn execute(
            &self,
            tool: &str,
            args: &BTreeMap<String, Value>,
            _budget: &McpBudget,
        ) -> Result<Value, ToolError> {
            if tool == "cie_impact" {
                let sym = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                if sym == "boom" {
                    return Err(ToolError::Engine("reverse deps unavailable".into()));
                }
                return Ok(honesty_envelope(
                    41,
                    "static",
                    vec!["call graph: parsed sources @ r41".into()],
                    vec!["dynamic calls unresolved".into()],
                    vec!["generated files withheld".into()],
                    Some(1_000),
                    serde_json::json!({ "callers": ["web/api.py:handle"] }),
                ));
            }
            Err(ToolError::UnknownTool(tool.into()))
        }
    }

    fn call(line: &str) -> RpcResponse {
        handle_message(line, &FakeExecutor, &McpBudget { ns: 1_000_000 }).unwrap()
    }

    #[test]
    fn initialize_and_tools_list_declare_read_only_scope() {
        let r = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        assert_eq!(r.result.unwrap()["protocolVersion"], PROTOCOL_VERSION);
        let r = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
        let tools = r.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 4);
        for t in &tools {
            assert_eq!(t["annotations"]["readOnlyHint"], true);
        }
        // every declared tool is read-only by construction of the registry
        assert!(TOOLS.iter().all(|t| !t.name.contains("write") && !t.name.contains("ingest")));
    }

    #[test]
    fn tools_call_returns_claim_class_first_envelope() {
        let r = call(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cie_impact","arguments":{"symbol":"pay::charge"}}}"#);
        let v = r.result.unwrap();
        assert_eq!(v["claim_class"], "static");
        assert!(!v["citations"].as_array().unwrap().is_empty());
        assert!(!v["gaps"].as_array().unwrap().is_empty());
        assert_eq!(v["revision"], 41);
        assert!(v["staleness"].is_object());
        assert_eq!(v["result"]["callers"][0], "web/api.py:handle");
    }

    #[test]
    fn out_of_scope_and_missing_args_are_explicit_refusals() {
        let r = call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"write_file","arguments":{}}}"#);
        assert_eq!(r.error.unwrap().message, "tool outside declared scope");
        let r = call(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cie_impact","arguments":{}}}"#);
        let e = r.error.unwrap();
        assert_eq!(e.code, -32602);
        assert_eq!(e.data.unwrap()["code"], "missing_argument");
        let r = call(r#"{"jsonrpc":"2.0","id":6,"method":"admin/shutdown","params":{}}"#);
        assert_eq!(r.error.unwrap().code, -32601);
    }

    #[test]
    fn engine_errors_are_honest_and_parse_errors_reported() {
        let r = call(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"cie_impact","arguments":{"symbol":"boom"}}}"#);
        let e = r.error.unwrap();
        assert_eq!(e.data.unwrap()["code"], "engine_error");
        assert!(e.message.contains("unavailable"));
        let r = call("not json at all");
        assert_eq!(r.error.unwrap().code, -32700);
    }
}
