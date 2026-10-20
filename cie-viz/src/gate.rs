//! The epistemic gate (CIE-VIZ PRD §4.1) — the ONLY honesty enforcement
//! point in the viz layer, implemented twice (Rust producer-side and the
//! TypeScript adapter) with identical stable error codes.
//!
//! `gate()` runs before anything is drawn: a spec that fails here never
//! reaches G6, and renderers downstream never re-check (they trust the
//! gate — VIZ-B-01).

use crate::model::{DisplayMode, ViewSpec};
use std::collections::{BTreeSet, HashMap};

/// Stable, wire-visible gate error codes (identical strings in the TS
/// adapter — TC-07 asserts on them verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GateCode {
    ElementWithoutEvidence,
    NonFactWithoutClaim,
    HypothesisWithoutCounter,
    FogWithoutReason,
    EdgeWithoutEvidence,
    EdgeUnknownEndpoint,
    DuplicateElementId,
    ElementCapExceeded,
    /// RFC-0024 patch 3 — an evidence-gated body line failed the gate.
    BodyLineWithoutEvidence,
    /// RFC-0024 patch 3 — a paired-popup second ref is malformed.
    AlsoEvidenceMalformed,
}

impl GateCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            GateCode::ElementWithoutEvidence => "ELEMENT_WITHOUT_EVIDENCE",
            GateCode::NonFactWithoutClaim => "NON_FACT_WITHOUT_CLAIM",
            GateCode::HypothesisWithoutCounter => "HYPOTHESIS_WITHOUT_COUNTER",
            GateCode::FogWithoutReason => "FOG_WITHOUT_REASON",
            GateCode::EdgeWithoutEvidence => "EDGE_WITHOUT_EVIDENCE",
            GateCode::EdgeUnknownEndpoint => "EDGE_UNKNOWN_ENDPOINT",
            GateCode::DuplicateElementId => "DUPLICATE_ELEMENT_ID",
            GateCode::ElementCapExceeded => "ELEMENT_CAP_EXCEEDED",
            GateCode::BodyLineWithoutEvidence => "BODY_LINE_WITHOUT_EVIDENCE",
            GateCode::AlsoEvidenceMalformed => "ALSO_EVIDENCE_MALFORMED",
        }
    }
}

/// One gate violation: code + the element/edge id it concerns + detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateViolation {
    pub code: GateCode,
    pub id: String,
    pub detail: String,
}

impl std::fmt::Display for GateViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}] {}", self.code.as_str(), self.id, self.detail)
    }
}

/// PRD VIZ-I-06: salience-bounded cap of 2 000 elements per view. The
/// producer truncates (recording hidden candidates + a gap); the gate
/// refuses any spec that still exceeds the cap.
pub const ELEMENT_CAP: usize = 2_000;

/// RFC-0024 patch 3 — well-formed EvidenceRef inside the open `fields`
/// bag (identical rule to the TS `isWellFormedRef`): non-empty file,
/// finite non-negative start, end ≥ start.
fn well_formed_ref(r: &serde_json::Value) -> bool {
    let Some(obj) = r.as_object() else { return false };
    let Some(file) = obj.get("file").and_then(|f| f.as_str()) else { return false };
    if file.is_empty() {
        return false;
    }
    let (Some(start), Some(end)) = (
        obj.get("start").and_then(|v| v.as_f64()),
        obj.get("end").and_then(|v| v.as_f64()),
    ) else {
        return false;
    };
    start.is_finite() && end.is_finite() && start >= 0.0 && end >= start
}

