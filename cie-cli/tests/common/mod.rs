//! Shared acceptance-scenario helpers and bodies — included by each
//! integration test binary (`acceptance.rs`, `acceptance_harness.rs`,
//! `task_072_076_drift_acceptance.rs`) so the TASK-077 harness can dispatch
//! the very same scenario code the standalone tests run.

#![allow(dead_code)]

use cie_analysis::findings::FindingKind;
use cie_claims::model::DriftStatus;
use cie_claims::persist::ClaimStoreExt;
use cie_concepts::ConceptStoreExt;
use cie_contract::Authority;
use cie_core::types::now_ns;
use cie_core::types::{Ns, SymbolId};
use cie_drift::persist::DriftStoreExt;
use cie_incremental::SnapshotStoreExt;
use cie_ingest::{self, IngestOptions};
use cie_llm::persist::LabelStoreExt;
use cie_store::Store;
use std::fs;
use std::path::{Path, PathBuf};

pub fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-acc-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn fixtures_root() -> PathBuf {
    // fixtures live at <workspace>/tests/fixtures; CARGO_MANIFEST_DIR = cie-cli
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .canonicalize()
        .expect("fixtures dir present")
}

pub fn copy_fixture(src: &str, dst: &Path) {
    copy_recursive(fixtures_root().join(src).as_path(), dst);
}

