//! cie-server — optional HTTP observability endpoints (PRD §10 member).
//! Scaffold stub; `tokio` sits behind the non-default `runtime` feature so
//! it compiles only when the server is actually requested.

pub mod mcp;

pub use mcp::{
    handle_message, honesty_envelope, McpBudget, RpcError, RpcRequest, RpcResponse, ToolDecl,
    ToolError, ToolExecutor, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION, TOOLS,
};

/// Placeholder so the crate is green from day one (TASK-001 DoD).
#[test]
fn scaffold_placeholder() {
    assert!(true, "server endpoints land post-v0.3");
}

#[cfg(feature = "runtime")]
pub mod runtime {
    /// Minimal liveness probe (TASK-001: tokio only behind this gate).
    pub async fn healthy() -> bool {
        true
    }
}
