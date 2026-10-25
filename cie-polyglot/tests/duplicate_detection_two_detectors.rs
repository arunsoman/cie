//! jscpd's role in duplicate detection, pinned by five scenarios.
//!
//! The engine runs TWO detectors over the same symbols, and each row's
//! `detection`/`epistemic` records which one claimed the pair:
//!
//! - `ir_hash` equality (the project's own IR lowering) — the Fact
//!   tier. Works across languages; answers "same control/data-flow shape".
//! - jscpd-rs token windows (the third-party clone matcher) — the
//!   `high`/`medium`/`low` Inference tiers. Within one format only;
//!   answers "copy-paste fragments with small edits".
//!
//! A verbatim copy is claimed by the hash tier even though jscpd would
//! also match it (canonical pairs are seeded first; jscpd never
//! re-inserts a pair it finds there). Containment pairs are excluded
//! before any detector runs — jscpd would otherwise happily pair a class
//! with its own method, whose text is a substring of the class's.

use cie_polyglot::duplicates::{create_dup_schema, detect, DupReport};
use cie_polyglot::ir::{cache_source_bytes, create_ir_schema, lower_all};
use cie_polyglot::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
use rusqlite::Connection;

fn fresh() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    create_schema(&conn).unwrap();
    create_ir_schema(&conn).unwrap();
    create_dup_schema(&conn).unwrap();
    conn
}

fn ingest(conn: &Connection, path: &str, lang: Lang, src: &str) {
    let mut st = IngestStats::default();
    let fid = ingest_parsed_file(conn, 1, 41, path, lang, src, &mut st).unwrap();
    cache_source_bytes(conn, fid, src).unwrap();
    lower_all(conn).unwrap();
}

/// The duplicate rows as (file:name, file:name, tier, detection, epistemic).
fn rows(conn: &Connection) -> Vec<(String, String, String, String, String)> {
    conn.prepare(
        "SELECT fa.path || ':' || pa.name, fb.path || ':' || pb.name, d.tier, d.detection, d.epistemic \
         FROM duplicates d \
         JOIN symbols pa ON pa.id = d.symbol_a JOIN files fa ON fa.id = pa.file_id \
         JOIN symbols pb ON pb.id = d.symbol_b JOIN files fb ON fb.id = pb.file_id",
    )
    .unwrap()
    .query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

const BODY: &str = "def process_order(qty):\n    rows = fetch(qty)\n    for r in rows:\n        if r.qty <= 0:\n            continue\n        r.price = unit(r)\n        r.tax = taxed(r)\n        audit('order', r)\n        mark(r)\n        emit(r)\n    return summary(rows)\n";

#[test]
fn verbatim_copy_is_a_fact_not_a_token_row() {
    // both detectors fire, but canonical pairs are seeded first and jscpd
    // never re-inserts a pair the hash tier already claimed
    let conn = fresh();
    ingest(&conn, "a.py", Lang::Python, BODY);
    ingest(&conn, "b.py", Lang::Python, BODY);
    let rep = detect(&conn).unwrap();
    assert_eq!(
        rep.canonical_hash, 1,
        "verbatim copy is an ir_hash twin: {rep:?}"
    );
    assert_eq!(
        rep.high + rep.medium + rep.low,
        0,
        "the token tier does not double-book: {rep:?}"
    );
    let r = rows(&conn);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].2.as_str(), r[0].3.as_str(), r[0].4.as_str()),
        ("canonical", "ir-hash-match", "Fact")
    );
}

const C: &str = "def ship_order(qty):\n    rows = fetch(qty)\n    for r in rows:\n        if r.qty <= 0:\n            continue\n        r.price = unit(r)\n        r.tax = taxed(r)\n        audit('order', r)\n        mark(r)\n        emit(r)\n        if strict:\n            gate(r)\n    return summary(rows)\n";
const D: &str = "def load_order(qty):\n    rows = fetch(qty)\n    for r in rows:\n        if r.qty <= 0:\n            continue\n        r.price = unit(r)\n        r.tax = taxed(r)\n        audit('order', r)\n        mark(r)\n        emit(r)\n        if strict:\n            gate(r)\n        if r.pending:\n            defer(r)\n    return summary(rows)\n";

