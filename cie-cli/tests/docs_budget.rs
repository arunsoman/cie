//! An optional documentation timeout must not discard a captured revision.
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn expired_docs_budget_publishes_incomplete_artifact_and_keeps_core_data() {
    let root = std::env::temp_dir().join(format!("cie-docs-budget-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("app.py"),
        "def pay(amount):\n    return amount + 1\n",
    )
    .unwrap();
    fs::write(
        root.join("README.md"),
        "The pay function must validate the amount.\n",
    )
    .unwrap();
    fs::write(
        root.join("GUIDE.md"),
        "The pay function must validate the amount.\n",
    )
    .unwrap();
    let output = root.join("bundle.json");
    let run = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args([
            "ingest-polyglot",
            root.to_str().unwrap(),
            "--web-out",
            output.to_str().unwrap(),
            "--docs-budget-ms",
            "0",
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let bundle: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert!(!bundle["report"]["symbols"].as_array().unwrap().is_empty());
    assert!(!bundle["polyglotSpecs"].is_null());
    assert_eq!(bundle["drift"]["complete"], false);
    assert_eq!(
        bundle["drift"]["incomplete_reason"],
        "documentation_budget_exceeded"
    );
    assert_eq!(bundle["drift"]["aggregate"], serde_json::json!({}));
    let events: Vec<Value> = String::from_utf8_lossy(&run.stderr)
        .lines()
        .filter_map(|l| l.strip_prefix("CIE_STAGE "))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(events.iter().any(|e| e["key"] == "drift"
        && e["state"] == "skipped"
        && e["detail"].as_str().unwrap().contains("incomplete")));
    assert!(events
        .iter()
        .any(|e| e["key"] == "publish" && e["state"] == "done"));
    let storage = cie_storage::retrieval::Storage::open(&root, Default::default()).unwrap();
    let reader = storage
        .open_revision(cie_storage::retrieval::RevisionSelector::Id(
            bundle["report"]["rev"].to_string(),
        ))
        .unwrap();
    let request = serde_json::json!({"query":{"select":{"kind":"file"}},"outputs":[{"id":"bootstrap","output":{"artifact":{"kind":"ui_bootstrap_v1"}}}]});
    let result = reader
        .retrieve(serde_json::from_value(request).unwrap())
        .unwrap();
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(
        value["outputs"][0]["data"]["document"]["drift"]["incomplete_reason"],
        "documentation_budget_exceeded"
    );
    drop(reader);
    // An incomplete report must not poison the cache of a later full check.
    let retry = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args([
            "ingest-polyglot",
            root.to_str().unwrap(),
            "--web-out",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let retry: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(retry["drift"]["complete"], true);
    assert!(!retry["drift"]["claims"].as_array().unwrap().is_empty());
    let ids: std::collections::BTreeSet<_> = retry["drift"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids.len(), 2, "claim IDs must not collide across documents");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unreadable_document_is_incomplete_instead_of_failing_ingestion() {
    let root = std::env::temp_dir().join(format!("cie-docs-error-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("app.py"), "def healthy():\n    return 1\n").unwrap();
    fs::write(root.join("README.md"), [0xff, 0xfe]).unwrap();
    let output = root.join("bundle.json");
    let run = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args([
            "ingest-polyglot",
            root.to_str().unwrap(),
            "--web-out",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let bundle: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(bundle["drift"]["complete"], false);
    assert_eq!(
        bundle["drift"]["incomplete_reason"],
        "documentation_check_failed"
    );
    assert!(!bundle["report"]["symbols"].as_array().unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn positive_docs_budget_can_expire_and_still_publish() {
    let root = std::env::temp_dir().join(format!("cie-docs-live-budget-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("app.py"),
        "def pay(amount):\n    return amount + 1\n",
    )
    .unwrap();
    fs::write(
        root.join("README.md"),
        "The pay function must validate the transaction amount before writing.\n".repeat(3_000),
    )
    .unwrap();
    let output = root.join("bundle.json");
    let run = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args([
            "ingest-polyglot",
            root.to_str().unwrap(),
            "--web-out",
            output.to_str().unwrap(),
            "--docs-budget-ms",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let bundle: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(bundle["drift"]["complete"], false);
    assert_eq!(
        bundle["drift"]["incomplete_reason"],
        "documentation_budget_exceeded"
    );
    assert!(!bundle["report"]["symbols"].as_array().unwrap().is_empty());
    assert!(root.join(".cie/revs/1/web-bundle.json").is_file());
    fs::remove_dir_all(root).unwrap();
}
