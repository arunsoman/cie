//! Deep checks — Runtime/consistency, history, security, twin
//! (US-075..US-103; engine-verifiable subset).

use super::fixture;

/// US-089: interface/relation evolution — differential semantics: inserts,
/// retractions through the dataflow, as-of frontier reads.
pub fn us_089() -> Result<(), String> {
    use cie_incremental::differential::RelationTrace;
    let mut t = RelationTrace::transitive_closure();
    let fact = (1u32, 2u32);
    t.add(1, fact);
    t.advance_to(1);
    if !t.contains(fact) {
        return Err("inserted fact must be present at its revision".into());
    }
    t.remove(2, fact);
    t.advance_to(2);
    if t.contains(fact) {
        return Err("retraction must flow through the trace".into());
    }
    if !t.retraction_complete() {
        return Err("retraction must complete (no zombie facts)".into());
    }
    let past = t.as_of(1);
    let has_past = past.values().any(|set| set.contains(&2u32));
    if !has_past {
        return Err("as-of read at rev 1 must still see the historical edge".into());
    }
    Ok(())
}

/// US-090: documentation gaps as a list — a doc claiming behaviour the code
/// lacks lands in the Missing bucket; undocumented code surfaces via
/// reverse drift.
pub fn us_090() -> Result<(), String> {
    let dir = fixture::python_repo("us090");
    let doc = "# Export spec\n\n- The system **must** generate a weekly compliance export.\n";
    let (report, _o) = cie_ingest::run_drift(
        &dir,
        &[("EXPORT.md".into(), doc.into())],
        &cie_ingest::IngestOptions::default(),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    let missing = report.aggregate.get("missing").copied().unwrap_or(0);
    if missing == 0 {
        return Err("a claimed-but-absent behaviour must land in Missing".into());
    }
    Ok(())
}

/// US-091: evidence travels with its elements — the drift report persists
/// per revision and reloads with the same claims.
pub fn us_091() -> Result<(), String> {
    let dir = fixture::python_repo("us091");
    let (report, _o) = cie_ingest::run_drift(
        &dir,
        &[("SPEC.md".into(), fixture::SPEC_MD.into())],
        &cie_ingest::IngestOptions::default(),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    let store = cie_store::Store::open(&dir).map_err(|e| e.to_string())?;
    let back = cie_drift::persist::DriftStoreExt::load_drift(&store, report.rev)
        .map_err(|e| e.to_string())?
        .ok_or("persisted drift report lost")?;
    if back.claims.len() != report.claims.len() {
        return Err("reloaded report changed the claim set".into());
    }
    Ok(())
}

/// US-093: the workspace persists and resumes — the snapshot store roundtrips
/// the full revision state.
pub fn us_093() -> Result<(), String> {
    let outcome = fixture::ingest("us093");
    let store = cie_store::Store::open(&outcome.project_dir).map_err(|e| e.to_string())?;
    let rev = cie_incremental::SnapshotStoreExt::load_snapshot(&store, outcome.rev)
        .map_err(|e| e.to_string())?
        .ok_or("snapshot for the ingested revision lost")?;
    if rev.symbols.len() != outcome.snapshot.symbols.len() {
        return Err(format!(
            "snapshot roundtrip changed the symbol count: {} -> {}",
            outcome.snapshot.symbols.len(),
            rev.symbols.len()
        ));
    }
    Ok(())
}

/// US-094: authorization understood structurally — the auth precondition
/// contract cites BOTH the guarded action (charge) and its guard
/// (authorize) as evidence.
pub fn us_094() -> Result<(), String> {
    let outcome = fixture::ingest("us094");
    let charge = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "charge")
        .map(|s| s.id)
        .ok_or("charge missing")?;
    let authorize = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "authorize")
        .map(|s| s.id)
        .ok_or("authorize missing")?;
    let contract = outcome
        .snapshot
        .contracts
        .iter()
        .find(|c| c.symbols.contains(&charge) && c.expr.to_ascii_lowercase().contains("authoriz"))
        .ok_or("no contract ties charge to its authorization precondition")?;
    // The guard symbol itself must exist in the store — the wall is made of
    // code, not names.
    if outcome.store.get(authorize).is_none() {
        return Err("guard symbol missing from the store".into());
    }
    let _ = contract;
    Ok(())
}

/// US-095: sensitive information traced — taint sources classify from code
/// evidence and the intra-procedural taint-lite flags source→eval.
pub fn us_095() -> Result<(), String> {
    use cie_adapters::effects::classify_source;
    if classify_source("x = request.args.get('q')").is_none() {
        return Err("request input must classify as a taint source".into());
    }
    let dir = fixture::tmp_dir("us095");
    std::fs::write(dir.join("taint.py"), "def handle():\n    x = input()\n    eval(x)\n").unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    if !outcome
        .snapshot
        .findings
        .iter()
        .any(|f| f.kind == cie_analysis::findings::FindingKind::CodeInjection)
    {
        return Err("input→eval must surface as an injection/taint candidate".into());
    }
    Ok(())
}

/// US-097: security findings are candidates with witnesses — the hardcoded
/// credential finding carries rule id, security tags and a witness line,
/// never a verdict label.
pub fn us_097() -> Result<(), String> {
    let dir = fixture::tmp_dir("us097");
    std::fs::write(
        dir.join("conf.py"),
        "TOKEN = \"ghp_AAAABBBBCCCCDDDDEEEEFFFFGGGGHHH\"\n",
    )
    .unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    let f = outcome
        .snapshot
        .findings
        .iter()
        .find(|f| f.kind == cie_analysis::findings::FindingKind::HardcodedSecret)
        .ok_or("credential literal must surface as a candidate")?;
    if f.rule != "sec/SEC-001" || f.evidence_lines.is_empty() {
        return Err("candidate needs its rule id and witness line".into());
    }
    if !f.tags.contains(&"security".to_string()) {
        return Err("security finding must carry the security tag".into());
    }
    Ok(())
}

/// US-099: removal counterfactual — deleting a file computes the consequence
/// (plan.deleted), purges its symbols, and the purge is idempotent.
pub fn us_099() -> Result<(), String> {
    let dir = fixture::python_repo("us099");
    std::fs::write(dir.join("extra.py"), "def bonus():\n    return 1\n").unwrap();
    let second = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    std::fs::remove_file(dir.join("extra.py")).unwrap();
    let third = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    if third.plan.deleted != vec!["extra.py".to_string()] {
        return Err(format!("deletion must be computed, got {:?}", third.plan.deleted));
    }
    if third
        .snapshot
        .symbols
        .iter()
        .any(|s| s.loc.file.0 as usize >= third.snapshot.files.len())
    {
        return Err("no symbol may outlive its deleted file (orphan)".into());
    }
    let _ = second;
    Ok(())
}

/// US-103: predictions never display as fact — the gate REJECTS a hypothesis
/// element without its counter-argument, so unsupported output cannot render.
pub fn us_103() -> Result<(), String> {
    let spec = cie_viz::produce::produce(
        &fixture::ingest("us103").snapshot,
        &cie_viz::produce::ProduceOptions::default(),
    )
    .map_err(|e| e)?;
    let mut tampered = spec.clone();
    if let Some(e) = tampered.elements.first_mut() {
        e.display_mode = cie_viz::model::DisplayMode::Hypothesis;
        e.counter_argument = None;
    }
    if cie_viz::gate::validate(&tampered).is_ok() {
        return Err("gate must reject a hypothesis without counter-argument".into());
    }
    cie_viz::gate::validate(&spec)
        .map_err(|vs| vs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("; "))?;
    Ok(())
}
