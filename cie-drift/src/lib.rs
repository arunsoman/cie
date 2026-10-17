//! cie-drift — the drift pipeline (TASK-076): claims to mappings to
//! verification to reports, reverse drift, reconciliation, deltas, SARIF
//! serialization and the human/mermaid renderers.

pub mod outputs;
pub mod persist;
pub mod pipeline;

pub use persist::DriftStoreExt;

pub mod sarif;

pub use pipeline::*;
