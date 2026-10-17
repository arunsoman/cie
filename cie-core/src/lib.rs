//! cie-core — shared foundation primitives for the Code Intelligence Engine.
//!
//! Scope (PRD §10): scalar/time/hash primitives (§47), §14.1 identity
//! newtypes, the generic `Interner`, `SourceLocation`, and the §9
//! AnalysisInput/AnalysisOutput protocol with the `CieError` taxonomy.
//! Fact families live in `cie-semantic`; storage in `cie-store`.

pub mod evidence;
pub mod interner;
pub mod protocol;
pub mod types;

// Crate-root re-exports: the §14.1/§9 primitives every crate (and every
// instructor sketch) imports as `use cie_core::{Ns, RevId, hash, …}`.
pub use crate::interner::{InternedStr, Interner, StrInterner};
pub use crate::protocol::{AnalysisInput, AnalysisOutput, CieError, RevisionId, CI_TOLERANCE};
pub use crate::types::*;
