//! RFC-0022 §6.1 / §8 — `cie trace` end-to-end through the real binary:
//! multi-hop navigation over the persisted RelationGraph, depth-bounded,
//! kind-filtered.

use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-rfc0022-trace-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `main` -> `middle` -> `helper`: a clean 2-hop resolved-call chain.
fn write_chain(dir: &std::path::Path) {
    std::fs::write(
        dir.join("app.py"),
        "def helper():\n    return 42\n\ndef middle():\n    return helper() + 1\n\ndef main():\n    return middle()\n",
    )
    .unwrap();
}

fn run_trace(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let mut full_args = vec!["trace", dir.to_str().unwrap()];
    full_args.extend_from_slice(args);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(&full_args)
        .output()
        .expect("binary runs");
    assert!(out.status.success(), "exit code {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("stdout is valid JSON")
}

#[test]
fn depth_bounds_the_hop_count() {
    let dir = tmp_dir("depth");
    write_chain(&dir);

    let v2 = run_trace(&dir, &["main", "--depth", "2", "--json"]);
    let hops2 = v2["hops"].as_array().unwrap();
    assert_eq!(hops2.len(), 2, "main->middle->helper is exactly 2 hops");
    assert_eq!(hops2[0][0]["symbol"], "app.middle");
    assert_eq!(hops2[1][0]["symbol"], "app.helper");

    let v1 = run_trace(&dir, &["main", "--depth", "1", "--json"]);
    let hops1 = v1["hops"].as_array().unwrap();
    assert_eq!(hops1.len(), 1, "depth 1 stops after the first hop");
    assert_eq!(hops1[0][0]["symbol"], "app.middle");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reverse_walks_callers_instead_of_callees() {
    let dir = tmp_dir("reverse");
    write_chain(&dir);

    let v = run_trace(&dir, &["helper", "--reverse", "--depth", "2", "--json"]);
    let hops = v["hops"].as_array().unwrap();
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[0][0]["symbol"], "app.middle");
    assert_eq!(hops[1][0]["symbol"], "app.main");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn kind_filter_excludes_non_matching_relations() {
    let dir = tmp_dir("kind");
    write_chain(&dir);

    // No `reads` relation exists on this pure-call chain — the filter
    // must produce zero hops, not fall back to Calls.
    let v = run_trace(&dir, &["main", "--kind", "reads", "--depth", "3", "--json"]);
    let hops = v["hops"].as_array().unwrap();
    assert!(hops.is_empty(), "no Reads edges in a pure call chain");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_kind_is_a_clean_error_not_a_panic() {
    let dir = tmp_dir("badkind");
    write_chain(&dir);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["trace", dir.to_str().unwrap(), "main", "--kind", "nonsense"])
        .output()
        .expect("binary runs");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown --kind"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_symbol_is_a_clean_error() {
    let dir = tmp_dir("badsym");
    write_chain(&dir);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["trace", dir.to_str().unwrap(), "does_not_exist"])
        .output()
        .expect("binary runs");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));

    std::fs::remove_dir_all(&dir).ok();
}
