//! RFC-0019 §6.4: the C4 tiers in the concept hierarchy.
//!
//! A synthetic two-app store (a spring-ish DDD app `billing`, a rust app
//! `alerts`) plus files outside every app, and a hand-built StackReport.
//! Gates:
//!   * with stack — Container nodes 1-per-app (recorded certainty, docker
//!     governance in intent); components per role with RECORDED certainty
//!     for annotation-named layers and INFERENCE for name-based ones; DDD
//!     subpackages as Subdomain nodes; file Modules under their component;
//!     classes keep the Module chain AND link to their component; the
//!     Aggregate verdict is evidence (repo-named entity / route-reachable
//!     repo-reaching non-role), never money vocabulary; lattice intents
//!     gain the stack facts (http-route / di-injected / docker-managed).
//!   * without stack — the exact pre-RFC shape: no Container/Subdomain
//!     nodes, every Module under the root, money-words only outside.

use cie_analysis::behavior::BehaviorResult;
use cie_analysis::hierarchy::build_hierarchy;
use cie_concepts::{ConceptKind, EdgeReason};
use cie_core::types::{FileTable, SourceLocation, SymbolId};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_stack::types::{
    AppGovernance, AppSite, BuildInfo, Coverage, DockerInfo, JvmEntity, JvmRepository, JvmService,
    JvmStack, MainSite, RouteSite, StackReport,
};
use cie_summary::FunctionSummary;
use std::collections::{BTreeMap, BTreeSet};

const REV: u64 = 21;

const F1: &str = "billing/src/main/kotlin/com/harbor/billing/domain/Invoice.kt";
const F2: &str = "billing/src/main/kotlin/com/harbor/billing/application/BillingService.kt";
const F3: &str = "billing/src/main/kotlin/com/harbor/billing/BillingController.kt";
const F4: &str = "billing/src/main/kotlin/com/harbor/billing/domain/InvoiceRepository.kt";
const F5: &str = "billing/src/main/kotlin/com/harbor/billing/domain/Ledger.kt";
const F6: &str = "alerts/src/store.rs";
const F7: &str = "misc/util.py";
const F8: &str = "misc/payments.py";

fn fact(id: u32, kind: SymbolKind, name: &str, qual: &str, file: u32, parent: Option<u32>) -> SymbolFact {
    SymbolFact {
            annotations: Vec::new(),
        id: SymbolId(id),
        kind,
        name: name.into(),
        qual_name: qual.into(),
        loc: SourceLocation::point(file.into(), 0),
        lang: LangId::Python,
        parent: parent.map(SymbolId),
        rev: REV,
        doc: None,
    }
}

/// billing: route → controller → service → ledger → repository, with the
/// repository's supertype NAMING Invoice as the entity it serves.
fn store() -> SymbolStore {
    let mut files = FileTable::new();
    for p in [F1, F2, F3, F4, F5, F6, F7, F8] {
        files.intern(p);
    }
    let symbols = vec![
        // 0..2 Invoice (DDD domain, repo-named entity)
        fact(0, SymbolKind::Class, "Invoice", "billing.Invoice", 0, None),
        fact(1, SymbolKind::Function, "add_line", "billing.Invoice.add_line", 0, Some(0)),
        fact(2, SymbolKind::Function, "settle", "billing.Invoice.settle", 0, Some(0)),
        // 3..4 BillingController (recorded route owner)
        fact(3, SymbolKind::Class, "BillingController", "billing.BillingController", 2, None),
        fact(4, SymbolKind::Function, "post_invoice", "billing.BillingController.post_invoice", 2, Some(3)),
        // 5..6 BillingService (recorded @Service, lives in application/)
        fact(5, SymbolKind::Class, "BillingService", "billing.BillingService", 1, None),
        fact(6, SymbolKind::Function, "add_line", "billing.BillingService.add_line", 1, Some(5)),
        // 7..8 InvoiceRepository (recorded repository + its entity name)
        fact(7, SymbolKind::Interface, "InvoiceRepository", "billing.InvoiceRepository", 3, None),
        fact(8, SymbolKind::Function, "save", "billing.InvoiceRepository.save", 3, Some(7)),
        // 9,12 Ledger (no annotations — aggregate ONLY via reach evidence)
        fact(9, SymbolKind::Class, "Ledger", "billing.Ledger", 4, None),
        fact(12, SymbolKind::Function, "post", "billing.Ledger.post", 4, Some(9)),
        // 10..11 Store (rust, name-based repository role)
        fact(10, SymbolKind::Struct, "Store", "alerts.Store", 5, None),
        fact(11, SymbolKind::Function, "load", "alerts.Store.load", 5, Some(10)),
        // 13 util_fn — outside every app
        fact(13, SymbolKind::Function, "util_fn", "misc.util_fn", 6, None),
        // 14..15 PaymentThing — outside every app, money-verb method
        fact(14, SymbolKind::Class, "PaymentThing", "misc.PaymentThing", 7, None),
        fact(15, SymbolKind::Function, "charge_card", "misc.PaymentThing.charge_card", 7, Some(14)),
    ];
    SymbolStore::build_with_files(symbols, files)
}

