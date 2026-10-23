//! Shared-tree parity, incremental reuse/deletion and memory-bound skip gates.
use cie_ingest::{default_registry, stream::StreamSession, IngestOptions};
use std::path::PathBuf;

fn temp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "cie-stream-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
const FIXTURES: &[(&str, &str)] = &[
    (
        "a.py",
        "def save(x):\n    return x\n\ndef run(x):\n    return save(x)\n",
    ),
    ("a.ts", "export function save(x: number) { return x + 1; }"),
    (
        "a.tsx",
        "export function View() { return <main>Hello</main>; }",
    ),
    ("a.js", "function save(x) { return x + 1; }"),
    (
        "a.go",
        "package main\nfunc Save(x int) int { return x + 1 }",
    ),
    ("a.rs", "pub fn save(x: u32) -> u32 { x + 1 }"),
    ("A.java", "class A { int save(int x) { return x + 1; } }"),
    ("a.c", "int save(int x) { return x + 1; }"),
    ("a.cpp", "class A { int save(int x) { return x + 1; } };"),
    ("A.cs", "class A { int Save(int x) { return x + 1; } }"),
    ("a.php", "<?php function save($x) { return $x + 1; }"),
    ("a.kt", "fun save(x: Int): Int { return x + 1 }"),
    ("a.lua", "function save(x) return x + 1 end"),
    (
        "a.sql",
        "CREATE TABLE wallets (id INT PRIMARY KEY, balance DECIMAL(12,2));",
    ),
    ("a.sh", "#!/bin/sh\necho ready\n"),
];
fn clean(p: &std::path::Path) {
    let db = cie_polyglot::schema::default_db_path(p);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    let _ = std::fs::remove_dir_all(p);
}
#[test]
fn shared_trees_preserve_all_language_adapter_facts() {
    let root = temp("parity");
    let registry = default_registry();
    for (path, src) in FIXTURES {
        let adapter = registry.adapter_for(path).unwrap();
        if let Some(grammar) = adapter.grammar(path) {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&grammar).unwrap();
            let tree = parser.parse(src, None).unwrap();
            assert_eq!(
                serde_json::to_value(adapter.extract(path, src)).unwrap(),
                serde_json::to_value(adapter.extract_tree(path, src, &tree)).unwrap(),
                "{path}"
            );
        }
        std::fs::write(root.join(path), src).unwrap();
    }
    let session = StreamSession::capture(&root, &registry, &IngestOptions::default()).unwrap();
    assert_eq!(session.receipt.files_read, FIXTURES.len());
    assert_eq!(session.receipt.trees_parsed, 13); // 12 grammar languages + TSX; SQL/Shell use text.
    let legacy: Vec<_> = FIXTURES
        .iter()
        .map(|(p, s)| {
            (
                p.to_string(),
                registry.adapter_for(p).unwrap().id(),
                s.to_string(),
            )
        })
        .collect();
    let old = cie_adapters::extract_project(&legacy, &registry);
    let new = session.extracted.as_ref().unwrap();
    // Different input order gets deterministic path order in the stream.
    let mut sorted = legacy;
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let old_sorted = cie_adapters::extract_project(&sorted, &registry);
    assert_eq!(
        serde_json::to_value(&old_sorted.per_file).unwrap(),
        serde_json::to_value(&new.per_file).unwrap()
    );
    assert_eq!(old.store.symbols.len(), new.store.symbols.len());
    drop(session);
    clean(&root);
}
#[test]
fn changes_reuse_other_facts_and_deletions_remove_ast_ir_and_symbols() {
    let root = temp("changes");
    std::fs::write(root.join("a.py"), FIXTURES[0].1).unwrap();
    std::fs::write(root.join("b.rs"), FIXTURES[5].1).unwrap();
    let first = cie_ingest::run_ingest(&root, &IngestOptions::default()).unwrap();
    assert_eq!(first.source_receipt.as_ref().unwrap().trees_parsed, 2);
    let cached =
        StreamSession::capture(&root, &default_registry(), &IngestOptions::default()).unwrap();
    assert_eq!(cached.receipt.trees_parsed, 0);
    assert_eq!(cached.receipt.facts_reused, 2);
    drop(cached);
    std::fs::write(root.join("a.py"), "def changed(x):\n    return x + 5\n").unwrap();
    let second = cie_ingest::run_ingest(&root, &IngestOptions::default()).unwrap();
    assert_eq!(second.source_receipt.as_ref().unwrap().facts_reused, 1);
    assert_eq!(second.source_receipt.as_ref().unwrap().trees_parsed, 1);
    std::fs::remove_file(root.join("b.rs")).unwrap();
    let third = cie_ingest::run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(!third.snapshot.files.iter().any(|p| p == "b.rs"));
    let db = rusqlite::Connection::open(cie_polyglot::schema::default_db_path(&root)).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM files WHERE path='b.rs'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    let revision: String = db
        .query_row(
            "SELECT value FROM ingest_meta WHERE key='revision'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revision, third.rev.to_string());
    let mut q = db.prepare("PRAGMA foreign_key_check").unwrap();
    assert!(q.query([]).unwrap().next().unwrap().is_none());
    drop(q);
    drop(db);
    clean(&root);
}
#[test]
fn binary_oversized_and_invalid_utf8_are_disclosed() {
    let root = temp("bounds");
    std::fs::write(root.join("binary.py"), b"\0binary").unwrap();
    std::fs::write(root.join("invalid.py"), [0xff, 0xfe]).unwrap();
    let f = std::fs::File::create(root.join("large.py")).unwrap();
    f.set_len(17 * 1024 * 1024).unwrap();
    let session =
        StreamSession::capture(&root, &default_registry(), &IngestOptions::default()).unwrap();
    assert_eq!(session.receipt.skipped.len(), 3);
    assert_eq!(session.receipt.trees_parsed, 0);
    drop(session);
    clean(&root);
}
