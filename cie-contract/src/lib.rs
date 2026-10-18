//! Contract registry types (PRD §19 / FR-6).
//!
//! `Authority` is the guardrail that keeps hypotheses and LLM output from
//! masquerading as requirements: only mechanical verification can produce
//! `Verified`, and the LLM can never set `Explicit`.

use cie_core::types::{ContractId, SymbolId};
pub mod dependency;
pub mod feature;
pub mod findings;
pub mod beliefs;
pub mod houdini;
pub mod obligations;
pub mod protocols;
pub mod naming;
pub mod registry;

pub use dependency::{
    Advisory, CompatibilityVerdict, Dependency, DependencyRiskMap, UpgradeCandidate, UsageEvidence,
};
pub use feature::{
    ClarificationQuestion, ClarificationSet, Deliverable, DetailLevel, FeatureContract,
    FeatureRequest, Requirement, RequirementOrigin,
};
pub use findings::{Finding, FindingKind};
pub use beliefs::{Belief, BeliefFactor, FactorGraph, Family, Predicate, SiteRef};
pub use houdini::{Houdini, RefutedEntry};
pub use obligations::{obligations, EffectProvenance, Event, State, StateModel, StateModelBuilder, Transition};
pub use protocols::{ProtocolExpr, ProtocolMiner, SurvivingCandidate, Traces};
pub use naming::{Action, Label, LabelSource, NamingAdapter, NamingKind, NamingRecord};
pub use registry::ContractRegistry;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Authority {
    /// Established by an explicit source (docstring contract, spec claim).
    Explicit,
    /// Inferred by rules/naming — a hypothesis only.
    InferredCandidate,
    /// Mechanically checked against code by a CIE checker.
    Verified,
    /// TASK-079: proposed by an LLM. Inert by construction — it can never
    /// become `Explicit` or `Verified`; only a deterministic re-derivation
    /// through Explicit/Verified channels can supersede it.
    LlmCandidate,
}

impl Authority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Authority::Explicit => "explicit",
            Authority::InferredCandidate => "inferred_candidate",
            Authority::Verified => "verified",
            Authority::LlmCandidate => "llm_candidate",
        }
    }

    /// TASK-079: the four binding sources, in table order (§ binding model).
    pub const ALL_SOURCES: [Authority; 4] = [
        Authority::Explicit,
        Authority::InferredCandidate,
        Authority::Verified,
        Authority::LlmCandidate,
    ];

    /// True when this authority may decide conformance verdicts on its own
    /// (`Explicit`, `Verified`). Hypotheses (`InferredCandidate`,
    /// `LlmCandidate`) may not.
    pub fn is_decisive(&self) -> bool {
        matches!(self, Authority::Explicit | Authority::Verified)
    }

    /// FR-44 / acceptance #28: `LlmCandidate` is INERT — an LLM proposal
    /// can never be promoted to a decisive authority; promotion must come
    /// from a deterministic re-derivation (a docstring or a checker run),
    /// never from the proposal itself.
    pub fn promote_to(&self, target: Authority) -> Result<Authority, cie_core::CieError> {
        if matches!(self, Authority::LlmCandidate)
            && matches!(target, Authority::Explicit | Authority::Verified)
        {
            return Err(cie_core::CieError::UnsupportedConstruct(
                "LlmCandidate is inert: it can never become Explicit or Verified \
                 (acceptance #28); re-derive deterministically instead"
                    .into(),
            ));
        }
        Ok(target)
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
    /// TASK-039: name/implementation-mismatch obligation (FR-10/FR-11).
    /// Enters the registry only as `Authority::InferredCandidate`.
    Naming,
    /// TASK-041: mined sequencing obligation ("commit requires prior begin").
    /// Enters the registry only as `Authority::InferredCandidate`.
    Protocol,
    /// TASK-052/FR-15: SAFETY — a bad state must never be reached
    /// ("fulfillment requires established authorization"). Distinct kind:
    /// never conflated with Liveness.
    Safety,
    /// TASK-052/FR-15: LIVENESS — a good state is eventually reached
    /// ("every debit eventually reaches fulfillment or an effective
    /// refund"). Distinct kind: never conflated with Safety.
    Liveness,
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
            ObligationKind::Naming => "naming".into(),
            ObligationKind::Protocol => "protocol".into(),
            ObligationKind::Safety => "safety".into(),
            ObligationKind::Liveness => "liveness".into(),
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
    /// TASK-079: proposed by an LLM (binding source 4). Inert — pairs only
    /// with `Authority::LlmCandidate` and can never decide a verdict.
    Llm,
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
