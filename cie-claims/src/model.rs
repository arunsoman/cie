//! Drift data model (PRD §29 / §54): claims, mappings, verifications,
//! statuses, reverse drift and reports (FR-32–39).

use cie_core::types::{ClaimId, Ns, NsDelta, RevId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClaimKind {
    Capability,
    Behavior,
    Constraint,
    Interface,
    DataModel,
    NonFunctional,
    Sequence,
    Prohibition,
}

impl ClaimKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaimKind::Capability => "capability",
            ClaimKind::Behavior => "behavior",
            ClaimKind::Constraint => "constraint",
            ClaimKind::Interface => "interface",
            ClaimKind::DataModel => "data_model",
            ClaimKind::NonFunctional => "non_functional",
            ClaimKind::Sequence => "sequence",
            ClaimKind::Prohibition => "prohibition",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Modality {
    Shall,
    Must,
    Should,
    May,
    Will,
}

impl Modality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Modality::Shall => "shall",
            Modality::Must => "must",
            Modality::Should => "should",
            Modality::May => "may",
            Modality::Will => "will",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSourceLocation {
    pub doc: String,
    pub section: String,
    pub line: u32,
    pub char_start: u64,
    pub char_end: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    /// Blame-derived commit metadata, not proof of semantic introduction.
    #[serde(default)]
    pub introduced: Option<Vec<cie_history::CommitMeta>>,
    pub id: ClaimId,
    pub doc: String,
    pub text: String,
    pub kind: ClaimKind,
    pub modality: Modality,
    pub loc: DocSourceLocation,
    pub confidence: f32,
    pub extractor: String,
    pub created_ns: Ns,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimMapping {
    pub claim: ClaimId,
    pub symbols: Vec<SymbolId>,
    pub confidence: f32,
    pub proposer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DriftStatus {
    Implemented,
    Partial,
    Missing,
    Contradicting,
    Drifted,
    Undocumented,
    Unverifiable,
    Unknown,
    Superseded,
}

impl DriftStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriftStatus::Implemented => "implemented",
            DriftStatus::Partial => "partial",
            DriftStatus::Missing => "missing",
            DriftStatus::Contradicting => "contradicting",
            DriftStatus::Drifted => "drifted",
            DriftStatus::Undocumented => "undocumented",
            DriftStatus::Unverifiable => "unverifiable",
            DriftStatus::Unknown => "unknown",
            DriftStatus::Superseded => "superseded",
        }
    }

    pub fn is_terminal_conclusion(&self) -> bool {
        matches!(self, DriftStatus::Implemented | DriftStatus::Contradicting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnverifiableReason {
    RequiresRuntimeEvidence,
    LowConfidenceMapping,
    NoOperationalDefinition,
    UnsupportedConstruct,
}

impl UnverifiableReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnverifiableReason::RequiresRuntimeEvidence => "requires_runtime_evidence",
            UnverifiableReason::LowConfidenceMapping => "low_confidence_mapping",
            UnverifiableReason::NoOperationalDefinition => "no_operational_definition",
            UnverifiableReason::UnsupportedConstruct => "unsupported_construct",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimVerification {
    pub claim: ClaimId,
    pub status: DriftStatus,
    /// Checker id, e.g. `verify/behavioral-v1`.
    pub checker: String,
    pub evidence: Vec<String>,
    pub counterexamples: Vec<String>,
    pub symbols: Vec<SymbolId>,
    pub reason: Option<String>,
    pub verified_ns: Ns,
    pub compute_ns: NsDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReverseDriftEntry {
    pub symbol: SymbolId,
    pub qual_name: String,
    pub reason: String,
}

/// Machine + human readable drift report (FR-37).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriftReport {
    pub rev: RevId,
    pub docs: Vec<String>,
    pub claims: Vec<Claim>,
    pub mappings: Vec<ClaimMapping>,
    pub verifications: Vec<ClaimVerification>,
    pub reverse_drift: Vec<ReverseDriftEntry>,
    pub aggregate: BTreeMap<String, u32>,
    pub complete: bool,
    pub generated_ns: Ns,
}

impl DriftReport {
    pub fn recount(&mut self) {
        let mut agg: BTreeMap<String, u32> = BTreeMap::new();
        for v in &self.verifications {
            *agg.entry(v.status.as_str().to_string()).or_default() += 1;
        }
        self.aggregate = agg;
    }

    /// FR-37 delta: only claims whose status changed between two reports.
    pub fn delta(&self, other: &DriftReport) -> Vec<(ClaimId, String, String, String)> {
        // (claim, text, from_status, to_status) matched by claim text+doc.
        let key = |c: &Claim| format!("{}::{}", c.doc, c.text);
        let from: BTreeMap<String, &ClaimVerification> = self
            .verifications
            .iter()
            .map(|v| {
                let c = self
                    .claims
                    .iter()
                    .find(|c| c.id == v.claim)
                    .map(key)
                    .unwrap_or_default();
                (c, v)
            })
            .collect();
        let mut out = vec![];
        for v in &other.verifications {
            let k = other
                .claims
                .iter()
                .find(|c| c.id == v.claim)
                .map(key)
                .unwrap_or_default();
            if let Some(prev) = from.get(&k) {
                if prev.status != v.status {
                    out.push((
                        v.claim,
                        k,
                        prev.status.as_str().into(),
                        v.status.as_str().into(),
                    ));
                }
            }
        }
        out
    }
}
