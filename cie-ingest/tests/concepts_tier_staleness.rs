//! RFC-0024 (concepts-tier staleness): the no-op ingest path must not serve
//! a concepts tree built by an older engine forever. A head snapshot that
//! predates CONCEPTS_TIER_VERSION (i.e. without the stack-fed C4 tiers)
//! sends the fast path to ONE full pass, which rebuilds the concepts —
//! Container + Service/Subdomain nodes included — and re-stamps the marker;
//! after that the fast path holds again, unchanged.

use cie_ingest::{run_ingest, CONCEPTS_TIER_VERSION, IngestOptions};
use cie_incremental::snapshot::RevSnapshot;
use cie_incremental::snapshot::SnapshotStoreExt;
use cie_store::Store;
use std::io::Write;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_tier_stale_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(root: &PathBuf, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn kinds(dag: &cie_concepts::ConceptDag) -> std::collections::BTreeMap<String, usize> {
    let mut m = std::collections::BTreeMap::new();
    for n in &dag.nodes {
        *m.entry(format!("{:?}", n.kind)).or_insert(0) += 1;
    }
    m
}

#[test]
fn a_pre_tier_snapshot_is_rebuilt_once_and_re_stamps_the_marker() {
    let root = unique_dir("proj");
    write_file(&root, "app.py", "def charge(a):\n    return a\n");
    write_file(&root, "requirements.txt", "flask==3.0.0\n");

    let opts = IngestOptions::default();

    // Full ingest #1 — the current engine: concepts built WITH the tier,
    // snapshot stamped.
    let out1 = run_ingest(&root, &opts).expect("ingest 1");
    let store = Store::open(&root).expect("store");
    let snap = store
        .load_snapshot(out1.rev)
        .expect("load snapshot 1")
        .expect("snapshot 1 exists");
    assert_eq!(snap.concepts_tier, Some(CONCEPTS_TIER_VERSION));
    let k1 = kinds(&snap.concepts);
    assert!(k1.get("Container").copied().unwrap_or(0) >= 1, "tier is in tree: {k1:?}");

    // Simulate an older engine: strip the marker (serde loads missing fields
    // as None) and keep everything else — the honest stand-in for a snapshot
    // that predates the tier.
    let old = RevSnapshot {
        concepts_tier: None,
        ..snap
    };
    store.save_snapshot(&old).unwrap();

    // Ingest #2 on an UNCHANGED tree: the plan is empty, but the head
    // snapshot predates the marker — the fast path must NOT serve it.
    let out2 = run_ingest(&root, &opts).expect("ingest 2");
    let store2 = Store::open(&root).expect("store 2");
    let snap2 = store2
        .load_snapshot(out2.rev)
        .expect("load snapshot 2")
        .expect("snapshot 2 exists");
    assert_eq!(
        snap2.concepts_tier,
        Some(CONCEPTS_TIER_VERSION),
        "the rebuild re-stamps the marker"
    );
    assert!(
        kinds(&snap2.concepts).get("Container").copied().unwrap_or(0) >= 1,
        "the rebuilt concepts carry the tier again"
    );

    // Ingest #3, tree still unchanged: the marker is current — the fast
    // path holds (no third revision, the head stays).
    let out3 = run_ingest(&root, &opts).expect("ingest 3");
    assert_eq!(out3.rev, out2.rev, "no-op ingest reuses the head revision");
    assert!(out3.snapshot.incremental, "the fast path ran");

    std::fs::remove_dir_all(&root).ok();
}