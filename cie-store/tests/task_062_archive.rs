//! TASK-062 validation: L3 archive/restore roundtrip byte-equivalence.
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_history_10_revs()` → 10 SnapGraph revisions; revs 1..=5
//!   marked evictable via a deterministic `Evictable`.
//! - The columnar writer is in-house zstd-per-column (module docs); the
//!   byte-equivalence + index-scan gates are the binding contract.

use cie_store::archive::{ArchiveStore, Evictable};
use cie_store::snapshot::{fixture_csr_graph, SnapshotReader, SnapshotWriter};

fn tempdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie062_{}_{}_{}",
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

struct Evictable1to5;

impl Evictable for Evictable1to5 {
    fn evictable_revs(&self) -> Vec<u64> {
        (1..=5).collect()
    }
}

/// 10 revisions of history; revs 1..=5 evictable.
fn fixture_history_10_revs() -> Vec<(u64, cie_store::snapshot::SnapGraph)> {
    (1..=10)
        .map(|rev| {
            let mut g = fixture_csr_graph(50 + rev as usize);
            g.nodes[0] = rev as u32; // revision-distinguishable content
            (rev, g)
        })
        .collect()
}

/// The sketch's `task_062_archive_restore_roundtrip_is_byte_equivalent`.
#[test]
fn task_062_archive_restore_roundtrip_is_byte_equivalent() {
    let root = tempdir("main");
    let l2 = root.join("l2");
    let history = fixture_history_10_revs();

    // materialize all 10 L2 snapshots
    for (rev, g) in &history {
        SnapshotWriter::write(&l2, *rev, g).unwrap();
    }

    // archive revs 1..=5 (only evictable ones)
    let mut store = ArchiveStore::open(&root.join("l3")).unwrap();
    let archived = store
        .archive_range(1, 5, &Evictable1to5, &|rev| {
            history
                .iter()
                .find(|(r, _)| *r == rev)
                .map(|(_, g)| g.clone())
                .ok_or_else(|| format!("no history for rev {rev}"))
        })
        .expect("archive range");
    assert_eq!(archived.len(), 5, "only evictable revs archived");
    assert!(store.scan_index().scan_rev(3).is_some());
    assert!(store.scan_index().scan_rev(9).is_none(), "hot frontier untouched");

    // restore each archived revision byte-equivalently
    for rev in 1..=5u64 {
        let restored = store.restore(rev, &l2).expect("restore");
        let original = history.iter().find(|(r, _)| *r == rev).unwrap().1.clone();
        assert_eq!(restored, original, "rev {rev} byte-equivalent");

        // the restored L2 snapshot passes TASK-060 round-trip equivalence
        let reader = SnapshotReader::open(&l2, rev).unwrap();
        let back = reader.graph_for_check().unwrap();
        assert_eq!(back, original, "snapshot equivalence after restore");
    }
    std::fs::remove_dir_all(&root).ok();
}

/// Tampered archives are REFUSED — no silent partial restore (rule 2).
#[test]
fn task_062_tampered_archive_refused() {
    let root = tempdir("tamper");
    let g = fixture_csr_graph(30);
    let mut store = ArchiveStore::open(&root.join("l3")).unwrap();
    let entry = store.archive_rev(1, &g).unwrap();

    // flip a byte in the archived payload
    let p = root.join("l3").join(&entry.file);
    let mut bytes = std::fs::read(&p).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&p, &bytes).unwrap();

    let l2 = root.join("l2");
    let err = store.restore(1, &l2).unwrap_err();
    assert!(err.contains("hash mismatch") || err.contains("refused"), "got: {err}");
    std::fs::remove_dir_all(&root).ok();
}

/// `--as-of-ns`: archived revisions carry verbatim Ns timestamps (rule 1).
#[test]
fn task_062_as_of_ns_queries() {
    let root = tempdir("ns");
    let g = fixture_csr_graph(20);
    let mut store = ArchiveStore::open(&root.join("l3")).unwrap();
    store.archive_rev(1, &g).unwrap();
    let valid = store.valid_at_ns(u64::MAX);
    assert_eq!(valid, vec![1]);
    assert!(store.valid_at_ns(0).is_empty(), "nothing valid before the epoch");
    // retention metadata carries valid_from_ns verbatim
    let e = store.scan_index().scan_rev(1).unwrap();
    assert!(e.retention.valid_from_ns > 0);
    assert!(e.retention.valid_to_ns.is_none());
    std::fs::remove_dir_all(&root).ok();
}

/// §15.3 L3 gate: metadata scan of a 100-rev archive index ≤ 100 ms ×
/// CI_TOLERANCE (`#[perf]` `#[ignore]`, global rule 5).
#[test]
#[ignore = "perf: run with `cargo test --release -- --ignored` (global rule 5)"]
fn task_062_perf_scan_100rev_index_under_100ms() {
    let root = tempdir("perf");
    let g = fixture_csr_graph(64);
    let mut store = ArchiveStore::open(&root.join("l3")).unwrap();
    for rev in 1..=100u64 {
        store.archive_rev(rev, &g).unwrap();
    }
    assert_eq!(store.scan_index().len(), 100);
    // batched metadata-scan latency (open + full index scan)
    let batch = 20usize;
    let mut samples = vec![];
    for _ in 0..10 {
        let t0 = std::time::Instant::now();
        for _ in 0..batch {
            let s = ArchiveStore::open(&root.join("l3")).unwrap();
            std::hint::black_box(s.scan_index().len());
        }
        samples.push(t0.elapsed().as_nanos() as f64 / batch as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p99 = samples[(samples.len() as f64 * 0.99) as usize];
    assert!(
        p99 <= 100.0 * 1_000_000.0 * cie_core::CI_TOLERANCE,
        "§15.3 L3 index scan p99 {p99:.0} ns > 100 ms × CI_TOLERANCE"
    );
    std::fs::remove_dir_all(&root).ok();
}
