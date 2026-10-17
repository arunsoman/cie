//! TASK-079 validation — the `cart` fixture (three-level concept hierarchy
//! with mixed binding sources) + binding-source coverage: all four sources
//! attach and `LlmCandidate` stays inert (never Explicit/Verified).

use cie_cli::engine::{self, IngestOptions};
use cie_core::contract::Authority;
use cie_core::findings::FindingKind;
use std::path::{Path, PathBuf};

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t079-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .canonicalize()
        .expect("fixtures dir present")
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

/// Three-level hierarchy: Domain root → Module nodes → Entity/UseCase/Rule
/// leaves, over the cart fixture.
#[test]
fn task_079_cart_three_level_hierarchy() {
    let dir = tmp_dir("levels");
    let src = fixtures_root().join("cart");
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let dag = &outcome.snapshot.concepts;

    assert!(dag.nodes.len() > 3, "cart produces a real hierarchy");

    // Level 1: a single Domain root.
    let roots: Vec<_> = dag
        .nodes
        .iter()
        .filter(|n| n.parent.is_none())
        .collect();
    assert_eq!(roots.len(), 1, "one domain root");
    assert_eq!(roots[0].kind, cie_core::concept::ConceptKind::Domain);

    // Level 2: Module nodes for the three files hang off the root.
    let modules: Vec<_> = dag
        .nodes
        .iter()
        .filter(|n| n.kind == cie_core::concept::ConceptKind::Module)
        .collect();
    assert!(modules.len() >= 3, "cart.py/items.py/pricing.py modules");
    for m in &modules {
        assert_eq!(m.parent, Some(roots[0].id), "module under the domain root");
    }

    // Level 3: leaves (entities/rules) under modules or the root.
    let leaves: Vec<_> = dag
        .nodes
        .iter()
        .filter(|n| {
            !matches!(
                n.kind,
                cie_core::concept::ConceptKind::Domain
                    | cie_core::concept::ConceptKind::Module
            ) && n.parent.is_some()
        })
        .collect();
    assert!(!leaves.is_empty(), "third level exists");

    // Depth check: root -> module -> leaf = 3 levels.
    let depth_of = |mut id: u32| -> usize {
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
    let max_depth = dag.nodes.iter().map(|n| depth_of(n.id)).max().unwrap();
    assert!(max_depth >= 3, "three-level hierarchy, got depth {max_depth}");
}

/// All four binding sources attach across the pipeline; the LLM channel
/// binds as inert LlmCandidate.
#[test]
fn task_079_all_four_binding_sources_attach() {
    let dir = tmp_dir("sources");
    let src = fixtures_root().join("cart");
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for c in &outcome.snapshot.contracts {
        seen.insert(c.authority.as_str().to_string());
    }
    for f in &outcome.snapshot.findings {
        seen.insert(f.authority.as_str().to_string());
    }
    // Explicit: docstring contracts on Cart.add_item/total.
    assert!(seen.contains("explicit"), "docstring contract binds Explicit");
    // InferredCandidate: naming-based rules (charge → authorization).
    assert!(
        seen.contains("inferred_candidate"),
        "naming rules bind InferredCandidate"
    );
    // Verified: mechanically checked findings (dead code on list_purchases).
    assert!(
        outcome
            .snapshot
            .findings
            .iter()
            .any(|f| f.kind == FindingKind::DeadCode
                && f.authority == Authority::Verified),
        "checker findings bind Verified"
    );

    // The LLM channel: an LLM-proposed label binds an LlmCandidate rule.
    let store = &outcome.store;
    let mut summaries = std::collections::BTreeMap::new();
    for s in &outcome.snapshot.summaries {
        summaries.insert(s.sym, s.clone());
    }
    let llm_labeler: cie_analysis::hierarchy::LabelFn =
        &|_intent, fallback| (format!("LLM proposes: {fallback}"), "llm");
    let beh = cie_analysis::behavior::compute_summaries(store, &[], 1, None);
    let dag = cie_analysis::hierarchy::build_hierarchy(
        store,
        &beh,
        &outcome.snapshot.contracts,
        llm_labeler,
        0,
    );
    let llm_rules: Vec<_> = dag
        .nodes
        .iter()
        .flat_map(|n| n.rules.iter())
        .filter(|r| r.authority == Authority::LlmCandidate)
        .collect();
    assert!(
        !llm_rules.is_empty(),
        "LLM-proposed labels bind LlmCandidate rules"
    );

    // Coverage: all four sources now attach somewhere in the engine model.
    seen.insert("llm_candidate".to_string());
    for a in Authority::ALL_SOURCES {
        assert!(
            seen.contains(a.as_str()),
            "binding source {} did not attach",
            a.as_str()
        );
    }
    assert_eq!(Authority::ALL_SOURCES.len(), 4);
}

/// FR-44 / acc #28: LlmCandidate is inert — never promotes to decisive.
#[test]
fn task_079_llm_candidate_inert() {
    use cie_core::contract::Authority;
    // The promotion API refuses LlmCandidate → Explicit/Verified.
    assert!(Authority::LlmCandidate.promote_to(Authority::Explicit).is_err());
    assert!(Authority::LlmCandidate.promote_to(Authority::Verified).is_err());
    // Non-LLM hypotheses may be re-targeted freely (they are re-derived).
    assert!(Authority::InferredCandidate.promote_to(Authority::Verified).is_ok());
    // LlmCandidate never decides conformance on its own.
    assert!(!Authority::LlmCandidate.is_decisive());
    assert!(Authority::Verified.is_decisive() && Authority::Explicit.is_decisive());
}
