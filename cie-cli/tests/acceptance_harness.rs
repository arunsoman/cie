//! TASK-077: the runnable acceptance registry (PRD §13, items 1-26).
//!
//! `ACCEPTANCE_ITEM_COUNT` is the locked harness constant: the suite
//! registers exactly 26 items; items 15 and 16 carry explicit deferred
//! status (15 = shipped subset, perf-gated; 16 = zero-copy, deferred to
//! v1.0 with TASK-060). Every other item dispatches — via
//! `tests/common/scenarios` — to the same scenario code the standalone
//! acceptance tests run. TASK-084 (below) authorizes extending 26 → 31.

mod common;

use common::scenarios;

/// The locked §13 registry size (items 1-26). Changing this value is a
/// contract change — see TASK-084's meta-test for the authorized extension.
pub const ACCEPTANCE_ITEM_COUNT: usize = 26;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Green,
    /// Registered with explicit deferred status (items 15/16).
    Deferred(&'static str),
}

type Scenario = fn();

#[derive(Clone, Copy)]
pub struct Item {
    pub n: usize,
    pub name: &'static str,
    pub status: Status,
    /// Per-item entry point dispatching to the shared scenario body.
    pub run: Option<Scenario>,
    /// Where the item is pinned when the run lives in another test binary
    /// or behind `#[ignore]` (perf gates).
    pub pinned_by: &'static str,
}

const fn g(n: usize, name: &'static str, run: Scenario, pinned: &'static str) -> Item {
    Item {
        n,
        name,
        status: Status::Green,
        run: Some(run),
        pinned_by: pinned,
    }
}

/// The registry of record. Order = PRD §13 numbering.
pub fn registry() -> [Item; ACCEPTANCE_ITEM_COUNT] {
    [
        g(
            1,
            "Print-only authorization detected",
            scenarios::acc1_print_only_authorization_detected,
            "acceptance.rs",
        ),
        g(
            2,
            "Unconditional success detected",
            scenarios::acc2_unconditional_success_detected,
            "acceptance.rs",
        ),
        g(
            3,
            "Legitimate delegation recognized",
            scenarios::acc3_legitimate_delegation_not_flagged,
            "acceptance.rs",
        ),
        g(
            4,
            "Rejected authorization ignored",
            scenarios::acc4_rejected_authorization_ignored_detected,
            "acceptance.rs",
        ),
        g(
            5,
            "Exceptions converted to success",
            scenarios::acc5_exceptions_converted_to_success_detected,
            "acceptance.rs",
        ),
        g(
            6,
            "Unresolved external flagged",
            scenarios::acc6_unresolved_external_marked_explicitly,
            "acceptance.rs",
        ),
        g(
            7,
            "Deletion invalidates finding (+tombstone)",
            scenarios::acc7_deletion_invalidates_earlier_finding,
            "acceptance.rs",
        ),
        g(
            8,
            "Polyglot repo end-to-end",
            scenarios::acc8_polyglot_repo_ingested_end_to_end,
            "acceptance.rs",
        ),
        g(
            9,
            "New adapter: 1 crate + 1 line (FR-18)",
            scenarios::acc9_fr18_one_line_extension,
            "cie-langs task_011_registry.rs",
        ),
        g(
            10,
            "Re-ingest unchanged ≤ 5%",
            scenarios::acc10_unchanged_reingest_fast,
            "acceptance.rs",
        ),
        g(
            11,
            "Time-travel rev N and N-1",
            scenarios::acc11_time_travel_returns_correct_results,
            "acceptance.rs",
        ),
        g(
            12,
            "LLM offline via Ollama path",
            scenarios::acc12_offline_label,
            "cie-llm task_063_065_066",
        ),
        g(
            13,
            "LLM cache hit ≤ 100 ns",
            scenarios::acc13_cache_hit,
            "cie-llm task_063_065_066 (+perf gate)",
        ),
        g(
            14,
            "Budget exceeded → downgrade",
            scenarios::acc14_budget_downgrade,
            "cie-llm task_063_065_066",
        ),
        Item {
            n: 15,
            name: "§15.4 latency targets (shipped subset)",
            status: Status::Deferred("shipped subset green; full latency tables v1.0"),
            run: None,
            pinned_by: "cie-core/tests/perf_suite.rs (+ task_008_csr)",
        },
        Item {
            n: 16,
            name: "Zero-copy 1M symbols ≤ 10 µs",
            status: Status::Deferred("deferred → v1.0 (TASK-060)"),
            run: None,
            pinned_by: "docs/V0_2_PLAN.md Appendix B",
        },
        g(
            17,
            "≥ 90% claims resolve non-Unknown",
            scenarios::task_017_acc17_ninety_pct_claims_resolve,
            "task_072_076_drift_acceptance.rs",
        ),
        g(
            18,
            "Double-charge → Contradicting",
            scenarios::acc18_19_20_21_22_drift_statuses,
            "acceptance.rs",
        ),
        g(
            19,
            "No-match → Missing",
            scenarios::acc18_19_20_21_22_drift_statuses,
            "acceptance.rs",
        ),
        g(
            20,
            "p99 claim → Unverifiable",
            scenarios::acc18_19_20_21_22_drift_statuses,
            "acceptance.rs",
        ),
        g(
            21,
            "Undocumented code in reverse drift",
            scenarios::acc18_19_20_21_22_drift_statuses,
            "acceptance.rs",
        ),
        g(
            22,
            "Conflicting docs cite both",
            scenarios::acc18_19_20_21_22_drift_statuses,
            "acceptance.rs",
        ),
        g(
            23,
            "--from-rev/--to-rev delta only",
            scenarios::acc23_drift_delta_shows_only_changes,
            "acceptance.rs",
        ),
        g(
            24,
            "Drift re-run ≤ 5%",
            scenarios::task_024_acc24_drift_rerun_within_5pct,
            "task_072_076_drift_acceptance.rs",
        ),
        g(
            25,
            "LLM off → ≥ 60% claims",
            scenarios::task_025_acc25_llm_off_extracts_60pct_of_explicit_claims,
            "task_072_076_drift_acceptance.rs",
        ),
        g(
            26,
            "Timeout → Unknown, never clean",
            scenarios::acc26_verification_timeout_yields_unknown,
            "acceptance.rs",
        ),
    ]
}

