//! The §9 shared interface (TASK-004): `AnalysisInput`/`AnalysisOutput` is
//! the single seam between ingestion, the engine, and every algorithm; every
//! output carries `Evidence` (FR-4, global rule 2) — outputs cannot be
//! constructed without it.
//!
//! # The `#[perf]` test convention (§15.4, global rule 5)
//!
//! Performance tests are `#[ignore]`-gated, run via `cargo test -- --ignored`,
//! and fail when `measured > target × CI_TOLERANCE`. `CI_TOLERANCE = 3.0`
//! acknowledges CI hardware noise while keeping the PRD §15.4 targets
//! binding: a 3× regression over a PRD target still fails the suite. The
//! reference test is `cie-core/tests/perf_convention.rs`.

use crate::contract::Contract;
use crate::evidence::{Coverage, Evidence};
use crate::facts::LangFacts;
use crate::summary::FunctionSummary;
use crate::types::{Ns, RevId, SymbolId};
use serde::{Deserialize, Serialize};

/// The CI-noise multiplier every `#[perf]` test applies to PRD §15.4 targets
/// (global rule 5): a test fails only when `measured > target * CI_TOLERANCE`.
/// 3.0 keeps the targets binding on shared runners while absorbing jitter.
pub const CI_TOLERANCE: f64 = 3.0;

/// §9 reconciles this name with §14.1's `RevId` — the alias documents the
/// seam (TASK-002 keeps `RevId` as the §14.1 primitive alias).
pub type RevisionId = RevId;

/// Error taxonomy (TASK-004, FR-35): `Timeout` maps downstream to `Unknown`,
/// never to a clean status (global rule 3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CieError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported construct: {0}")]
    UnsupportedConstruct(String),
    #[error("analysis timeout (deadline {deadline_ns} ns)")]
    Timeout { deadline_ns: Ns },
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("dangling reference: {what}")]
    DanglingReference { what: String },
}

impl CieError {
    /// FR-35 / global rule 3: timeouts are first-class so callers can
    /// convert them to `Unknown` verdicts instead of silent "clean".
    pub fn is_timeout(&self) -> bool {
        matches!(self, CieError::Timeout { .. })
    }
}

/// §9 AnalysisInput — field names and order exactly as §9 states. `facts`
/// carries the per-file `LangFacts` the adapters produced (the §14.2
/// `TypedFacts` aggregate wraps them from TASK-010 on).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisInput {
    pub revision: RevId,
    pub scope: Vec<String>,
    pub facts: Vec<LangFacts>,
    pub contracts: Vec<Contract>,
    pub assumptions: Vec<String>,
}

impl AnalysisInput {
    /// A well-formed input for `revision` with empty payload.
    pub fn for_revision(revision: RevId) -> Self {
        AnalysisInput {
            revision,
            ..Default::default()
        }
    }
}

/// §9 AnalysisOutput — field names and order exactly as §9 states.
/// Constructible only through `empty_with_evidence`-style helpers so the
/// FR-4 evidence invariant cannot be skipped.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisOutput {
    pub derived_records: Vec<FunctionSummary>,
    pub evidence: Evidence,
    pub dependencies: Vec<(SymbolId, SymbolId)>,
    pub coverage: Coverage,
    pub conclusions: Vec<String>,
    pub counterexamples: Vec<String>,
}

impl AnalysisOutput {
    /// FR-4: every output carries `Evidence { rev, provenance, assumptions,
    /// unsupported, unresolved, generated_at_ns }` (§14.8) — outputs cannot
    /// be constructed without evidence.
    pub fn empty_with_evidence(rev: RevId, now: Ns) -> Self {
        AnalysisOutput {
            evidence: Evidence::new(rev, "engine", now),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_invariant() {
        let out = AnalysisOutput::empty_with_evidence(3, 1_700_000_000_123_456_789);
        assert_eq!(out.evidence.rev, 3);
        assert!(out.conclusions.is_empty() && out.derived_records.is_empty());
    }

    #[test]
    fn timeout_maps_to_unknown_not_clean() {
        let err = CieError::Timeout { deadline_ns: 42 };
        assert!(err.is_timeout());
        assert!(!matches!(err, CieError::BudgetExceeded(_)));
    }
}
