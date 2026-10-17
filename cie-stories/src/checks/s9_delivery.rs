//! Deep checks — P2F/delivery/editor-agent engine subset (US-104..US-134).

use super::fixture;

/// US-107: conflicting requirement statements are captured as classified,
/// individually inspectable claims (prohibition vs obligation) — the raw
/// material for conflict resolution, neither averaged away nor silently
/// dropped.
pub fn us_107() -> Result<(), String> {
    let doc = "# Policy\n\n- The system **must not** refund without authorization.\n- The system **must** refund without authorization when the user asks.\n";
    let claims = cie_drift::pipeline::extract_claims("POLICY.md", doc, cie_core::types::now_ns());
    if claims.len() < 2 {
        return Err(format!("both conflicting statements must be captured, got {}", claims.len()));
    }
    let prohibitions = claims
        .iter()
        .filter(|c| c.kind == cie_claims::model::ClaimKind::Prohibition)
        .count();
    if prohibitions != 1 {
        return Err("the prohibition must be classified as such, not averaged with the obligation".into());
    }
    Ok(())
}

/// US-133: definitions and references navigable from the semantic graph —
/// a symbol resolves to its definition site AND its referents.
pub fn us_133() -> Result<(), String> {
    let outcome = fixture::ingest("us133");
    let checkout = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "checkout")
        .cloned()
        .ok_or("definition not found")?;
    if checkout.loc.start >= checkout.loc.end {
        return Err("definition must carry a usable span".into());
    }
    let charge = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "charge")
        .map(|s| s.id)
        .ok_or("charge missing")?;
    let refs = outcome.reverse_deps.get(&charge);
    match refs {
        Some(r) if r.contains(&checkout.id) => Ok(()),
        other => Err(format!("references must resolve through reverse deps, got {other:?}")),
    }
}

/// US-134: evidence-shaped answers for agents — the ViewSpec wire protocol
/// roundtrips under the strict schema (deny_unknown_fields), so a machine
/// consumer can rely on it; records carry typed JSON payloads.
pub fn us_134() -> Result<(), String> {
    let spec = cie_viz::produce::produce(
        &fixture::ingest("us134").snapshot,
        &cie_viz::produce::ProduceOptions::default(),
    )
    .map_err(|e| e)?;
    let wire = serde_json::to_string(&spec).map_err(|e| e.to_string())?;
    let back: cie_viz::ViewSpec = serde_json::from_str(&wire)
        .map_err(|e| format!("wire protocol not schema-stable: {e}"))?;
    if back.semantic_hash != spec.semantic_hash {
        return Err("protocol roundtrip changed the sealed hash".into());
    }
    if back.elements.len() != spec.elements.len() {
        return Err("protocol roundtrip changed the element population".into());
    }
    Ok(())
}
