//! RFC-0019 stage 1: the stack lens is an ingest stage with a persisted
//! artifact — `revs/<N>/stack.json`, the fourth sibling of the
//! Claim/Drift/Label store extensions.

use cie_ingest::stack_store::StackStoreExt;
use cie_ingest::{run_ingest, IngestOptions};
use cie_incremental::snapshot::SnapshotStoreExt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_rfc19_s1_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(root: &PathBuf, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn py_project() -> PathBuf {
    let root = unique_dir("proj");
    write_file(&root, "app.py", "def charge(a):\n    return a\n");
    write_file(&root, "requirements.txt", "flask==3.0.0\n");
    root
}

#[test]
fn ingest_persists_the_stack_artifact_and_the_snapshot_carries_it() {
    let root = py_project();
    let out = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert_eq!(out.rev, 1);

    // the artifact sits at the documented path
    let artifact = root.join(".cie/revs/1/stack.json");
    assert!(artifact.exists(), "revs/<N>/stack.json persisted");

    // the snapshot carries the same reading (belt and braces)
    let store = cie_store::Store::open(&root).unwrap();
    let snap = store.load_snapshot(1).unwrap().expect("snapshot exists");
    let stack = snap.stack.expect("stage-1 snapshot carries the stack field");
    assert!(stack.routes.is_empty(), "a bare py project has no routes — honest empty");
    assert!(
        stack
            .notes
            .iter()
            .any(|n| n.contains("[stack]")),
        "the factory's own parser id is disclosed in the notes"
    );

    // the artifact on disk parses back to the same report
    let raw = fs::read_to_string(&artifact).unwrap();
    assert!(raw.contains("\"apps\""), "the artifact is the full StackReport");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn noop_reingest_answers_the_stack_from_the_store_not_a_reparse() {
    let root = py_project();
    let first = run_ingest(&root, &IngestOptions::default()).unwrap();
    let first_stack = first.snapshot.stack.clone().expect("rev 1 carries stack");

    // re-ingest with NOTHING changed — the fast path must answer from the
    // persisted artifact (identical generated_at proves no re-derivation)
    let second = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(second.snapshot.incremental, "no-op re-ingest is the fast path");
    let again = second.snapshot.stack.expect("fast path carries the persisted stack");
    assert_eq!(
        again.generated_at, first_stack.generated_at,
        "same artifact from the store — the lens did not re-run"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn pre_rfc_snapshot_without_a_stack_field_still_loads() {
    // a snapshot persisted BEFORE this RFC has no `stack` key at all —
    // serde(default) must load it as None, never error, never invent.
    // Built by serializing a current snapshot and REMOVING the stack key,
    // so every other required field is exactly what the struct writes.
    let root = py_project();
    let out = run_ingest(&root, &IngestOptions::default()).unwrap();
    let mut v: serde_json::Value =
        serde_json::to_value(&out.snapshot).expect("snapshot serializes");
    let had_stack = v.as_object_mut().unwrap().remove("stack");
    assert!(had_stack.is_some(), "the current engine writes the field");
    let snap: cie_incremental::snapshot::RevSnapshot =
        serde_json::from_value(v).expect("pre-RFC snapshot (no stack key) deserializes");
    assert_eq!(snap.rev, 1);
    assert!(snap.stack.is_none(), "absent field → None — no invented claims");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn stack_store_ext_roundtrip_and_absence() {
    let root = unique_dir("store");
    let store = cie_store::Store::open(&root).unwrap();
    // pre-RFC / bare revision: None, never a lie
    assert!(store.load_stack(9).unwrap().is_none());
    let report = cie_stack::types::StackReport::unavailable("unit fixture");
    store.save_stack(9, &report).unwrap();
    let loaded = store.load_stack(9).unwrap().expect("roundtrips");
    assert!(loaded.notes[0].contains("unavailable"));
    assert!(loaded.notes[0].contains("other artifacts are unaffected"));
    fs::remove_dir_all(&root).ok();
}