#[test]
fn near_clone_with_ir_divergence_is_the_jscpd_high_tier() {
    // d copies c and adds one guard branch: the op sequences differ, so the
    // ir_hash twins never form; jscpd's windowed token matcher still sees
    // the ~90% same token body and reports the fragment pair
    let conn = fresh();
    ingest(&conn, "c.py", Lang::Python, C);
    ingest(&conn, "d.py", Lang::Python, D);
    let rep = detect(&conn).unwrap();
    assert_eq!(
        rep.canonical_hash, 0,
        "different IR shapes are not Facts: {rep:?}"
    );
    assert_eq!(rep.high, 1, "jscpd fires for the near-clone: {rep:?}");
    let r = rows(&conn);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].2.as_str(), r[0].3.as_str(), r[0].4.as_str()),
        ("high", "token-clone", "Inference")
    );
}

#[test]
fn pasted_fragment_inside_different_bodies_is_the_jscpd_low_tier() {
    // different whole-symbol ops both sides (ir_hash can never fire);
    // only the windowed token matcher sees the shared guard fragment
    let conn = fresh();
    let g = "def import_users(rows):\n    out = []\n    for r in rows:\n        if not r.get('email'):\n            continue\n        out.append(dict(r, row=r.get('email')))\n        audit('user', out)\n        emit(out)\n        mark(out)\n        purge_temp(out)\n        return out\n    return out\n";
    let h = "def cleanup_users(rows):\n    done = []\n    for r in rows:\n        if not r.get('email'):\n            continue\n        done.append(dict(r, row=r.get('email')))\n        flush(done)\n        vacuum(done)\n        return done\n    return done\n";
    ingest(&conn, "g.py", Lang::Python, g);
    ingest(&conn, "h.py", Lang::Python, h);
    let rep = detect(&conn).unwrap();
    assert_eq!(rep.canonical_hash, 0, "no whole-symbol IR twin: {rep:?}");
    assert_eq!(
        rep.low, 1,
        "the fragment clears the floor, not the top tiers: {rep:?}"
    );
    let r = rows(&conn);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].2.as_str(), r[0].3.as_str(), r[0].4.as_str()),
        ("low", "token-clone", "Inference")
    );
}

#[test]
fn cross_language_twins_are_a_fact_jscpd_can_never_see() {
    // same logic in java and python: tokens differ per format, and jscpd
    // groups files per format and compares within one only — structurally
    // blind. Identical control/data flow lowers to the same op sequence,
    // so the IR hash pair is a canonical Fact ACROSS languages.
    let conn = fresh();
    let j = "public class Calc {\n    public int calc(int x) {\n        if (x > 0) {\n            return x + 1;\n        }\n    return 0;\n    }\n}\n";
    let p = "def calc(x):\n    if x > 0:\n        return x + 1\n    return 0\n";
    ingest(&conn, "Calc.java", Lang::Java, j);
    ingest(&conn, "calc.py", Lang::Python, p);
    let rep = detect(&conn).unwrap();
    assert_eq!(
        rep.canonical_hash, 1,
        "cross-language ir_hash twin: {rep:?}"
    );
    assert_eq!(
        rep.high + rep.medium + rep.low,
        0,
        "jscpd cannot compare formats: {rep:?}"
    );
    let r = rows(&conn);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].2.as_str(), r[0].3.as_str(), r[0].4.as_str()),
        ("canonical", "ir-hash-match", "Fact")
    );
    let cross: i64 = conn
        .prepare(
            "SELECT COUNT(*) FROM duplicates d JOIN symbols sa ON sa.id=d.symbol_a JOIN files fa ON fa.id=sa.file_id JOIN symbols sb ON sb.id=d.symbol_b JOIN files fb ON fb.id=sb.file_id WHERE fa.language <> fb.language",
        )
        .unwrap()
        .query_row([], |r| r.get(0))
        .unwrap();
    assert_eq!(cross, 1, "the canonical pair spans java × python");
}

#[test]
fn containment_is_never_a_duplicate_for_either_detector() {
    // the class's own substring would be jscpd's favorite match; the kind
    // rule excludes it before any detector tier ever sees it
    let conn = fresh();
    let k = "class Repo:\n    def save(self):\n        if self.c > 0:\n            return write(self)\n        return 0\n";
    ingest(&conn, "k.py", Lang::Python, k);
    let rep = detect(&conn).unwrap();
    assert_eq!(rep.pairs, 0, "containment is not duplication: {rep:?}");
    assert_eq!(
        rep.skipped_kind_mismatch, 1,
        "the class × method pair is excluded: {rep:?}"
    );
}
