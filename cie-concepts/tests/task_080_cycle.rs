//! TASK-080 validation — DAG merge with cycle rejection (acceptance #29).

use cie_concepts::{ConceptDag, ConceptKind, ConceptNode, EdgeReason};
use cie_core::CieError;

fn dag_with(a_label: &str, b_label: &str) -> (ConceptDag, u32, u32) {
    let now = 0;
    let mut dag = ConceptDag::default();
    let a = dag.push(ConceptNode::new(0, a_label, ConceptKind::Aggregate, now));
    let b = dag.push(ConceptNode::new(0, b_label, ConceptKind::Module, now));
    (dag, a, b)
}

#[test]
fn task_080_merge_is_dag_cycles_rejected() {
    // A -> B legal; B -> A would close a cycle → rejected.
    let (mut dag, a, b) = dag_with("Cart", "Items");
    dag.try_link_child(a, b, EdgeReason::PackageContainment)
        .expect("legal edge");
    let err = dag
        .try_link_child(b, a, EdgeReason::ClusterCohesion)
        .expect_err("B->A closes a cycle");
    assert!(matches!(err, CieError::Cycle(_)));
    assert!(err.to_string().contains("cycle"));

    // Self-edge rejected.
    assert!(matches!(
        dag.try_link_child(a, a, EdgeReason::CallOwnership),
        Err(CieError::Cycle(_))
    ));

    // The DAG survived rejection intact: A still has exactly one child.
    assert_eq!(dag.get(a).unwrap().children, vec![b]);
    assert_eq!(dag.get(b).unwrap().parent, Some(a));

    // Deeper chain: A -> B -> C; C -> A rejected.
    let now = 0;
    let c = dag.push(ConceptNode::new(0, "Qty", ConceptKind::Value, now));
    dag.try_link_child(b, c, EdgeReason::FcaSubsumption)
        .unwrap();
    assert!(matches!(
        dag.try_link_child(c, a, EdgeReason::CallOwnership),
        Err(CieError::Cycle(_))
    ));

    // Legal extra edges keep their traceable reason (FR-42): D under both
    // A (primary) and C (extra, with CallOwnership reason preserved).
    let d = dag.push(ConceptNode::new(0, "Line", ConceptKind::Entity, now));
    dag.try_link_child(a, d, EdgeReason::PackageContainment)
        .unwrap();
    dag.try_link_child(c, d, EdgeReason::CallOwnership).unwrap();
    assert_eq!(dag.get(d).unwrap().parent, Some(a));
    assert_eq!(
        dag.get(d).unwrap().extra_edges,
        vec![(c, EdgeReason::CallOwnership)],
        "EdgeReason preserved on legal extra edges"
    );
}
