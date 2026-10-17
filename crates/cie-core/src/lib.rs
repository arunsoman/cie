//! cie-core — shared foundation primitives for the Code Intelligence Engine.
//!
//! Layers (PRD §6): this crate implements the typed facts, summaries,
//! contracts, evidence/coverage, ConceptNode DAG, drift model and the
//! incremental engine that every capability domain builds on.

pub mod concept;
pub mod contract;
pub mod drift;
pub mod evidence;
pub mod facts;
pub mod findings;
pub mod incremental;
pub mod interner;
pub mod summary;
pub mod types;

pub mod prelude {
    pub use crate::concept::*;
    pub use crate::contract::*;
    pub use crate::drift::*;
    pub use crate::evidence::*;
    pub use crate::facts::*;
    pub use crate::findings::*;
    pub use crate::summary::*;
    pub use crate::types::*;
}
