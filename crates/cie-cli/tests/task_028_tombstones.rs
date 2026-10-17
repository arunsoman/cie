//! TASK-028 validation: tombstones + orphan purge. Deleting a file records
//! `file:{path}:tombstone` (§15.1); the next snapshot build purges its
//! symbols, summaries, findings and drops orphaned call edges. Deleting
//! then re-adding the file clears the tombstone.

use cie_cli::engine::{self, IngestOptions};
use cie_storage::Store;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t028-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

#[test]
fn task_028_delete_tombstones_and_purges_all_stale_facts() {
    let dir = tmp_dir("purge");
    fs::write(
        dir.join("auth.py"),
        "def authorize(user, token):\n    print(\"ok\")\n    return True\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.py"),
        "import auth\n\ndef run(user, token):\n    return auth.authorize(user, token)\n",
    )
    .unwrap();

    let first = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(first.snapshot.symbols.iter().any(|s| s.loc.path == "auth.py"));
    assert!(first.snapshot.symbols.iter().any(|s| s.loc.path == "main.py"));
    // main.run resolves auth.authorize → a call edge exists pre-deletion.
    assert!(first.snapshot.summaries.iter().any(|s| !s.callees.is_empty()));

    // Delete auth.py → tombstone + purge.
    fs::remove_file(dir.join("auth.py")).unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();

    let store = Store::open(&dir).unwrap();
    assert!(
        store.tombstones().unwrap().contains("auth.py"),
        "deletion must record file:auth.py:tombstone (§15.1)"
    );

    // Zero facts reference the deleted file's symbols.
    assert!(
        !second.snapshot.symbols.iter().any(|s| s.loc.path == "auth.py"),
        "stale symbols must be purged"
    );
    // No dangling references anywhere: every summary/finding/call edge
    // anchors on a symbol that exists in the post-purge snapshot.
    let live: std::collections::BTreeSet<cie_core::types::SymbolId> =
        second.snapshot.symbols.iter().map(|s| s.id).collect();
    for sum in &second.snapshot.summaries {
        assert!(
            live.contains(&sum.sym),
            "summary of a purged symbol must not survive"
        );
        for c in &sum.callees {
            assert!(
                live.contains(c),
                "orphaned call edge {c:?} must be dropped"
            );
        }
    }
    assert!(
        second
            .snapshot
            .findings
            .iter()
            .all(|f| live.contains(&f.symbol)),
        "findings anchored on purged symbols must not survive"
    );
    assert!(!second.snapshot.files.iter().any(|f| f == "auth.py"));
    assert!(second.rev > first.rev);

    // Re-adding the file clears the tombstone and restores facts.
    fs::write(
        dir.join("auth.py"),
        "def authorize(user, token):\n    print(\"ok\")\n    return True\n",
    )
    .unwrap();
    let third = engine::run_ingest(&dir, &opts()).unwrap();
    let store = Store::open(&dir).unwrap();
    assert!(!store.tombstones().unwrap().contains("auth.py"));
    assert!(third.snapshot.symbols.iter().any(|s| s.loc.path == "auth.py"));
}

/// TASK-028: purge is idempotent — running the builder again purges nothing.
#[test]
fn task_028_purge_idempotent() {
    let dir = tmp_dir("idem");
    fs::write(dir.join("a.py"), "def f():\n    return 1\n").unwrap();
    engine::run_ingest(&dir, &opts()).unwrap();
    let store = Store::open(&dir).unwrap();
    let mut snap = store.load_snapshot(store.last_rev()).unwrap().unwrap();
    assert_eq!(store.purge_tombstoned(&mut snap).unwrap(), 0, "no tombstones");
    store.mark_tombstone("a.py").unwrap();
    let purged = store.purge_tombstoned(&mut snap).unwrap();
    assert!(purged > 0);
    assert_eq!(store.purge_tombstoned(&mut snap).unwrap(), 0, "second pass: nothing left");
}
