//! A1/A6: the induction chain — usecases hang off their broader usecases.
//!
//! fca::Concept.parents are lower covers (more specific concepts). The DAG
//! inverts them: a narrower usecase's PRIMARY parent is its broadest
//! surviving upper cover (FcaSubsumption), remaining covers become extra
//! edges recorded only on the usecase itself, and usecases with no
//! surviving cover stay under the Domain root. Gates:
//!   * nested intents form a chain, and `depth_of` counts it
//!   * the diamond case keeps the second cover traceable WITHOUT
//!     mirroring it into the cover's children (weakest-link isolation)
//!   * rollup keeps the inferred chain at its own 0.8 confidence
//!   * the chain reaches the serialized artifact (parent + extra_edges +
//!     intent), so the web layer never recomputes it

use cie_analysis::behavior::BehaviorResult;
use cie_analysis::hierarchy::build_hierarchy;
use cie_concepts::{ConceptDag, ConceptKind, ConceptNode, EdgeReason};
use cie_core::types::{FileTable, SourceLocation, SymbolId};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

const REV: u64 = 7;

fn fact(id: u32, name: &str, file: u32) -> SymbolFact {
    SymbolFact {
        id: SymbolId(id),
        kind: SymbolKind::Function,
        name: name.into(),
        qual_name: format!("app.{name}"),
        loc: SourceLocation::point(file.into(), 0),
        lang: LangId::Python,
        parent: None,
        rev: REV,
        doc: None,
        annotations: Vec::new(),
    }
}

/// One file per function keeps the majority-module extra edge out of the
/// assertions' way; attributes come only from the summary tags.
fn run(functions: &[(u32, &[&str])]) -> ConceptDag {
    let label_offline = |intent: &[String],
                         fallback: &str,
                         _evidence: &[cie_analysis::hierarchy::LabelSymbolEvidence]|
     -> (String, &'static str) {
        (
            if intent.is_empty() {
                fallback.to_string()
            } else {
                intent.join("+")
            },
            "offline",
        )
    };
    run_with(functions, &label_offline)
}

fn run_with(
    functions: &[(u32, &[&str])],
    label_fn: cie_analysis::hierarchy::LabelFn,
) -> ConceptDag {
    let mut files = FileTable::new();
    let mut symbols = Vec::new();
    let mut summaries: BTreeMap<SymbolId, FunctionSummary> = BTreeMap::new();
    for (ix, (id, tags)) in functions.iter().enumerate() {
        files.intern(&format!("src/f{ix}.py"));
        symbols.push(fact(*id, &format!("fn_{id}"), ix as u32));
        let mut s = FunctionSummary::empty(SymbolId(*id), REV, 0);
        for t in *tags {
            s.tags.insert(t.to_string());
        }
        summaries.insert(SymbolId(*id), s);
    }
    let store = SymbolStore::build_with_files(symbols, files);
    let beh = BehaviorResult {
        summaries,
        reverse_deps: BTreeMap::new(),
        reuse: Default::default(),
    };
    build_hierarchy(&store, &beh, &[], label_fn, 0, None)
}

fn usecases(dag: &ConceptDag) -> Vec<&ConceptNode> {
    dag.nodes
        .iter()
        .filter(|n| n.kind == ConceptKind::Facet)
        .collect()
}

#[test]
fn a_narrower_usecase_hangs_off_its_broader_cover() {
    // f1 {money, db_write}; f2, f3 {money, db_write, network}
    //   -> C{db_write,money}        ext {f1,f2,f3}  (broader)
    //   -> C{db_write,money,network} ext {f2,f3}   (narrower)
    let dag = run(&[
        (0, &["money", "db_write"]),
        (1, &["money", "db_write", "network"]),
        (2, &["money", "db_write", "network"]),
    ]);
    let ucs = usecases(&dag);
    assert_eq!(ucs.len(), 2, "exactly two surviving usecases");
    let broad = ucs.iter().find(|n| n.intent.len() == 2).unwrap();
    let narrow = ucs.iter().find(|n| n.intent.len() == 3).unwrap();
    assert_eq!(narrow.intent, vec!["db_write", "money", "network"]);
    assert_eq!(
        narrow.parent,
        Some(broad.id),
        "the climb the artifact owes the reader: narrower -> broader"
    );
    assert_eq!(narrow.primary_edge, EdgeReason::FcaSubsumption);
    assert!(broad.children.contains(&narrow.id));
    assert_eq!(
        broad.parent,
        Some(0),
        "nothing broader survives above the broadest -> the Domain root"
    );
    assert_eq!(dag.depth_of(narrow.id), 3, "root -> broad -> narrow");
    assert_eq!(
        broad.provenance, "fca:godin-v1",
        "an offline labeler keeps the mechanical provenance"
    );
}

