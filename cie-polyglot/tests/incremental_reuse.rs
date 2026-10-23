//! INCREMENTAL-ON (backlog item 6) — the reuse tier's acceptance gate.
//!
//! The contract under test:
//! * a warm rebuild of an UNCHANGED tree is row-for-row the full rebuild —
//!   every table, parse tier and derivations alike (DS-EVAL honesty: reuse
//!   changes nothing);
//! * a warm rebuild of a changed tree re-parses exactly the changed/new
//!   subset, drops gone paths, and keeps referential integrity;
//! * the reuse decision is hash-verified per file — a tampered grammar
//!   version or content forces the re-parse;
//! * a parse-error file carries its honesty flag through the copy tier;
//! * an unusable previous store degrades to a full parse with a named note.

use std::path::PathBuf;

use cie_polyglot::schema::{open_db, IngestStats};
use cie_polyglot::{ir, schema};
use rusqlite::types::ValueRef;
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

const SOURCES: &[(&str, &str)] = &[
    (
        "cart.py",
        "class Cart:\n    def add_item(self, item):\n        return item\n\ndef total(cart):\n    return 42\n",
    ),
    (
        "price.rs",
        "struct Price { cents: u64 }\ntrait Calc { fn compute(&self) -> u64; }\nfn price_of(p: &Price) -> u64 { p.cents }\n",
    ),
    ("add.go", "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n"),
    (
        "handler.ts",
        "interface Handler { handle(x: string): void }\nclass Cart { add(item: string): void {} }\n",
    ),
    ("Cart.java", "public class Cart {\n    public int total() { return 1; }\n}\n"),
    ("cart.js", "class Cart { add(item) {} }\nfunction total() { return 1; }\n"),
    ("total.c", "int total(int x) { return x; }\n"),
    ("total.php", "<?php\nfunction total($c) { return 1; }\n"),
];

/// A parse-error file: the honesty rule says it still indexes, flagged.
const BROKEN_PY: &str = "def broken(:\n";

fn write_tree(root: &std::path::Path, files: &[(&str, &str)]) {
    for (rel, src) in files {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, src).unwrap();
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cie-incr-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Full pipeline build into a fresh store file (the cold path the CLI runs).
fn build_cold(
    db: &std::path::Path,
    root: &std::path::Path,
) -> (IngestStats, ir::LowerReport, schema::ReuseReport) {
    let conn = open_db(db).unwrap();
    let out = ir::ingest_and_lower_reusing(&conn, 1, 41, root, None).unwrap();
    drop(conn);
    out
}

/// Full pipeline build reusing a previous store file (the warm path).
fn build_warm(
    db: &std::path::Path,
    root: &std::path::Path,
    prev: &std::path::Path,
) -> (IngestStats, ir::LowerReport, schema::ReuseReport) {
    let conn = open_db(db).unwrap();
    let out = ir::ingest_and_lower_reusing(&conn, 1, 41, root, Some(prev)).unwrap();
    drop(conn);
    out
}

/// Every user table, every row, every column — serialized in a stable order.
/// This is the parity artifact: warm rebuild vs full rebuild must produce
/// byte-identical dumps when the tree is unchanged.
fn dump_all(db: &std::path::Path) -> String {
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open store for dump");
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let mut out = String::new();
    for t in &tables {
        out.push_str(&format!("== {t}\n"));
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{t}\" ORDER BY rowid"))
            .unwrap();
        let ncols = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for c in 0..ncols {
                let cell = match row.get_ref(c).unwrap() {
                    ValueRef::Null => "\u{2205}".to_string(),
                    ValueRef::Integer(i) => format!("{i}"),
                    ValueRef::Real(f) => format!("{f:?}"),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                    ValueRef::Blob(b) => b.iter().map(|x| format!("{x:02x}")).collect::<String>(),
                };
                out.push_str(&cell);
                out.push('\u{1}');
            }
            out.push('\n');
        }
    }
    out
}

