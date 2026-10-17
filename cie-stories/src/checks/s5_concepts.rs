//! Deep checks — Behaviour, concepts and business rules (US-050..US-056).

use super::fixture;
use cie_concepts::persist::ConceptStoreExt;
use std::collections::BTreeMap;

fn summaries_map(
    outcome: &cie_ingest::IngestOutcome,
) -> BTreeMap<cie_core::types::SymbolId, cie_summary::FunctionSummary> {
    outcome
        .snapshot
        .summaries
        .iter()
        .map(|s| (s.sym, s.clone()))
        .collect()
}

/// US-050: concept cards persist and reload (reviewable card set that
/// outlives the process).
pub fn us_050() -> Result<(), String> {
    let outcome = fixture::ingest("us050");
    let dir = outcome.project_dir.clone();
    let store = cie_store::Store::open(&dir).map_err(|e| e.to_string())?;
    let dag = outcome.snapshot.concepts.clone();
    if dag.nodes.is_empty() {
        return Err("fixture must produce concept cards to persist".into());
    }
    store
        .save_concept_six(outcome.rev, &dag)
        .map_err(|e| e.to_string())?;
    let back = store
        .load_concept_six(outcome.rev)
        .map_err(|e| e.to_string())?
        .ok_or("persisted concept set lost")?;
    if back.nodes.len() != dag.nodes.len() {
        return Err(format!(
            "card roundtrip changed the set: {} -> {}",
            dag.nodes.len(),
            back.nodes.len()
        ));
    }
    Ok(())
}

/// US-051: two axes — structural containment (parent/children links) and
/// specialisation (FCA intent) coexist on the same card set.
pub fn us_051() -> Result<(), String> {
    let outcome = fixture::ingest("us051");
    let nodes = &outcome.snapshot.concepts.nodes;
    if nodes.len() < 2 {
        return Err("hierarchy needs >=2 nodes (root + derived)".into());
    }
    let linked = nodes.iter().filter(|n| n.parent.is_some()).count();
    if linked == 0 {
        return Err("no structural containment links in the concept DAG".into());
    }
    let specialised = nodes.iter().filter(|n| !n.intent.is_empty()).count();
    if specialised == 0 {
        return Err("no specialisation (FCA intent) axis in the concept DAG".into());
    }
    Ok(())
}

/// US-053: the rule catalogue comes from the naming lexicon — promised
/// actions are derived from names deterministically, with registry kinds.
pub fn us_053() -> Result<(), String> {
    let promised = cie_contract::naming::NamingAdapter::promised_actions("authorize_user");
    if promised.is_empty() {
        return Err("lexicon must promise actions for 'authorize_user'".into());
    }
    let promised2 = cie_contract::naming::NamingAdapter::promised_actions("load_record");
    if promised2.is_empty() {
        return Err("lexicon must promise actions for 'load_record'".into());
    }
    let neutral = cie_contract::naming::NamingAdapter::promised_actions("do_stuff");
    if !neutral.is_empty() {
        return Err("lexicon must not invent promises for neutral names".into());
    }
    Ok(())
}

/// US-054: intended vs actual with named statuses — the doc's authorize-
/// before-charge rule is verified against the real fixture code, and every
/// verification lands in a named DriftStatus bucket.
pub fn us_054() -> Result<(), String> {
    let dir = fixture::python_repo("us054");
    let (report, _o) = cie_ingest::run_drift(
        &dir,
        &[("SPEC.md".into(), fixture::SPEC_MD.into())],
        &cie_ingest::IngestOptions::default(),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    if report.aggregate.is_empty() {
        return Err("drift report must aggregate statuses".into());
    }
    let known: std::collections::BTreeSet<&str> = [
        "implemented",
        "partial",
        "missing",
        "contradicting",
        "drifted",
        "undocumented",
        "unverifiable",
        "unknown",
        "superseded",
    ]
    .into_iter()
    .collect();
    for k in report.aggregate.keys() {
        if !known.contains(k.as_str()) {
            return Err(format!("unknown status bucket '{k}' in aggregate"));
        }
    }
    Ok(())
}

/// US-055: workflow states with honest authority — the FR-15 template edges
/// always exist, but a refund CALL without an external write can only
/// evidence RefundCalled, never Refunded (RefundCalled ≠ Refunded).
pub fn us_055() -> Result<(), String> {
    let outcome = fixture::ingest("us055");
    let store = &outcome.store;
    let summaries = summaries_map(&outcome);
    let model =
        cie_contract::obligations::StateModelBuilder::from_effects(store, &summaries);
    if model.edges.is_empty() {
        return Err("FR-15 template edges must always be present".into());
    }
    if cie_contract::obligations::StateModelBuilder::refund_effective_is_evidenced(&model) {
        return Err(
            "fixture refund() has no external write — Refunded must NOT be evidenced".to_string(),
        );
    }
    // The call side IS evidenced: a transition provenance cites the refund symbol.
    let refund = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "refund")
        .map(|s| s.id)
        .ok_or("refund symbol missing")?;
    if !model
        .edges
        .iter()
        .any(|t| t.provenance.iter().any(|p| p.symbol == refund))
    {
        return Err("RefundCalled provenance must cite the refund symbol".into());
    }
    Ok(())
}

/// US-056: misleading names caught — `authorize` that only prints: the
/// lexicon promises an auth action, the summary observes Print, and the
/// naming adapter records the mismatch (InferredCandidate authority).
pub fn us_056() -> Result<(), String> {
    let dir = fixture::tmp_dir("us056");
    std::fs::write(
        dir.join("fake.py"),
        "def authorize(user):\n    print(\"authorized \" + user)\n",
    )
    .unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    let auth = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "authorize")
        .map(|s| s.id)
        .ok_or("authorize symbol missing")?;
    let s = outcome
        .snapshot
        .summaries
        .iter()
        .find(|s| s.sym == auth)
        .ok_or("no summary for authorize")?;
    if !s.effects.contains(&cie_summary::EffectKind::Print) {
        return Err("observed behaviour (print) must be visible next to the name's promise".to_string());
    }
    let summaries = summaries_map(&outcome);
    let adapter = cie_contract::naming::NamingAdapter::lexicon_default();
    let records = adapter.run(&outcome.store, &summaries, outcome.rev);
    let rec = records
        .iter()
        .find(|r| r.subject == auth)
        .ok_or("naming adapter must record the authorize mismatch")?;
    if rec.kind != cie_contract::naming::NamingKind::NameMismatch {
        return Err("expected NameMismatch kind".into());
    }
    Ok(())
}