#[test]
fn a_usecase_with_no_broader_cover_stays_at_the_root() {
    let dag = run(&[(0, &["auth"]), (1, &["auth"])]);
    let ucs = usecases(&dag);
    assert_eq!(ucs.len(), 1);
    assert_eq!(ucs[0].parent, Some(0));
    assert_eq!(ucs[0].primary_edge, EdgeReason::FcaSubsumption);
    assert!(
        !ucs[0]
            .extra_edges
            .iter()
            .any(|(_, r)| *r == EdgeReason::FcaSubsumption),
        "no broader cover exists, so no cover edges (the majority-module \
         ClusterCohesion edge is pre-existing and unrelated)"
    );
}

#[test]
fn a_diamond_records_extra_covers_as_traceable_edges() {
    // f1 {a}; f2, f3 {a, b}; f4 {b}
    //   -> C{a} ext {f1,f2,f3}, C{b} ext {f2,f3,f4}, C{a,b} ext {f2,f3}
    // C{a,b}'s broader covers are BOTH C{a} and C{b}. Primary = broadest
    // (equal extents -> intent order breaks the tie), the other stays
    // traceable as an extra edge and NEVER becomes a mirrored child.
    let dag = run(&[
        (0, &["a"]),
        (1, &["a", "b"]),
        (2, &["a", "b"]),
        (3, &["b"]),
    ]);
    let ucs = usecases(&dag);
    assert_eq!(ucs.len(), 3, "all three concepts survive (extent >= 2)");
    let ca = ucs
        .iter()
        .find(|n| n.intent.len() == 1 && n.intent[0] == "a")
        .unwrap();
    let cb = ucs
        .iter()
        .find(|n| n.intent.len() == 1 && n.intent[0] == "b")
        .unwrap();
    let cab = ucs.iter().find(|n| n.intent.len() == 2).unwrap();
    assert_eq!(
        cab.parent,
        Some(ca.id),
        "broadest surviving cover is the primary parent"
    );
    assert!(ca.children.contains(&cab.id));
    assert!(
        !cb.children.contains(&cab.id),
        "extra covers are never mirrored into the cover's children"
    );
    assert!(
        cab.extra_edges
            .iter()
            .any(|(p, r)| *p == cb.id && *r == EdgeReason::FcaSubsumption),
        "the second cover stays traceable on the usecase itself"
    );
}

#[test]
fn the_chain_survives_rollup_at_its_own_confidence() {
    let mut dag = run(&[
        (0, &["a"]),
        (1, &["a", "b"]),
        (2, &["a", "b"]),
        (3, &["b"]),
    ]);
    dag.rollup();
    let (ca_id, ca_conf, ca_syms) = {
        let ca = dag
            .nodes
            .iter()
            .find(|n| n.kind == ConceptKind::Facet && n.intent.len() == 1 && n.intent[0] == "a")
            .unwrap();
        (ca.id, ca.confidence, ca.symbols.len())
    };
    let cab_id = {
        let cab = dag
            .nodes
            .iter()
            .find(|n| n.kind == ConceptKind::Facet && n.intent.len() == 2)
            .unwrap();
        cab.id
    };
    assert_eq!(
        ca_conf, 0.8,
        "the inferred chain keeps its own 0.8 — nothing recorded leaks in"
    );
    assert_eq!(
        ca_syms, 3,
        "the broader extent already contains the narrower one; rollup adds nothing"
    );
    assert_eq!(dag.get(cab_id).unwrap().symbols.len(), 2);
    // the primary chain rides the parent walk, so the lazy invalidation
    // wave (TASK-082) reaches every broader usecase of a dirty concept
    dag.invalidate(cab_id);
    assert!(dag.dirty.contains(&cab_id) && dag.dirty.contains(&ca_id));
}