/// RFC-0024 patch 3 check 1 — `fields.bodyLines[]` narration is
/// evidence-gated: every line carries its own EvidenceRef. A narration
/// line without evidence is a fabricated fact — rejected, never drawn.
fn check_body_lines(el: &crate::model::ViewElement, v: &mut Vec<GateViolation>) {
    let Some(lines) = el.fields.get("bodyLines") else { return };
    let ok = lines
        .as_array()
        .map(|arr| {
            arr.iter().all(|line| {
                line.as_object()
                    .and_then(|o| o.get("evidence"))
                    .map(well_formed_ref)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !ok {
        v.push(GateViolation {
            code: GateCode::BodyLineWithoutEvidence,
            id: el.id.clone(),
            detail: "fields.bodyLines must be an array of lines each carrying a well-formed evidence ref".into(),
        });
    }
}

/// RFC-0024 patch 3 check 2 — `fields.alsoEvidence[]` second locations
/// for paired popups: every ref must be well-formed. A paired popup is
/// only drawn when BOTH refs exist (ux-spec §2.2); a malformed ref would
/// draw a blank column.
fn check_also_evidence(el: &crate::model::ViewElement, v: &mut Vec<GateViolation>) {
    let Some(also) = el.fields.get("alsoEvidence") else { return };
    let ok = also
        .as_array()
        .map(|arr| arr.iter().all(well_formed_ref))
        .unwrap_or(false);
    if !ok {
        v.push(GateViolation {
            code: GateCode::AlsoEvidenceMalformed,
            id: el.id.clone(),
            detail: "fields.alsoEvidence must be an array of well-formed evidence refs".into(),
        });
    }
}

/// Validate a spec against every gate clause. Empty `Ok(())` means the
/// spec may be adapted and drawn; anything else lists ALL violations
/// (not just the first) so producers can fix specs in one pass.
pub fn validate(spec: &ViewSpec) -> Result<(), Vec<GateViolation>> {
    let mut v: Vec<GateViolation> = Vec::new();

    // Element ids must be unique (provenance audit prerequisite, VIZ-H-01).
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for el in &spec.elements {
        if !seen.insert(el.id.as_str()) {
            v.push(GateViolation {
                code: GateCode::DuplicateElementId,
                id: el.id.clone(),
                detail: "element id appears twice".into(),
            });
        }
        // VIZ-A-03: every element has evidence — no element without it.
        if el.evidence.file.is_empty() && el.evidence.start == 0 && el.evidence.end == 0 {
            v.push(GateViolation {
                code: GateCode::ElementWithoutEvidence,
                id: el.id.clone(),
                detail: "element carries no evidence ref".into(),
            });
        }
        // VIZ-A-04: displayMode ≠ Fact ⇒ claimId present.
        if el.display_mode != DisplayMode::Fact && el.claim_id.is_none() {
            v.push(GateViolation {
                code: GateCode::NonFactWithoutClaim,
                id: el.id.clone(),
                detail: format!("displayMode {} requires claimId", el.display_mode.as_str()),
            });
        }
        // VIZ-A-05: Hypothesis ⇒ counterArgument present.
        if el.display_mode == DisplayMode::Hypothesis && el.counter_argument.is_none() {
            v.push(GateViolation {
                code: GateCode::HypothesisWithoutCounter,
                id: el.id.clone(),
                detail: "hypothesis without a counter-argument".into(),
            });
        }
        // VIZ-A-06: Fog ⇒ fogReason present.
        if el.display_mode == DisplayMode::Fog && el.fog_reason.is_none() {
            v.push(GateViolation {
                code: GateCode::FogWithoutReason,
                id: el.id.clone(),
                detail: "fogged element without fogReason".into(),
            });
        }
        // RFC-0024 patch 3: evidence-gated bodyLines + well-formed
        // alsoEvidence (the two additive checks run HERE — no bypass).
        check_body_lines(el, &mut v);
        check_also_evidence(el, &mut v);
    }

    // VIZ-I-06: the cap is enforced at the gate — an over-cap spec must
    // have been salience-bounded by the producer before it gets here.
    if spec.elements.len() > ELEMENT_CAP {
        v.push(GateViolation {
            code: GateCode::ElementCapExceeded,
            id: "*".into(),
            detail: format!(
                "{} elements exceed the {}-element cap",
                spec.elements.len(),
                ELEMENT_CAP
            ),
        });
    }

    // Edges: evidence required (PRD §4.1) and both endpoints must exist
    // (provenance audit VIZ-H-01 support — nothing is drawn that is not
    // in the spec).
    let ids: HashMap<&str, &crate::model::ViewElement> =
        spec.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    for e in &spec.edges {
        if e.evidence.file.is_empty() && e.evidence.start == 0 && e.evidence.end == 0 {
            v.push(GateViolation {
                code: GateCode::EdgeWithoutEvidence,
                id: e.id.clone(),
                detail: "edge carries no evidence ref".into(),
            });
        }
        if !ids.contains_key(e.source.as_str()) || !ids.contains_key(e.target.as_str()) {
            v.push(GateViolation {
                code: GateCode::EdgeUnknownEndpoint,
                id: e.id.clone(),
                detail: format!("{} -> {}", e.source, e.target),
            });
        }
    }

    if v.is_empty() {
        Ok(())
    } else {
        Err(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        EpistemicFooter, EvidenceRef, Salience, ScopeAttestation, ScopeCoverage, ViewElement,
        ViewSpec,
    };
    use std::collections::BTreeMap;

    fn el(id: &str, mode: DisplayMode) -> ViewElement {
        ViewElement {
            id: id.into(),
            element_kind: "concept".into(),
            display_mode: mode,
            label: id.into(),
            evidence: EvidenceRef { file: "a.py".into(), start: 1, end: 9 },
            claim_id: None,
            counter_argument: None,
            fog_reason: None,
            salience: Salience {
                factors: vec![],
                weight: 0.5,
                why_shown: "test".into(),
            },
            fields: BTreeMap::new(),
            parent_id: None,
        }
    }

    fn spec(els: Vec<ViewElement>) -> ViewSpec {
        ViewSpec {
            elements: els,
            edges: vec![],
            groups: None,
            footer: EpistemicFooter::empty(),
            gaps: vec![],
            hidden: vec![],
            scope: ScopeAttestation {
                revision: 1,
                environments: vec![],
                coverage: ScopeCoverage { analyzed: true, ratio: None },
            },
            layout_hint: None,
            preferences: None,
            semantic_hash: String::new(),
            render_hash: String::new(),
        }
    }

    /// VIZ-A-03 — unevidenced element is rejected (TC-07's exact case).
    #[test]
    fn rejects_element_without_evidence() {
        let mut s = spec(vec![el("x", DisplayMode::Fact)]);
        s.elements[0].evidence = EvidenceRef { file: String::new(), start: 0, end: 0 };
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::ElementWithoutEvidence));
    }

    /// VIZ-A-04 — non-Fact requires a claimId.
    #[test]
    fn rejects_non_fact_without_claim() {
        let s = spec(vec![el("x", DisplayMode::Inference)]);
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::NonFactWithoutClaim));
    }

    /// VIZ-A-05 — Hypothesis requires a counter-argument.
    #[test]
    fn rejects_hypothesis_without_counter() {
        let mut s = spec(vec![el("x", DisplayMode::Hypothesis)]);
        s.elements[0].claim_id = Some("C-1".into());
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::HypothesisWithoutCounter));
    }

    /// VIZ-A-06 — Fog requires fogReason.
    #[test]
    fn rejects_fog_without_reason() {
        let mut s = spec(vec![el("x", DisplayMode::Fog)]);
        s.elements[0].claim_id = Some("C-2".into());
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::FogWithoutReason));
    }

    /// Fully-attested elements pass; Fact needs no claimId.
    #[test]
    fn accepts_fully_attested_fact() {
        let s = spec(vec![el("x", DisplayMode::Fact)]);
        assert_eq!(validate(&s), Ok(()));
    }

    /// Edge with unknown endpoint is rejected (provenance VIZ-H-01).
    #[test]
    fn rejects_edge_with_unknown_endpoint() {
        let mut s = spec(vec![el("a", DisplayMode::Fact)]);
        s.edges.push(crate::model::ViewEdge {
            id: "e1".into(),
            source: "a".into(),
            target: "ghost".into(),
            edge_kind: "calls".into(),
            display_mode: DisplayMode::Fact,
            evidence: EvidenceRef { file: "a.py".into(), start: 1, end: 2 },
            counter_argument: None,
        });
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::EdgeUnknownEndpoint));
    }

    /// VIZ-I-06 — over-cap spec is rejected by the gate itself.
    #[test]
    fn rejects_over_cap_spec() {
        let els: Vec<ViewElement> = (0..ELEMENT_CAP + 1)
            .map(|i| el(&format!("e{i}"), DisplayMode::Fact))
            .collect();
        let errs = validate(&spec(els)).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::ElementCapExceeded));
    }

    // ── RFC-0024 patch 3 — the two additive checks (TS-mirrored) ─────

    fn el_with_fields(id: &str, fields: BTreeMap<String, serde_json::Value>) -> ViewElement {
        let mut e = el(id, DisplayMode::Fact);
        e.fields = fields;
        e
    }

    fn ref_json(file: &str, start: i64, end: i64) -> serde_json::Value {
        serde_json::json!({ "file": file, "start": start, "end": end })
    }

    /// Evidence-gated bodyLines pass the gate.
    #[test]
    fn accepts_evidenced_body_lines() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "bodyLines".into(),
            serde_json::json!([
                { "text": "holds the ledger", "line": 12, "evidence": ref_json("svc/Ledger.java", 8, 9) }
            ]),
        );
        let s = spec(vec![el_with_fields("board-1", fields)]);
        assert_eq!(validate(&s), Ok(()));
    }

    /// A body line without evidence is a fabricated fact — rejected.
    #[test]
    fn rejects_body_line_without_evidence() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "bodyLines".into(),
            serde_json::json!([{ "text": "no evidence", "line": 3 }]),
        );
        let s = spec(vec![el_with_fields("board-2", fields)]);
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::BodyLineWithoutEvidence));
    }

    /// A body line with an inverted byte range is rejected (same code).
    #[test]
    fn rejects_body_line_with_inverted_range() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "bodyLines".into(),
            serde_json::json!([
                { "text": "x", "line": 1, "evidence": ref_json("a.py", 9, 2) }
            ]),
        );
        let s = spec(vec![el_with_fields("board-3", fields)]);
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::BodyLineWithoutEvidence));
    }

    /// Well-formed alsoEvidence (paired popups) passes.
    #[test]
    fn accepts_well_formed_also_evidence() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "alsoEvidence".into(),
            serde_json::json!([ref_json("docs/adr-007.md", 210, 260)]),
        );
        let s = spec(vec![el_with_fields("drift-1", fields)]);
        assert_eq!(validate(&s), Ok(()));
    }

    /// A malformed alsoEvidence ref is rejected.
    #[test]
    fn rejects_malformed_also_evidence() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "alsoEvidence".into(),
            serde_json::json!([{ "start": 5 }]),
        );
        let s = spec(vec![el_with_fields("drift-2", fields)]);
        let errs = validate(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.code == GateCode::AlsoEvidenceMalformed));
    }

    /// Elements without the RFC-0024 fields are untouched (additive only).
    #[test]
    fn accepts_elements_without_patch3_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("anyBag".into(), serde_json::json!("value"));
        let s = spec(vec![el_with_fields("plain-1", fields)]);
        assert_eq!(validate(&s), Ok(()));
    }
}
