//! Contract registry types (PRD §19 / FR-6).
//!
//! `Authority` is the guardrail that keeps hypotheses and LLM output from
//! masquerading as requirements: only mechanical verification can produce
//! `Verified`, and the LLM can never set `Explicit`.

use crate::types::{ContractId, SymbolId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Authority {
    /// Established by an explicit source (docstring contract, spec claim).
    Explicit,
    /// Inferred by rules/naming/LLM — a hypothesis only.
    InferredCandidate,
    /// Mechanically checked against code by a CIE checker.
    Verified,
}

impl Authority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Authority::Explicit => "explicit",
            Authority::InferredCandidate => "inferred_candidate",
            Authority::Verified => "verified",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObligationKind {
    /// Fulfillment requires established authorization (Safety).
    MustAuthorize,
    /// No double spend / double charge (idempotency).
    MustNotDoubleSpend,
    MustLog,
    MustValidateInput,
    MustHandleErrors,
    MustNotLeakSecrets,
    Custom(String),
}

impl ObligationKind {
    pub fn as_str(&self) -> String {
        match self {
            ObligationKind::MustAuthorize => "must_authorize".into(),
            ObligationKind::MustNotDoubleSpend => "must_not_double_spend".into(),
            ObligationKind::MustLog => "must_log".into(),
            ObligationKind::MustValidateInput => "must_validate_input".into(),
            ObligationKind::MustHandleErrors => "must_handle_errors".into(),
            ObligationKind::MustNotLeakSecrets => "must_not_leak_secrets".into(),
            ObligationKind::Custom(s) => format!("custom:{s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractSource {
    Docstring,
    NamingConvention,
    /// Derived from a document claim (drift pipeline); carries the claim id.
    Claim(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub obligation: ObligationKind,
    /// Human-readable expression, e.g. "charge must be preceded by authorize".
    pub expr: String,
    pub authority: Authority,
    pub symbols: Vec<SymbolId>,
    pub source: ContractSource,
}
