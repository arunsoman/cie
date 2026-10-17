//! Deep checks — Debugging and investigations (US-064..US-074).

use super::fixture;
use cie_core::types::now_ns;
use cie_store::records::{Conclusion, DerivedRecord, RecordKind};

/// US-065: competing explanations stay individually inspectable — every
/// experiment carries a distinct audit hash and per-fault provenance.
pub fn us_065() -> Result<(), String> {
    use cie_fault::ldfi::{
        LdfiPlanner, LineageGraph, LineageKind, Property, RoundLimits, SimInjector,
    };
    let mut g = LineageGraph::empty();
    g.add("e1", LineageKind::Message, "debit", "ledger");
    g.add("e2", LineageKind::Message, "ledger", "credit_event");
    let mut planner = LdfiPlanner::new(g, SimInjector::new());
    let results = planner
        .run(&Property::liveness_from_task_052(), RoundLimits::default())
        .map_err(|e| format!("{e:?}"))?;
    let mut hashes: Vec<&str> = results.results.iter().map(|r| r.content_hash.as_str()).collect();
    let n = hashes.len();
    hashes.sort_unstable();
    hashes.dedup();
    if hashes.len() != n {
        return Err("distinct experiments must not share audit hashes".into());
    }
    Ok(())
}

/// US-067: the bookkeeping refuses fabricated conclusions — a Clean record
/// whose evidence carries a timeout marker is rejected; the honest Unknown
/// is accepted.
pub fn us_067() -> Result<(), String> {
    let mut lying = DerivedRecord {
        kind: RecordKind::Status,
        payload: serde_json::json!({"symbol_id": "s:x", "state": "ok"}),
        evidence: cie_core::evidence::Evidence {
            rev: 1,
            provenance: "pass".into(),
            assumptions: vec!["verification timed out after 3ms".into()],
            unsupported: vec![],
            unresolved: vec![],
            generated_at_ns: now_ns(),
        },
        deps: vec![],
        coverage: 1.0,
        conclusion: Conclusion::Clean,
        counterexample: None,
        created_ns: now_ns(),
        compute_ns: 1,
    };
    if lying.validate().is_ok() {
        return Err("a clean conclusion over a timed-out verification must be rejected".into());
    }
    lying.conclusion = Conclusion::Unknown;
    lying
        .validate()
        .map_err(|e| format!("the honest Unknown must validate: {e}"))?;
    Ok(())
}

/// US-068: reproduction candidates are packaged and minimized — the shrinker
/// keeps the error reachable (>= min_hits) and the package lands in a record.
pub fn us_068() -> Result<(), String> {
    use cie_counterexample::package::{package, shrink, RawCex};
    let raw = RawCex {
        path: vec![
            "debit".into(),
            "noise1".into(),
            "credit_event".into(),
            "error".into(),
            "noise2".into(),
            "error".into(),
        ],
        assignments: vec![("user".into(), true), ("amount".into(), false)],
        error_atom: "error".into(),
        min_hits: 2,
        rev: 3,
        provenance: "bounded/engine-v1".into(),
        suspected: true, // rule authority flag (rule 4)
    };
    let shrunk = shrink(&raw);
    let hits = shrunk.iter().filter(|s| s.as_str() == "error").count();
    if hits < raw.min_hits {
        return Err("shrink must keep the error reachable (min_hits)".into());
    }
    if shrunk.len() > raw.path.len() {
        return Err("shrink must not grow the trail".into());
    }
    let mut record = DerivedRecord {
        kind: RecordKind::Counterexample,
        payload: serde_json::json!({"symbol_id": "s:charge"}),
        evidence: cie_core::evidence::Evidence::default(),
        deps: vec![],
        coverage: 1.0,
        conclusion: Conclusion::Violation,
        counterexample: None,
        created_ns: now_ns(),
        compute_ns: 2,
    };
    let id = package(&mut record, &raw, Some("test_repro".into()));
    if record.counterexample.is_none() {
        return Err("packaging must attach the counterexample to the record".into());
    }
    let _ = id;
    Ok(())
}

/// US-072: defect candidates carry their rule, witness and remediation —
/// candidates, never verdicts.
pub fn us_072() -> Result<(), String> {
    let outcome = fixture::ingest("us072");
    let f = outcome
        .snapshot
        .findings
        .iter()
        .find(|f| f.kind == cie_analysis::findings::FindingKind::IgnoredErrorValue)
        .ok_or("fixture must produce a defect candidate")?;
    if f.rule.is_empty() || f.remediation.is_empty() || f.evidence_lines.is_empty() {
        return Err("candidate needs rule + remediation + witness".into());
    }
    if f.authority != cie_contract::Authority::InferredCandidate {
        return Err("static findings are InferredCandidate, never Verified by construction".to_string());
    }
    Ok(())
}

/// US-074: performance candidates from patterns — the N+1 query detector
/// names its pattern and location on a looped-execute fixture.
pub fn us_074() -> Result<(), String> {
    let dir = fixture::tmp_dir("us074");
    std::fs::write(
        dir.join("batch.py"),
        "def load(ids):\n    for i in ids:\n        cursor.execute(\"SELECT * FROM t WHERE id=?\", (i,))\n",
    )
    .unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    let perf = outcome
        .snapshot
        .findings
        .iter()
        .find(|f| f.kind == cie_analysis::findings::FindingKind::NPlusOneQuery)
        .ok_or("looped execute must surface as an N+1 candidate")?;
    if perf.symbol_name.is_empty() || perf.message.is_empty() {
        return Err("performance candidate must name its pattern and location".into());
    }
    Ok(())
}
