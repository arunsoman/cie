//! TASK-009 validation sketch — instructor contract, bodies intact.
//! Single documented mapping: `cie_relations` → `cie_core::relations`; the
//! sketch's `SymbolId`-keyed graph is materialized through `add_node` (the
//! delivered `RelationGraph` is `NodeIndex`-addressed per §14.3).

use cie_core::SymbolId;
use cie_relations::{Pdg, RelKind, RelationGraph};
use petgraph::graph::NodeIndex;

fn node(n: u32) -> SymbolId {
    SymbolId(n)
}

fn fow_fixture() -> RelationGraph {
    // 1: entry | 2: cond (x > 0) | 3: y := f(x), then-branch | 4: use(y) after join
    let mut g = RelationGraph::new();
    let n1 = g.add_node(node(1));
    let n2 = g.add_node(node(2));
    let n3 = g.add_node(node(3));
    let n4 = g.add_node(node(4));
    g.add_edge(n1, n2, RelKind::ControlFlow);
    g.add_edge(n2, n3, RelKind::ControlFlow);
    g.add_edge(n3, n4, RelKind::DataFlow);
    g.add_edge(n2, n4, RelKind::ControlFlow);
    g
}

#[test]
fn task_009_pdg_backward_slice_fow() {
    let fixture = fow_fixture();
    let pdg = Pdg::build(&fixture);
    // Provenance: build is pure — the input graph is untouched.
    assert_eq!(fixture.node_count(), 4);
    assert_eq!(fixture.edge_count(), 4);

    let slice = pdg.backward_slice(node(4));
    assert!(
        slice.contains(&node(3)),
        "data dependence: value flows 3 → 4"
    );
    assert!(
        slice.contains(&node(2)),
        "control dependence: 4 guarded by condition 2"
    );

    let bounded = pdg.backward_slice_bounded(node(4), 1);
    assert_eq!(bounded.len(), 1, "FR-7: budget caps slice work");
    assert!(bounded.contains(&node(4)));
}

/// TASK-009 DoD: the slice of an unconditional statement contains only
/// data-dependence ancestors (control deps stay with the guarded statement).
#[test]
fn task_009_unconditional_slice_data_only() {
    let pdg = Pdg::build(&fow_fixture());
    // The branch condition (2) is unconditional after entry: its slice
    // must contain no control ancestors (entry is virtual).
    let slice = pdg.backward_slice(node(2));
    assert!(slice.contains(&node(2)));
    assert!(
        !slice
            .iter()
            .any(|n| pdg.control_edges.contains(&(*n, node(2)))),
        "unconditional statement: no control ancestors (entry is virtual)"
    );
    assert!(
        pdg.entry_trivial.contains(&node(1)),
        "entry has no CF predecessor"
    );

    // Provenance stamping variant.
    let pdg_rev = Pdg::build_with_rev(&fow_fixture(), 42);
    assert_eq!(pdg_rev.rev, 42, "FR-4: PDG records the input rev");
}

/// TASK-009 DoD: bounded slice with budget 1 returns exactly the seed node.
#[test]
fn task_009_deterministic_slices() {
    let pdg = Pdg::build(&fow_fixture());
    let s1 = pdg.backward_slice(node(4));
    let s2 = pdg.backward_slice(node(4));
    assert_eq!(s1, s2, "FR-4: same revision ⇒ same slice, sorted set");
    assert_eq!(
        pdg.backward_slice_bounded(node(4), 0).len(),
        0,
        "budget 0: empty"
    );
    assert_eq!(
        pdg.backward_slice_bounded(node(4), 2).len(),
        2,
        "budget 2: seed + 1"
    );
}

// NodeIndex re-used for readability of future fixtures.
#[allow(unused)]
fn _ix(n: usize) -> NodeIndex<u32> {
    NodeIndex::new(n)
}
