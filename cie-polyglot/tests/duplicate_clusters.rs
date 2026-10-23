use rusqlite::{params, Connection};
#[test]
fn only_canonical_edges_form_persisted_transitive_families() {
    let db = Connection::open_in_memory().unwrap();
    cie_polyglot::ensure_all_schemas(&db).unwrap();
    let source="def a(x):\n    return x+1\ndef b(x):\n    return x+2\ndef c(x):\n    return x+3\ndef d(x):\n    return x+4\n";
    cie_polyglot::schema::ingest_parsed_file(
        &db,
        1,
        7,
        "a.py",
        cie_polyglot::schema::Lang::Python,
        source,
        &mut Default::default(),
    )
    .unwrap();
    let ids: Vec<i64> = db
        .prepare("SELECT id FROM symbols WHERE kind='function' ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(ids.len(), 4);
    for (a, b, tier) in [
        (ids[0], ids[1], "canonical"),
        (ids[1], ids[2], "canonical"),
        (ids[2], ids[3], "high"),
    ] {
        db.execute("INSERT INTO duplicates(symbol_a,symbol_b,similarity,tier,detection,epistemic) VALUES (?1,?2,1,?3,'test','Fact')",params![a,b,tier]).unwrap();
    }
    assert_eq!(cie_polyglot::duplicates::cluster_canonical(&db).unwrap(), 1);
    let members: Vec<i64> = db
        .prepare("SELECT symbol_id FROM duplicate_clusters ORDER BY ordinal")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(members, ids[..3]);
    // A repeated stage replaces its result rather than accumulating members.
    assert_eq!(cie_polyglot::duplicates::cluster_canonical(&db).unwrap(), 1);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM duplicate_clusters", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        3
    );
}
