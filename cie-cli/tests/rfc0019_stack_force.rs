//! RFC-0019 CLI: `cie stack` store-first semantics, and the --force
//! amendment — a forced re-derivation persists what it derived and syncs
//! the head snapshot's embedded copy, so the CLI, the store, and the
//! consumers can no longer disagree after a forced run.

use std::{fs, process::Command};

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_rfc19_force_{}_{}_{}",
        tag,
        std::process::id(),
        cie_core::types::now_ns()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn cie(dir: &std::path::Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_cie"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "cie {:?} failed: {}",
        args,
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn forced_rederivation_persists_and_syncs_the_snapshot() {
    let root = unique_dir("force");
    // smallest honest project: one python file + one manifest
    fs::write(root.join("app.py"), "def charge(a):\n    return a\n").unwrap();
    fs::write(root.join("requirements.txt"), "flask==3.0.0\n").unwrap();

    let path = root.to_str().unwrap().to_string();
    // 1. ingest builds revision 1 with a persisted stack artifact
    cie(&root, &["ingest", &path]);
    let artifact = root.join(".cie/revs/1/stack.json");
    assert!(artifact.exists(), "revs/1/stack.json persisted");

    // 2. doctor the persisted artifact with a stale marker (still valid JSON
    //    — only the notes change), and doctor the snapshot's copy the same way
    let doctor = |path: &std::path::Path| {
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        v["notes"] = serde_json::json!(["STALE-STACK-NOTE doctored placeholder — the forced run must replace this"]);
        fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    };
    doctor(&artifact);
    let snap_path = root.join(".cie/revs/1/snapshot.json");
    if snap_path.exists() {
        let snap_doc = fs::read_to_string(&snap_path).unwrap();
        let mut doc: serde_json::Value = serde_json::from_str(&snap_doc).unwrap();
        if doc["stack"].is_object() {
            doc["stack"]["notes"] = serde_json::json!(["STALE-STACK-NOTE doctored placeholder — the forced run must replace this"]);
            fs::write(&snap_path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        }
    }

    // 3. store-first `cie stack` honestly serves the (doctored) store bytes
    let stale = cie(&root, &["stack", &path, "--json"]);
    assert!(
        stale.contains("STALE-STACK-NOTE"),
        "store-first reads the persisted artifact as-is"
    );

    // 4. --force re-derives AND persists: the CLI, the artifact, and the
    //    snapshot's embedded copy all lose the stale marker
    let fresh = cie(&root, &["stack", &path, "--json", "--force"]);
    assert!(
        !fresh.contains("STALE-STACK-NOTE"),
        "forced derivation is not the stale store copy"
    );
    let persisted = fs::read_to_string(&artifact).unwrap();
    assert!(
        !persisted.contains("STALE-STACK-NOTE"),
        "--force persisted over the doctored artifact"
    );
    if snap_path.exists() {
        let snap = fs::read_to_string(&snap_path).unwrap();
        assert!(
            !snap.contains("STALE-STACK-NOTE"),
            "--force synced the snapshot's embedded stack copy"
        );
    }

    // 5. a following store-first run agrees with the forced run — one story
    //    (the CLI prints the file's bytes plus a trailing newline)
    let again = cie(&root, &[
        "stack", &path, "--json",
    ]);
    assert_eq!(
        again.trim_end_matches('\n'),
        persisted.trim_end_matches('\n'),
        "the store-first answer is the persisted bytes"
    );

    // 6. a no-op re-ingest keeps the forced artifact (its inputs never changed)
    cie(&root, &["ingest", &path]);
    let after = fs::read_to_string(&artifact).unwrap();
    assert!(!after.contains("STALE-STACK-NOTE"), "the forced artifact survives");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn force_on_a_bare_folder_creates_no_store() {
    let root = unique_dir("bare");
    fs::write(root.join("app.py"), "def greet():\n    print('hi')\n").unwrap();
    let path = root.to_str().unwrap().to_string();
    let out = cie(&root, &["stack", &path, "--json", "--force"]);
    assert!(out.contains("\"languages\""), "bare-folder force still derives");
    assert!(
        !root.join(".cie").exists(),
        "no store materializes for a bare folder (nothing to update)"
    );
    let _ = fs::remove_dir_all(&root);
}