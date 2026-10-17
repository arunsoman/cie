//! TASK-051 validation: counterexample packaging + minimal reproduction.
//!
//! Sketch deviations (documented, single-point):
//! - `RecordKind::ProhibitionViolation` → `RecordKind::Finding` (the
//!   FR-9 payload tag set; the prohibition id lives in the payload).
//! - `record.counterexample` is the store's `Option<String>`; the
//!   structured §14.9 counterexample serializes into it (FR-9
//!   payload-through) and the round-trip test parses it back.
//! - `base_record(REV_42)` → `violation_record(..)`.

use cie_counterexample::{
    package, package_symbolic, shrink, violation_record, ExecLimits, PathTermination, RawCex,
    Step, SummaryProgram, SymbolicExecutor, VIOLATION_DOUBLE_CHARGE,
};
use cie_core::types::RevId;
use cie_store::records::{Conclusion, DerivedRecord, RecordKind, RecordStoreExt};
use cie_store::Store;

const REV: RevId = 42;

/// The sketch's `fixture_double_charge_raw_cex()`: a 5-step symbolic path
/// with the retry branch.
fn fixture_double_charge_raw_cex() -> RawCex {
    RawCex {
        path: vec![
            "pay".into(),
            "charge".into(),
            "retry=true".into(),
            "charge".into(), // the error step
            "log".into(),
        ],
        assignments: vec![("retry".into(), true)],
        error_atom: "charge".into(),
        min_hits: 2, // double-charge: BOTH charges are the error
        rev: REV,
        provenance: "symbolic path to double_charge".into(),
        suspected: false,
    }
}

/// The sketch's `task_051_package_double_charge_counterexample`.
#[test]
fn task_051_package_double_charge_counterexample() {
    let raw = fixture_double_charge_raw_cex();
    let mut record = violation_record(
        RecordKind::Finding,
        serde_json::json!({ "prohibition": VIOLATION_DOUBLE_CHARGE }),
        REV,
        1_000,
    );
    assert!(record.counterexample.is_none());

    // act: package + minimize
    let id = package(&mut record, &raw, None);
    let cex_json = record.counterexample.expect("§14.9: counterexample must be attached");
    let cex: cie_counterexample::PackagedCounterexample = serde_json::from_str(&cex_json).unwrap();

    // minimality: the 5-step path shrank to the error-reaching core
    // (pay → charge → retry=true → charge); trailing/leading non-error
    // steps are cut while the error step remains
    assert!(
        cex.path.len() < raw.path.len(),
        "shrank from {} to {}",
        raw.path.len(),
        cex.path.len()
    );
    assert!(cex.path.iter().any(|s| s == "charge"), "still reaches the error");
    assert!(cex.path.iter().filter(|s| s.as_str() == "charge").count() == 2,
        "both charges retained — minimality never weakens the counterexample");

    // concrete assignments + populated evidence (rule 2)
    assert_eq!(cex.assignments, vec![("retry".to_string(), true)]);
    assert!(!cex.evidence.provenance.is_empty());
    assert!(!cex.evidence.assumptions.is_empty());
    assert!(cex.evidence.provenance.contains("after shrink"));

    // packaging stamps created_ns (rule 1)
    assert!(record.created_ns > 0);
    // conclusion stays Violation for an established rule; NOT upgraded
    // from anything lower (rule 4 — tested below with suspected=true)
    assert_eq!(record.conclusion, Conclusion::Violation);
    let _ = id;
}

/// Shrinker soundness: a path without the error step is returned
/// unchanged (nothing sound to cut) and minimality never drops the
/// error.
#[test]
fn task_051_shrinker_soundness() {
    let raw = fixture_double_charge_raw_cex();
    let shrunk = shrink(&raw);
    assert!(shrunk.iter().any(|s| s == "charge"));

    // degenerate: no error step at all → unchanged
    let mut bogus = raw.clone();
    bogus.path = vec!["a".into(), "b".into()];
    bogus.error_atom = "charge".into();
    assert_eq!(shrink(&bogus).len(), 2);

    // determinism
    assert_eq!(shrink(&raw), shrink(&raw));
}

/// Round-trip: struct shape + storage keys survive the store (§15.1:
/// `record:{id}` / `evidence:{id}`).
#[test]
fn task_051_store_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "cie051_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir).expect("store opens");

    let raw = fixture_double_charge_raw_cex();
    let mut record = violation_record(
        RecordKind::Finding,
        serde_json::json!({ "prohibition": VIOLATION_DOUBLE_CHARGE }),
        REV,
        2_000,
    );
    let id = package(&mut record, &raw, None);
    let stored_id = store.put_record(REV, &record).expect("insert validates");
    assert_eq!(stored_id, id, "content-addressed id is stable");

    let back = store.get_record(REV, &id).expect("read").expect("present");
    assert_eq!(back.kind, RecordKind::Finding);
    assert_eq!(back.conclusion, Conclusion::Violation);
    let cex: cie_counterexample::PackagedCounterexample =
        serde_json::from_str(back.counterexample.as_ref().unwrap()).unwrap();
    assert_eq!(cex.path, shrink(&raw));
    assert_eq!(cex.assignments, vec![("retry".to_string(), true)]);
    std::fs::remove_dir_all(&dir).ok();
}

/// §13 item 18 bridge: a symbolic path packages onto a record with the
/// generated reproduction test attached, and a SUSPECTED rule keeps its
/// suspected marker (never upgraded authority — rule 4).
#[test]
fn task_051_symbolic_packaging_and_suspected_marker() {
    let prog = SummaryProgram {
        steps: vec![
            Step::Action { name: "charge".into() },
            Step::Branch { flag: "retry".into() },
            Step::Action { name: "charge".into() },
        ],
        entry: cie_core::types::SymbolId(9001),
    };
    let exe = SymbolicExecutor::from_summaries(
        prog,
        &[VIOLATION_DOUBLE_CHARGE.to_string()],
        REV,
    );
    let paths = exe.run(
        cie_core::types::SymbolId(9001),
        ExecLimits { max_steps: 512, budget_ns: 200_000_000 },
    );
    let bad = paths
        .iter()
        .find(|p| p.reached_error(VIOLATION_DOUBLE_CHARGE) && p.termination == PathTermination::ErrorState)
        .expect("double-charge path");

    let mut record = violation_record(
        RecordKind::Finding,
        serde_json::json!({ "prohibition": VIOLATION_DOUBLE_CHARGE, "rule": "inferred" }),
        REV,
        3_000,
    );
    let _id = package_symbolic(&mut record, bad, Some("generated test src".into()), true);
    let cex: cie_counterexample::PackagedCounterexample =
        serde_json::from_str(record.counterexample.as_ref().unwrap()).unwrap();
    assert!(cex.generated_test.as_deref() == Some("generated test src"));
    // suspected marker rides in evidence; the verdict is not silently
    // promoted to an established violation label downstream (FR-12)
    assert!(record.evidence.provenance.contains("SUSPECTED"));
}
