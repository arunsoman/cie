//! The ViewSpec wire contract (CIE-VIZ PRD §3).
//!
//! CIE never knows what a chart is: no chart name, no layout catalogue, no
//! colour, no pixel value appears anywhere in these types (VIZ-A-01). The
//! only open fields are `LayoutHint.strategy` (a verbatim G6 layout name),
//! `fields` (a free JSON bag per element) and `Preferences` (render hints
//! that never change semantic content, VIZ-A-10).
//!
//! Wire format is camelCase JSON, identical to the TypeScript adapter's
//! `ViewSpec` interface; `deny_unknown_fields` on the closed structs is the
//! deserialization half of the VIZ-A-01 schema test.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Epistemic display mode — the four honest states (PRD §8).
/// Type-level non-optional on every element/edge (VIZ-A-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DisplayMode {
    Fact,
    Inference,
    Hypothesis,
    Fog,
}

impl DisplayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DisplayMode::Fact => "Fact",
            DisplayMode::Inference => "Inference",
            DisplayMode::Hypothesis => "Hypothesis",
            DisplayMode::Fog => "Fog",
        }
    }
}

/// `EvidenceRef` — shape EXACTLY `{ file, start, end }` (PRD §3.1 / TC-01).
/// Byte offsets into the named file; mirrors `cie_core::SourceLocation`
/// with the `FileId` resolved through the snapshot's file table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub file: String,
    pub start: u64,
    pub end: u64,
}

/// Why this element is on screen — honesty about selection, not confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Salience {
    pub factors: Vec<String>,
    pub weight: f32,
    pub why_shown: String,
}

/// One semantic node. `elementKind` is an OPEN string (VIZ-B-05: adding a
/// kind is a new `NODE_TYPE` entry in the adapter, never a schema change).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewElement {
    pub id: String,
    pub element_kind: String,
    pub display_mode: DisplayMode,
    pub label: String,
    /// REQUIRED — no element without it (VIZ-A-03, enforced by [`gate`]).
    pub evidence: EvidenceRef,
    /// REQUIRED iff displayMode ≠ Fact (VIZ-A-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    /// REQUIRED iff displayMode = Hypothesis (VIZ-A-05).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_argument: Option<String>,
    /// REQUIRED iff displayMode = Fog (VIZ-A-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fog_reason: Option<String>,
    pub salience: Salience,
    /// Open per-element payload (never rendered as semantics).
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
    /// Group / hierarchy containment (G6 combo parent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// One semantic relation. Edges carry the same epistemic law as elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    /// Open verb: calls | reads | writes | exposes | contains | ...
    pub edge_kind: String,
    pub display_mode: DisplayMode,
    pub evidence: EvidenceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_argument: Option<String>,
}

/// G6 combo payload (`groups[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewGroup {
    pub id: String,
    pub label: String,
    pub kind: String,
}

/// Incompleteness is INSIDE the spec, never hidden outside it (PRD §3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gap {
    pub id: String,
    pub area: String,
    pub why: String,
}

/// Elements the producer withheld (salience cap, no evidence, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HiddenCandidate {
    pub id: String,
    pub reason: String,
    pub salience_weight: f32,
}

/// The epistemic footer — ALWAYS present, even when all counts are zero
/// (VIZ-A-07). Rendered by a registered overlay plugin, outside the canvas
/// (VIZ-F-06); gap ids must remain reachable through `caption` (VIZ-H-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EpistemicFooter {
    pub facts: u32,
    pub inferences: u32,
    pub hypotheses: u32,
    pub fog: u32,
    pub stale: u32,
    pub caption: String,
}

impl EpistemicFooter {
    /// The zero footer — `footer` is never absent (VIZ-A-07).
    pub fn empty() -> Self {
        EpistemicFooter {
            facts: 0,
            inferences: 0,
            hypotheses: 0,
            fog: 0,
            stale: 0,
            caption: "Display categories, not a confidence score.".into(),
        }
    }
}

/// Attestation block — makes cache, staleness and audit possible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeAttestation {
    pub revision: u64,
    #[serde(default)]
    pub environments: Vec<String>,
    pub coverage: ScopeCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeCoverage {
    pub analyzed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f32>,
}

/// Render hint — OPTIONAL; absence means the adapter chooses (VIZ-A-09).
/// `strategy` is passed VERBATIM to G6 `layout.type` (VIZ-D-01); the
/// adapter never enumerates layouts (VIZ-D-08).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutHint {
    pub strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, serde_json::Value>>,
}

/// Fisheye pass-through (VIZ-E-02): every documented G6 option round-trips
/// unchanged. Flattened extras (style, nodeStyle, preventDefault, ...) are
/// carried verbatim; this struct is the one place `deny_unknown_fields`
/// does NOT apply (the plugin surface is open by contract).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FisheyePrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_r: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_r: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_d: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_d: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_r_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_d_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_d_percent: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Render preferences. NEVER part of the content hash (VIZ-A-10 / VIZ-E-08:
/// enabling fisheye or swapping renderer changes no semantic content).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fisheye: Option<FisheyePrefs>,
    /// `boolean | { size, position }` per PRD §3.1 — kept as raw JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimap: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_bundling: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
}

