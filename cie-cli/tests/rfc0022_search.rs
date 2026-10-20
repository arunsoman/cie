//! RFC-0022 §6.2 / §8 — `cie search` end-to-end through the real binary:
//! embed() -> HnswRsStore, with hits re-attached to concepts/contracts.

use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-rfc0022-search-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run_search(dir: &std::path::Path, query: &str) -> serde_json::Value {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["search", dir.to_str().unwrap(), query, "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success(), "exit code {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("stdout is valid JSON")
}

#[test]
fn semantically_closest_symbol_ranks_first() {
    let dir = tmp_dir("rank");
    std::fs::write(
        dir.join("app.py"),
        "def authorize(user):\n    \"\"\"Checks whether the caller has permission to proceed.\"\"\"\n    return user.is_admin\n\ndef format_date(ts):\n    \"\"\"Formats a unix timestamp as a date string.\"\"\"\n    return str(ts)\n",
    )
    .unwrap();

    let v = run_search(&dir, "check user permission");
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["symbol"], "app.authorize", "the permission-check docstring should rank closest");
    let d_first = hits[0]["distance"].as_f64().unwrap();
    let d_rest = hits.iter().skip(1).map(|h| h["distance"].as_f64().unwrap()).fold(f64::MAX, f64::min);
    assert!(d_first <= d_rest, "closest hit must have the smallest distance");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_query_with_no_close_match_still_returns_ranked_results_not_an_error() {
    // Every adapter emits at least a module symbol, so "nothing to
    // search" (zero embedded records) isn't reachable from a real,
    // ingestible file — the honest behavior for an unrelated query is
    // ranked-but-distant results, never a refusal.
    let dir = tmp_dir("nomatch");
    std::fs::write(dir.join("app.py"), "# just a comment, no functions\n").unwrap();
    let v = run_search(&dir, "something entirely unrelated to this file");
    assert!(v["hits"].as_array().unwrap().len() >= 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hit_carries_real_evidence_and_contract_join() {
    let dir = tmp_dir("evidence");
    std::fs::write(
        dir.join("app.py"),
        "def authorize(user):\n    \"\"\"Checks whether the caller has permission to proceed.\"\"\"\n    return user.is_admin\n",
    )
    .unwrap();

    let v = run_search(&dir, "permission check");
    let hits = v["hits"].as_array().unwrap();
    let hit = hits.iter().find(|h| h["symbol"] == "app.authorize").expect("authorize found");
    assert_eq!(hit["evidence"], "app.py:1");
    // The naming-convention contract extractor already recognizes
    // "authorize"-named functions (cie-analysis::conformance) — the
    // search hit should carry it, proving the hybrid join is real.
    assert!(!hit["contracts"].as_array().unwrap().is_empty(), "authorize should carry a contract");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_missing_embeddings_are_rebuilt_and_cached_without_rewriting() {
    let dir = tmp_dir("legacy");
    std::fs::write(dir.join("app.py"), "def authorize():\n    return True\n").unwrap();
    let first = run_search(&dir, "authorize");
    let store = cie_store::Store::open(&dir).unwrap();
    let rev = store.last_rev();
    let artifact = store.root().join("revs").join(rev.to_string()).join("embeddings.json");
    let before = std::fs::metadata(&artifact).unwrap().modified().unwrap();
    assert_eq!(first, run_search(&dir, "authorize"));
    assert_eq!(before, std::fs::metadata(&artifact).unwrap().modified().unwrap());
    std::fs::remove_file(&artifact).unwrap();
    assert_eq!(first, run_search(&dir, "authorize"));
    assert_eq!(rev, store.last_rev());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn exhausted_external_provider_budget_uses_offline_and_timeout_is_explicit() {
    let dir = tmp_dir("budget");
    std::fs::write(dir.join("app.py"), "def authorize():\n    return True\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["search", dir.to_str().unwrap(), "authorize", "--llm-provider", "ollama", "--llm-budget-usd", "0", "--json"])
        .env("OLLAMA_HOST", "http://127.0.0.1:1")
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["embedding_provider"], "offline-v1");
    assert!(v["fallback_reason"].as_str().unwrap().contains("budget"));
    assert!(!v["hits"].as_array().unwrap().is_empty());
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(["search", dir.to_str().unwrap(), "authorize", "--budget-ms", "0", "--json"])
        .output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["timed_out"], true);
    assert!(v["hits"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
