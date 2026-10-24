//! Optional protocol hosts. Storage remains an in-process library in each host.
pub mod mcp;
pub use mcp::{
    handle_message, run_stdio, tool_declarations, McpBudget, RpcError, RpcResponse, StorageMcp,
    PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};
