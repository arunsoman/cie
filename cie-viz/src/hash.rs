//! The hash split (CIE-VIZ-MULTI PRD Part I §3) — the correction that
//! makes multi-engine caching work.
//!
//! * `semantic_hash` — sha256 over the canonical SEMANTIC payload
//!   `{ elements, edges, groups, gaps, footer, scope }`. EXCLUDED: the
//!   hashes themselves, `layoutHint`, `preferences`, and the `hidden[]`
//!   ledger (a disclosure artifact, not semantic content). This is the
//!   CACHE key: a layout or engine swap never invalidates semantic
//!   content (VIZ-02).
//! * `render_hash` — sha256 over `{ semanticHash, layoutHint, preferences,
//!   engineId }`. This is the render-ARTIFACT key: a layout swap produces
//!   a new artifact (VIZ-03). The producer seals it with the
//!   engine-neutral baseline `engineId: ""`; adapters recompute it with
//!   their own id for the `RenderHandle` (CIE never names an engine —
//!   VIZ-10). Audit baseline: `(semantic_hash, engine_id, layout_hint)`.
//!
//! Canonical form: the payload is built from the WIRE text (serialize →
//! reparse) so `f32` fields keep their shortest repr ("0.4", not the f64
//! promotion "0.4000000059604645"), object keys are sorted (BTreeMap), and
//! integral floats render the JavaScript way ("1", never "1.0") — the
//! exact form a TS adapter recomputing over the parsed spec produces.
//!
//! Note: repository-internal content addressing stays blake3 (FR-8); the
//! viz wire format follows the PRD's explicit `sha256:…` rendering.

use crate::model::ViewSpec;
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};

/// Canonical number rendering — the one place Rust and JavaScript JSON
/// disagree. serde_json renders the f64 `1.0` as `1.0`; JS `JSON.stringify`
/// renders it as `1`. Since the TS adapter recomputes the digest from the
/// PARSED spec (all numbers are f64 by then), the Rust canonical payload
/// must render integral floats the JS way: `1.0` → `1`, `0.0` → `0`.
fn normalize_numbers(v: &Value) -> Value {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() <= 9.007_199_254_740_992e15 {
                    if let Some(i) = i64::try_from(f as i128).ok() {
                        return Value::Number(Number::from(i));
                    }
                }
            }
            v.clone()
        }
        Value::Array(a) => Value::Array(a.iter().map(normalize_numbers).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, val) in o {
                out.insert(k.clone(), normalize_numbers(val));
            }
            Value::Object(out) // BTreeMap-backed Map: keys stay sorted
        }
        other => other.clone(),
    }
}

fn wire_part<T: serde::Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

/// The SEMANTIC payload as a canonical JSON value — PRD §3's enumerated
/// domain, built from the wire text (see module docs for why).
pub fn semantic_payload(spec: &ViewSpec) -> Value {
    let wire = format!(
        r#"{{"elements":{},"edges":{},"groups":{},"footer":{},"gaps":{},"scope":{}}}"#,
        wire_part(&spec.elements),
        wire_part(&spec.edges),
        match &spec.groups {
            Some(g) => wire_part(g),
            None => "null".into(),
        },
        wire_part(&spec.footer),
        wire_part(&spec.gaps),
        wire_part(&spec.scope),
    );
    let parsed: Value = serde_json::from_str(&wire).unwrap_or(Value::Null);
    normalize_numbers(&parsed)
}

