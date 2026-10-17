//! Deep checks — Interactive visual exploration (US-018..US-030).
//!
//! The engine's role is the sealed ViewSpec producer + epistemic gate; the
//! host layer (viz-adapter) renders. Checks assert the spec-side obligations.

use super::fixture;
use cie_viz::produce::{produce, ProduceOptions};

fn snap(name: &str) -> cie_incremental::snapshot::RevSnapshot {
    fixture::ingest(name).snapshot
}

/// US-018: the produced view is data-driven — every element carries a
/// display mode and evidence, so the answer's form follows the facts.
pub fn us_018() -> Result<(), String> {
    let spec = produce(&snap("us018"), &ProduceOptions::default()).map_err(|e| e)?;
    if spec.elements.is_empty() {
        return Err("spec must have elements for a non-empty repo".into());
    }
    for e in &spec.elements {
        if e.evidence.file.is_empty() {
            return Err(format!("element {} has no evidence file", e.id));
        }
    }
    cie_viz::gate::validate(&spec).map_err(|vs| {
        vs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("; ")
    })?;
    Ok(())
}

/// US-019: representation switches keep meaning — the semantic hash is
/// stable across layout choices (layout is presentation, not semantics).
pub fn us_019() -> Result<(), String> {
    let s = snap("us019");
    let a = produce(&s, &ProduceOptions::default()).map_err(|e| e)?;
    let b = produce(
        &s,
        &ProduceOptions { layout: Some("grid".into()), ..ProduceOptions::default() },
    )
    .map_err(|e| e)?;
    if a.semantic_hash != b.semantic_hash {
        return Err("semantic hash changed across a layout-only swap".into());
    }
    if a.layout_hint == b.layout_hint {
        return Err("layout override must be visible in the spec".into());
    }
    Ok(())
}

/// US-021: drill to code — elements carry exact source spans.
pub fn us_021() -> Result<(), String> {
    let spec = produce(&snap("us021"), &ProduceOptions::default()).map_err(|e| e)?;
    let with_spans = spec
        .elements
        .iter()
        .filter(|e| e.evidence.end > e.evidence.start)
        .count();
    if with_spans == 0 {
        return Err("no element carries a byte span — drill-down would be dead".into());
    }
    Ok(())
}

/// US-025: focus control — a salience cap hides candidates openly (hidden[]
/// ledger) and never silently drops them.
pub fn us_025() -> Result<(), String> {
    let s = snap("us025");
    let full = produce(&s, &ProduceOptions::default()).map_err(|e| e)?;
    let capped = produce(
        &s,
        &ProduceOptions { max_elements: 3, ..ProduceOptions::default() },
    )
    .map_err(|e| e)?;
    if capped.elements.len() > 3 {
        return Err(format!("cap ignored: {} elements", capped.elements.len()));
    }
    if full.elements.len() > 3 && capped.hidden.is_empty() {
        return Err("hidden candidates must be disclosed when the cap bites".into());
    }
    Ok(())
}

/// US-026: the inspector's data exists — every element states role (kind),
/// label, and how it is known (display mode); hypotheses carry
/// counter-arguments; the gate enforces consistency.
pub fn us_026() -> Result<(), String> {
    let spec = produce(&snap("us026"), &ProduceOptions::default()).map_err(|e| e)?;
    for e in &spec.elements {
        if e.label.is_empty() || e.element_kind.is_empty() {
            return Err(format!("element {} lacks label/kind", e.id));
        }
        if e.display_mode == cie_viz::model::DisplayMode::Hypothesis
            && e.counter_argument.is_none()
        {
            return Err(format!("hypothesis {} without counter-argument", e.id));
        }
    }
    cie_viz::gate::validate(&spec).map_err(|vs| {
        vs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("; ")
    })?;
    Ok(())
}

/// US-027: views stay aligned — the producer is deterministic (same
/// snapshot ⇒ identical ids + hash) and names no engine (VIZ-10).
pub fn us_027() -> Result<(), String> {
    let s = snap("us027");
    let a = produce(&s, &ProduceOptions::default()).map_err(|e| e)?;
    let b = produce(&s, &ProduceOptions::default()).map_err(|e| e)?;
    if a.elements.iter().map(|e| e.id.as_str()).collect::<Vec<_>>()
        != b.elements.iter().map(|e| e.id.as_str()).collect::<Vec<_>>()
    {
        return Err("element ids must be stable across identical produces".into());
    }
    if a.semantic_hash != b.semantic_hash {
        return Err("semantic hash must be deterministic".into());
    }
    let json = serde_json::to_string(&a).map_err(|e| e.to_string())?;
    for banned in ["\"g6\"", "\"jointjs\"", "\"reactflow\"", "\"mermaid\""] {
        if json.contains(banned) {
            return Err(format!("engine id {banned} leaked into CIE output (VIZ-10)"));
        }
    }
    Ok(())
}

/// US-029: overlay source data — the test-confidence overlay merges from
/// the test map; the engine provides it without executing anything.
pub fn us_029() -> Result<(), String> {
    let outcome = fixture::ingest("us029");
    let map = &outcome.test_map;
    if map.reach.is_empty() {
        return Err("fixture must produce a non-empty test map for overlays".into());
    }
    Ok(())
}

/// US-030: the drawing discloses its layout strategy (never hidden meaning).
pub fn us_030() -> Result<(), String> {
    let spec = produce(&snap("us030"), &ProduceOptions::default()).map_err(|e| e)?;
    let hint = spec
        .layout_hint
        .as_ref()
        .ok_or("layout hint missing — strategy would be invisible")?;
    if hint.strategy.is_empty() {
        return Err("layout strategy name empty".into());
    }
    Ok(())
}
