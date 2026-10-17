//! CIE acceptance suite (PRD §70 subset, v0.1 scope).

use cie_cli::engine::{self, IngestOptions};
use cie_core::drift::DriftStatus;
use cie_core::types::SymbolId;
use cie_core::findings::FindingKind;
use cie_storage::Store;
use std::fs;
use std::path::{Path, PathBuf};

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-acc-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn fixtures_root() -> PathBuf {
    // tests live at <workspace>/tests/fixtures; CARGO_MANIFEST_DIR = crates/cie-cli
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .canonicalize()
        .expect("fixtures dir present")
}

fn copy_fixture(src: &str, dst: &Path) {
    copy_recursive(fixtures_root().join(src).as_path(), dst);
}

fn copy_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        // Never copy analysis state into fresh test projects.
        if entry.file_name() == ".cie" {
            continue;
        }
        let target = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

fn kinds(outcome: &engine::IngestOutcome) -> Vec<FindingKind> {
    outcome.snapshot.findings.iter().map(|f| f.kind).collect()
}

const FIXTURE: &str = "bankapp";

#[test]
fn acc1_print_only_authorization_detected() {
    let dir = tmp_dir("acc1");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(
        kinds(&outcome).contains(&FindingKind::PrintOnlyAuthorization),
        "expected PrintOnlyAuthorization on auth.authorize"
    );
}

#[test]
fn acc2_unconditional_success_detected() {
    let dir = tmp_dir("acc2");
    copy_fixture(FIXTURE, &dir);
    let extra = dir.join("branchy.py");
    fs::write(
        &extra,
        "def decide(x):\n    if x:\n        return True\n    return 200\n",
    )
    .unwrap();
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(
        kinds(&outcome).contains(&FindingKind::UnconditionalSuccess),
        "expected UnconditionalSuccess for branch-shaped success-only function"
    );
}

#[test]
fn acc3_legitimate_delegation_not_flagged() {
    let dir = tmp_dir("acc3");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let flagged = outcome
        .snapshot
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                FindingKind::PrintOnlyAuthorization | FindingKind::UnconditionalSuccess
            ) && f.symbol_name.contains("good_auth")
        })
        .count();
    assert_eq!(flagged, 0, "good_auth delegates to real verification");
}

#[test]
fn acc4_rejected_authorization_ignored_detected() {
    let dir = tmp_dir("acc4");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(kinds(&outcome).contains(&FindingKind::IgnoredRejection));
}

#[test]
fn acc5_exceptions_converted_to_success_detected() {
    let dir = tmp_dir("acc5");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(kinds(&outcome).contains(&FindingKind::ExceptionToSuccess));
    assert!(kinds(&outcome).contains(&FindingKind::SwallowedException));
}

#[test]
fn acc6_unresolved_external_marked_explicitly() {
    let dir = tmp_dir("acc6");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(!outcome.snapshot.evidence.unresolved.is_empty());
    assert!(outcome
        .snapshot
        .evidence
        .unresolved
        .iter()
        .any(|u| u.contains("do_refund") || u.contains("unresolved")));
}

#[test]
fn acc7_deletion_invalidates_earlier_finding() {
    let dir = tmp_dir("acc7");
    copy_fixture(FIXTURE, &dir);
    let first = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(kinds(&first).contains(&FindingKind::PrintOnlyAuthorization));

    fs::remove_file(dir.join("auth.py")).unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(!kinds(&second).contains(&FindingKind::PrintOnlyAuthorization));
    assert!(second.rev > first.rev);
    assert!(!second.snapshot.files.iter().any(|f| f == "auth.py"));

    // TASK-028 extension: the deletion is recorded explicitly and no stale
    // symbol survives — purge is a guarantee, not an accident of rebuilds.
    let store = Store::open(&dir).unwrap();
    assert!(
        store.tombstones().unwrap().contains("auth.py"),
        "file:auth.py:tombstone must be recorded on deletion"
    );
    assert!(
        !second.snapshot.symbols.iter().any(|s| s.loc.path == "auth.py"),
        "zero symbols may reference a tombstoned file"
    );
}