fn behavior() -> BehaviorResult {
    let mut summaries: BTreeMap<SymbolId, FunctionSummary> = BTreeMap::new();
    let mut call = |id: u32, callees: Vec<u32>| {
        let mut s = FunctionSummary::empty(SymbolId(id), REV, 0);
        s.callees = callees.into_iter().map(SymbolId).collect();
        summaries.insert(SymbolId(id), s);
    };
    call(4, vec![6]); // controller → service
    call(6, vec![12, 8]); // service → ledger, repository
    call(12, vec![8]); // ledger → repository
    for id in [1u32, 2, 8, 11, 13, 15] {
        summaries.insert(SymbolId(id), FunctionSummary::empty(SymbolId(id), REV, 0));
    }
    BehaviorResult {
        summaries,
        reverse_deps: BTreeMap::new(),
        reuse: Default::default(),
    }
}

fn stack() -> StackReport {
    StackReport {
        cross_service: vec![],
        v: 1,
        build: BuildInfo { tool: "unknown".into(), modules: vec![], artifact_name: String::new() },
        languages: vec![],
        frameworks: vec![],
        docker: DockerInfo { present: true, dockerfiles: vec![], compose: None, note: String::new() },
        jvm: Some(JvmStack {
            config_classes: vec![],
            services: vec![JvmService {
                name: "BillingService".into(),
                rel: F2.into(),
                line: 5,
                lang: "kotlin".into(),
                injections: vec![],
            annotations: vec![],
            }],
            repositories: vec![JvmRepository {
                name: "InvoiceRepository".into(),
                rel: F4.into(),
                line: 3,
                lang: "kotlin".into(),
                extends: "JpaRepository<Invoice, Long>".into(),
                entity: Some("Invoice".into()),
            annotations: vec![],
            }],
            entities: vec![JvmEntity {
                name: "Invoice".into(),
                rel: F1.into(),
                line: 2,
                lang: "kotlin".into(),
                table: None,
                props: vec![],
            annotations: vec![],
            }],
            scheduled_in: vec![],
            listeners: vec![],
            outbound: vec![],
            scanned: 4,
            java_files: 0,
            kotlin_files: 4,
        }),
        routes: vec![RouteSite {
            verb: "POST".into(),
            path: "/invoices".into(),
            owner: "BillingController".into(),
            method: "post_invoice".into(),
            service: "billing".into(),
            rel: F3.into(),
            line: 12,
            params: vec![],
            returns: String::new(),
            evidence: "@PostMapping".into(),
        }],
        mains: vec![MainSite {
            name: "BillingApplication".into(),
            rel: "billing/src/main/kotlin/com/harbor/billing/BillingApplication.kt".into(),
            line: 3,
            kind: "spring-boot".into(),
            lang: "kotlin".into(),
        }],
        flows: vec![],
        apps: vec![
            AppSite {
                dir: "billing".into(),
                name: "billing".into(),
                build: "gradle".into(),
                framework: "spring".into(),
                evidence: "billing/build.gradle.kts".into(),
                manifests: vec![],
                docker: Some(AppGovernance {
                    kind: "compose".into(),
                    service: Some("billing".into()),
                    ports: vec!["8082:8082".into()],
                    evidence: "docker-compose.yml:9".into(),
                }),
            },
            AppSite {
                dir: "alerts".into(),
                name: "alerts".into(),
                build: "cargo".into(),
                framework: "axum".into(),
                evidence: "alerts/Cargo.toml".into(),
                manifests: vec![],
                docker: Some(AppGovernance {
                    kind: "own-dockerfile".into(),
                    service: None,
                    ports: vec!["8083:8083".into()],
                    evidence: "alerts/Dockerfile".into(),
                }),
            },
        ],
        config: vec![],
        external_docker: vec![],
        notes: vec![],
        coverage: Coverage { files_scanned: 8, manifests_read: vec![], dockerfiles_read: vec![] },
        generated_at: 0,
    }
}

fn label_offline(
    intent: &[String],
    fallback: &str,
    _evidence: &[cie_analysis::hierarchy::LabelSymbolEvidence],
) -> (String, &'static str) {
    (if intent.is_empty() { fallback.to_string() } else { intent.join("+") }, "offline")
}

