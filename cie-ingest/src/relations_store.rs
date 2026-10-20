//! RFC-0022 §6.1 — RelationGraph persistence: the one real call-graph
//! substrate (`cie-relations::RelationGraph`/`Pdg`, TASK-007/009) becomes
//! a fifth sibling of the Claim/Drift/Label/Stack store extensions,
//! persisted per revision as `<repo>/.cie/revs/<N>/relations.json`.
//!
//! Before this, `relation_graph_from_facts` had exactly one call site in
//! the whole workspace — a test. The trait shape mirrors `StackStoreExt`
//! (`cie-ingest/src/stack_store.rs`) so a maintainer who reads one
//! already understands the other.

use cie_core::types::RevId;
use cie_relations::RelationGraph;
use cie_store::Store;

/// Extension trait: per-revision relation-graph persistence over the
/// generic store. `RelationGraph` already has hand-written `Serialize`/
/// `Deserialize` impls (`cie-relations/src/lib.rs`), so this is a plain
/// JSON round-trip, no new wire format.
pub trait RelationGraphStoreExt {
    /// Persist the graph for `rev` (`revs/<N>/relations.json`), whole.
    fn save_relations(&self, rev: RevId, graph: &RelationGraph) -> std::io::Result<()>;
    /// Load the graph persisted for `rev`, when this revision has one
    /// (`None` = pre-RFC revision or a bare folder — compute-if-absent).
    fn load_relations(&self, rev: RevId) -> std::io::Result<Option<RelationGraph>>;
}

impl RelationGraphStoreExt for Store {
    fn save_relations(&self, rev: RevId, graph: &RelationGraph) -> std::io::Result<()> {
        self.write_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("relations.json"),
            graph,
        )
    }

    fn load_relations(&self, rev: RevId) -> std::io::Result<Option<RelationGraph>> {
        self.read_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("relations.json"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::SymbolId;
    use cie_relations::RelKind;

    #[test]
    fn relations_artifact_roundtrips_per_rev() {
        let dir = std::env::temp_dir().join(format!("cie-relations-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = cie_store::Store::open(&dir).unwrap();
        let rev: RevId = 3;

        // absent before save — a pre-RFC revision answers None, never lies
        assert!(store.load_relations(rev).unwrap().is_none());

        let mut g = RelationGraph::new();
        let a = g.add_node(SymbolId(1));
        let b = g.add_node(SymbolId(2));
        g.add_edge(a, b, RelKind::Calls);
        store.save_relations(rev, &g).unwrap();

        let loaded = store.load_relations(rev).unwrap().expect("saved graph loads");
        assert_eq!(loaded.node_count(), 2);
        assert_eq!(loaded.edge_count(), 1);
        let a2 = loaded.find_node(SymbolId(1)).unwrap();
        let b2 = loaded.find_node(SymbolId(2)).unwrap();
        assert_eq!(
            loaded.edges_from(a2).collect::<Vec<_>>(),
            vec![(b2, RelKind::Calls)]
        );

        assert!(dir
            .join(".cie")
            .join("revs")
            .join("3")
            .join("relations.json")
            .exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
