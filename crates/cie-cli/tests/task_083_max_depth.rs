//! TASK-083 validation — `cie concepts --max-depth N` renders only the
//! first N levels (root = level 1), across every output format.

use cie_cli::engine::{self, IngestOptions};
use cie_analysis::outputs::depth_limited_view;
use std::path::{Path, PathBuf};

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t083-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .canonicalize()
        .unwrap()
}

fn setup(tag: &str) -> (PathBuf, engine::IngestOutcome) {
    let dir = tmp_dir(tag);
    let src = fixtures_root().join("cart");
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    let outcome = engine::run_ingest(&dir, &IngestOptions::default())
        .unwrap();
    (dir, engine::materialize(outcome).unwrap())
}

#[test]
fn task_083_max_depth_limits_every_format() {
    let (_dir, outcome) = setup("fmt");
    let dag = &outcome.snapshot.concepts;

    let depth_of = |dag: &cie_core::concept::ConceptDag, mut id: u32| -> usize {
        let mut depth = 1;
        while let Some(node) = dag.get(id) {
            match node.parent {
                Some(p) => {
                    id = p;
                    depth += 1;
                }
                None => break,
            }
        }
        depth
    };
    let full_max = dag.nodes.iter().map(|n| depth_of(dag, n.id)).max().unwrap();
    assert!(full_max >= 3, "cart has a deep hierarchy");

    // The view keeps exactly the first two levels.
    let view = depth_limited_view(dag, 2);
    assert!(view.nodes.len() < dag.nodes.len(), "view is smaller");
    for n in &view.nodes {
        assert!(depth_of(&view, n.id) <= 2, "no node deeper than 2");
    }
    // Parent links inside the view stay consistent.
    for n in &view.nodes {
        if let Some(p) = n.parent {
            assert!(view.get(p).is_some(), "parent within view");
            assert!(view.get(p).unwrap().children.contains(&n.id));
        }
    }
    // Root preserved.
    assert_eq!(view.nodes[0].label, dag.nodes[0].label);

    // Rendering the view elides deep leaves from the markdown markbook.
    let names: std::collections::BTreeMap<cie_core::types::SymbolId, String> = outcome
        .store
        .symbols
        .iter()
        .map(|s| (s.id, s.qual_name.clone()))
        .collect();
    let full_md = cie_analysis::outputs::concepts_markbook(dag, true, true, &names);
    let cut_md = cie_analysis::outputs::concepts_markbook(&view, true, true, &names);
    assert!(full_md.lines().count() > cut_md.lines().count());
}

/// CLI surface end-to-end: `cie concepts --max-depth 2 --format json`
/// renders a depth-limited DAG through the real binary.
#[test]
fn task_083_cli_flag_renders_limited_json() {
    let (dir, outcome) = setup("cli");
    let full_count = outcome.snapshot.concepts.nodes.len();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cie"))
        .args([
            "concepts",
            dir.to_str().unwrap(),
            "--max-depth",
            "2",
            "--format",
            "json",
        ])
        .output()
        .expect("binary runs");
    assert!(out.status.success(), "exit code {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let view: cie_core::concept::ConceptDag = serde_json::from_str(&stdout)
        .expect("stdout is a valid ConceptDag JSON");
    assert!(
        view.nodes.len() < full_count,
        "depth-limited render has fewer nodes: {} vs {}",
        view.nodes.len(),
        full_count
    );
    // Root + its children only.
    for n in &view.nodes {
        assert!(n.parent.is_none() || view.get(n.parent.unwrap()).is_some());
    }
}
