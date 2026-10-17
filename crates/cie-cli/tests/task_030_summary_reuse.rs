//! TASK-030 + TASK-056 validation: per-file/per-symbol summary reuse keyed
//! by (blake3(file), blake3(dep set)) and the symbol-level dirty worklist.
//! An unchanged symbol whose local facts are unchanged reuses its stored
//! summary; a changed symbol and its transitive callers recompute.

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

const STORE: &str = "def query_db(q):\n    return db.execute(q)\n";
const AUTH: &str = "import store\n\ndef authorize(user, token):\n    store.query_db(\"select\")\n    print(\"ok\")\n    return True\n";
const MAIN: &str = "import auth\n\ndef run(u, t):\n    return auth.authorize(u, t)\n";
const UTILS: &str = "def fmt(x):\n    return str(x)\n";

#[test]
fn task_030_summary_reuse_counters_on_partial_edit() {
    let dir = tmp_dir("counters");
    fs::write(dir.join("store.py"), STORE).unwrap();
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

    // Partial edit: only utils.py changes. store/auth/main keep their file
    // hashes AND dep-set hashes → their summaries are reused.
    fs::write(dir.join("utils.py"), "def fmt(x):\n    return str(x).strip()\n").unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(
        second.summary_reuse.reused_files, 3,
        "store/auth/main are (file, deps)-unchanged"
    );
    assert_eq!(
        second.summary_reuse.recomputed_files, 1,
        "only the edited file recomputes"
    );
    assert!(second.snapshot.summary_reuse.is_some());
    // TASK-056 symbol counters on the leaf edit: exactly one dirty symbol.
    assert_eq!(second.summary_reuse.reused_symbols, 3);
    assert_eq!(second.summary_reuse.recomputed_symbols, 1);

    // Reused summaries are value-identical for untouched files.
    let auth_effects_after = second
        .snapshot
        .summaries
        .iter()
        .find(|s| s.sym == second.store.resolve_qual("auth.authorize").unwrap())
        .map(|s| (s.effects.clone(), s.callees.clone()))
        .unwrap();
    assert_eq!(auth_effects_before, auth_effects_after);

    // Adding a NEW symbol in store.py: the worklist invalidates exactly the
    // new symbol — query_db's facts are unchanged, so nothing else recomputes.
    fs::write(dir.join("store.py"), STORE.to_string() + "def query_log():\n    return db.query(\"log\")\n").unwrap();
    let third = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(
        third.summary_reuse.reused_symbols, 4,
        "query_db/authorize/run/fmt all seed (TASK-056: exact invalidation)"
    );
    assert_eq!(third.summary_reuse.recomputed_symbols, 1, "only the new symbol");
    // File-level rollup: store.py was touched (new symbol), the other three
    // files are (file, deps)-unchanged.
    assert_eq!(third.summary_reuse.reused_files, 3);
    assert_eq!(third.summary_reuse.recomputed_files, 1);

    // Changing query_db's body: dirty = query_db + transitive callers
    // (authorize, run); query_log and fmt keep their local facts and seed.
    fs::write(dir.join("store.py"), "def query_db(q):\n    print(\"debug\")\n    return db.execute(q)\n\ndef query_log():\n    return db.query(\"log\")\n").unwrap();
    let fourth = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(
        fourth.summary_reuse.reused_symbols, 2,
        "query_log and utils.fmt seed"
    );
    assert_eq!(
        fourth.summary_reuse.recomputed_symbols, 3,
        "query_db changed; authorize + run are transitive callers"
    );
    assert_eq!(fourth.summary_reuse.reused_files, 1, "utils.py only");
    assert_eq!(fourth.summary_reuse.recomputed_files, 3);
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
    assert_eq!(second.summary_reuse.reused_symbols, 0);
    assert_eq!(second.summary_reuse.recomputed_symbols, 2);
}