fn count(db: &std::path::Path, sql: &str) -> i64 {
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

// ---------------------------------------------------------------------------
// the acceptance gate
// ---------------------------------------------------------------------------

/// THE acceptance: warm rebuild of an unchanged tree == full rebuild,
/// row for row, across EVERY table — parse tier (files/ast/symbols/kinds/
/// texts) and every recomputed derivation (uast, ir, cache) alike.
#[test]
fn warm_unchanged_rebuild_is_row_for_row_the_full_build() {
    let dir = scratch("parity");
    let root = dir.join("repo");
    let mut files: Vec<(&str, &str)> = SOURCES.to_vec();
    files.push(("broken.py", BROKEN_PY)); // error files are part of the store
    std::fs::create_dir_all(&root).unwrap();
    write_tree(&root, &files);
    let n_files = files.len() as i64;

    let cold = build_cold(&dir.join("cold.db"), &root);
    let warm = build_warm(&dir.join("warm.db"), &root, &dir.join("cold.db"));
    let full_again = build_cold(&dir.join("full.db"), &root);

    // the receipt says everything was carried and nothing re-parsed
    assert_eq!(
        warm.2.files_reused as i64, n_files,
        "every file carried over"
    );
    assert_eq!(warm.2.files_parsed, 0, "no file re-parsed");
    assert_eq!(warm.2.files_dropped, 0, "nothing gone from the tree");
    assert!(warm.2.note.is_none());
    // the cold path reports the same invariant the other way
    assert_eq!(cold.2.files_reused, 0);
    assert_eq!(cold.2.files_parsed as i64, n_files);
    // stats stay honest: carried rows count exactly as parsed rows would
    assert_eq!(warm.0.files_ingested, cold.0.files_ingested);
    assert_eq!(warm.0.ast_nodes, cold.0.ast_nodes);
    assert_eq!(warm.0.symbols, cold.0.symbols);
    assert_eq!(warm.0.files_parse_errors, cold.0.files_parse_errors);
    assert_eq!(warm.0.files_parse_errors, 1, "broken.py's flag carries");
    assert_eq!(warm.0.files_reused as i64, n_files);
    // and the lowering still ran over the carried rows
    assert_eq!(warm.1.symbols_lowered, cold.1.symbols_lowered);
    assert!(warm.1.symbols_lowered > 0);

    // row-for-row equality, every table
    let warm_dump = dump_all(&dir.join("warm.db"));
    let full_dump = dump_all(&dir.join("full.db"));
    assert_eq!(
        warm_dump, full_dump,
        "warm rebuild must equal the full rebuild"
    );
    // and the full rebuild is deterministic against the first cold build
    assert_eq!(full_dump, dump_all(&dir.join("cold.db")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Changed tree: only what changed re-parses; gone paths drop; carried rows
/// keep their identity; integrity survives.
#[test]
fn changed_tree_reparses_only_what_changed() {
    let dir = scratch("changed");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    write_tree(&root, SOURCES); // 8 files
    let prev_db = dir.join("prev.db");
    let _ = build_cold(&prev_db, &root);

    // mutate: one changed, one deleted, one added — the other 6 carry
    let mut v2: Vec<(String, String)> = SOURCES
        .iter()
        .filter(|(p, _)| *p != "add.go")
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
    for f in v2.iter_mut() {
        if f.0 == "price.rs" {
            f.1.push_str("\nfn extra(p: &Price) -> u64 { p.cents + 1 }\n");
        }
    }
    v2.push((
        "New.java".to_string(),
        "public class New {\n    public int go() { return 2; }\n}\n".to_string(),
    ));
    let refs: Vec<(&str, &str)> = v2.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    write_tree(&root, &refs);
    std::fs::remove_file(root.join("add.go")).unwrap(); // gone from the tree

    let warm = build_warm(&dir.join("warm.db"), &root, &prev_db);
    assert_eq!(
        warm.2.files_reused, 6,
        "unchanged files carry: {:?}",
        warm.2
    );
    assert_eq!(warm.2.files_parsed, 2, "changed price.rs + new New.java");
    assert_eq!(warm.2.files_dropped, 1, "add.go gone from the tree");
    assert_eq!(warm.0.files_ingested, 8);

    // carried rows keep their identity — same ids, same spans
    let prev_conn =
        Connection::open_with_flags(&prev_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let warm_conn = Connection::open_with_flags(
        &dir.join("warm.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for carried in ["cart.py", "handler.ts", "Cart.java"] {
        let q = |c: &Connection, path: &str| -> (i64, i64, i64) {
            c.query_row(
                "SELECT COUNT(*), MIN(start_byte), MAX(end_byte) FROM ast_nodes
                 WHERE file_id = (SELECT id FROM files WHERE path = ?1)",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        assert!(q(&prev_conn, carried).0 > 0, "{carried} exists before");
        assert_eq!(
            q(&prev_conn, carried),
            q(&warm_conn, carried),
            "{carried} rows carried verbatim"
        );
        // the file row id itself is preserved by the copy
        let id = |c: &Connection, path: &str| -> i64 {
            c.query_row("SELECT id FROM files WHERE path = ?1", [path], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(
            id(&prev_conn, carried),
            id(&warm_conn, carried),
            "{carried} keeps its file id"
        );
    }
    // the gone file is really gone; the new file is really there
    assert_eq!(
        warm_conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'add.go'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        warm_conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'New.java'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    // the changed file re-parsed under a NEW file id
    let prev_rs: i64 = prev_conn
        .query_row("SELECT id FROM files WHERE path = 'price.rs'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let warm_rs: i64 = warm_conn
        .query_row("SELECT id FROM files WHERE path = 'price.rs'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(prev_rs, warm_rs, "changed file re-parses with a fresh id");
    // referential integrity holds across the whole rebuilt store
    assert_eq!(
        warm_conn
            .query_row("PRAGMA foreign_key_check", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0),
        0,
        "no FK violations"
    );
    // lowering ran over the mixed store
    let lowered: i64 = warm_conn
        .query_row("SELECT COUNT(*) FROM ir_meta", [], |r| r.get(0))
        .unwrap();
    assert!(lowered > 0, "IR recomputed over carried + parsed rows");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The reuse decision is content-verified, never path-trusted: a tampered
/// grammar version in the previous row forces the re-parse.
#[test]
fn tampered_grammar_version_forces_reparse() {
    let dir = scratch("grammar");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    write_tree(&root, SOURCES);
    let prev_db = dir.join("prev.db");
    {
        let conn = open_db(&prev_db).unwrap();
        ir::ingest_and_lower_reusing(&conn, 1, 41, &root, None).unwrap();
        // simulate a store written by an older engine with a different pin
        conn.execute(
            "UPDATE files SET tree_sitter_grammar_version = 'tree-sitter-python 0.0' WHERE path = 'cart.py'",
            [],
        )
        .unwrap();
        drop(conn);
    }
    let warm = build_warm(&dir.join("warm.db"), &root, &prev_db);
    assert_eq!(warm.2.files_reused, SOURCES.len() - 1);
    assert_eq!(warm.2.files_parsed, 1, "cart.py re-parses: {:?}", warm.2);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A parse-error file carried by the copy tier keeps its honesty flag.
#[test]
fn parse_error_file_carries_its_flag() {
    let dir = scratch("errfile");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let mut files: Vec<(&str, &str)> = SOURCES.to_vec();
    files.push(("broken.py", BROKEN_PY));
    write_tree(&root, &files);
    let prev_db = dir.join("prev.db");
    let cold = build_cold(&prev_db, &root);
    assert_eq!(cold.0.files_parse_errors, 1);

    let warm = build_warm(&dir.join("warm.db"), &root, &prev_db);
    assert_eq!(
        warm.0.files_parse_errors, 1,
        "the flag carries with the rows"
    );
    assert_eq!(
        count(
            &dir.join("warm.db"),
            "SELECT COUNT(*) FROM ast_nodes WHERE (is_error = 1 OR is_missing = 1)
             AND file_id = (SELECT id FROM files WHERE path = 'broken.py')"
        ),
        1,
        "the error/missing node preserved verbatim"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// An unusable previous store degrades to a full parse with a named note —
/// never a failed build, never a silently stale store.
#[test]
fn unusable_previous_store_falls_back_named() {
    let dir = scratch("garbage");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    write_tree(&root, SOURCES);
    let garbage = dir.join("garbage.db");
    std::fs::write(&garbage, b"this is not a sqlite database, just noise").unwrap();

    let conn = open_db(&dir.join("warm.db")).unwrap();
    let (stats, _lower, reuse) =
        ir::ingest_and_lower_reusing(&conn, 1, 41, &root, Some(&garbage)).unwrap();
    drop(conn);
    assert!(
        reuse.note.is_some(),
        "the degradation is named: {:?}",
        reuse.note
    );
    assert_eq!(reuse.files_reused, 0);
    assert_eq!(reuse.files_parsed as usize, SOURCES.len());
    assert_eq!(
        stats.files_ingested as usize,
        SOURCES.len(),
        "the full parse still ran"
    );
    assert_eq!(
        count(&dir.join("warm.db"), "SELECT COUNT(*) FROM files"),
        SOURCES.len() as i64
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The unchanged-tree short-circuit's fingerprint: deterministic for the
/// same tree, sensitive to every change class it must catch, and round-trip
/// through a real build's store_meta stamp.
#[test]
fn tree_fingerprint_tracks_the_tree_and_round_trips() {
    let dir = scratch("fingerprint");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    write_tree(&root, SOURCES);

    let fp = schema::hash_tree(&root).unwrap();
    assert_eq!(fp.files, SOURCES.len());
    assert_eq!(
        schema::hash_tree(&root).unwrap(),
        fp,
        "deterministic for the same tree"
    );

    // content change → new fingerprint
    let (p0, prev_src) = SOURCES[0];
    std::fs::write(root.join(p0), "class Different:\n    pass\n").unwrap();
    let fp_changed = schema::hash_tree(&root).unwrap();
    assert_ne!(
        fp_changed.hash, fp.hash,
        "content change must move the fingerprint"
    );
    std::fs::write(root.join(p0), prev_src).unwrap();
    assert_eq!(
        schema::hash_tree(&root).unwrap(),
        fp,
        "restored tree fingerprints identically"
    );

    // new file → new fingerprint
    std::fs::write(root.join("new.py"), "x = 1\n").unwrap();
    let fp_new = schema::hash_tree(&root).unwrap();
    assert_ne!(fp_new.hash, fp.hash);
    assert_eq!(fp_new.files, SOURCES.len() + 1);
    std::fs::remove_file(root.join("new.py")).unwrap();

    // new directory (even empty) → new fingerprint — the C4 DDD dir checks
    // read structure, so structure is fingerprinted
    std::fs::create_dir_all(root.join("domain")).unwrap();
    assert_ne!(
        schema::hash_tree(&root).unwrap().hash,
        fp.hash,
        "a new dir moves the fingerprint"
    );
    std::fs::remove_dir(root.join("domain")).unwrap();
    assert_eq!(schema::hash_tree(&root).unwrap(), fp);

    // round-trip through a real build's store_meta stamp
    let prev_db = dir.join("prev.db");
    let _ = build_cold(&prev_db, &root);
    let stamped = schema::read_tree_fingerprint(&prev_db).expect("a stamped store carries its fp");
    assert_eq!(
        stamped, fp,
        "the build's stamp equals the tree's fingerprint"
    );

    // unusual previous stores read as None — rebuild, no short-circuit
    assert_eq!(schema::read_tree_fingerprint(&dir.join("missing.db")), None);
    let garbage = dir.join("garbage.db");
    std::fs::write(&garbage, b"not a database").unwrap();
    assert_eq!(schema::read_tree_fingerprint(&garbage), None);
    // a store predating store_meta has no row
    let bare = dir.join("bare.db");
    {
        let conn = Connection::open(&bare).unwrap();
        conn.execute_batch("CREATE TABLE t(x)").unwrap();
        drop(conn);
    }
    assert_eq!(schema::read_tree_fingerprint(&bare), None);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Chunk boundary: more than one IN-list chunk (900 ids) of reused files,
/// with shared intern rows across chunks — OR IGNORE must keep the copy
/// idempotent and the ids stable.
#[test]
fn chunked_reuse_stays_idempotent() {
    let dir = scratch("chunks");
    let root = dir.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let n = 950usize;
    let mut files: Vec<(String, String)> = Vec::with_capacity(n);
    for i in 0..n {
        files.push((
            format!("m{i:04}.py"),
            format!("def fn_{i}():\n    return {i}\n"),
        ));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();
    write_tree(&root, &refs);
    let prev_db = dir.join("prev.db");
    let cold = build_cold(&prev_db, &root);
    assert_eq!(cold.0.files_ingested, n);

    let warm = build_warm(&dir.join("warm.db"), &root, &prev_db);
    assert_eq!(warm.2.files_reused, n, "all 950 carried (two chunks)");
    assert_eq!(warm.2.files_parsed, 0);
    assert_eq!(warm.0.ast_nodes, cold.0.ast_nodes);
    assert_eq!(warm.0.symbols, cold.0.symbols);
    let warm_dump = dump_all(&dir.join("warm.db"));
    let full = build_cold(&dir.join("full.db"), &root);
    assert_eq!(full.0.files_ingested, n);
    assert_eq!(
        warm_dump,
        dump_all(&dir.join("full.db")),
        "chunked warm == full"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
