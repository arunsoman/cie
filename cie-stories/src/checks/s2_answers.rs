//! Deep checks — Asking questions and working with answers (US-008..US-017).

use super::fixture;
use cie_core::types::now_ns;
use std::fs;
use cie_store::records::{Conclusion, DerivedRecord, RecordKind, RecordStoreExt};
use cie_store::Store;

fn sym_id<'a>(
    s: &'a cie_ingest::IngestOutcome,
    name: &str,
) -> Option<cie_core::types::SymbolId> {
    s.snapshot.symbols.iter().find(|f| f.name == name).map(|f| f.id)
}

/// US-008: answers are evidence-assembled — summaries exist and carry the
/// call evidence (checkout → charge), so any answer can cite code.
pub fn us_008() -> Result<(), String> {
    let outcome = fixture::ingest("us008");
    let checkout = sym_id(&outcome, "checkout").ok_or("checkout symbol missing")?;
    let charge = sym_id(&outcome, "charge").ok_or("charge symbol missing")?;
    let co = outcome
        .snapshot
        .summaries
        .iter()
        .find(|s| s.sym == checkout)
        .ok_or("no summary for checkout")?;
    if !co.callees.contains(&charge) {
        return Err("checkout's summary must cite the call to charge".into());
    }
    Ok(())
}

/// US-011: multi-part analysis runs as read-only steps with per-step
/// isolation — a broken file never aborts the healthy files.
pub fn us_011() -> Result<(), String> {
    let dir = fixture::python_repo("us011");
    fs::write(dir.join("bad.py"), "def broken(:\n    pass\n").unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| format!("pipeline aborted instead of isolating the bad step: {e}"))?;
    let s = &outcome.snapshot;
    if sym_id(&outcome, "checkout").is_none() {
        return Err("healthy file's symbols lost when a sibling file is broken".into());
    }
    if !s.files.iter().any(|f| f == "bad.py") {
        return Err("broken file must remain reported (visible), not hidden".into());
    }
    Ok(())
}

/// US-013: the same facts render at multiple levels — JSON roundtrip and
/// SARIF export preserve the finding population exactly.
pub fn us_013() -> Result<(), String> {
    let outcome = fixture::ingest("us013");
    let findings = &outcome.snapshot.findings;
    if findings.is_empty() {
        return Err("fixture should yield at least one finding (ignored error value)".into());
    }
    let json = serde_json::to_string(&outcome.snapshot).map_err(|e| e.to_string())?;
    let back: cie_incremental::snapshot::RevSnapshot =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if back.findings.len() != findings.len() {
        return Err("JSON roundtrip changed the finding population".into());
    }
    let sarif = cie_drift::sarif::findings_to_sarif(&outcome.store, findings);
    let results = sarif["runs"][0]["results"]
        .as_array()
        .ok_or("SARIF must carry runs[0].results")?;
    if results.len() != findings.len() {
        return Err(format!(
            "SARIF exported {} of {} findings — formats must not diverge",
            results.len(),
            findings.len()
        ));
    }
    Ok(())
}

