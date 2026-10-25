//! Ingest a tree into a PERSISTENT db and dump every table in deterministic
//! order — the before/after equivalence artifact for ingest-side rewrites
//! (ids included, since insertion order is deterministic). Sub-phase timings
//! go to stderr so stdout stays pure data.
//!
//! Usage: ingest_dump <source-path> <db-path>

use std::time::Instant;

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn main() {
    let raw = std::env::args()
        .nth(1)
        .expect("usage: ingest_dump <path> <db>");
    let path = std::path::Path::new(&raw)
        .canonicalize()
        .expect("canonicalize");
    let db = std::env::args()
        .nth(2)
        .expect("usage: ingest_dump <path> <db>");
    let _ = std::fs::remove_file(&db);

    let conn = cie_polyglot::schema::open_db(std::path::Path::new(&db)).expect("open db");

    let t = Instant::now();
    cie_polyglot::ensure_all_schemas(&conn).expect("schemas");
    let mut st = cie_polyglot::schema::IngestStats::default();
    cie_polyglot::schema::ingest_folder(&conn, 1, 41, &path, &mut st).expect("ingest_folder");
    let t_ingest = t.elapsed();

    let t = Instant::now();
    cie_polyglot::ir::store_sources(&conn, &path).expect("store_sources");
    let t_sources = t.elapsed();

    let t = Instant::now();
    let rep = cie_polyglot::ir::lower_all(&conn).expect("lower_all");
    let t_lower = t.elapsed();

    eprintln!(
        "ingest_folder {:>9.2?} | store_sources {:>9.2?} | lower_all {:>9.2?} | files={} ast={} syms={} lowered={}",
        t_ingest, t_sources, t_lower, st.files_ingested, st.ast_nodes, st.symbols, rep.symbols_lowered
    );

    let w = std::io::BufWriter::new(std::io::stdout().lock());
    let mut out = w;
    dump_tables(&conn, &mut out);
    let _ = std::fs::remove_file(&db);
}