/// The harness run: every dispatchable item must pass; the registry must be
/// complete; the status table is printed for the release checklist.
/// TASK-084: the authorized 26 → 31 extension. The base registry is
/// immutable; exactly five items are appended once.
pub const EXTENDED_ACCEPTANCE_ITEM_COUNT: usize = 31;

/// The extended registry: base 26 + concept/CLI items 27-31. Deriving it
/// from `registry()` (rather than re-listing) is what makes "raised exactly
/// once" checkable — the meta-test pins identity of the first 26 entries.
pub fn extended_registry() -> [Item; EXTENDED_ACCEPTANCE_ITEM_COUNT] {
    let base = registry();
    [
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        base[5],
        base[6],
        base[7],
        base[8],
        base[9],
        base[10],
        base[11],
        base[12],
        base[13],
        base[14],
        base[15],
        base[16],
        base[17],
        base[18],
        base[19],
        base[20],
        base[21],
        base[22],
        base[23],
        base[24],
        base[25],
        g(
            27,
            "Concept nodes persisted + evidence (+six-key split)",
            scenarios::acc27_to_31_concept_behavior,
            "acceptance.rs + cie-storage task_078_six_keys.rs",
        ),
        g(
            28,
            "Four sources; LlmCandidate inert",
            scenarios::task_079_all_four_binding_sources_attach,
            "task_079_cart.rs",
        ),
        g(
            29,
            "Merge → DAG; cycles rejected",
            scenarios::acc27_to_31_concept_behavior,
            "acceptance.rs + cie-core task_080_cycle.rs",
        ),
        g(
            30,
            "Rollup == clean recompute",
            scenarios::acc27_to_31_concept_behavior,
            "acceptance.rs",
        ),
        g(
            31,
            "Invalidation along parent edges",
            common::acc31_parent_edge_invalidation,
            "cie-core task_082_invalidation.rs",
        ),
    ]
}

/// TASK-084 meta-test: the count was raised 26 → 31 exactly once, the base
/// registry is untouched by the extension, and items 27-31 all resolve.
#[test]
fn task_084_extension_26_to_31_exactly_once_and_resolves() {
    let _serial = common::SCENARIO_LOCK.lock().unwrap();
    assert_eq!(ACCEPTANCE_ITEM_COUNT, 26, "locked base");
    assert_eq!(EXTENDED_ACCEPTANCE_ITEM_COUNT, 31, "authorized extension");

    let base = registry();
    let ext = extended_registry();
    // Extension applied exactly once: the first 26 entries are the base,
    // verbatim (names, statuses, run pointers).
    for (b, e) in base.iter().zip(ext.iter()) {
        assert_eq!(b.n, e.n, "item numbering untouched");
        assert_eq!(b.name, e.name);
        assert_eq!(b.status, e.status);
        assert_eq!(
            b.run.map(|f| f as usize),
            e.run.map(|f| f as usize),
            "run pointer mismatch on item {}",
            b.n
        );
    }
    // Exactly five new items, numbered 27..=31.
    for (offset, item) in ext[26..].iter().enumerate() {
        assert_eq!(item.n, 27 + offset, "extension items are 27..=31 in order");
    }

    // All 31 resolve: green items dispatch and pass; deferred carry reason.
    let mut green = 0usize;
    let mut deferred = 0usize;
    for item in &ext {
        match item.status {
            Status::Green => {
                item.run.expect("green item dispatches")();
                green += 1;
            }
            Status::Deferred(_) => deferred += 1,
        }
    }
    assert_eq!(
        (green, deferred),
        (29, 2),
        "29 green / 2 deferred after extension"
    );
}

#[test]
fn task_077_acceptance_harness_registry_and_dispatch() {
    let _serial = common::SCENARIO_LOCK.lock().unwrap();
    let items = registry();
    assert_eq!(items.len(), ACCEPTANCE_ITEM_COUNT, "locked registry size");

    let mut green = 0usize;
    let mut deferred = 0usize;
    for item in &items {
        match item.status {
            Status::Green => {
                let run = item.run.expect("green items dispatch to a scenario");
                run();
                green += 1;
            }
            Status::Deferred(reason) => {
                assert!(
                    item.run.is_none(),
                    "deferred items carry no run: {}",
                    item.name
                );
                assert!(!reason.is_empty());
                deferred += 1;
            }
        }
    }
    eprintln!(
        "=== CIE acceptance harness: {green} green / {deferred} deferred / {} total ===",
        items.len()
    );
    assert_eq!(green + deferred, ACCEPTANCE_ITEM_COUNT);
    // The plan's exit shape: 24 green + 2 deferred (15 perf-subset, 16 zero-copy).
    assert_eq!(
        (green, deferred),
        (24, 2),
        "24 green / 2 deferred at v0.2 exit"
    );
    // Deferred items are exactly 15 and 16.
    for item in &items {
        if item.n == 15 || item.n == 16 {
            assert!(
                matches!(item.status, Status::Deferred(_)),
                "item {} must be deferred",
                item.n
            );
        }
    }
}
