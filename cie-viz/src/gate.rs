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
}