fn dump_tables(conn: &rusqlite::Connection, out: &mut dyn std::io::Write) {
    // Written as plain loops for clarity.

    let mut s = conn
        .prepare("SELECT id, name, language FROM node_kinds ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "node_kinds\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, String>(1).unwrap(),
            r.get::<_, Option<String>>(2).unwrap().unwrap_or_default()
        );
    }

    let mut s = conn
        .prepare("SELECT id, content, hash, kind_hint FROM node_text ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "node_text\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, String>(1).unwrap(),
            hex(&r.get::<_, Vec<u8>>(2).unwrap()),
            r.get::<_, Option<String>>(3).unwrap().unwrap_or_default()
        );
    }

    let mut s = conn
        .prepare("SELECT id, name FROM uast_kinds ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "uast_kinds\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, String>(1).unwrap()
        );
    }

    let mut s = conn
        .prepare(
            "SELECT ast_kind_id, uast_kind_id, confidence, provenance FROM uast_map ORDER BY id",
        )
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "uast_map\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, f64>(2).unwrap(),
            r.get::<_, String>(3).unwrap()
        );
    }

    let mut s = conn
        .prepare("SELECT id, repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version FROM files ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "files\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, String>(2).unwrap(),
            hex(&r.get::<_, Vec<u8>>(3).unwrap()),
            r.get::<_, String>(4).unwrap(),
            r.get::<_, i64>(5).unwrap(),
            r.get::<_, i64>(6).unwrap(),
            r.get::<_, String>(7).unwrap()
        );
    }

    let mut s = conn
        .prepare("SELECT id, file_id, parent_id, kind_id, child_index, start_byte, end_byte, start_row, start_col, end_row, end_col, is_named, is_error, is_missing, text_id, shape_hash, uast_kind_id FROM ast_nodes ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "ast_nodes\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, Option<i64>>(2).unwrap().map_or(-1, |v| v),
            r.get::<_, i64>(3).unwrap(),
            r.get::<_, i64>(4).unwrap(),
            r.get::<_, i64>(5).unwrap(),
            r.get::<_, i64>(6).unwrap(),
            r.get::<_, i64>(7).unwrap(),
            r.get::<_, i64>(8).unwrap(),
            r.get::<_, i64>(9).unwrap(),
            r.get::<_, i64>(10).unwrap(),
            r.get::<_, i64>(11).unwrap(),
            r.get::<_, i64>(12).unwrap(),
            r.get::<_, i64>(13).unwrap(),
            r.get::<_, Option<i64>>(14).unwrap().map_or(-1, |v| v),
            r.get::<_, Option<Vec<u8>>>(15)
                .unwrap()
                .map_or(String::new(), |b| hex(&b)),
            r.get::<_, Option<i64>>(16).unwrap().map_or(-1, |v| v)
        );
    }

    let mut s = conn
        .prepare("SELECT id, file_id, ast_node_id, name, qual_name, kind, parent_symbol_id, start_byte, end_byte, sig_end_byte, subtree_hash, annotations FROM symbols ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "symbols\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, i64>(2).unwrap(),
            r.get::<_, String>(3).unwrap(),
            r.get::<_, String>(4).unwrap(),
            r.get::<_, String>(5).unwrap(),
            r.get::<_, Option<i64>>(6).unwrap().map_or(-1, |v| v),
            r.get::<_, i64>(7).unwrap(),
            r.get::<_, i64>(8).unwrap(),
            r.get::<_, Option<i64>>(9).unwrap().map_or(-1, |v| v),
            hex(&r.get::<_, Vec<u8>>(10).unwrap()),
            r.get::<_, Option<String>>(11).unwrap().unwrap_or_default()
        );
    }

    let mut s = conn
        .prepare("SELECT file_id, revision, zstd_bytes FROM file_source_cache ORDER BY file_id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let blob: Vec<u8> = r.get(2).unwrap();
        let _ = writeln!(
            out,
            "file_source_cache\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            blob.len(),
            hex(&blake3::hash(&blob).as_bytes()[..])
        );
    }

    let mut s = conn
        .prepare("SELECT id, symbol_id, block_index, kind FROM ir_blocks ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "ir_blocks\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, i64>(2).unwrap(),
            r.get::<_, String>(3).unwrap()
        );
    }

    let mut s = conn
        .prepare("SELECT id, block_id, inst_index, op, operands, ast_node_id, span_start, span_end FROM ir_instructions ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "ir_instructions\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, i64>(2).unwrap(),
            r.get::<_, String>(3).unwrap(),
            hex(&r.get::<_, Vec<u8>>(4).unwrap()),
            r.get::<_, Option<i64>>(5).unwrap().map_or(-1, |v| v),
            r.get::<_, i64>(6).unwrap(),
            r.get::<_, i64>(7).unwrap()
        );
    }

    let mut s = conn
        .prepare("SELECT from_block_id, to_block_id, kind FROM ir_edges ORDER BY from_block_id, to_block_id, kind")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "ir_edges\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, String>(2).unwrap()
        );
    }

    let mut s = conn
        .prepare("SELECT id, file_id, call_node_id, caller_symbol_id, callee_symbol_id, callee_text, resolution, stmt_index, block_id, call_kind, throws_set, guard_summary FROM calls ORDER BY id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "calls\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, i64>(2).unwrap(),
            r.get::<_, i64>(3).unwrap(),
            r.get::<_, Option<i64>>(4).unwrap().map_or(-1, |v| v),
            r.get::<_, Option<String>>(5).unwrap().unwrap_or_default(),
            r.get::<_, String>(6).unwrap(),
            r.get::<_, i64>(7).unwrap(),
            r.get::<_, Option<i64>>(8).unwrap().map_or(-1, |v| v),
            r.get::<_, String>(9).unwrap(),
            r.get::<_, Option<Vec<u8>>>(10)
                .unwrap()
                .map_or(String::new(), |b| hex(&b)),
            r.get::<_, Option<String>>(11).unwrap().unwrap_or_default()
        );
    }

    let mut s = conn
        .prepare("SELECT symbol_id, ir_hash FROM ir_meta ORDER BY symbol_id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "ir_meta\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            hex(&r.get::<_, Vec<u8>>(1).unwrap())
        );
    }
}