#[test]
fn the_chain_is_visible_in_the_serialized_artifact() {
    let dag = run(&[
        (0, &["money", "db_write"]),
        (1, &["money", "db_write", "network"]),
        (2, &["money", "db_write", "network"]),
    ]);
    let broad = usecases(&dag)
        .iter()
        .find(|n| n.intent.len() == 2)
        .copied()
        .unwrap();
    let json = serde_json::to_string(&dag).expect("the DAG serializes");
    assert!(json.contains("\"extra_edges\""), "cover edges reach the artifact");
    assert!(json.contains("\"intent\""), "the attribute set reaches the artifact");
    // the narrow usecase's record names its broader parent by id
    let marker = format!("\"parent\":{}", broad.id);
    let narrow_chunk = json
        .split("{\"behaviors\"")
        .find(|chunk| chunk.contains(&format!("\"db_write\",\"money\",\"network\"")))
        .expect("the narrow usecase's serialized record");
    assert!(
        narrow_chunk.contains(&marker),
        "the serialized usecase names its broader parent ({} in: {})",
        marker,
        narrow_chunk
    );
}

#[test]
fn the_labeler_sees_the_members_not_just_the_tag_words() {
    // A3: the hook receives the concept's member evidence — names, tags,
    // effects — bounded and deterministic, so a real provider can name the
    // story. Captured here through the same hook the ingest wires to the
    // Labeler.
    use std::cell::RefCell;
    let seen: RefCell<Vec<Vec<(String, Vec<String>)>>> = RefCell::new(Vec::new());
    let capture = |intent: &[String],
                   fallback: &str,
                   evidence: &[cie_analysis::hierarchy::LabelSymbolEvidence]|
     -> (String, &'static str) {
        seen.borrow_mut().push(
            evidence
                .iter()
                .map(|e| (e.name.clone(), e.tags.clone()))
                .collect(),
        );
        (
            if intent.is_empty() {
                fallback.to_string()
            } else {
                intent.join("+")
            },
            "offline",
        )
    };
    let dag = run_with(
        &[
            (0, &["money", "db_write"]),
            (1, &["money", "db_write", "network"]),
            (2, &["money", "db_write", "network"]),
        ],
        &capture,
    );
    let seen = seen.into_inner();
    assert_eq!(seen.len(), 2, "one evidence bundle per surviving usecase");
    // broader concept ext {fn_0, fn_1, fn_2}; narrower ext {fn_1, fn_2}
    let broad = seen.iter().find(|b| b.len() == 3).expect("broader bundle");
    let narrow = seen.iter().find(|b| b.len() == 2).expect("narrower bundle");
    assert_eq!(broad[0].0, "fn_0", "extent order, deterministic");
    assert!(
        broad[0].1.contains(&"money".into()) && broad[0].1.contains(&"db_write".into()),
        "the member's tags ride along: {:?}",
        broad[0]
    );
    assert!(
        narrow.iter().all(|(n, t)| t.contains(&"network".into()) && n.starts_with("fn_")),
        "the narrower cluster's members all carry the differentiating tag"
    );
    let _ = dag;
}

#[test]
fn an_llm_label_is_provenance_and_an_inert_candidate_never_membership() {
    // A6: the llm-vs-offline provenance split. A labeler that reports
    // source "llm" stamps the node "label:llm" and binds ONE inert
    // LlmCandidate rule naming the proposal — and touches NOTHING else:
    // the extent (membership) stays exactly what the lattice computed.
    let dag = run_with(
        &[(0, &["auth"]), (1, &["auth"])],
        &|_intent: &[String],
           _fallback: &str,
           _ev: &[cie_analysis::hierarchy::LabelSymbolEvidence]| {
            ("Sensible auth story".to_string(), "llm")
        },
    );
    let ucs = usecases(&dag);
    assert_eq!(ucs.len(), 1);
    let uc = ucs[0];
    assert_eq!(uc.provenance, "label:llm", "the source is visible on the node");
    assert_eq!(uc.label, "Sensible auth story");
    assert_eq!(
        uc.symbols.len(),
        2,
        "the LLM names the story; membership stays mechanical"
    );
    assert!(
        uc.behaviors.is_empty(),
        "no behavior invented by the label path"
    );
    let candidates: Vec<_> = uc
        .rules
        .iter()
        .filter(|r| r.authority == cie_contract::Authority::LlmCandidate)
        .collect();
    assert_eq!(candidates.len(), 1, "exactly one inert candidate rule");
    assert_eq!(candidates[0].contract, 9_000_000u32 + uc.id);
    assert!(
        candidates[0].kind.contains("Sensible auth story"),
        "the rule names the proposal for audit: {}",
        candidates[0].kind
    );
}
