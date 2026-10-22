//! A5/A6: the usecase-first projection — the artifact's read-friendly view
//! of the induction chain and the membership.
//!
//! Gates:
//!   * one record per UseCase node, in deterministic (id) order
//!   * `chain` mirrors the primary-parent climb, nearest-broader first,
//!     ending at the Domain root
//!   * `narrower` lists immediate UseCase children
//!   * `members` resolve the extent to names/locations/owners
//!   * `member_classes` resolve the class-like cut with exactly the
//!     participating member ids (the pass-4 rule: sym.parent)
//!   * the projection is a pure function of (dag, store): two builds of
//!     the same input serialize identically

use cie_analysis::behavior::BehaviorResult;
use cie_analysis::hierarchy::{build_hierarchy, usecase_projection};
use cie_concepts::ConceptKind;
use cie_core::types::{FileId, FileTable, SourceLocation, SymbolId};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

const REV: u64 = 7;

/// id 0: class Cart (file 0); ids 1-2: its methods; ids 3-4: loose fns.
/// Tags: {money,db_write} everywhere; 2 and 4 add {network} — so the
/// lattice yields C{db_write,money} ext {1,2,3,4} (broader) over
/// C{db_write,money,network} ext {2,4} (narrower), and the broader story
/// cuts across the Cart class while the loose fns have no owner.
fn fixture() -> (SymbolStore, BehaviorResult) {
    let mut files = FileTable::new();
    // 13-byte first line ("let pad = 0;\n") -> line_starts [0, 13];
    // point(file, 13) is the 1-based line 2, so the projection's line
    // resolution is pinned below.
    let mut filler = |p: &str| files.intern_with_source(p, b"let pad = 0;\n");
    let f0 = filler("src/cart.py");
    let f1 = filler("src/pay.py");

    let sym = |id: u32, kind: SymbolKind, name: &str, file: FileId, parent: Option<u32>| SymbolFact {
        id: SymbolId(id),
        kind,
        name: name.into(),
        qual_name: format!("app.{name}"),
        loc: SourceLocation::point(file, 13),
        lang: LangId::Python,
        parent: parent.map(SymbolId),
        rev: REV,
        doc: None,
        annotations: Vec::new(),
    };

    let symbols = vec![
        sym(0, SymbolKind::Class, "Cart", f0, None),
        sym(1, SymbolKind::Method, "add_item", f0, Some(0)),
        sym(2, SymbolKind::Method, "checkout", f0, Some(0)),
        sym(3, SymbolKind::Function, "pay_now", f1, None),
        sym(4, SymbolKind::Function, "refund", f1, None),
    ];

    let tagged = |id: u32, tags: &[&str]| {
        let mut s = FunctionSummary::empty(SymbolId(id), REV, 0);
        for t in tags {
            s.tags.insert(t.to_string());
        }
        (SymbolId(id), s)
    };
    let summaries: BTreeMap<SymbolId, FunctionSummary> = [
        tagged(1, &["money", "db_write"]),
        tagged(2, &["money", "db_write", "network"]),
        tagged(3, &["money", "db_write"]),
        tagged(4, &["money", "db_write", "network"]),
    ]
    .into_iter()
    .collect();

    (
        SymbolStore::build_with_files(symbols, files),
        BehaviorResult {
            summaries,
            reverse_deps: BTreeMap::new(),
            reuse: Default::default(),
        },
    )
}

fn label_offline(
    intent: &[String],
    fallback: &str,
    _evidence: &[cie_analysis::hierarchy::LabelSymbolEvidence],
) -> (String, &'static str) {
    (
        if intent.is_empty() {
            fallback.to_string()
        } else {
            intent.join("+")
        },
        "offline",
    )
}

fn build() -> cie_concepts::ConceptDag {
    let (store, beh) = fixture();
    build_hierarchy(&store, &beh, &[], &label_offline, 0, None)
}

