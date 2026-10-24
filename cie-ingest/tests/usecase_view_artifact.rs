//! A5/A6: the usecase-first projection reaches the persisted artifact.
//!
//! The projection is computed after the purge pass, so its symbol ids
//! speak the FINAL id space of the snapshot — every member id resolves
//! through `snapshot.symbols`, every chain id through `concepts.nodes`,
//! and every member_class's participants are a subset of the usecase's
//! extent. The concepts_tier stamp moves so a folder's cached store
//! re-derives the projection once, then the fast path holds again.

use cie_concepts::ConceptKind;
use cie_ingest::{run_ingest, IngestOptions, CONCEPTS_TIER_VERSION};
use std::io::Write;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_usecase_view_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(root: &PathBuf, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn the_projection_rides_the_artifact_with_a_consistent_id_space() {
    let root = unique_dir("proj");
    write_file(
        &root,
        "shop.py",
        // charge (method, money) + refund (loose fn, money) share one
        // attribute -> one usecase; charge's owner is the Cart class, so
        // the usecase cuts across a class node with exactly one member.
        "class Cart:\n    def charge(self, x):\n        return x\n\ndef refund(a):\n    return a\n",
    );
    write_file(&root, "requirements.txt", "flask==3.0.0\n");

    let out = run_ingest(&root, &IngestOptions::default()).expect("ingest");
    let snap = &out.snapshot;

    assert_eq!(snap.concepts_tier, Some(CONCEPTS_TIER_VERSION));
    assert!(
        !snap.usecase_view.is_empty(),
        "the projection is filled on a full ingest"
    );

    // one record per UseCase node, same ids
    let uc_nodes: Vec<_> = snap
        .concepts
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, ConceptKind::UseCase | ConceptKind::Facet))
        .collect();
    assert_eq!(snap.usecase_view.len(), uc_nodes.len());
    for (v, n) in snap.usecase_view.iter().zip(&uc_nodes) {
        assert_eq!(v.id, n.id);
        assert_eq!(v.label, n.label);

        // chain ids resolve; nearest-broader first, ends at the Domain root
        assert!(!v.chain.is_empty(), "every usecase reaches a root");
        for link in &v.chain {
            let node = snap
                .concepts
                .nodes
                .get(link.id as usize)
                .unwrap_or_else(|| panic!("chain id {} resolves", link.id));
            assert_eq!(node.id, link.id);
            assert_eq!(node.label, link.label);
            assert!(matches!(node.kind, ConceptKind::UseCase | ConceptKind::Facet | ConceptKind::Domain));
        }
        let last = v.chain.last().unwrap();
        assert_eq!(
            snap.concepts.nodes[last.id as usize].kind,
            ConceptKind::Domain,
            "the climb ends at the Domain root"
        );

        // member ids resolve through the FINAL symbol table (post-purge)
        for m in &v.members {
            let sym = snap
                .symbols
                .get(m.symbol.0 as usize)
                .unwrap_or_else(|| panic!("member id {} resolves", m.symbol.0));
            assert_eq!(sym.name, m.name);
            assert_eq!(sym.qual_name, m.qual_name);
        }

        // member classes: ids resolve, participants are extent members
        for c in &v.member_classes {
            let node = snap
                .concepts
                .nodes
                .get(c.id as usize)
                .unwrap_or_else(|| panic!("class id {} resolves", c.id));
            assert_eq!(node.label, c.label);
            for sid in &c.members {
                assert!(
                    v.members.iter().any(|m| &m.symbol == sid),
                    "a member class never lists a non-member: {sid:?} not in {:?}",
                    v.members.iter().map(|m| m.symbol).collect::<Vec<_>>()
                );
            }
        }
    }

    // the charge/refund story: one usecase, cutting across Cart with
    // exactly the charge member
    let money = snap
        .usecase_view
        .iter()
        .find(|v| v.intent.contains(&"money".to_string()))
        .expect("the shared money tag yields a usecase");
    assert_eq!(money.members.len(), 2, "charge + refund");
    assert_eq!(money.member_classes.len(), 1, "Cart is the class cut");
    assert_eq!(money.member_classes[0].label, "Cart");
    assert_eq!(money.member_classes[0].members.len(), 1);
    let charge = snap
        .symbols
        .iter()
        .find(|s| s.name == "charge")
        .expect("charge in the table");
    assert_eq!(money.member_classes[0].members[0], charge.id);
    assert_eq!(
        money
            .members
            .iter()
            .find(|m| m.name == "charge")
            .unwrap()
            .owner
            .as_deref(),
        Some("Cart"),
        "the method's owner rides the member record"
    );
    assert_eq!(
        money
            .members
            .iter()
            .find(|m| m.name == "refund")
            .unwrap()
            .owner
            .as_deref(),
        // the python adapter gives every top-level function its module as
        // the container — the projection reports the table's own parent
        // chain, never a guessed one
        Some("shop"),
        "the loose function's container is its module"
    );

    // and the artifact roundtrips: the persisted snapshot carries it too
    use cie_incremental::snapshot::SnapshotStoreExt;
    let store = cie_store::Store::open(&root).expect("store");
    let persisted = store
        .load_snapshot(out.rev)
        .expect("load")
        .expect("snapshot persisted");
    assert_eq!(persisted.usecase_view, snap.usecase_view);
}