pub fn copy_recursive(src: &Path, dst: &Path) {
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

pub fn opts() -> IngestOptions {
    IngestOptions::default()
}

pub fn kinds(outcome: &cie_ingest::IngestOutcome) -> Vec<FindingKind> {
    outcome.snapshot.findings.iter().map(|f| f.kind).collect()
}

const FIXTURE: &str = "bankapp";

pub fn drift_report(dir: &Path, docs: &[PathBuf]) -> cie_claims::model::DriftReport {
    let doc_contents: Vec<(String, String)> = docs
        .iter()
        .map(|d| {
            (
                d.file_name().unwrap().to_string_lossy().to_string(),
                fs::read_to_string(d).unwrap(),
            )
        })
        .collect();
    let (report, _) = cie_ingest::run_drift(dir, &doc_contents, &opts(), None, false).unwrap();
    report
}

/// Serializes scenario dispatch within one test process: the harness
/// meta-tests (TASK-077/TASK-084) both dispatch the same scenario bodies,
/// and two concurrent copies would race on the same (name, pid) temp dir.
pub static SCENARIO_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub mod scenarios {
    use super::*;

    /// A synthetic-but-realistic corpus: 40 modules with real cross-file calls
    /// so the first (cold) drift pass is solidly measurable.
    pub fn build_corpus(dir: &std::path::Path) {
        fs::write(
        dir.join("store.py"),
        "import db\n\ndef query_user(uid):\n    return db.fetch_one(\"select * from users where id=?\", (uid,))\n",
    )
    .unwrap();
        for i in 0..20 {
            fs::write(
            dir.join(format!("svc_{i:02}.py")),
            format!(
                "import store\n\ndef handle_{i}(req):\n    user = store.query_user(req)\n    if not user:\n        return False\n    print(\"processed {{}}\")\n    return True\n"
            ),
        )
        .unwrap();
        }
        fs::write(dir.join("db.py"), "import sqlite3\n\ndef fetch_one(sql, params):\n    return sqlite3.connect(\"app.db\").execute(sql, params).fetchone()\n").unwrap();
    }

    pub fn docs_for() -> Vec<(String, String)> {
        let mut svc = String::from("# Service docs\n\n");
        for i in 0..10 {
            svc.push_str(&format!(
            "Service {i} must query the user store before processing.\nService {i} should return False when the user is missing.\nService {i} will print a confirmation after processing.\n"
        ));
        }
        let mut pay = String::from("# Payments\n\n");
        for i in 0..8 {
            pay.push_str(&format!(
            "The service will charge card {i} and must log the attempt.\nCharging card {i} must fail when the card is declined.\n"
        ));
        }
        vec![("svc-docs.md".into(), svc), ("payments.md".into(), pay)]
    }

    pub fn acc1_print_only_authorization_detected() {
        let dir = tmp_dir("acc1");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(
            kinds(&outcome).contains(&FindingKind::PrintOnlyAuthorization),
            "expected PrintOnlyAuthorization on auth.authorize"
        );
    }

    pub fn acc2_unconditional_success_detected() {
        let dir = tmp_dir("acc2");
        copy_fixture(FIXTURE, &dir);
        let extra = dir.join("branchy.py");
        fs::write(
            &extra,
            "def decide(x):\n    if x:\n        return True\n    return 200\n",
        )
        .unwrap();
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(
            kinds(&outcome).contains(&FindingKind::UnconditionalSuccess),
            "expected UnconditionalSuccess for branch-shaped success-only function"
        );
    }

    pub fn acc3_legitimate_delegation_not_flagged() {
        let dir = tmp_dir("acc3");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
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

    pub fn acc4_rejected_authorization_ignored_detected() {
        let dir = tmp_dir("acc4");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(kinds(&outcome).contains(&FindingKind::IgnoredRejection));
    }

    pub fn acc5_exceptions_converted_to_success_detected() {
        let dir = tmp_dir("acc5");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(kinds(&outcome).contains(&FindingKind::ExceptionToSuccess));
        assert!(kinds(&outcome).contains(&FindingKind::SwallowedException));
    }

    pub fn acc6_unresolved_external_marked_explicitly() {
        let dir = tmp_dir("acc6");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(!outcome.snapshot.evidence.unresolved.is_empty());
        assert!(outcome
            .snapshot
            .evidence
            .unresolved
            .iter()
            .any(|u| u.contains("do_refund") || u.contains("unresolved")));
    }

    pub fn acc7_deletion_invalidates_earlier_finding() {
        let dir = tmp_dir("acc7");
        copy_fixture(FIXTURE, &dir);
        let first = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert!(kinds(&first).contains(&FindingKind::PrintOnlyAuthorization));

        fs::remove_file(dir.join("auth.py")).unwrap();
        let second = cie_ingest::run_ingest(&dir, &opts()).unwrap();
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
            !second
                .snapshot
                .symbols
                .iter()
                .any(|s| {
                    second.snapshot.files.get(s.loc.file.0 as usize).map(|p| p.as_str())
                        == Some("auth.py")
                }),
            "zero symbols may reference a tombstoned file"
        );
    }

    pub fn acc8_polyglot_repo_ingested_end_to_end() {
        let dir = tmp_dir("acc8");
        copy_fixture("polyglot", &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let mut langs: Vec<_> = outcome.store.symbols.iter().map(|s| s.lang).collect();
        langs.sort();
        langs.dedup();
        for want in [
            cie_semantic::LangId::Python,
            cie_semantic::LangId::Go,
            cie_semantic::LangId::Rust,
            cie_semantic::LangId::TypeScript,
        ] {
            assert!(langs.contains(&want), "missing language {want:?}");
        }
        assert!(
            kinds(&outcome).contains(&FindingKind::SqlInjection),
            "TS req.query -> SQL sink must be detected"
        );
    }

    pub fn acc10_unchanged_reingest_fast() {
        let dir = tmp_dir("acc10");
        copy_fixture(FIXTURE, &dir);
        let first = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let second = cie_ingest::run_ingest(&dir, &opts()).unwrap();
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

    pub fn acc11_time_travel_returns_correct_results() {
        let dir = tmp_dir("acc11");
        copy_fixture(FIXTURE, &dir);
        cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let store = Store::open(&dir).unwrap();
        let rev1 = store.last_rev();
        let snap1 = store.load_snapshot(rev1).unwrap().unwrap();
        let symbols1 = snap1.symbols.len();

        fs::write(
            dir.join("extra.py"),
            "def brand_new_function():\n    return 1\n",
        )
        .unwrap();
        let second = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        assert_eq!(second.rev, rev1 + 1);
        assert!(second.snapshot.symbols.len() > symbols1);

        let as_of = cie_ingest::load_outcome(&dir, rev1).unwrap();
        assert!(as_of
            .store
            .symbols
            .iter()
            .all(|s| s.name != "brand_new_function"));
        assert!(second
            .store
            .symbols
            .iter()
            .any(|s| s.name == "brand_new_function"));
    }

    pub fn acc18_19_20_21_22_drift_statuses() {
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
        assert!(report
            .reverse_drift
            .iter()
            .any(|r| r.qual_name.contains("list_purchases")));
        let conflicted = report
            .verifications
            .iter()
            .find(|v| {
                v.claim
                    == report
                        .claims
                        .iter()
                        .find(|c| c.text.contains("log every refund"))
                        .unwrap()
                        .id
            })
            .unwrap();
        assert_eq!(conflicted.status, DriftStatus::Contradicting);
        assert!(conflicted
            .evidence
            .iter()
            .any(|e| e.contains("multi-document conflict")));
    }

    pub fn acc23_drift_delta_shows_only_changes() {
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

    pub fn acc26_verification_timeout_yields_unknown() {
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
        let (report, _) = cie_ingest::run_drift(&dir, &doc_contents, &opts(), None, true).unwrap();
        assert!(
            report
                .verifications
                .iter()
                .all(|v| v.status == DriftStatus::Unknown),
            "timeout must yield Unknown, never Implemented"
        );
    }

    pub fn acc27_to_31_concept_behavior() {
        let dir = tmp_dir("acc27");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let dag = &outcome.snapshot.concepts;

        assert!(dag.nodes.len() > 3);
        assert!(dag
            .nodes
            .iter()
            .any(|n| n.kind == cie_concepts::ConceptKind::UseCase));
        assert!(dag
            .nodes
            .iter()
            .all(|n| n.parent.is_some() || n.kind == cie_concepts::ConceptKind::Domain));

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
            .filter(|n| n.kind == cie_concepts::ConceptKind::Module)
            .flat_map(|n| n.behaviors.iter().map(|b| b.summary))
            .collect();
        let root_behaviors: std::collections::BTreeSet<SymbolId> =
            root.behaviors.iter().map(|b| b.summary).collect();
        assert!(module_behaviors.is_subset(&root_behaviors));

        assert!(dag.nodes.iter().any(|n| !n.extra_edges.is_empty()));

        for n in dag.nodes.iter() {
            for s in &n.symbols {
                assert!(
                    outcome.store.get(*s).is_some(),
                    "node {} references missing symbol {s}",
                    n.label
                );
            }
        }
    }

    pub fn acc41_to_44_capability_spot_checks() {
        let dir = tmp_dir("acc41");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
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

    pub fn acc46_minimum_test_set_computed() {
        let dir = tmp_dir("acc46");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let target = outcome
            .store
            .symbols
            .iter()
            .find(|s| s.qual_name == "auth.good_auth")
            .expect("good_auth symbol");
        let affected =
            cie_analysis::testing::affected_by_change(&outcome.reverse_deps, &[target.id]);
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

    pub fn sarif_export_is_valid() {
        let dir = tmp_dir("sarif");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let sarif = cie_drift::sarif::findings_to_sarif(&outcome.store, &outcome.snapshot.findings);
        assert_eq!(sarif["version"], "2.1.0");
        assert!(sarif["runs"][0]["results"].as_array().unwrap().len() >= 5);
        assert!(
            sarif["runs"][0]["tool"]["driver"]["rules"]
                .as_array()
                .unwrap()
                .len()
                >= 3
        );
    }

    pub fn coverage_gaps_and_test_map() {
        let dir = tmp_dir("cvg");
        copy_fixture(FIXTURE, &dir);
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
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

    pub fn task_024_acc24_drift_rerun_within_5pct() {
        let dir = tmp_dir("rerun");
        build_corpus(&dir);
        let docs = docs_for();

        // t0: cold ingest (full extract + analysis) — one-time cost.
        let t0: Ns = now_ns();
        cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let ingest_cold = now_ns() - t0;

        // t1: no-op re-ingest — the per-run floor (acc #10).
        let t1: Ns = now_ns();
        cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let ingest_noop = now_ns() - t1;

        // t2: cold drift pipeline (cache empty) over the no-op ingest.
        let t2: Ns = now_ns();
        let (first, _) = cie_ingest::run_drift(&dir, &docs, &opts(), None, false).unwrap();
        let drift_cold_total = now_ns() - t2;

        // t3: cached re-run over identical inputs.
        let t3: Ns = now_ns();
        let (second, _) = cie_ingest::run_drift(&dir, &docs, &opts(), None, false).unwrap();
        let drift_warm_total = now_ns() - t3;

        assert_eq!(
            first.claims.len(),
            second.claims.len(),
            "cached re-run returns the same claims"
        );
        assert_eq!(first.rev, second.rev);
        let statuses = |r: &cie_claims::model::DriftReport| -> Vec<DriftStatus> {
            r.verifications.iter().map(|v| v.status).collect()
        };
        assert_eq!(statuses(&first), statuses(&second), "same verdicts");

        // Isolate the drift pipeline on both sides by removing the shared
        // no-op ingest floor; a small absolute epsilon covers the cached
        // report's JSON load and timer granularity.
        let drift_cold = drift_cold_total.saturating_sub(ingest_noop);
        let drift_warm = drift_warm_total.saturating_sub(ingest_noop);
        // Release: the binding ≤ 5% gate with a small load/granularity epsilon.
        // Debug: JSON parsing is ~5× slower — proportionally looser gate.
        let (ratio_div, epsilon_ns): (u64, u64) = if cfg!(debug_assertions) {
            (4, 900_000)
        } else {
            (20, 150_000)
        };
        assert!(
            drift_warm <= drift_cold / ratio_div + epsilon_ns,
            "acc #24: drift re-run {}ns must be ≤ 5% of cold pipeline {}ns \
         (cold ingest {}ns, noop ingest {}ns, totals {}/{}ns)",
            drift_warm,
            drift_cold,
            ingest_cold,
            ingest_noop,
            drift_warm_total,
            drift_cold_total
        );
    }

    pub fn task_025_acc25_llm_off_extracts_60pct_of_explicit_claims() {
        let dir = tmp_dir("llmoff");
        build_corpus(&dir);
        let mut o = opts();
        o.no_llm = true; // LLM fully off — rule-based extraction only
        let docs = docs_for();

        let (report, _) = cie_ingest::run_drift(&dir, &docs, &o, None, false).unwrap();

        // Count explicit claim sentences in the corpus (modal-verb sentences).
        let mut explicit = 0usize;
        for (_, text) in &docs {
            for sentence in text.split(['\n', '.']) {
                let s = sentence.trim();
                if s.split(' ').any(|w| {
                    matches!(
                        w.to_ascii_lowercase().as_str(),
                        "must" | "should" | "shall" | "will"
                    )
                }) && !s.starts_with('#')
                {
                    explicit += 1;
                }
            }
        }
        let extracted = report.claims.len();
        assert!(explicit >= 4, "corpus sanity: some explicit claims exist");
        let ratio = extracted as f64 / explicit as f64;
        assert!(
            ratio >= 0.6,
            "acc #25: LLM-off extraction {extracted}/{} = {:.0}% must be ≥ 60%",
            explicit,
            ratio * 100.0
        );
    }

    pub fn task_017_acc17_ninety_pct_claims_resolve() {
        let dir = tmp_dir("resolve");
        build_corpus(&dir);
        let docs = docs_for();
        let (report, _) = cie_ingest::run_drift(&dir, &docs, &opts(), None, false).unwrap();

        let total = report.verifications.len();
        assert!(total >= 3, "corpus sanity: claims verified");
        let unknown = report
            .verifications
            .iter()
            .filter(|v| v.status == DriftStatus::Unknown)
            .count();
        let ratio = (total - unknown) as f64 / total as f64;
        assert!(
            ratio >= 0.9,
            "acc #17: non-Unknown {}/{} = {:.0}% must be ≥ 90%",
            total - unknown,
            total,
            ratio * 100.0
        );
    }

    // ---- Cross-crate acceptance items, re-dispatched locally (the
    // canonical pinning tests live in cie-langs / cie-llm) ----

    /// Item 9 (FR-18): a new language = 1 line registration + 0 core edits.
    pub fn acc9_fr18_one_line_extension() {
        struct MiniLangAdapter;
        impl cie_adapters::LanguageAdapter for MiniLangAdapter {
            fn id(&self) -> cie_semantic::LangId {
                cie_semantic::LangId::Unknown
            }
            fn extensions(&self) -> &'static [&'static str] {
                &["zz9"]
            }
            fn analyzer(&self) -> &'static str {
                "mini"
            }
            fn extract(&self, path: &str, _src: &str) -> cie_semantic::facts::LangFacts {
                cie_semantic::facts::LangFacts::new(path, cie_semantic::LangId::Unknown, "mini")
            }
        }
        let mut registry = cie_adapters::LanguageRegistry::new();
        registry.register(Box::new(MiniLangAdapter)); // THE one line (§16.2)
        assert!(
            registry.by_extension("zz9").is_some(),
            "FR-18: one register() line resolves the new language"
        );
    }

    /// Item 12: offline labeling — provenance, no network.
    pub fn acc12_offline_label() {
        let provider = cie_llm::OfflineProvider::default();
        let mut cache = cie_llm::LabelCache::default();
        let budget = cie_llm::Budget::unlimited();
        let mut labeler = cie_llm::Labeler::new(&provider, &mut cache, &budget);
        let rec = labeler.label_symbol("auth.authorize", "fn");
        assert_eq!(rec.source, "offline");
        assert_eq!(rec.model_id, "offline-v1");
        assert!(!rec.label.is_empty());
    }

    /// Item 13: cache hits do no model call.
    pub fn acc13_cache_hit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct OnceProvider(AtomicUsize);
        impl cie_llm::LlmProvider for OnceProvider {
            fn id(&self) -> &'static str {
                "once-v1"
            }
            fn kind(&self) -> cie_llm::ProviderKind {
                cie_llm::ProviderKind::OpenAi
            }
            fn complete(&self, _p: &str, _m: u32) -> Result<String, cie_llm::LlmError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("Authorize user".into())
            }
            fn embed(&self, t: &[&str]) -> Result<Vec<Vec<f32>>, cie_llm::LlmError> {
                Ok(t.iter().map(|x| vec![x.len() as f32]).collect())
            }
            fn cost_per_1k(&self) -> (f64, f64) {
                (0.0, 0.0)
            }
        }
        let provider = OnceProvider(AtomicUsize::new(0));
        let mut cache = cie_llm::LabelCache::default();
        let budget = cie_llm::Budget::unlimited();
        let mut labeler = cie_llm::Labeler::new(&provider, &mut cache, &budget);
        let a = labeler.label_symbol("cart.total", "fn");
        let b = labeler.label_symbol("cart.total", "fn");
        assert_eq!(b.source, "cache");
        assert_eq!(provider.0.load(Ordering::SeqCst), 1);
        assert_eq!(a.label, b.label);
    }

    /// Item 14: budget exceeded → downgrade-and-continue.
    pub fn acc14_budget_downgrade() {
        struct PanickyProvider;
        impl cie_llm::LlmProvider for PanickyProvider {
            fn id(&self) -> &'static str {
                "panicky"
            }
            fn kind(&self) -> cie_llm::ProviderKind {
                cie_llm::ProviderKind::OpenAi
            }
            fn complete(&self, _: &str, _: u32) -> Result<String, cie_llm::LlmError> {
                panic!("downgrade must prevent the model call")
            }
            fn embed(&self, _: &[&str]) -> Result<Vec<Vec<f32>>, cie_llm::LlmError> {
                panic!("no network on the downgrade path")
            }
            fn cost_per_1k(&self) -> (f64, f64) {
                (10.0, 30.0)
            }
        }
        let budget = cie_llm::Budget::new(0.0);
        assert!(budget.exceeded());
        let provider = PanickyProvider;
        let mut cache = cie_llm::LabelCache::default();
        let mut labeler = cie_llm::Labeler::new(&provider, &mut cache, &budget);
        let rec = labeler.label_concept(&["charge".into()], "Cart");
        assert_eq!(rec.source, "offline");
        assert!(!rec.label.is_empty());
    }

    /// leaves, over the cart fixture.
    pub fn task_079_cart_three_level_hierarchy() {
        let dir = tmp_dir("levels");
        let src = fixtures_root().join("cart");
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
        }
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();
        let dag = &outcome.snapshot.concepts;

        assert!(dag.nodes.len() > 3, "cart produces a real hierarchy");

        // Level 1: a single Domain root.
        let roots: Vec<_> = dag.nodes.iter().filter(|n| n.parent.is_none()).collect();
        assert_eq!(roots.len(), 1, "one domain root");
        assert_eq!(roots[0].kind, cie_concepts::ConceptKind::Domain);

        // Level 2: Module nodes for the three files hang off the root.
        let modules: Vec<_> = dag
            .nodes
            .iter()
            .filter(|n| n.kind == cie_concepts::ConceptKind::Module)
            .collect();
        assert!(modules.len() >= 3, "cart.py/items.py/pricing.py modules");
        for m in &modules {
            assert_eq!(m.parent, Some(roots[0].id), "module under the domain root");
        }

        // Level 3: leaves (entities/rules) under modules or the root.
        let leaves: Vec<_> = dag
            .nodes
            .iter()
            .filter(|n| {
                !matches!(
                    n.kind,
                    cie_concepts::ConceptKind::Domain | cie_concepts::ConceptKind::Module
                ) && n.parent.is_some()
            })
            .collect();
        assert!(!leaves.is_empty(), "third level exists");

        // Depth check: root -> module -> leaf = 3 levels.
        let depth_of = |mut id: u32| -> usize {
            let mut depth = 1;
            while let Some(node) = dag.get(id) {
                match node.parent {
                    Some(p) => {
                        id = p;
                        depth += 1;
                    }
                    None => break,
                }
            }
            depth
        };
        let max_depth = dag.nodes.iter().map(|n| depth_of(n.id)).max().unwrap();
        assert!(
            max_depth >= 3,
            "three-level hierarchy, got depth {max_depth}"
        );
    }

    /// binds as inert LlmCandidate.
    pub fn task_079_all_four_binding_sources_attach() {
        let dir = tmp_dir("sources");
        let src = fixtures_root().join("cart");
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
        }
        let outcome = cie_ingest::run_ingest(&dir, &opts()).unwrap();

        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in &outcome.snapshot.contracts {
            seen.insert(c.authority.as_str().to_string());
        }
        for f in &outcome.snapshot.findings {
            seen.insert(f.authority.as_str().to_string());
        }
        // Explicit: docstring contracts on Cart.add_item/total.
        assert!(
            seen.contains("explicit"),
            "docstring contract binds Explicit"
        );
        // InferredCandidate: naming-based rules (charge → authorization).
        assert!(
            seen.contains("inferred_candidate"),
            "naming rules bind InferredCandidate"
        );
        // Verified: mechanically checked findings (dead code on list_purchases).
        assert!(
            outcome
                .snapshot
                .findings
                .iter()
                .any(|f| f.kind == FindingKind::DeadCode && f.authority == Authority::Verified),
            "checker findings bind Verified"
        );

        // The LLM channel: an LLM-proposed label binds an LlmCandidate rule.
        let store = &outcome.store;
        let mut summaries = std::collections::BTreeMap::new();
        for s in &outcome.snapshot.summaries {
            summaries.insert(s.sym, s.clone());
        }
        let llm_labeler: cie_analysis::hierarchy::LabelFn =
            &|_intent, fallback| (format!("LLM proposes: {fallback}"), "llm");
        let beh = cie_analysis::behavior::compute_summaries(store, &[], 1, None);
        let dag = cie_analysis::hierarchy::build_hierarchy(
            store,
            &beh,
            &outcome.snapshot.contracts,
            llm_labeler,
            0,
        );
        let llm_rules: Vec<_> = dag
            .nodes
            .iter()
            .flat_map(|n| n.rules.iter())
            .filter(|r| r.authority == Authority::LlmCandidate)
            .collect();
        assert!(
            !llm_rules.is_empty(),
            "LLM-proposed labels bind LlmCandidate rules"
        );

        // Coverage: all four sources now attach somewhere in the engine model.
        seen.insert("llm_candidate".to_string());
        for a in Authority::ALL_SOURCES {
            assert!(
                seen.contains(a.as_str()),
                "binding source {} did not attach",
                a.as_str()
            );
        }
        assert_eq!(Authority::ALL_SOURCES.len(), 4);
    }

    /// FR-44 / acc #28: LlmCandidate is inert — never promotes to decisive.
    pub fn task_079_llm_candidate_inert() {
        use cie_contract::Authority;
        // The promotion API refuses LlmCandidate → Explicit/Verified.
        assert!(Authority::LlmCandidate
            .promote_to(Authority::Explicit)
            .is_err());
        assert!(Authority::LlmCandidate
            .promote_to(Authority::Verified)
            .is_err());
        // Non-LLM hypotheses may be re-targeted freely (they are re-derived).
        assert!(Authority::InferredCandidate
            .promote_to(Authority::Verified)
            .is_ok());
        // LlmCandidate never decides conformance on its own.
        assert!(!Authority::LlmCandidate.is_decisive());
        assert!(Authority::Verified.is_decisive() && Authority::Explicit.is_decisive());
    }
}
/// Item 31: invalidation along parent edges (mini re-dispatch; the
/// full property lives in cie-core/tests/task_082_invalidation.rs).
pub fn acc31_parent_edge_invalidation() {
    use cie_concepts::{ConceptDag, ConceptKind, ConceptNode};
    let now = 0;
    let mut dag = ConceptDag::default();
    let root = dag.push(ConceptNode::new(0, "D", ConceptKind::Domain, now));
    let leaf = dag.push(ConceptNode::new(0, "L", ConceptKind::Entity, now));
    dag.link_child(root, leaf);
    dag.rollup();
    dag.get_mut(leaf).unwrap().confidence = 0.5;
    dag.invalidate(leaf);
    assert!(dag.dirty.contains(&root), "primary parent invalidated");
    dag.rollup_dirty();
    assert!(dag.dirty.is_empty());
    assert!(dag.get(root).unwrap().confidence <= 0.5 + f32::EPSILON);
}
