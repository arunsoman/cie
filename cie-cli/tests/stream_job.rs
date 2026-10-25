//! CLI/web contract: one ingestion command publishes a coherent artifact set.
use std::{fs, process::Command};
#[test]
fn unified_job_exports_views_and_preserves_the_legacy_alias() {
    let root = std::env::temp_dir().join(format!("cie-cli-stream-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("app.py"),"def charge(amount):\n    return amount + 1\n\ndef pay(amount):\n    return charge(amount)\n").unwrap();
    fs::write(
        root.join("README.md"),
        "# Example\nThe charge function accepts an amount.\n",
    )
    .unwrap();
    let output = root.with_extension("artifacts.json");
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
    let artifacts: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    for key in [
        "report",
        "viewspec",
        "concepts",
        "stack",
        "polyglotSpecs",
        "deps",
        "overlap",
    ] {
        assert!(!artifacts[key].is_null(), "{key}");
    }
    assert!(!artifacts["report"]["symbols"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        artifacts["polyglotSpecs"]["kind"],
        "cie-polyglot-viewspec-bundle"
    );
    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&run.stderr)
        .lines()
        .filter_map(|line| line.strip_prefix("CIE_STAGE "))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let storage_heartbeats: Vec<&serde_json::Value> = events
        .iter()
        .filter(|e| e["key"] == "storage" && e["state"] == "running")
        .collect();
    assert!(
        storage_heartbeats.iter().any(|e| e["detail"].as_str().unwrap_or("").contains("Building view:")),
        "storage step should emit building-view heartbeats; got: {storage_heartbeats:?}"
    );
    let completed: Vec<&str> = events
        .iter()
        .filter(|e| e["state"] == "done")
        .filter_map(|e| e["key"].as_str())
        .collect();
    assert_eq!(&completed[..5], &["read", "resolve", "stack", "concepts", "storage"]);
    assert_eq!(completed.last(), Some(&"publish"));
    // Cached runs must also settle every core phase rather than leaving the UI pending.
    let warm = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["ingest-polyglot", root.to_str().unwrap(), "--web-out", output.to_str().unwrap()])
        .output().unwrap();
    assert!(warm.status.success());
    let warm_events: Vec<serde_json::Value> = String::from_utf8_lossy(&warm.stderr)
        .lines().filter_map(|line| line.strip_prefix("CIE_STAGE "))
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    for key in ["read", "resolve", "stack", "concepts", "storage", "publish"] {
        assert!(warm_events.iter().any(|e| e["key"] == key && e["state"] == "done"), "{key}");
    }
    let alias = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["ingest", root.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        alias.status.success(),
        "{}",
        String::from_utf8_lossy(&alias.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&alias.stdout).unwrap();
    assert_eq!(report["rev"], artifacts["report"]["rev"]);
    let db = cie_polyglot::schema::default_db_path(&root);
    let _ = fs::remove_file(db);
    let _ = fs::remove_file(output);
    let _ = fs::remove_dir_all(root);
}
