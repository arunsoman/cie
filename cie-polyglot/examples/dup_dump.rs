//! Ingest + lower a tree, run duplicates.detect, then dump the duplicates
//! output tables in deterministic order — the before/after equivalence
//! artifact for duplicates-side rewrites (ids included; insertion order is
//! deterministic). Timings go to stderr so stdout stays pure data.
//!
//! detect() reads only ir_meta + symbols + calls, so ingest + lower is the
//! complete upstream — no other pipeline phase can influence these tables.
//!
//! Usage: dup_dump <source-path> <db-path>

use std::time::Instant;

fn main() {
    let raw = std::env::args().nth(1).expect("usage: dup_dump <path> <db>");
    let path = std::path::Path::new(&raw).canonicalize().expect("canonicalize");
    let db = std::env::args().nth(2).expect("usage: dup_dump <path> <db>");
    let _ = std::fs::remove_file(&db);

    let conn = cie_polyglot::schema::open_db(std::path::Path::new(&db)).expect("open db");

    let t = Instant::now();
    cie_polyglot::ensure_all_schemas(&conn).expect("schemas");
    let mut st = cie_polyglot::schema::IngestStats::default();
    cie_polyglot::schema::ingest_folder(&conn, 1, 41, &path, &mut st).expect("ingest_folder");
    cie_polyglot::ir::store_sources(&conn, &path).expect("store_sources");
    let rep = cie_polyglot::ir::lower_all(&conn).expect("lower_all");
    eprintln!(
        "ingest+lower {:>9.2?} | files={} ast={} syms={} lowered={}",
        t.elapsed(), st.files_ingested, st.ast_nodes, st.symbols, rep.symbols_lowered
    );

    let t = Instant::now();
    let dup = cie_polyglot::duplicates::detect(&conn).expect("duplicates");
    eprintln!(
        "duplicates.detect {:>9.2?} | pairs={} canonical={} high={} medium={} low={} hidden={}",
        t.elapsed(), dup.pairs, dup.canonical_hash, dup.high, dup.medium, dup.low, dup.hidden_gaps
    );

    let w = std::io::BufWriter::new(std::io::stdout().lock());
    let mut out = w;
    dump_dup_tables(&conn, &mut out);
    let _ = std::fs::remove_file(&db);
}

fn dump_dup_tables(conn: &rusqlite::Connection, out: &mut dyn std::io::Write) {
    let mut s = conn
        .prepare(
            "SELECT id, symbol_a, symbol_b, similarity, tier, detection, epistemic,
                    canonical_symbol_id, counter_argument
             FROM duplicates ORDER BY id",
        )
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "duplicates\t{}\t{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap(),
            r.get::<_, i64>(2).unwrap(),
            r.get::<_, f64>(3).unwrap(), // {:?} = shortest round-trip, deterministic
            r.get::<_, String>(4).unwrap(),
            r.get::<_, String>(5).unwrap(),
            r.get::<_, String>(6).unwrap(),
            r.get::<_, Option<i64>>(7).unwrap().map_or(-1, |v| v),
            r.get::<_, Option<String>>(8).unwrap().unwrap_or_default()
        );
    }

    let mut s = conn
        .prepare("SELECT symbol_id, hidden_count FROM duplicate_gaps ORDER BY symbol_id")
        .unwrap();
    let mut q = s.query([]).unwrap();
    while let Some(r) = q.next().unwrap() {
        let _ = writeln!(
            out,
            "duplicate_gaps\t{}\t{}",
            r.get::<_, i64>(0).unwrap(),
            r.get::<_, i64>(1).unwrap()
        );
    }
}