/// Canonical JSON string of the semantic payload (sorted object keys).
pub fn canonical_json(spec: &ViewSpec) -> String {
    serde_json::to_string(&semantic_payload(spec)).unwrap_or_default()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let digest = h.finalize();
    let mut s = String::with_capacity(7 + 64);
    s.push_str("sha256:");
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// `semantic_hash` — the cache key (PRD §3).
pub fn compute_semantic(spec: &ViewSpec) -> String {
    sha256_hex(canonical_json(spec).as_bytes())
}

/// The RENDER payload: `{ semanticHash, layoutHint, preferences, engineId }`
/// (PRD §3). `engineId` is the adapter's id — the producer seals the
/// engine-neutral baseline with `""` (VIZ-10: CIE output names no engine).
pub fn render_payload(spec: &ViewSpec, engine_id: &str) -> Value {
    let wire = format!(
        r#"{{"engineId":{},"layoutHint":{},"preferences":{},"semanticHash":{}}}"#,
        wire_part(engine_id),
        match &spec.layout_hint {
            Some(h) => wire_part(h),
            None => "null".into(),
        },
        match &spec.preferences {
            Some(p) => wire_part(p),
            None => "null".into(),
        },
        wire_part(&compute_semantic(spec)),
    );
    let parsed: Value = serde_json::from_str(&wire).unwrap_or(Value::Null);
    normalize_numbers(&parsed)
}

/// `render_hash` — the render-artifact key (PRD §3).
pub fn compute_render(spec: &ViewSpec, engine_id: &str) -> String {
    sha256_hex(serde_json::to_string(&render_payload(spec, engine_id)).unwrap_or_default().as_bytes())
}

/// Seal a spec in place: `semantic_hash` + the engine-neutral
/// `render_hash` baseline (`engineId: ""`).
pub fn seal(spec: &mut ViewSpec) {
    spec.semantic_hash = compute_semantic(spec);
    spec.render_hash = compute_render(spec, "");
}

/// VIZ-02 recomputation test: the semantic seal is valid iff the stored
/// hash equals the freshly computed digest.
pub fn verify_semantic(spec: &ViewSpec) -> bool {
    spec.semantic_hash == compute_semantic(spec)
}

/// Render-baseline verification: `engine_id = ""` checks the producer's
/// engine-neutral seal; an adapter passes its own id (VIZ-03).
pub fn verify_render(spec: &ViewSpec, engine_id: &str) -> bool {
    spec.render_hash == compute_render(spec, engine_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        EpistemicFooter, HiddenCandidate, LayoutHint, Preferences, Salience, ScopeAttestation,
        ScopeCoverage, ViewElement, ViewSpec,
    };
    use std::collections::BTreeMap;

    fn el(id: &str, weight: f32) -> ViewElement {
        ViewElement {
            id: id.into(),
            element_kind: "concept".into(),
            display_mode: crate::model::DisplayMode::Fact,
            label: id.into(),
            evidence: crate::model::EvidenceRef { file: "a.py".into(), start: 1, end: 9 },
            claim_id: None,
            counter_argument: None,
            fog_reason: None,
            salience: Salience { factors: vec![], weight, why_shown: "t".into() },
            fields: BTreeMap::new(),
            parent_id: None,
        }
    }

    fn spec() -> ViewSpec {
        ViewSpec {
            elements: vec![el("a", 0.9), el("b", 0.4)],
            edges: vec![],
            groups: None,
            footer: EpistemicFooter::empty(),
            gaps: vec![],
            hidden: vec![],
            scope: ScopeAttestation {
                revision: 7,
                environments: vec![],
                coverage: ScopeCoverage { analyzed: true, ratio: None },
            },
            layout_hint: None,
            preferences: None,
            semantic_hash: String::new(),
            render_hash: String::new(),
        }
    }

    /// VIZ-02 — the semantic hash covers the semantic fields and survives
    /// a serialization roundtrip (the wire form hashes identically).
    #[test]
    fn semantic_hash_survives_roundtrip() {
        let mut s = spec();
        seal(&mut s);
        assert!(s.semantic_hash.starts_with("sha256:"));
        assert!(verify_semantic(&s));

        let wire = serde_json::to_string(&s).unwrap();
        let back: ViewSpec = serde_json::from_str(&wire).unwrap();
        assert!(verify_semantic(&back));
        assert_eq!(back.semantic_hash, s.semantic_hash);
    }

    /// Any semantic change moves the semantic hash.
    #[test]
    fn semantic_hash_moves_on_semantic_change() {
        let mut s = spec();
        seal(&mut s);
        let before = s.semantic_hash.clone();

        s.footer.facts = 2;
        assert_ne!(compute_semantic(&s), before, "footer counts are semantic");

        s.footer.facts = 0;
        s.elements[0].salience.weight = 0.95;
        assert_ne!(compute_semantic(&s), before, "salience is semantic");

        s.elements[0].salience.weight = 0.9;
        s.elements[1].display_mode = crate::model::DisplayMode::Fog;
        assert_ne!(compute_semantic(&s), before, "display modes are semantic");
    }

    /// PRD §3 — the `hidden[]` ledger is OUTSIDE the semantic domain.
    #[test]
    fn semantic_hash_excludes_hidden_ledger() {
        let mut a = spec();
        seal(&mut a);
        let mut b = spec();
        b.hidden.push(HiddenCandidate {
            id: "c9".into(),
            reason: "salience-bounded element cap".into(),
            salience_weight: 0.1,
            label: None,
            kind: None,
        });
        assert_ne!(a.hidden, b.hidden);
        assert_eq!(compute_semantic(&a), compute_semantic(&b));
    }

    /// VIZ-02 — layoutHint is OUTSIDE the semantic hash, but INSIDE the
    /// render hash (VIZ-03: a layout swap produces a new render artifact).
    #[test]
    fn layout_swap_semantic_stable_render_new() {
        let mut a = spec();
        seal(&mut a);
        let mut b = spec();
        let mut params = BTreeMap::new();
        params.insert("rankdir".into(), serde_json::json!("LR"));
        b.layout_hint = Some(LayoutHint { strategy: "dagre".into(), params: Some(params) });
        seal(&mut b);
        assert_ne!(a.layout_hint, b.layout_hint);
        assert_eq!(a.semantic_hash, b.semantic_hash, "VIZ-02");
        assert_ne!(a.render_hash, b.render_hash, "VIZ-03");
    }

    /// VIZ-02 / VIZ-E-08 — preferences are OUTSIDE the semantic hash and
    /// INSIDE the render hash.
    #[test]
    fn preferences_semantic_stable_render_new() {
        let mut a = spec();
        seal(&mut a);
        let mut b = spec();
        b.preferences = Some(Preferences {
            fisheye: Some(crate::model::FisheyePrefs {
                trigger: Some("pointermove".into()),
                d: Some(1.5),
                r: Some(120.0),
                ..Default::default()
            }),
            minimap: Some(serde_json::json!(true)),
            renderer: Some("svg".into()),
            ..Default::default()
        });
        seal(&mut b);
        assert_ne!(a.preferences, b.preferences);
        assert_eq!(a.semantic_hash, b.semantic_hash);
        assert_ne!(a.render_hash, b.render_hash);
    }

    /// The render hash binds the engine id: the same spec renders as a
    /// different artifact per engine, while the semantic hash stands.
    #[test]
    fn render_hash_binds_engine_id() {
        let mut s = spec();
        seal(&mut s);
        let neutral = s.render_hash.clone();
        assert_ne!(compute_render(&s, "g6"), neutral);
        assert_ne!(compute_render(&s, "mermaid"), compute_render(&s, "g6"));
        assert_eq!(compute_render(&s, "g6"), compute_render(&s, "g6"), "deterministic");
        assert!(verify_render(&s, ""), "producer baseline is engine-neutral");
        assert!(!verify_render(&s, "g6"), "adapter must recompute its own");
    }

    /// Neither hash covers itself: tampering the STORED values cannot
    /// change the freshly computed digests.
    #[test]
    fn hashes_exclude_themselves() {
        let mut s = spec();
        seal(&mut s);
        let semantic = compute_semantic(&s);
        let render = compute_render(&s, "g6");
        s.semantic_hash = "sha256:tampered".into();
        s.render_hash = "sha256:tampered".into();
        assert_eq!(compute_semantic(&s), semantic);
        assert_eq!(compute_render(&s, "g6"), render);
    }

    /// Cross-language canonical numbers: integral floats render the JS way
    /// ("1", never "1.0") so the TS adapter's recompute over the parsed
    /// fixture matches the Rust seal byte-for-byte.
    #[test]
    fn canonical_numbers_are_js_compatible() {
        let mut s = spec(); // weights 0.9 / 0.4 (non-integral)
        s.elements[0].salience.weight = 1.0;
        s.scope.coverage.ratio = Some(1.0);
        s.elements[0].fields.insert("count".into(), serde_json::json!(1.0f64));
        let canonical = canonical_json(&s);
        assert!(canonical.contains("\"weight\":1,"), "got: {canonical}");
        assert!(canonical.contains("\"count\":1}"), "got: {canonical}");
        assert!(!canonical.contains("1.0"), "got: {canonical}");
        // Non-integral floats keep their shortest repr.
        assert!(canonical.contains("\"weight\":0.4,"), "got: {canonical}");
    }
}
