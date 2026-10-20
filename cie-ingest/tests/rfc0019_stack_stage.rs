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

// ---- RFC-0019 stage 2: stack-input hashing + invalidation rules ----

fn read_stack_generated_at(root: &PathBuf) -> Option<u64> {
    let store = cie_store::Store::open(root).unwrap();
    let rev = store.last_rev();
    store
        .load_stack(rev)
        .ok()
        .flatten()
        .map(|s| s.generated_at)
}

/// A pom-only change invalidates the stack artifact — and ONLY the stack
/// stage: the adapter lens' fast path holds (incremental re-ingest, no
/// re-analysis).
#[test]
fn stack_input_change_alone_reruns_only_the_stack_stage() {
    let root = py_project();
    let first = run_ingest(&root, &IngestOptions::default()).unwrap();
    let first_at = first.snapshot.stack.as_ref().unwrap().generated_at;
    // wait a hair so generated_at (seconds) moves if the stage re-runs
    std::thread::sleep(std::time::Duration::from_secs(2));

    // touch ONLY the manifest
    write_file(&root, "requirements.txt", "flask==3.0.1\nflask-cors==4.0.0\n");
    let second = run_ingest(&root, &IngestOptions::default()).unwrap();

    assert!(
        second.snapshot.incremental,
        "no language work — the summary-reuse fast path holds"
    );
    let second_at = second
        .snapshot
        .stack
        .as_ref()
        .expect("the refreshed stack rides the outcome")
        .generated_at;
    assert_ne!(
        first_at, second_at,
        "the stack artifact re-derived on a manifest-only change"
    );
    // the persisted artifact + snapshot agree (materialize reads the latter)
    let persisted_at = read_stack_generated_at(&root).expect("artifact on disk");
    assert_eq!(persisted_at, second_at, "stack.json refreshed in place");

    fs::remove_dir_all(&root).ok();
}

/// A language-only change re-derives the stack via §6.2 rule 1 (facts may
/// change — routes/flows live in source), and the stack-inputs diff itself
/// stays empty (the machinery must not misfire).
#[test]
fn language_change_rederives_stack_with_empty_input_diff() {
    let root = py_project();
    let first = run_ingest(&root, &IngestOptions::default()).unwrap();
    let first_at = first.snapshot.stack.as_ref().unwrap().generated_at;
    std::thread::sleep(std::time::Duration::from_secs(2));

    write_file(&root, "app.py", "def charge(a):\n    return a * 2\n");
    let second = run_ingest(&root, &IngestOptions::default()).unwrap();

    assert!(!second.snapshot.incremental, "language work → full path");
    assert!(second.plan.stack_inputs_changed.is_empty(), "manifest untouched");
    let second_at = second.snapshot.stack.as_ref().unwrap().generated_at;
    assert_ne!(first_at, second_at, "rule 1: the stack re-derives");

    fs::remove_dir_all(&root).ok();
}

/// A no-op (nothing changed anywhere) still answers from the store.
#[test]
fn noop_after_stage2_still_loads_stack_from_disk() {
    let root = py_project();
    let first = run_ingest(&root, &IngestOptions::default()).unwrap();
    let first_at = first.snapshot.stack.as_ref().unwrap().generated_at;
    let second = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(second.snapshot.incremental);
    let again = second.snapshot.stack.expect("carried");
    assert_eq!(again.generated_at, first_at, "no re-derivation on a no-op");
    fs::remove_dir_all(&root).ok();
}

/// Invalidation completeness: the detector READS settings.gradle(.kts) at
/// the repo root (it populates build.modules/tool), so a settings-gradle-only
/// change must ride the stack-inputs diff too — never a silent stale stack.
#[test]
fn settings_gradle_change_invalidates_the_stack() {
    let root = py_project();
    let first = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(
        first.snapshot.stack.as_ref().unwrap().build.modules.is_empty(),
        "no settings file yet — nothing to include"
    );

    std::thread::sleep(std::time::Duration::from_secs(2));
    write_file(&root, "settings.gradle", "include(\"extra\")\n");
    let second = run_ingest(&root, &IngestOptions::default()).unwrap();

    assert!(
        second
            .plan
            .stack_inputs_changed
            .iter()
            .any(|p| p.ends_with("settings.gradle")),
        "settings.gradle rides the stack-inputs diff"
    );
    assert!(second.snapshot.incremental, "no language work — fast path holds");
    let refreshed = second.snapshot.stack.expect("the refreshed stack rides the outcome");
    assert_eq!(
        refreshed.build.modules,
        vec!["extra".to_string()],
        "build.modules re-derived from the new settings"
    );
    let persisted_at = read_stack_generated_at(&root).expect("artifact on disk");
    assert_eq!(persisted_at, refreshed.generated_at, "stack.json refreshed in place");

    fs::remove_dir_all(&root).ok();
}
