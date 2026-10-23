//! BRIDGE parity gate (backlog item 5, un-deferred): `cie ingest` in its
//! new DEFAULT mode (facts-from polyglot) must produce byte-identical
//! artifacts to the own-parse path. This is the hard precondition the
//! worklog attached to any default switch — the default IS switched here,
//! so this gate runs against the real pipeline on every `cargo test`.
//!
//! Two identical repos are built in a temp dir:
//!   * repo_a gets a real polyglot store (`cie_polyglot::ir::ingest_and_lower`
//!     — the exact call the CLI's Polyglot command makes), then a bridged
//!     ingest (default mode);
//!   * repo_b gets today's disk-only ingest (`--facts-from own`);
//!   * repo_b ALSO gets a bridged ingest with no store at its path — the
//!     store is keyed to repo_a's canonical path, so this proves the
//!     fallback and that one project's store never leaks into another's.
//!
//! Everything except the run-dependent `ingest_wall_ns` and the bridge's
//! own disclosure (`warnings`) must match byte-for-byte: the returned
//! snapshot, AND the persisted `.cie/revs/1/snapshot.json` — the artifact
//! `cie report` actually serves.

use std::path::{Path, PathBuf};

use cie_ingest::{FactsSource, IngestOptions};

/// Fixture: four files inside polyglot's 10-language set (its store holds
/// their bytes) plus two outside it (Kotlin, SQL — disk fallback), exercising
/// both sourcing paths in one run. Real code, real cross-file calls.
fn write_fixture(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("db")).unwrap();

    std::fs::write(
        dir.join("src/main.py"),
        "import lib\n\n\ndef authorize(user, token):\n    lib.query_db(\"select * from users\")\n    print(\"ok\")\n    return True\n\n\ndef run_check(name):\n    return authorize(name, \"t\")\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("src/util.rs"),
        "pub fn compute(x: u32) -> u32 {\n    let doubled = x * 2;\n    log_it(doubled);\n    doubled\n}\n\npub fn log_it(v: u32) -> u32 {\n    println!(\"{}\", v);\n    v\n}\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("src/handler.ts"),
        "import { store } from \"./lib\";\n\nexport function handle(req: string): boolean {\n  const saved = store(req);\n  console.log(saved);\n  return true;\n}\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("src/svc.go"),
        "package main\n\nimport \"fmt\"\n\nfunc Run(u string) bool {\n\tfmt.Println(u)\n\treturn Authorize(u)\n}\n\nfunc Authorize(u string) bool {\n\treturn u != \"\"\n}\n",
    )
    .unwrap();

    // Outside polyglot's language set — the disk-fallback path.
    std::fs::write(
        dir.join("src/extra.kt"),
        "fun greet(name: String): String {\n    println(\"hi $name\")\n    return name\n}\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("db/schema.sql"),
        "CREATE TABLE users (\n    id INTEGER PRIMARY KEY,\n    email TEXT NOT NULL\n);\n\nCREATE VIEW active_users AS SELECT id FROM users;\n",
    )
    .unwrap();
}

fn unique_tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie-bridge-parity-{}-{}",
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Build the polyglot store at the convention path for `root` — the same
/// open_db + ingest_and_lower the CLI's Polyglot command runs, followed by
/// the same WAL checkpoint + sidecar cleanup the build-then-rename
/// promotion does, so the bridge reads exactly what a real `cie polyglot`
/// leaves behind.
fn seed_polyglot_store(root: &Path) {
    let db_path = cie_polyglot::schema::default_db_path(root);
    let conn = cie_polyglot::schema::open_db(&db_path).expect("open polyglot store");
    let (_, _) =
        cie_polyglot::ir::ingest_and_lower(&conn, 1, 41, root).expect("polyglot ingest_and_lower");
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint");
    drop(conn);
    for suffix in ["-wal", "-shm"] {
        let mut p = db_path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(std::path::PathBuf::from(p));
    }
}

/// Zero the fields that legitimately differ between runs: the wall clock,
/// the bridge's own disclosure, and the analysis-side serial noise set
/// (concept creation stamps, coverage wall) — the same noise set PERF-L1
/// named when it proved artifact identity. Everything else must be equal.
fn normalized(snap: &mut cie_incremental::RevSnapshot) {
    snap.ingest_wall_ns = 0;
    snap.warnings = vec![];
    snap.coverage.wall_ns = 0;
    snap.evidence.generated_at_ns = 0;
    if let Some(st) = &mut snap.stack {
        st.generated_at = 0;
    }
    for node in &mut snap.concepts.nodes {
        node.created_ns = 0;
    }
    for s in &mut snap.summaries {
        s.compute_ns = 0;
    }
}