#[test]
fn acc8_polyglot_repo_ingested_end_to_end() {
    let dir = tmp_dir("acc8");
    copy_fixture("polyglot", &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let mut langs: Vec<_> = outcome.store.symbols.iter().map(|s| s.lang).collect();
    langs.sort();
    langs.dedup();
    for want in [
        cie_core::types::LangId::Python,
        cie_core::types::LangId::Go,
        cie_core::types::LangId::Rust,
        cie_core::types::LangId::TypeScript,
    ] {
        assert!(langs.contains(&want), "missing language {want:?}");
    }
    assert!(
        kinds(&outcome).contains(&FindingKind::SqlInjection),
        "TS req.query -> SQL sink must be detected"
    );
}

#[test]
fn acc10_unchanged_reingest_fast() {
    let dir = tmp_dir("acc10");
    copy_fixture(FIXTURE, &dir);
    let first = engine::run_ingest(&dir, &opts()).unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert!(!second.plan.has_work());
    assert_eq!(second.plan.unchanged.len(), 4);
    assert!(
        second.snapshot.ingest_wall_ns < first.snapshot.ingest_wall_ns / 4,
        "re-ingest {}ns should be far below first-ingest {}ns",
        second.snapshot.ingest_wall_ns,
        first.snapshot.ingest_wall_ns
    );
    assert_eq!(second.rev, first.rev, "no new revision without changes");
}

#[test]
fn acc11_time_travel_returns_correct_results() {
    let dir = tmp_dir("acc11");
    copy_fixture(FIXTURE, &dir);
    engine::run_ingest(&dir, &opts()).unwrap();
    let store = Store::open(&dir).unwrap();
    let rev1 = store.last_rev();
    let snap1 = store.load_snapshot(rev1).unwrap().unwrap();
    let symbols1 = snap1.symbols.len();

    fs::write(dir.join("extra.py"), "def brand_new_function():\n    return 1\n").unwrap();
    let second = engine::run_ingest(&dir, &opts()).unwrap();
    assert_eq!(second.rev, rev1 + 1);
    assert!(second.snapshot.symbols.len() > symbols1);

    let as_of = engine::load_outcome(&dir, rev1).unwrap();
    assert!(as_of
        .store
        .symbols
        .iter()
        .all(|s| s.name != "brand_new_function"));
    assert!(second.store.symbols.iter().any(|s| s.name == "brand_new_function"));
}

fn drift_report(dir: &Path, docs: &[PathBuf]) -> cie_core::drift::DriftReport {
    let doc_contents: Vec<(String, String)> = docs
        .iter()
        .map(|d| {
            (
                d.file_name().unwrap().to_string_lossy().to_string(),
                fs::read_to_string(d).unwrap(),
            )
        })
        .collect();
    let (report, _) = engine::run_drift(dir, &doc_contents, &opts(), None, false).unwrap();
    report
}

#[test]
fn acc18_19_20_21_22_drift_statuses() {
    let dir = tmp_dir("acc18");
    copy_fixture(FIXTURE, &dir);
    let docs = vec![
        fixtures_root().join("docs/prd-payments.md"),
        fixtures_root().join("docs/security-policy.md"),
    ];
    let report = drift_report(&dir, &docs);

    let status_of = |needle: &str| -> DriftStatus {
        let claim = report
            .claims
            .iter()
            .find(|c| c.text.contains(needle))
            .unwrap_or_else(|| panic!("claim containing {needle} not found"));
        report
            .verifications
            .iter()
            .find(|v| v.claim == claim.id)
            .unwrap()
            .status
    };

    assert_eq!(status_of("double-charge"), DriftStatus::Contradicting);
    assert!(
        report
            .verifications
            .iter()
            .any(|v| v.status == DriftStatus::Missing),
        "expected at least one Missing status"
    );
    assert_eq!(status_of("p99"), DriftStatus::Unverifiable);
    let p99 = report
        .verifications
        .iter()
        .find(|v| {
            report
                .claims
                .iter()
                .any(|c| c.id == v.claim && c.text.contains("p99"))
        })
        .unwrap();
    assert!(p99.reason.as_deref().unwrap_or("").contains("runtime"));
    assert!(
        report
            .reverse_drift
            .iter()
            .any(|r| r.qual_name.contains("list_purchases"))
    );
    let conflicted = report
        .verifications
        .iter()
        .find(|v| {
            v.claim == report
                .claims
                .iter()
                .find(|c| c.text.contains("log every refund"))
                .unwrap()
                .id
        })
        .unwrap();
    assert_eq!(conflicted.status, DriftStatus::Contradicting);
    assert!(conflicted.evidence.iter().any(|e| e.contains("multi-document conflict")));
}

#[test]
fn acc23_drift_delta_shows_only_changes() {
    let dir = tmp_dir("acc23");
    copy_fixture(FIXTURE, &dir);
    let docs = vec![fixtures_root().join("docs/prd-payments.md")];
    let r1 = drift_report(&dir, &docs);

    let p = dir.join("payments.py");
    let src = fs::read_to_string(&p).unwrap();
    fs::write(
        &p,
        src.replace(
            "def refund(amount, user):",
            "def refund(amount, user):\n    logger.info('refund start')",
        ),
    )
    .unwrap();
    let r2 = drift_report(&dir, &docs);

    let delta = r1.delta(&r2);
    assert!(
        delta
            .iter()
            .any(|(_, _, from, to)| from == "missing" && to == "implemented"),
        "delta must show missing -> implemented, got {delta:?}"
    );
}

#[test]
fn acc26_verification_timeout_yields_unknown() {
    let dir = tmp_dir("acc26");
    copy_fixture(FIXTURE, &dir);
    let docs = vec![fixtures_root().join("docs/prd-payments.md")];
    let doc_contents: Vec<(String, String)> = docs
        .iter()
        .map(|d| {
            (
                d.file_name().unwrap().to_string_lossy().to_string(),
                fs::read_to_string(d).unwrap(),
            )
        })
        .collect();
    let (report, _) = engine::run_drift(&dir, &doc_contents, &opts(), None, true).unwrap();
    assert!(
        report
            .verifications
            .iter()
            .all(|v| v.status == DriftStatus::Unknown),
        "timeout must yield Unknown, never Implemented"
    );
}

#[test]
fn acc27_to_31_concept_behavior() {
    let dir = tmp_dir("acc27");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let dag = &outcome.snapshot.concepts;

    assert!(dag.nodes.len() > 3);
    assert!(dag
        .nodes
        .iter()
        .any(|n| n.kind == cie_core::concept::ConceptKind::UseCase));
    assert!(dag.nodes.iter().all(|n| n.parent.is_some()
        || n.kind == cie_core::concept::ConceptKind::Domain));

    let attached: std::collections::BTreeSet<SymbolId> = dag
        .nodes
        .iter()
        .flat_map(|n| n.behaviors.iter().map(|b| b.summary))
        .collect();
    let total = outcome.snapshot.summaries.len();
    assert_eq!(
        attached.len() + dag.unattached_behaviors.len(),
        total,
        "behaviors must be attached or listed unattached"
    );

    let root = &dag.nodes[0];
    let module_behaviors: std::collections::BTreeSet<SymbolId> = dag
        .nodes
        .iter()
        .filter(|n| n.kind == cie_core::concept::ConceptKind::Module)
        .flat_map(|n| n.behaviors.iter().map(|b| b.summary))
        .collect();
    let root_behaviors: std::collections::BTreeSet<SymbolId> =
        root.behaviors.iter().map(|b| b.summary).collect();
    assert!(module_behaviors.is_subset(&root_behaviors));

    assert!(dag.nodes.iter().any(|n| !n.extra_edges.is_empty()));

    for n in dag.nodes.iter() {
        for s in &n.symbols {
            assert!(outcome.store.get(*s).is_some(), "node {} references missing symbol {s}", n.label);
        }
    }
}

#[test]
fn acc41_to_44_capability_spot_checks() {
    let dir = tmp_dir("acc41");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let ks = kinds(&outcome);
    assert!(ks.contains(&FindingKind::LayeringViolation));
    assert!(ks.contains(&FindingKind::SqlInjection));
    assert!(ks.contains(&FindingKind::NPlusOneQuery));
    assert!(ks.contains(&FindingKind::DeadCode));
    assert!(outcome
        .snapshot
        .findings
        .iter()
        .any(|f| f.kind == FindingKind::DeadCode && f.symbol_name.contains("list_purchases")));
}

#[test]
fn acc46_minimum_test_set_computed() {
    let dir = tmp_dir("acc46");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let target = outcome
        .store
        .symbols
        .iter()
        .find(|s| s.qual_name == "auth.good_auth")
        .expect("good_auth symbol");
    let affected = cie_analysis::testing::affected_by_change(&outcome.reverse_deps, &[target.id]);
    let mut changed: Vec<SymbolId> = affected.into_iter().collect();
    changed.push(target.id);
    let min_tests = cie_analysis::testing::minimum_test_set(&outcome.test_map, &changed);
    assert!(!min_tests.is_empty());
    let test_names: Vec<String> = min_tests
        .iter()
        .filter_map(|t| outcome.store.get(*t).map(|s| s.qual_name.clone()))
        .collect();
    assert!(test_names.iter().any(|n| n.contains("test_good_auth")));
}

#[test]
fn sarif_export_is_valid() {
    let dir = tmp_dir("sarif");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let sarif = cie_analysis::sarif::findings_to_sarif(&outcome.store, &outcome.snapshot.findings);
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["runs"][0]["results"].as_array().unwrap().len() >= 5);
    assert!(sarif["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap().len() >= 3);
}

#[test]
fn coverage_gaps_and_test_map() {
    let dir = tmp_dir("cvg");
    copy_fixture(FIXTURE, &dir);
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let good = outcome
        .store
        .symbols
        .iter()
        .find(|s| s.qual_name == "auth.good_auth")
        .unwrap();
    assert!(outcome.test_map.tests_of.contains_key(&good.id));
    let charge = outcome
        .store
        .symbols
        .iter()
        .find(|s| s.qual_name == "payments.charge")
        .unwrap();
    assert!(!outcome.test_map.tests_of.contains_key(&charge.id));
}
