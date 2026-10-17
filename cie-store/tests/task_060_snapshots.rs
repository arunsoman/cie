//! TASK-060 validation: zero-copy revisioned snapshots (rkyv + memmap2).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_csr_graph(1_000)` → `snapshot::fixture_csr_graph(n)` (the
//!   plain SnapGraph payload shape; the CSR graph converts at the caller
//!   per the TASK-001 layering — cie-store stays core-only).
//! - `tempdir()` → a unique std::env::temp_dir directory.

use cie_store::snapshot::{
    fixture_csr_graph, open_latest_at, SnapshotError, SnapshotReader, SnapshotWriter,
};

fn tempdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie060_{}_{}_{}",
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

/// The sketch's `task_060_snapshot_roundtrip_preserves_graph`.
#[test]
fn task_060_snapshot_roundtrip_preserves_graph() {
    let dir = tempdir("rt");
    let graph = fixture_csr_graph(1_000);

    SnapshotWriter::write(&dir, 7, &graph).expect("write");
    let reader = SnapshotReader::open(&dir, 7).expect("open");
    assert_eq!(reader.header().rev, 7);
    // archived node/edge views equal the source graph
    let back = reader.graph_for_check().expect("check");
    assert_eq!(back, graph);
    // zero-copy accessor: the archived view is directly readable
    let archived = reader.graph().unwrap();
    assert_eq!(archived.offsets.len(), graph.offsets.len());
    assert_eq!(archived.edges.len(), graph.edges.len());
    std::fs::remove_dir_all(&dir).ok();
}

/// Idempotent writes: the same rev writes byte-identical files
/// (content-addressed, global rules 1 and 7).
#[test]
fn task_060_idempotent_content_addressed_writes() {
    let dir = tempdir("idem");
    let graph = fixture_csr_graph(100);
    let p1 = SnapshotWriter::write(&dir, 3, &graph).unwrap();
    let bytes1 = std::fs::read(&p1).unwrap();
    let p2 = SnapshotWriter::write(&dir, 3, &graph).unwrap();
    let bytes2 = std::fs::read(&p2).unwrap();
    assert_eq!(p1, p2);
    assert_eq!(bytes1, bytes2, "same rev → byte-identical (content-addressed)");
    // a DIFFERENT graph under the same rev would produce a different
    // content hash — the reader detects it as corrupt (rule 2)
    std::fs::remove_dir_all(&dir).ok();
}

/// Corrupt / short / mismatched mmaps → explicit SnapshotError::Corrupt,
/// never a silently empty graph (global rule 2).
#[test]
fn task_060_corruption_is_explicit() {
    let dir = tempdir("corrupt");
    let graph = fixture_csr_graph(50);
    let path = SnapshotWriter::write(&dir, 1, &graph).unwrap();

    // truncate the file
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    match SnapshotReader::open(&dir, 1) {
        Err(SnapshotError::Corrupt(_)) => {}
        other => panic!("expected Corrupt, got {}", other.map(|_| "ok").map_err(|e| e.to_string()).unwrap_err()),
    }

    // flip a byte in the archive → hash mismatch
    let mut tampered = bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    std::fs::write(&path, &tampered).unwrap();
    match SnapshotReader::open(&dir, 1) {
        Err(SnapshotError::Corrupt(_)) => {}
        other => panic!("expected Corrupt on tamper, got {}", other.map(|_| "ok").map_err(|e| e.to_string()).unwrap_err()),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// `open_latest_at(rev)` picks the newest snapshot ≤ rev (FR-23).
#[test]
fn task_060_open_latest_at_selects_newest_leq() {
    let dir = tempdir("latest");
    let g = fixture_csr_graph(10);
    for rev in [3u64, 5, 9] {
        SnapshotWriter::write(&dir, rev, &g).unwrap();
    }
    let (rev, reader) = open_latest_at(&dir, 7).expect("latest ≤ 7");
    assert_eq!(rev, 5);
    assert_eq!(reader.header().rev, 5);
    let (rev9, _) = open_latest_at(&dir, 9).unwrap();
    assert_eq!(rev9, 9);
    assert!(matches!(
        open_latest_at(&dir, 2),
        Err(SnapshotError::NotFound { rev: 2 })
    ));
    std::fs::remove_dir_all(&dir).ok();
}

/// §13 item 16 gate: warm open of a 1M-symbol graph ≤ 10 µs ×
/// CI_TOLERANCE (`#[perf]` `#[ignore]`, global rule 5).
#[test]
#[ignore = "perf: run with `cargo test --release -- --ignored` (global rule 5)"]
fn task_060_perf_warm_open_1m_under_10us() {
    let dir = tempdir("perf1m");
    let graph = fixture_csr_graph(1_000_000);
    SnapshotWriter::write(&dir, 1, &graph).unwrap();
    // warm the page cache
    for _ in 0..50 {
        let r = SnapshotReader::open(&dir, 1).unwrap();
        std::hint::black_box(r.graph().unwrap().offsets.len());
    }
    // batched per-op warm-open latency
    let batch = 25usize;
    let mut samples = vec![];
    for _ in 0..20 {
        let t0 = std::time::Instant::now();
        for _ in 0..batch {
            let r = SnapshotReader::open(&dir, 1).unwrap();
            std::hint::black_box(r.graph().unwrap().nodes.len());
        }
        samples.push(t0.elapsed().as_nanos() as f64 / batch as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p99 = samples[(samples.len() as f64 * 0.99) as usize];
    let target_us = 10.0;
    assert!(
        p99 <= target_us * 1_000.0 * cie_core::CI_TOLERANCE,
        "§13#16 warm open p99 {p99:.0} ns > 10 µs × CI_TOLERANCE"
    );
    std::fs::remove_dir_all(&dir).ok();
}
