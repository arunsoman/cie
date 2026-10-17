//! TASK-030 validation: per-file summary reuse keyed by
//! (blake3(file), blake3(dep set)). An unchanged file with an unchanged
//! dependency set reuses its stored summaries; a changed file is recomputed
//! and the fixpoint re-propagates into its transitive reverse-deps.

use cie_cli::engine::{self, IngestOptions};
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t030-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

const LIB: &str = "def query_db(q):\n    return db.execute(q)\n";
const AUTH: &str = "import lib\n\ndef authorize(user, token):\n    lib.query_db(\"select\")\n    print(\"ok\")\n    return True\n";
const MAIN: &str = "import auth\n\ndef run(u, t):\n    return auth.authorize(u, t)\n";
const UTILS: &str = "def fmt(x):\n    return str(x)\n";

#[test]
fn task_030_summary_reuse_counters_on_partial_edit() {
    let dir = tmp_dir("counters");
    fs::write(dir.join("lib.py"), LIB).unwrap();
    fs::write(dir.join("auth.py"), AUTH).unwrap();
    fs::write(dir.join("main.py"), MAIN).unwrap();
    fs::write(dir.join("utils.py"), UTILS).unwrap();

    // First ingest: no prior cache — reuse cannot fire yet.
    let first = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(first.summary_reuse.reused_files, 0);
    assert_eq!(first.summary_reuse.recomputed_files, 0, "no prior keys");

    let auth_effects_before = first
        .snapshot
        .summaries
        .iter()
        .find(|s| s.sym == first.store.resolve_qual("auth.authorize").unwrap())
        .map(|s| (s.effects.clone(), s.callees.clone()))
        .unwrap();

    // Partial edit: only utils.py changes. lib/auth/main keep their file
    // hashes AND dep-set hashes → their summaries are reused.
    fs::write(dir.join("utils.py"), "def fmt(x):\n    return str(x).strip()\n").unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(
        second.summary_reuse.reused_files, 3,
        "lib/auth/main are (file, deps)-unchanged"
    );
    assert_eq!(
        second.summary_reuse.recomputed_files, 1,
        "only the edited file recomputes"
    );
    assert!(second.snapshot.summary_reuse.is_some());

    // Reused summaries are value-identical for untouched files.
    let auth_effects_after = second
        .snapshot
        .summaries
        .iter()
        .find(|s| s.sym == second.store.resolve_qual("auth.authorize").unwrap())
        .map(|s| (s.effects.clone(), s.callees.clone()))
        .unwrap();
    assert_eq!(auth_effects_before, auth_effects_after);

    // Changing a dependency invalidates dependents via the dep-set hash.
    fs::write(dir.join("lib.py"), "def query_db(q):\n    return db.execute(q)\n\ndef query_log():\n    return db.query(\"log\")\n").unwrap();
    let third = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(
        third.summary_reuse.reused_files, 2,
        "main.py (deps auth unchanged) and utils.py (no deps) reuse"
    );
    assert_eq!(
        third.summary_reuse.recomputed_files, 2,
        "lib.py changed; auth.py's dep set (lib) changed"
    );
}

/// TASK-030: a full rewrite (every file touched) recomputes everything.
#[test]
fn task_030_full_edit_recomputes_all() {
    let dir = tmp_dir("full");
    fs::write(dir.join("a.py"), "def a1():\n    return 1\n").unwrap();
    fs::write(dir.join("b.py"), "def b1():\n    return 2\n").unwrap();
    let first = engine::run_ingest(&dir, &opts()).unwrap();
    let _ = first;

    fs::write(dir.join("a.py"), "def a1():\n    return 11\n").unwrap();
    fs::write(dir.join("b.py"), "def b1():\n    return 22\n").unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(second.summary_reuse.reused_files, 0);
    assert_eq!(second.summary_reuse.recomputed_files, 2);
}
