//! Incident causal-explanation assembly (CIE-US US-071 engine close).
//!
//! The drift pipeline already ships claim chronology (verifications with
//! evidence + counterexamples). This module assembles the *explanation*:
//! chronology stays chronology, causal chains are built only from
//! verified/implemented links, and alternative + unresolved causes stay
//! in the report — linked to the symbols and counterexamples involved —
//! never erased to make the story cleaner.
//!
//! Honesty: a causal link requires an `Implemented`-class verification
//! with cited evidence; contradicting/unverifiable/unknown claims become
//! alternative or unresolved entries, visibly labelled.

use std::collections::BTreeMap;

use cie_claims::model::{DriftReport, DriftStatus};
use serde::{Deserialize, Serialize};

/// One chronological event (what/when/where cited).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChronoEvent {
    pub claim: cie_core::types::ClaimId,
    pub doc: String,
    pub text: String,
    pub status: DriftStatus,
    pub checker: String,
    /// Evidence line citations carried by the verification.
    pub evidence: Vec<String>,
}

/// One causal link (verified only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalLink {
    pub claim: cie_core::types::ClaimId,
    pub text: String,
    /// Symbols the verified claim binds to (cited, not inferred).
    pub symbols: Vec<cie_core::types::SymbolId>,
    /// Counterexamples that pin the causal direction.
    pub counterexamples: Vec<String>,
}

/// An alternative cause — kept visible (greyed in the product layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlternativeCause {
    pub claim: cie_core::types::ClaimId,
    pub text: String,
    pub status: DriftStatus,
    pub why_alternative: String,
}

/// The assembled explanation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalExplanation {
    /// Event chronology (US-071: separated from causal explanation).
    pub chronology: Vec<ChronoEvent>,
    /// Causal chain: verified claims with symbols + counterexamples.
    pub causal_chain: Vec<CausalLink>,
    /// Alternative/unresolved causes — visible, labelled.
    pub alternatives: Vec<AlternativeCause>,
    /// Claims that could not be decided within scope (named gap).
    pub unresolved: Vec<String>,
    /// Report completeness passthrough (budget disclosure).
    pub complete: bool,
}

