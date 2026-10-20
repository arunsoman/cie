use cie_core::types::FileId;
use cie_ingest::{load_outcome, navigation::ensure_navigation, run_ingest, IngestOptions};

#[test]
fn historical_citations_survive_source_edits_but_missing_graph_cannot_use_changed_source() {
    let dir = std::env::temp_dir().join(format!("cie-rfc22-historical-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.py"),
        "def helper():\n    return 1\n\ndef main():\n    return helper()\n",
    )
    .unwrap();
    let first = run_ingest(&dir, &IngestOptions::default()).unwrap();
    let rev = first.rev;
    std::fs::write(
        dir.join("app.py"),
        "# different source and offsets\n# another line\ndef unrelated():\n    return 2\n",
    )
    .unwrap();
    let mut old = load_outcome(&dir, rev).unwrap();
    let main = old.store.symbols.iter().find(|s| s.name == "main").unwrap();
    assert_eq!(old.store.files.path(main.loc.file), "app.py");
    assert_eq!(
        old.store.files.line(main.loc),
        4,
        "coordinates belong to the recorded revision"
    );
    let cached = ensure_navigation(&dir, &mut old).unwrap();
    assert!(
        cached.edge_count() > 0,
        "an existing historical graph requires no current source"
    );
    let artifact = dir
        .join(".cie/revs")
        .join(rev.to_string())
        .join("relations.json");
    std::fs::remove_file(&artifact).unwrap();
    let error = ensure_navigation(&dir, &mut old).unwrap_err();
    assert!(error.to_string().contains("source changed since revision"));
    assert!(
        !artifact.exists(),
        "failed repair must not publish the wrong revision graph"
    );
    // Legacy source tables must not reconstruct historical lines from new bytes.
    std::fs::remove_file(
        dir.join(".cie/revs")
            .join(rev.to_string())
            .join("file-table.json"),
    )
    .unwrap();
    let old = load_outcome(&dir, rev).unwrap();
    assert_eq!(old.store.files.path(FileId(0)), "app.py");
    let main = old.store.symbols.iter().find(|s| s.name == "main").unwrap();
    assert_eq!(
        old.store.files.line(main.loc),
        0,
        "unknown is safer than invented old offsets"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