#[test]
fn one_record_per_usecase_with_the_chain_members_and_class_cut() {
    let (store, _) = fixture();
    let dag = build();

    let views = usecase_projection(&dag, &store);
    assert_eq!(views.len(), 2, "one record per surviving usecase");

    let broad = views.iter().find(|v| v.intent.len() == 2).unwrap();
    let narrow = views.iter().find(|v| v.intent.len() == 3).unwrap();
    assert_eq!(broad.intent, vec!["db_write", "money"]);
    assert_eq!(narrow.intent, vec!["db_write", "money", "network"]);
    assert_eq!(
        broad.id, 4,
        "views are in dag id order: the broader extent is pushed first"
    );
    assert_eq!(narrow.id, 5);

    // chain: nearest-broader first, ending at the Domain root
    assert_eq!(
        broad.chain,
        vec![cie_concepts::UsecaseLink {
            id: 0,
            label: "Project".into(),
            kind: ConceptKind::Domain,
        }]
    );
    assert_eq!(
        narrow.chain,
        vec![
            cie_concepts::UsecaseLink {
                id: broad.id,
                label: broad.label.clone(),
                kind: ConceptKind::UseCase,
            },
            cie_concepts::UsecaseLink {
                id: 0,
                label: "Project".into(),
                kind: ConceptKind::Domain,
            },
        ],
        "the reader climbs narrow -> broad -> root without re-deriving it"
    );

    // narrower: immediate UseCase children only
    assert_eq!(broad.narrower, vec![narrow.id]);
    assert!(narrow.narrower.is_empty());

    // members: the extent resolved, extent order (SymbolId ascending)
    let want: Vec<(&str, Option<&str>, &str)> = vec![
        ("add_item", Some("Cart"), "src/cart.py"),
        ("checkout", Some("Cart"), "src/cart.py"),
        ("pay_now", None, "src/pay.py"),
        ("refund", None, "src/pay.py"),
    ];
    assert_eq!(broad.members.len(), 4);
    for (m, (name, owner, file)) in broad.members.iter().zip(want) {
        assert_eq!(m.name, name);
        assert_eq!(m.owner.as_deref(), owner);
        assert_eq!(m.file, file);
        assert_eq!(m.line, 2, "1-based line resolved through the file table");
        assert!(m.qual_name.ends_with(name));
    }

    // member_classes: the class cut with exactly the participating members
    assert_eq!(broad.member_classes.len(), 1);
    let cut = &broad.member_classes[0];
    assert_eq!(cut.label, "Cart");
    assert_eq!(cut.kind, ConceptKind::Entity);
    assert_eq!(cut.members, vec![SymbolId(1), SymbolId(2)]);
    // narrower participates in Cart through checkout only
    assert_eq!(narrow.member_classes.len(), 1);
    assert_eq!(narrow.member_classes[0].members, vec![SymbolId(2)]);

    // the honest parts ride along unchanged
    assert_eq!(broad.provenance, "fca:godin-v1");
    assert!((broad.confidence - 0.8).abs() < f32::EPSILON);
    assert!(broad.coverage_complete);
}

#[test]
fn the_projection_is_a_pure_function_of_dag_and_store() {
    let (store, _) = fixture();
    let dag = build();
    let a = usecase_projection(&dag, &store);
    let b = usecase_projection(&dag, &store);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "same input, byte-identical projection"
    );
}

#[test]
fn the_projection_survives_a_full_rollup_intact() {
    // The projection reads the DAG as emitted; a consumer that re-rolls
    // (incremental refresh) gets the same membership — rollup unions
    // descendants INTO ancestors but never mutates the usecase's own
    // extent, and the projection reads node.symbols of UseCase nodes only.
    let (store, _) = fixture();
    let mut dag = build();
    let before = usecase_projection(&dag, &store);
    dag.rollup();
    let after = usecase_projection(&dag, &store);
    assert_eq!(before, after, "rollup never rewrites the projection");
}