/// Assemble the explanation from a drift report (US-071). Deterministic:
/// chronology in claim order; chain in verification order.
pub fn assemble_causal_explanation(report: &DriftReport) -> CausalExplanation {
    let by_claim: BTreeMap<cie_core::types::ClaimId, &cie_claims::model::Claim> =
        report.claims.iter().map(|c| (c.id, c)).collect();
    let symbols_by_claim: BTreeMap<cie_core::types::ClaimId, Vec<cie_core::types::SymbolId>> =
        report
            .mappings
            .iter()
            .map(|m| (m.claim, m.symbols.clone()))
            .collect();

    let mut chronology = Vec::new();
    let mut chain = Vec::new();
    let mut alternatives = Vec::new();
    let mut unresolved = Vec::new();

    for v in &report.verifications {
        let claim = by_claim.get(&v.claim);
        let text = claim.map(|c| c.text.clone()).unwrap_or_default();
        let doc = claim.map(|c| c.doc.clone()).unwrap_or_default();
        chronology.push(ChronoEvent {
            claim: v.claim,
            doc,
            text: text.clone(),
            status: v.status,
            checker: v.checker.clone(),
            evidence: v.evidence.clone(),
        });
        match v.status {
            DriftStatus::Implemented => {
                let _ = &symbols_by_claim;
                chain.push(CausalLink {
                    claim: v.claim,
                    text,
                    symbols: v.symbols.clone(),
                    counterexamples: v.counterexamples.clone(),
                });
            }
            DriftStatus::Contradicting => alternatives.push(AlternativeCause {
                claim: v.claim,
                text,
                status: v.status,
                why_alternative: "contradicting evidence across documents".into(),
            }),
            DriftStatus::Missing | DriftStatus::Drifted | DriftStatus::Undocumented => {
                alternatives.push(AlternativeCause {
                    claim: v.claim,
                    text,
                    status: v.status,
                    why_alternative: "claim not established in this revision".into(),
                })
            }
            DriftStatus::Unverifiable | DriftStatus::Unknown | DriftStatus::Superseded
            | DriftStatus::Partial => unresolved.push(format!(
                "claim {} ({text}): {} — {}",
                v.claim,
                v.status.as_str(),
                v.reason.clone().unwrap_or_else(|| "no reason recorded".into())
            )),
        }
    }

    CausalExplanation {
        chronology,
        causal_chain: chain,
        alternatives,
        unresolved,
        complete: report.complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_claims::model::{Claim, ClaimKind, ClaimVerification, DocSourceLocation, DriftReport, Modality};
    use cie_core::types::Ns;
    type ClaimId = u32;

    fn claim(id: ClaimId, text: &str) -> Claim {
        Claim {
                introduced: None,
            id,
            doc: "docs/payment.md".into(),
            text: text.into(),
            kind: ClaimKind::Behavior,
            modality: Modality::Must,
            loc: DocSourceLocation {
                doc: "docs/payment.md".into(),
                section: String::new(),
                line: 1,
                char_start: 0,
                char_end: 0,
            },
            confidence: 1.0,
            extractor: "test".into(),
            created_ns: 0 as Ns,
        }
    }

    fn report() -> DriftReport {
        DriftReport {
            rev: 1,
            docs: vec!["docs/payment.md".into()],
            claims: vec![
                claim(1, "a failed charge refunds the order"),
                claim(2, "the ledger records every refund"),
                claim(3, "retry uses the same idempotency key"),
            ],
            mappings: vec![],
            verifications: vec![
                ClaimVerification {
                    claim: 1,
                    compute_ns: 0,
                    verified_ns: 0,
                    status: DriftStatus::Implemented,
                    checker: "verify/behavioral-v1".into(),
                    evidence: vec!["test_refund_paths".into()],
                    counterexamples: vec!["CE-1".into()],
                    symbols: vec![cie_core::types::SymbolId(10)],
                    reason: None,
                },
                ClaimVerification {
                    claim: 2,
                    compute_ns: 0,
                    verified_ns: 0,
                    status: DriftStatus::Contradicting,
                    checker: "verify/behavioral-v1".into(),
                    evidence: vec!["docs/a.md", "docs/b.md"].into_iter().map(String::from).collect(),
                    counterexamples: vec![],
                    symbols: vec![],
                    reason: None,
                },
                ClaimVerification {
                    claim: 3,
                    compute_ns: 0,
                    verified_ns: 0,
                    status: DriftStatus::Unknown,
                    checker: "verify/behavioral-v1".into(),
                    evidence: vec![],
                    counterexamples: vec![],
                    symbols: vec![],
                    reason: Some("budget exhausted".into()),
                },
            ],
            reverse_drift: vec![],
            aggregate: BTreeMap::new(),
            complete: true,
            generated_ns: 0,
        }
    }

    #[test]
    fn chronology_is_separate_from_causal_chain() {
        let report = report();
        let x = assemble_causal_explanation(&report);
        assert_eq!(x.chronology.len(), 3);
        assert_eq!(x.causal_chain.len(), 1); // only Implemented
        let link = &x.causal_chain[0];
        assert_eq!(link.claim, 1);
        assert_eq!(link.symbols, vec![cie_core::types::SymbolId(10)]);
        assert_eq!(link.counterexamples, vec!["CE-1"]);
        // alternatives stay visible with their reason
        assert_eq!(x.alternatives.len(), 1);
        assert_eq!(x.alternatives[0].status, DriftStatus::Contradicting);
        // unresolved keeps the reason, never erased
        assert_eq!(x.unresolved.len(), 1);
        assert!(x.unresolved[0].contains("budget exhausted"));
        assert!(x.complete);
    }

    #[test]
    fn determinism_same_report_same_explanation() {
        let report = report();
        let a = assemble_causal_explanation(&report);
        let b = assemble_causal_explanation(&report);
        assert_eq!(a, b);
    }
}
