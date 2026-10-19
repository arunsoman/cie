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

// The ViewSpec wire-protocol roundtrip is a real, shipped property — but it
// is not what US-134 (the MCP agent seam) claims. It used to sit at this
// module as `us_134`, which made `cie stories verify --id US-134` audit an
// unrelated capability while the real MCP semantics went unchecked (issue
// #9). US-134's dispatch now points at s11_closes::us_134; this property
// stays covered as a plain test, labelled with what it actually verifies.
#[cfg(test)]
mod wire_roundtrip {
    use super::fixture;

    /// The ViewSpec wire protocol roundtrips under the strict schema
    /// (deny_unknown_fields), so a machine consumer can rely on it.
    #[test]
    fn view_spec_wire_roundtrip_is_schema_stable() -> Result<(), String> {
        let spec = cie_viz::produce::produce(
            &fixture::ingest("us134").snapshot,
            &cie_viz::produce::ProduceOptions::default(),
        )
        .map_err(|e| e)?;
        let wire = serde_json::to_string(&spec).map_err(|e| e.to_string())?;
        let back: cie_viz::ViewSpec = serde_json::from_str(&wire)
            .map_err(|e| format!("wire protocol not schema-stable: {e}"))?;
        assert_eq!(
            back.semantic_hash, spec.semantic_hash,
            "protocol roundtrip changed the sealed hash"
        );
        assert_eq!(
            back.elements.len(),
            spec.elements.len(),
            "protocol roundtrip changed the element population"
        );
        Ok(())
    }
}