/// US-014: intended vs actual — document claims map to code symbols and get
/// verified statuses with evidence (mismatches classified, not averaged).
pub fn us_014() -> Result<(), String> {
    let dir = fixture::python_repo("us014");
    let (report, _outcome) = cie_ingest::run_drift(
        &dir,
        &[("SPEC.md".into(), fixture::SPEC_MD.into())],
        &cie_ingest::IngestOptions::default(),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    if report.claims.len() < 2 {
        return Err(format!("expected >=2 claims extracted, got {}", report.claims.len()));
    }
    if report.mappings.is_empty() {
        return Err("claims must map to code symbols (intended vs actual needs both sides)".into());
    }
    if report.verifications.len() != report.claims.len() {
        return Err("every claim needs a verification record".into());
    }
    for v in &report.verifications {
        if v.evidence.is_empty() {
            return Err("verification without evidence is a verdict, not a finding".into());
        }
    }
    Ok(())
}

/// US-015: how-it-is-known travels with the data — requirement modality on
/// claims, honest Unknown on timeout (never Implemented), and an explicit
/// unknown conclusion is storable while fabricated clean results are not.
pub fn us_015() -> Result<(), String> {
    let dir = fixture::python_repo("us015");
    // (a) claims carry their modality.
    let claims = cie_drift::pipeline::extract_claims("SPEC.md", fixture::SPEC_MD, now_ns());
    if claims.len() < 3 {
        return Err("fixture doc should extract >=3 modality-bearing claims".into());
    }
    let labels: std::collections::BTreeSet<&str> =
        claims.iter().map(|c| c.modality.as_str()).collect();
    if !labels.contains("must") || !labels.contains("shall") || !labels.contains("should") {
        return Err(format!("modality labels incomplete: {labels:?}"));
    }
    // (b) a timed-out verification reports Unknown with honest evidence.
    let (mut report, _o) = cie_ingest::run_drift(
        &dir,
        &[("SPEC.md".into(), fixture::SPEC_MD.into())],
        &cie_ingest::IngestOptions::default(),
        None,
        true, // inject_timeout
    )
    .map_err(|e| e.to_string())?;
    report.recount();
    let unknown = report
        .verifications
        .iter()
        .find(|v| v.status == cie_claims::model::DriftStatus::Unknown)
        .ok_or("timeout must surface as Unknown status")?;
    if !unknown
        .evidence
        .iter()
        .any(|e| e.contains("never Implemented"))
    {
        return Err("unknown must say it is not a conclusion".into());
    }
    // (c) an explicit unknown conclusion is a storable, honest record.
    let record = DerivedRecord {
        kind: RecordKind::Status,
        payload: serde_json::json!({"symbol_id": "s:demo", "state": "unknown"}),
        evidence: cie_core::evidence::Evidence::default(),
        deps: vec![],
        coverage: 0.0,
        conclusion: Conclusion::Unknown,
        counterexample: None,
        created_ns: now_ns(),
        compute_ns: 0,
    };
    record
        .validate()
        .map_err(|e| format!("honest unknown record must validate: {e}"))?;
    Ok(())
}

/// US-016: answers persist as rev-scoped, content-addressed records that
/// survive reload; time-travel reads never show newer facts as current.
pub fn us_016() -> Result<(), String> {
    let dir = fixture::tmp_dir("us016");
    let store = Store::open(&dir).map_err(|e| e.to_string())?;
    let record = |state: &str| DerivedRecord {
        kind: RecordKind::Status,
        payload: serde_json::json!({"symbol_id": "s:demo", "state": state}),
        evidence: cie_core::evidence::Evidence::default(),
        deps: vec![],
        coverage: 1.0,
        conclusion: Conclusion::Clean,
        counterexample: None,
        created_ns: now_ns(),
        compute_ns: 5,
    };
    let rev: u64 = 7;
    let id1 = store
        .put_record(rev, &record("clean"))
        .map_err(|e| e.to_string())?;
    let id2 = store
        .put_record(rev, &record("clean"))
        .map_err(|e| e.to_string())?;
    if id1 != id2 {
        return Err("content-addressed ids must be deterministic".into());
    }
    let got = store
        .get_record(rev, &id1)
        .map_err(|e| e.to_string())?
        .ok_or("record lost after put")?;
    if got.payload["state"] != "clean" {
        return Err("roundtrip payload mismatch".into());
    }
    // Time travel: the record exists at rev 7, not "at" earlier revs.
    let past = store.get_record_as_of(3, &id1).map_err(|e| e.to_string())?;
    if past.is_some() {
        return Err("as-of read must not leak newer records into older revisions".into());
    }
    let now = store.get_record_as_of(rev, &id1).map_err(|e| e.to_string())?;
    if now.is_none() {
        return Err("as-of read at the storing revision must find the record".into());
    }
    Ok(())
}

/// US-017: failures preserve progress honestly — the LDFI experiment runner
/// refuses to invent a lineage (explicit unknown), ranks minimal fault sets,
/// and every fault cites lineage-edge provenance with an audit hash.
pub fn us_017() -> Result<(), String> {
    use cie_fault::ldfi::{
        Fault, LdfiPlanner, LdfiUnknown, LineageEdge, LineageGraph, LineageKind, Property,
        RoundLimits, SimInjector,
    };
    // Empty lineage → honest unknown, never a fabricated result.
    let mut empty = LdfiPlanner::new(LineageGraph::empty(), SimInjector::new());
    match empty.run(&Property::liveness_from_task_052(), RoundLimits::default()) {
        Err(cie_fault::ldfi::LdfiError::Unknown(LdfiUnknown::NoLineage)) => {}
        other => return Err(format!("empty lineage must be an explicit unknown, got {other:?}")),
    }
    // Real lineage: a→b→c with a state update; Drop of any chain edge breaks it.
    let mut g = LineageGraph::empty();
    let _edge = |id: &str, from: &str, to: &str, kind: LineageKind| LineageEdge {
        id: id.into(),
        kind,
        from: from.into(),
        to: to.into(),
    };
    // The modeled TASK-052 success chain (see SimInjector): the fixture
    // mirrors it so Drop faults actually bite the modeled chain.
    g.add("e1", LineageKind::Message, "debit", "ledger");
    g.add("e2", LineageKind::Message, "ledger", "credit_event");
    g.add("e3", LineageKind::StateUpdate, "credit_event", "balance_update");
    let mut planner = LdfiPlanner::new(g, SimInjector::new());
    let results = planner
        .run(&Property::liveness_from_task_052(), RoundLimits::default())
        .map_err(|e| format!("lineage experiment failed: {e:?}"))?;
    if results.results.is_empty() {
        return Err("planner must run its bounded experiment round".into());
    }
    let dropped = results
        .results
        .iter()
        .find(|r| r.property_violated && matches!(r.faults.as_slice(), [Fault::Drop(_)]))
        .ok_or("a single chain drop must be found to break liveness")?;
    if dropped.provenance.len() != dropped.faults.len() {
        return Err("every fault must cite provenance (global rule 2)".into());
    }
    if dropped.content_hash.is_empty() {
        return Err("experiments need audit hashes (FR-8)".into());
    }
    Ok(())
}
