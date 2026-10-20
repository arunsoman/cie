//! cie-viz — CIE-VIZ ViewSpec producer + epistemic gate.
//!
//! Visualization is a first-class citizen of the platform (CIE-VIZ PRD):
//! CIE emits a `ViewSpec` — a semantic graph with epistemic metadata — and
//! NEVER knows what a chart is. No chart name, no layout catalogue, no
//! colour and no pixel value exists in this crate (VIZ-A-01); geometry is
//! G6's responsibility, reached through the TypeScript adapter
//! (`viz-adapter/`, the only bridge, PRD §4).
//!
//! Layers:
//! * [`model`] — the closed ViewSpec wire contract (camelCase JSON,
//!   `deny_unknown_fields` = the schema half of VIZ-A-01);
//! * [`gate`] — the single honesty enforcement point: evidence required,
//!   claim/counter-argument/fog-reason clauses, endpoint provenance, the
//!   2 000-element cap (stable error codes shared with the TS adapters);
//! * [`hash`] — the PRD §3 hash split: `semantic_hash` (sha256 over the
//!   canonical semantic payload — cache key; layout/engine swaps never
//!   invalidate it) and `render_hash` (over semantic+layout+preferences+
//!   engineId — render-artifact key; producer seals the engine-neutral
//!   baseline, adapters recompute with their own id, VIZ-10);
//! * [`produce`] — `RevSnapshot` → sealed ViewSpec: evidence-backed
//!   elements, honest display modes, salience-bounded cap with a
//!   `hidden[]`/`gaps[]` ledger, footer recomputed from the final set,
//!   VIZ-D-03 layout auto-selection.
//!
//! ```no_run
//! use cie_viz::{ProduceOptions, produce, gate, hash};
//! # fn snap() -> cie_incremental::RevSnapshot { unimplemented!() }
//! # let snap = snap();
//! let spec = produce(&snap, &ProduceOptions::default()).expect("gate-clean");
//! assert!(gate::validate(&spec).is_ok());
//! assert!(hash::verify_semantic(&spec));
//! assert!(hash::verify_render(&spec, "")); // engine-neutral baseline
//! ```

pub mod flow;
pub mod gate;
pub mod hash;
pub mod model;
pub mod produce;

pub use gate::{GateCode, GateViolation, ELEMENT_CAP};
pub use hash::{
    canonical_json, compute_render, compute_semantic, render_payload, seal, semantic_payload,
    verify_render, verify_semantic,
};
pub use model::{
    DisplayMode, EpistemicFooter, EvidenceRef, FisheyePrefs, Gap, HiddenCandidate, LayoutHint,
    Preferences, Salience, ScopeAttestation, ScopeCoverage, ViewEdge, ViewElement, ViewGroup,
    ViewSpec,
};
pub use produce::{produce, ProduceOptions};
