//! TASK-082 validation — parent-edge-only invalidation (acceptance #31):
//! invalidation propagates only along primary-parent edges; deep children
//! re-rollup lazily; dirty rollup equals a clean recompute.

use cie_concepts::{BehaviorRef, ConceptDag, ConceptKind, ConceptNode};

fn dag_with_leaf_tweaked() -> (ConceptDag, u32, u32, u32) {
    // root -> mid -> leaf (primary chain); root -> side (second child);
    // leaf2 under mid (a deep child that must stay lazy).
    let now = 0;
    let mut dag = ConceptDag::default();
    let root = dag.push(ConceptNode::new(0, "Domain", ConceptKind::Domain, now));
    let mid = dag.push(ConceptNode::new(0, "Module", ConceptKind::Module, now));
    let leaf = dag.push(ConceptNode::new(0, "Leaf", ConceptKind::Entity, now));
    let side = dag.push(ConceptNode::new(0, "Side", ConceptKind::Entity, now));
    let leaf2 = dag.push(ConceptNode::new(0, "Leaf2", ConceptKind::Entity, now));
    dag.link_child(root, mid);
    dag.link_child(mid, leaf);
    dag.link_child(root, side);
    dag.link_child(mid, leaf2);
    dag.get_mut(leaf).unwrap().behaviors = vec![BehaviorRef {
        summary: 1.into(),
        kind: "Add item to cart".into(),
        confidence: 0.9,
    }];
    dag.get_mut(side).unwrap().behaviors = vec![BehaviorRef {
        summary: 2.into(),
        kind: "Side behavior".into(),
        confidence: 0.8,
    }];
    dag.rollup();
    (dag, root, mid, leaf)
}

#[test]
fn task_082_invalidation_propagates_primary_parents_only() {
    let (mut dag, root, mid, leaf) = dag_with_leaf_tweaked();
    let side = 3u32;
    let leaf2 = 4u32;
    assert!(dag.dirty.is_empty(), "clean after full rollup");

    // Invalidate the leaf: the wave climbs ONLY the primary-parent chain
    // leaf -> mid -> root. Siblings (side) stay clean.
    dag.invalidate(leaf);
    assert!(dag.dirty.contains(&leaf));
    assert!(dag.dirty.contains(&mid), "primary parent invalidated");
    assert!(dag.dirty.contains(&root), "primary grandparent invalidated");
    assert!(
        !dag.dirty.contains(&leaf2) && !dag.dirty.contains(&side),
        "deep children and siblings are NOT invalidated (lazy)"
    );

    // Change the leaf payload (new behavior + lower node confidence), then
    // re-roll exactly the dirty set.
    dag.get_mut(leaf).unwrap().behaviors = vec![BehaviorRef {
        summary: 9.into(),
        kind: "Changed behavior".into(),
        confidence: 0.5,
    }];
    dag.get_mut(leaf).unwrap().confidence = 0.5;
    let side_confidence_before = dag.get(side).unwrap().confidence;
    dag.rollup_dirty();
    assert!(dag.dirty.is_empty(), "dirty set cleared");

    // Root and mid now see the new behavior through primary edges.
    assert!(dag
        .get(root)
        .unwrap()
        .behaviors
        .iter()
        .any(|b| b.summary == 9.into()));
    assert!(dag
        .get(mid)
        .unwrap()
        .behaviors
        .iter()
        .any(|b| b.summary == 9.into()));
    // Weakest-link confidence propagated along primary parents to the root.
    assert!(dag.get(root).unwrap().confidence <= 0.5 + f32::EPSILON);
    // Untouched subtree kept its stored aggregate (lazy).
    assert_eq!(dag.get(side).unwrap().confidence, side_confidence_before);
    // Deep child leaf2 still carries its STALE aggregate — it re-rolls
    // lazily when invalidated or fully rolled.
    assert!(dag.get(leaf2).unwrap().behaviors.is_empty());
    dag.rollup(); // full rollup
    assert_eq!(dag.get(root).unwrap().confidence, 0.5);
}

/// Dirty rollup equals a clean recompute when the whole changed subtree is
/// invalidated (FR-43 invariant, acc #30/#31 interplay).
#[test]
fn task_082_dirty_rollup_equals_clean_recompute() {
    let (mut dag, _root, _mid, leaf) = dag_with_leaf_tweaked();
    let snapshot = dag.clone();
    let mut clean = snapshot.clone();
    clean.rollup();

    // Simulate an edit: new behavior + lower node confidence on the leaf,
    // invalidate, dirty-roll.
    let mut edited = snapshot;
    edited.get_mut(leaf).unwrap().behaviors = vec![BehaviorRef {
        summary: 9.into(),
        kind: "Changed behavior".into(),
        confidence: 0.5,
    }];
    edited.get_mut(leaf).unwrap().confidence = 0.5;
    edited.invalidate(leaf);
    edited.rollup_dirty();

    // The same edit under a clean full rollup gives the same aggregates.
    clean.get_mut(leaf).unwrap().behaviors = vec![BehaviorRef {
        summary: 9.into(),
        kind: "Changed behavior".into(),
        confidence: 0.5,
    }];
    clean.get_mut(leaf).unwrap().confidence = 0.5;
    clean.rollup();
    assert_eq!(
        edited.get(0).unwrap().behaviors,
        clean.get(0).unwrap().behaviors,
        "root aggregates equal a clean recompute"
    );
    assert_eq!(
        edited.get(0).unwrap().confidence,
        clean.get(0).unwrap().confidence
    );
}
