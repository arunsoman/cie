use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
struct Agent {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    root: PathBuf,
    next: u64,
}
impl Agent {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cie-mcp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("app.py"),
            "def one(x):\n    return x + 1\n\ndef two():\n    return one(1)\n",
        )
        .unwrap();
        Self::ingest(&root);
        let mut process = Command::new(env!("CARGO_BIN_EXE_cie"))
            .args(["mcp", root.to_str().unwrap(), "--budget-ms", "10000"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = process.stdin.take().unwrap();
        let output = BufReader::new(process.stdout.take().unwrap());
        Self {
            process,
            input,
            output,
            root,
            next: 0,
        }
    }
    fn ingest(root: &PathBuf) {
        let run = Command::new(env!("CARGO_BIN_EXE_cie"))
            .args([
                "ingest-polyglot",
                root.to_str().unwrap(),
                "--web-out",
                root.join("bundle.json").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    fn send(&mut self, message: Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }
    fn read(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "MCP exited without a response"
        );
        serde_json::from_str(&line).unwrap()
    }
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let result = self.read();
        assert_eq!(result["id"], id);
        result
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let response = self.rpc("tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(response["result"]["isError"], false, "{response}");
        let structured = response["result"]["structuredContent"].clone();
        assert_eq!(
            serde_json::from_str::<Value>(
                response["result"]["content"][0]["text"].as_str().unwrap()
            )
            .unwrap(),
            structured
        );
        structured
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_file(cie_polyglot::schema::default_db_path(&self.root));
    }
}
#[test]
fn real_stdio_uses_storage_schemas_contexts_and_captured_evidence() {
    let mut agent = Agent::new();
    let init=agent.rpc("initialize",json!({"protocolVersion":"2025-11-25","clientInfo":{"name":"storage-test","version":"1"},"capabilities":{}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "cie-storage-mcp");
    agent.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert!(agent.rpc("ping", json!({}))["result"].is_object());
    let list = agent.rpc("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools
        .iter()
        .all(|tool| tool["annotations"]["readOnlyHint"] == true));
    assert!(tools[1]["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("context")));
    let first = agent.tool(
        "cie_describe",
        json!({"topics":["symbol.fields","outputs.source"]}),
    );
    let context = first["context"].clone();
    assert_eq!(first["revision"], "1");
    let rows=agent.tool("cie_retrieve",json!({"context":context,"select":{"kind":"symbol","where":[{"field":"name","op":"eq","value":"one"}]},"outputs":[{"id":"rows","entities":{"fields":["qualified_name","location"],"page":{"limit":1}}}]}));
    assert_eq!(rows["outputs"][0]["schema_version"], 2);
    assert!(!rows["evidence"].as_object().unwrap().is_empty());
    let id = rows["outputs"][0]["data"]["items"][0]["id"].clone();
    std::fs::write(
        agent.root.join("app.py"),
        "def one(x):\n    return x + 99\n",
    )
    .unwrap();
    Agent::ingest(&agent.root);
    std::fs::remove_file(agent.root.join("app.py")).unwrap();
    let same = agent.tool("cie_describe", json!({"context":context}));
    assert_eq!(same["revision"], "1");
    let source=agent.tool("cie_retrieve",json!({"context":context,"query":{"select":{"kind":"symbol","filter":{"ids":[id]}}},"outputs":[{"id":"source","detail":{"source":{}}}]}));
    assert!(source["outputs"][0]["data"]["text"]
        .as_str()
        .unwrap()
        .contains("x + 1"));
    assert!(!source["outputs"][0]["data"]["text"]
        .as_str()
        .unwrap()
        .contains("99"));
    let next = agent.tool("cie_describe", json!({}));
    assert_eq!(next["revision"], "2");
    let unknown = agent.rpc("tools/call", json!({"name":"cie_report","arguments":{}}));
    assert_eq!(unknown["error"]["data"]["code"], "unknown_tool");
    let invalid = agent.rpc(
        "tools/call",
        json!({"name":"cie_describe","arguments":{"root":"/etc"}}),
    );
    assert_eq!(invalid["result"]["isError"], true);
    assert_eq!(
        invalid["result"]["structuredContent"]["code"],
        "InvalidRequest"
    );
    let expired=agent.rpc("tools/call",json!({"name":"cie_retrieve","arguments":{"context":"unknown","select":{"kind":"file"},"outputs":[]}}));
    assert_eq!(
        expired["result"]["structuredContent"]["code"],
        "CursorExpired"
    );
}
#[test]
fn cancellation_notifications_are_consumed_while_requests_are_queued() {
    let mut agent = Agent::new();
    let described = agent.tool("cie_describe", json!({}));
    agent.send(json!({"jsonrpc":"2.0","id":"cancel-target","method":"tools/call","params":{"name":"cie_retrieve","arguments":{"context":described["context"],"select":{"kind":"symbol"},"outputs":[{"id":"rows","entities":{}}]}}}));
    agent.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"cancel-target","reason":"test"}}));
    let response = agent.read();
    assert_eq!(response["id"], "cancel-target");
    // Either completion won the race or cancellation returned explicitly; the
    // next call must keep the reader usable and notifications produce no reply.
    if response["result"]["isError"] == true {
        assert_eq!(response["result"]["structuredContent"]["code"], "Cancelled");
    }
    assert!(agent.rpc("ping", json!({}))["result"].is_object());
    let again = agent.tool("cie_describe", json!({"context":described["context"]}));
    assert_eq!(again["revision"], "1");
}

#[test]
fn rejected_descriptions_and_late_cancellation_do_not_poison_session() {
    let mut agent = Agent::new();
    for _ in 0..35 {
        let response = agent.rpc(
            "tools/call",
            json!({"name":"cie_describe","arguments":{"topics":["invalid-topic"]}}),
        );
        assert_eq!(
            response["result"]["structuredContent"]["code"], "InvalidRequest",
            "{response}"
        );
    }
    let described = agent.tool("cie_describe", json!({}));
    let id = agent.next;
    agent.send(
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":id}}),
    );
    agent.rpc("ping", json!({}));
    agent.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"cie_describe","arguments":{"context":described["context"]}}}));
    assert_eq!(agent.read()["result"]["isError"], false);
}