/// The closed top-level contract (PRD §3.1). Unknown top-level keys are a
/// schema violation (VIZ-A-01) — `deny_unknown_fields` rejects a spec that
/// smuggles chart names, colours or pixel values past the producer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewSpec {
    pub elements: Vec<ViewElement>,
    pub edges: Vec<ViewEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<ViewGroup>>,
    pub footer: EpistemicFooter,
    #[serde(default)]
    pub gaps: Vec<Gap>,
    #[serde(default)]
    pub hidden: Vec<HiddenCandidate>,
    pub scope: ScopeAttestation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hint: Option<LayoutHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Preferences>,
    /// sha256 over the canonical SEMANTIC payload (elements, edges, groups,
    /// gaps, footer, scope) — PRD CIE-VIZ-MULTI §3. Excluded: the hashes
    /// themselves, `layoutHint`, `preferences`, and the `hidden[]` ledger
    /// (a disclosure artifact, not semantic content). Cache key: a layout
    /// or engine swap NEVER invalidates semantic content.
    pub semantic_hash: String,
    /// sha256 over { semanticHash, layoutHint, preferences, engineId }
    /// (PRD §3) — the render-artifact key. The producer seals it with the
    /// engine-neutral baseline `engineId: ""`; an adapter recomputes it
    /// with its own id for the `RenderHandle` (CIE never names an engine,
    /// VIZ-10). A layout/preference/engine change produces a NEW render
    /// artifact while `semanticHash` stands.
    pub render_hash: String,
}

impl ViewSpec {
    /// Ids of every element, in spec order (provenance audits).
    pub fn element_ids(&self) -> std::collections::BTreeSet<&str> {
        self.elements.iter().map(|e| e.id.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> EvidenceRef {
        EvidenceRef { file: "checkout.py".into(), start: 88, end: 120 }
    }

    fn element(mode: DisplayMode) -> ViewElement {
        ViewElement {
            id: "checkout".into(),
            element_kind: "concept".into(),
            display_mode: mode,
            label: "Checkout".into(),
            evidence: evidence(),
            claim_id: None,
            counter_argument: None,
            fog_reason: None,
            salience: Salience {
                factors: vec!["TASK_MATCH".into()],
                weight: 0.9,
                why_shown: "matches question".into(),
            },
            fields: BTreeMap::new(),
            parent_id: None,
        }
    }

    fn footer() -> EpistemicFooter {
        let mut f = EpistemicFooter::empty();
        f.facts = 1;
        f
    }

    fn scope() -> ScopeAttestation {
        ScopeAttestation {
            revision: 42,
            environments: vec![],
            coverage: ScopeCoverage { analyzed: true, ratio: Some(1.0) },
        }
    }

    fn spec(el: ViewElement) -> ViewSpec {
        ViewSpec {
            elements: vec![el],
            edges: vec![],
            groups: None,
            footer: footer(),
            gaps: vec![],
            hidden: vec![],
            scope: scope(),
            layout_hint: None,
            preferences: None,
            semantic_hash: "sha256:unsealed".into(),
            render_hash: "sha256:unsealed".into(),
        }
    }

    /// VIZ-A-01: a spec cannot smuggle chart/layout/colour keys past the
    /// schema — unknown fields are rejected at deserialization.
    #[test]
    fn viz_a01_unknown_top_level_keys_rejected() {
        let json = serde_json::json!({
            "elements": [], "edges": [], "footer": footer(), "gaps": [],
            "hidden": [], "scope": scope(), "semanticHash": "sha256:x",
            "renderHash": "sha256:y",
            "chartType": "pie", "color": "#f00"
        });
        let err = serde_json::from_value::<ViewSpec>(json).unwrap_err();
        assert!(err.to_string().contains("unknown"), "got: {err}");
    }

    /// VIZ-A-02: displayMode is type-level non-optional.
    #[test]
    fn viz_a02_display_mode_required() {
        let json = serde_json::json!({
            "id": "x", "elementKind": "concept", "label": "X",
            "evidence": {"file": "a.py", "start": 0, "end": 1},
            "salience": {"factors": [], "weight": 0.5, "whyShown": "y"}
        });
        assert!(serde_json::from_value::<ViewElement>(json).is_err());
    }

    /// Wire format is camelCase and roundtrips losslessly.
    #[test]
    fn wire_roundtrip_camel_case() {
        let mut el = element(DisplayMode::Hypothesis);
        el.claim_id = Some("C-0042".into());
        el.counter_argument = Some("no test asserts idempotency".into());
        let json = serde_json::to_value(&el).unwrap();
        assert_eq!(json["elementKind"], "concept");
        assert_eq!(json["displayMode"], "Hypothesis");
        assert_eq!(json["claimId"], "C-0042");
        assert_eq!(json["counterArgument"], "no test asserts idempotency");
        let back: ViewElement = serde_json::from_value(json).unwrap();
        assert_eq!(back, el);
    }
}
