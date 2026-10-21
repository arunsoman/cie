//! Per-phase timing for `cie polyglot` on a real tree — the numbers that
//! decide what to optimize. Usage: cargo run --release -p cie-polyglot \
//! --example bench_phases -- <path>

use std::time::Instant;

fn main() {
    let raw = std::env::args().nth(1).expect("usage: bench_phases <path>");
    let path = std::path::Path::new(&raw).canonicalize().expect("canonicalize");
    let db = std::env::temp_dir().join("cie-bench-phases.db");
    let _ = std::fs::remove_file(&db);

    let conn = cie_polyglot::schema::open_db(&db).expect("open db");

    let t = Instant::now();
    let res = cie_polyglot::ir::ingest_and_lower(&conn, 1, 41, &path);
    let ingest = t.elapsed();
    let (_stats, _lower) = match res {
        Ok(x) => x,
        Err(e) => {
            eprintln!("ingest failed: {e}");
            std::process::exit(1);
        }
    };
    println!("1. ingest_and_lower   {:>10.2?}", ingest);

    let n_symbols: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))
        .unwrap_or(0);
    let n_ir: i64 = conn
        .query_row("SELECT COUNT(*) FROM ir_meta", [], |r| r.get(0))
        .unwrap_or(0);
    println!("   symbols={n_symbols} ir_meta={n_ir}");

    let t = Instant::now();
    let _ = cie_polyglot::frameworks::install_registry(&conn);
    let fw = cie_polyglot::frameworks::detect(&conn);
    println!("2. frameworks.detect  {:>10.2?}", t.elapsed());
    let _ = fw;

    let t = Instant::now();
    let c4 = cie_polyglot::c4::detect_c4(&conn, &path, 41);
    println!("3. c4.detect_c4       {:>10.2?}", t.elapsed());
    let _ = c4;

    let t = Instant::now();
    let sm = cie_polyglot::statemachines::detect(&conn);
    println!("4. statemachines      {:>10.2?}", t.elapsed());
    let _ = sm;

    let t = Instant::now();
    let wf = cie_polyglot::workflows::detect(&conn);
    println!("5. workflows          {:>10.2?}", t.elapsed());
    let _ = wf;

    let t = Instant::now();
    let dup = cie_polyglot::duplicates::detect(&conn);
    println!("6. duplicates.detect  {:>10.2?}", t.elapsed());
    match dup {
        Ok(r) => println!(
            "   shown={} (canon={} high={} med={} low={}) hidden={}",
            r.pairs, r.canonical_hash, r.high, r.medium, r.low, r.hidden_gaps
        ),
        Err(e) => eprintln!("   duplicates failed: {e}"),
    }
    let dup_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM duplicates", [], |r| r.get(0))
        .unwrap_or(0);
    println!("   duplicates table rows: {dup_rows}");
    println!("TOTAL                  {:>10.2?}", t.elapsed());

    let _ = std::fs::remove_file(&db);
}