#[test]
fn with_stack_the_dag_carries_the_c4_tiers() {
    let store = store();
    let beh = behavior();
    let dag = build_hierarchy(&store, &beh, &[], &label_offline, 0, Some(&stack()));

    // ---- L1: one container per app, recorded, governance in intent
    let containers: Vec<_> = dag
        .nodes
        .iter()
        .filter(|n| n.kind == ConceptKind::Container)
        .collect();
    assert_eq!(containers.len(), 2, "one Container per AppSite");
    assert!(
        containers
            .iter()
            .all(|n| n.provenance == "stack:apps" && n.parent == Some(0)),
        "containers hang off the root, recorded provenance (confidence rolls up as the subtree's weakest link)"
    );
    let billing = containers.iter().find(|n| n.label == "billing").expect("billing container");
    assert_eq!(billing.intent, vec!["docker:compose".to_string()]);
    let alerts = containers.iter().find(|n| n.label == "alerts").expect("alerts container");
    assert_eq!(alerts.intent, vec!["docker:own-dockerfile".to_string()]);

    // ---- L2: components — DDD subpackages, recorded roles, named roles
    let comps: Vec<_> = dag
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, ConceptKind::Service | ConceptKind::Subdomain))
        .collect();
    let names: BTreeSet<_> = comps.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(
        names,
        BTreeSet::from(["billing domain", "billing application", "billing web", "alerts repository"]),
        "one component per distinct home: {:?}",
        names
    );
    let domain = comps.iter().find(|n| n.label == "billing domain").unwrap();
    assert_eq!(domain.kind, ConceptKind::Subdomain, "DDD package → Subdomain");
    assert_eq!(domain.confidence, 0.7, "structure-based → inference");
    assert_eq!(domain.provenance, "stack:roles");
    let web = comps.iter().find(|n| n.label == "billing web").unwrap();
    assert_eq!(web.confidence, 1.0, "a recorded route file → recorded certainty");
    let repo = comps.iter().find(|n| n.label == "alerts repository").unwrap();
    assert_eq!(repo.confidence, 0.7, "name-based role → inference");
    assert_eq!(repo.parent, Some(alerts.id), "components live under their container");
    assert_eq!(web.parent, Some(billing.id));

    // ---- Modules re-parent under their component
    let module_of = |p: &str| {
        dag.nodes
            .iter()
            .find(|n| n.kind == ConceptKind::Module && n.label == p)
            .unwrap_or_else(|| panic!("module {p}"))
    };
    let inv_mod = module_of(F1);
    assert_eq!(inv_mod.parent, Some(domain.id), "the file's module sits under its component");
    let misc_mod = module_of(F7);
    assert_eq!(misc_mod.parent, Some(0), "files outside every app stay at the root");

    // ---- the primary chain Project → Container → Component → Module → class
    let invoice = dag.nodes.iter().find(|n| n.label == "Invoice").expect("Invoice node");
    assert_eq!(invoice.parent, Some(inv_mod.id));
    assert!(
        invoice.extra_edges.iter().any(|(p, r)| *p == domain.id && *r == EdgeReason::CallOwnership),
        "the class also links straight to its component (CallOwnership)"
    );

    // ---- Aggregates by evidence, never by vocabulary
    assert_eq!(invoice.kind, ConceptKind::Aggregate, "the repository's named entity");
    assert_eq!(invoice.provenance, "stack:aggregate");
    let ledger = dag.nodes.iter().find(|n| n.label == "Ledger").expect("Ledger node");
    assert_eq!(
        ledger.kind,
        ConceptKind::Aggregate,
        "route-reachable AND repository-reaching, no annotation needed"
    );
    let svc = dag.nodes.iter().find(|n| n.label == "BillingService").unwrap();
    assert_eq!(svc.kind, ConceptKind::Entity, "a recorded framework role is never an aggregate");
    let ctl = dag.nodes.iter().find(|n| n.label == "BillingController").unwrap();
    assert_eq!(ctl.kind, ConceptKind::Entity, "the route owner is furniture, not an aggregate");

    // money-words survives ONLY outside the stack's reach
    let pay = dag.nodes.iter().find(|n| n.label == "PaymentThing").unwrap();
    assert_eq!(pay.kind, ConceptKind::Aggregate, "the fallback fires where no stack facts reach");
    assert_eq!(pay.provenance, "rule:structure");

    // ---- lattice intents gained the stack facts
    let uc = dag
        .nodes
        .iter()
        .find(|n| n.kind == ConceptKind::Facet)
        .expect("the shared docker-managed intent forms a use case");
    assert_eq!(uc.intent, vec!["docker-managed".to_string()]);
}

#[test]
fn without_stack_the_pre_rfc_shape_holds_exactly() {
    let store = store();
    let beh = behavior();
    let dag = build_hierarchy(&store, &beh, &[], &label_offline, 0, None);

    assert!(
        !dag.nodes
            .iter()
            .any(|n| matches!(n.kind, ConceptKind::Container | ConceptKind::Subdomain)),
        "no C4 tiers without the stack artifact"
    );
    assert!(
        dag.nodes
            .iter()
            .filter(|n| n.kind == ConceptKind::Module)
            .all(|n| n.parent == Some(0)),
        "every module stays at the root"
    );
    let invoice = dag.nodes.iter().find(|n| n.label == "Invoice").unwrap();
    assert_eq!(invoice.kind, ConceptKind::Entity, "no evidence, no aggregate");
    let pay = dag.nodes.iter().find(|n| n.label == "PaymentThing").unwrap();
    assert_eq!(pay.kind, ConceptKind::Aggregate, "money-words is the whole rule pre-RFC");
    assert_eq!(pay.provenance, "rule:structure");
}
