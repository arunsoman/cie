//! cie-drift — the drift pipeline (TASK-076): claims to mappings to
//! verification to reports, reverse drift, reconciliation, deltas, SARIF
//! serialization and the human/mermaid renderers.

pub mod causal;
pub mod conflicts;

pub use conflicts::{ConflictEvent, ConflictLedger, ConflictRecord, ConflictStatus, EventKind, ResolutionOption, Statement};
pub mod outputs;
pub mod persist;
pub mod pipeline;

pub use causal::{
    assemble_causal_explanation, AlternativeCause, CausalExplanation, CausalLink, ChronoEvent,
};
pub use persist::DriftStoreExt;

pub mod sarif;

pub use pipeline::*;