fn persisted_snapshot(repo: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(repo.join(".cie/revs/1/snapshot.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["ingest_wall_ns"] = serde_json::json!(0);
    v["warnings"] = serde_json::json!([]);
    if let Some(ev) = v.get_mut("evidence") {
        ev["generated_at_ns"] = serde_json::json!(0);
    }
    if let Some(sums) = v.get_mut("summaries").and_then(|s| s.as_array_mut()) {
        for s in sums {
            s["compute_ns"] = serde_json::json!(0);
        }
    }
    if let Some(cov) = v.get_mut("coverage") {
        cov["wall_ns"] = serde_json::json!(0);
    }
    if let Some(st) = v.get_mut("stack") {
        st["generatedAt"] = serde_json::json!(0);
    }
    if let Some(nodes) = v.get_mut("concepts").and_then(|c| c.get_mut("nodes")) {
        if let Some(arr) = nodes.as_array_mut() {
            for n in arr {
                n["created_ns"] = serde_json::json!(0);
            }
        }
    }
    v
}

#[test]
fn bridged_ingest_is_byte_identical_to_own_parse() {
    let tmp = unique_tmp("main");
    let repo_a = tmp.join("repo_a");
    let repo_b = tmp.join("repo_b");
    write_fixture(&repo_a);
    write_fixture(&repo_b);

    // 1. repo_a gets a real polyglot store, then the DEFAULT (bridged) ingest.
    seed_polyglot_store(&repo_a);
    let outcome_a = cie_ingest::run_ingest(&repo_a, &IngestOptions::default()).unwrap();

    // The receipt names the store and the split: 4 sources verified from
    // the store, 2 (Kotlin, SQL) honestly from disk.
    let rcpt = outcome_a.source_receipt.as_ref().expect("receipt");
    assert_eq!(rcpt.from_store, 4, "four polyglot-language sources from the store");
    assert_eq!(rcpt.from_disk, 2, "kotlin + sql fall back to disk");
    assert_eq!(
        rcpt.store_path.as_deref(),
        Some(cie_polyglot::schema::default_db_path(&repo_a).as_path()),
        "receipt names the convention-path store"
    );
    let disclosure = outcome_a.snapshot.warnings.join("\n");
    assert!(
        disclosure.contains("facts-from polyglot:") && disclosure.contains("4 of 6"),
        "snapshot carries the persistent disclosure, got: {disclosure}"
    );

    // 2. repo_b, own-parse (today's path, the parity baseline).
    let own_opts = IngestOptions {
        facts_from: FactsSource::Own,
        ..IngestOptions::default()
    };
    let outcome_b = cie_ingest::run_ingest(&repo_b, &own_opts).unwrap();
    let rcpt_b = outcome_b.source_receipt.as_ref().expect("receipt");
    assert_eq!(rcpt_b.store_path, None, "own mode never touches the store");
    assert_eq!(rcpt_b.from_disk, 6);
    assert!(
        outcome_b.snapshot.warnings.is_empty(),
        "own mode keeps today's snapshot bytes (no disclosure)"
    );

    // 3. THE GATE — returned snapshots identical modulo wall/warnings.
    let (mut snap_a, mut snap_b) = (outcome_a.snapshot.clone(), outcome_b.snapshot.clone());
    normalized(&mut snap_a);
    normalized(&mut snap_b);
    assert_eq!(
        serde_json::to_string_pretty(&snap_a).unwrap(),
        serde_json::to_string_pretty(&snap_b).unwrap(),
        "bridged vs own-parse snapshots must be byte-identical (modulo ingest_wall_ns + warnings)"
    );

    // 4. And the PERSISTED artifact — what `cie report` actually serves.
    let (disk_a, disk_b) = (persisted_snapshot(&repo_a), persisted_snapshot(&repo_b));
    assert_eq!(
        disk_a, disk_b,
        "persisted .cie/revs/1/snapshot.json must be byte-identical (modulo normalized fields)"
    );

    // 5. The store is PATH-KEYED: a bridged ingest of a repo with NO store
    // at its convention path (repo_c — repo_b's own snapshot is already at
    // rev 1, so a re-run there would hit the fast path) falls back to disk
    // for all six — one project's store never leaks into another's
    // ingestion.
    let repo_c = tmp.join("repo_c");
    write_fixture(&repo_c);
    let outcome_c = cie_ingest::run_ingest(
        &repo_c,
        &IngestOptions {
            facts_from: FactsSource::Polyglot,
            ..IngestOptions::default()
        },
    )
    .unwrap();
    let rcpt_c = outcome_c.source_receipt.as_ref().expect("receipt");
    assert_eq!(rcpt_c.store_path, None, "no store seeded for repo_c");
    assert_eq!(rcpt_c.from_disk, 6, "all sources from disk without a store");

    // 6. And repo_c's artifacts still match repo_b's own-parse artifacts.
    let (mut snap_c, mut snap_b_own) = (outcome_c.snapshot.clone(), outcome_b.snapshot.clone());
    normalized(&mut snap_c);
    normalized(&mut snap_b_own);
    assert_eq!(
        serde_json::to_string_pretty(&snap_c).unwrap(),
        serde_json::to_string_pretty(&snap_b_own).unwrap(),
        "bridged-without-store must equal own-parse exactly"
    );

    let _ = std::fs::remove_dir_all(&tmp);
    // Clean the seeded store so the temp namespace doesn't accumulate.
    let _ = std::fs::remove_file(cie_polyglot::schema::default_db_path(&repo_a));
}

#[test]
fn default_db_path_matches_the_cli_convention() {
    // FNV-1a 64 over the canonical path, $TMPDIR/cie-polyglot-<h:x>.db —
    // the exact construction in cie-cli Command::Polyglot and the web's
    // fnv1a64Hex. Pin it with an independently computed vector.
    let dir = unique_tmp("fnv");
    let repo = dir.join("some repo");
    std::fs::create_dir_all(&repo).unwrap();

    let canon = repo.canonicalize().unwrap();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in canon.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let expected = std::env::temp_dir().join(format!("cie-polyglot-{h:x}.db"));
    assert_eq!(
        cie_polyglot::schema::default_db_path(&repo),
        expected,
        "bridge must resolve the SAME store path the CLI promotes and the web reads"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
