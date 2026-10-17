//! TASK-078 validation — concept persistence split across the six
//! `concept_node:{id}:*` keys, with roundtrip + per-key file checks.

use cie_core::concept::{
    BehaviorRef, ConceptDag, ConceptKind, ConceptNode, EdgeReason, RuleRef,
};
use cie_storage::Store;
use std::fs;

fn tmpdir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t078-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    d
}

fn sample_dag() -> ConceptDag {
    let now = 42;
    let mut dag = ConceptDag::default();
    let root = dag.push(ConceptNode::new(0, "Cart", ConceptKind::Aggregate, now));
    let item = dag.push(ConceptNode::new(0, "CartItem", ConceptKind::Entity, now));
    let qty = dag.push(ConceptNode::new(0, "Quantity", ConceptKind::Value, now));
    let line = dag.push(ConceptNode::new(0, "Line", ConceptKind::UseCase, now));

    dag.get_mut(item).unwrap().primary_edge = EdgeReason::PackageContainment;
    dag.try_link_child(root, item, EdgeReason::PackageContainment).unwrap();
    dag.try_link_child(item, qty, EdgeReason::FcaSubsumption).unwrap();
    // Multi-parent: Line under root (primary) and item (extra, traceable).
    dag.get_mut(line).unwrap().primary_edge = EdgeReason::PackageContainment;
    dag.try_link_child(root, line, EdgeReason::PackageContainment).unwrap();
    dag.try_link_child(item, line, EdgeReason::CallOwnership).unwrap();

    dag.get_mut(item).unwrap().behaviors = vec![BehaviorRef {
        summary: cie_core::SymbolId(7),
        kind: "Add item to cart".into(),
        confidence: 0.9,
    }];
    dag.get_mut(item).unwrap().symbols = vec![cie_core::SymbolId(7), cie_core::SymbolId(9)];
    dag.get_mut(root).unwrap().rules = vec![RuleRef {
        contract: 3,
        kind: "no-negative-qty".into(),
        authority: cie_core::contract::Authority::Verified,
    }];
    dag.unattached_behaviors = vec![BehaviorRef {
        summary: cie_core::SymbolId(11),
        kind: "orphan behavior".into(),
        confidence: 0.4,
    }];
    dag
}

#[test]
fn task_078_six_keys_roundtrip() {
    let dir = tmpdir("roundtrip");
    let store = Store::open(&dir).unwrap();
    let dag = sample_dag();
    store.save_concept_six(1, &dag).unwrap();

    // All six keys exist per node, plus the DAG-level unattached record.
    for id in 0..4u32 {
        for key in [
            "node.json",
            "behaviors.json",
            "rules.json",
            "parent.json",
            "children.json",
            "rollup.json",
        ] {
            let p = dir.join(".cie/revs/1/concepts").join(id.to_string()).join(key);
            assert!(p.exists(), "missing key file {key} for node {id}");
        }
    }
    assert!(dir.join(".cie/revs/1/concepts/unattached.json").exists());

    let back = store.load_concept_six(1).unwrap().expect("dag rebuilt");
    assert_eq!(back.nodes.len(), dag.nodes.len());

    // Semantic equality: identity, links with reasons, payloads.
    for (orig, got) in dag.nodes.iter().zip(back.nodes.iter()) {
        assert_eq!(orig.id, got.id);
        assert_eq!(orig.label, got.label);
        assert_eq!(orig.kind, got.kind);
        assert_eq!(orig.parent, got.parent);
        assert_eq!(orig.primary_edge, got.primary_edge);
        assert_eq!(orig.symbols, got.symbols);
        assert_eq!(orig.behaviors, got.behaviors);
        assert_eq!(orig.rules, got.rules);
        let orig_children: std::collections::BTreeSet<u32> =
            orig.children.iter().copied().collect();
        let got_children: std::collections::BTreeSet<u32> =
            got.children.iter().copied().collect();
        assert_eq!(orig_children, got_children, "children of {}", orig.label);
        let orig_extra: std::collections::BTreeSet<(u32, EdgeReason)> =
            orig.extra_edges.iter().copied().collect();
        let got_extra: std::collections::BTreeSet<(u32, EdgeReason)> =
            got.extra_edges.iter().copied().collect();
        assert_eq!(
            orig_extra, got_extra,
            "extra edges with reasons of {}",
            orig.label
        );
    }
    assert_eq!(
        back.unattached_behaviors, dag.unattached_behaviors,
        "unattached behaviors roundtrip"
    );
}

/// The `:rollup` key exposes the aggregate view without recomputation.
#[test]
fn task_078_rollup_key_carries_aggregate() {
    let dir = tmpdir("rollup");
    let store = Store::open(&dir).unwrap();
    let mut dag = sample_dag();
    dag.rollup();
    store.save_concept_six(2, &dag).unwrap();
    let rollup: cie_storage::RollupView = serde_json::from_str(
        &fs::read_to_string(dir.join(".cie/revs/2/concepts/0/rollup.json")).unwrap(),
    )
    .unwrap();
    let root = dag.get(0).unwrap();
    assert_eq!(
        rollup.behaviors,
        root.behaviors.iter().map(|b| b.summary).collect::<Vec<_>>()
    );
    assert_eq!(rollup.confidence, root.confidence);
}
