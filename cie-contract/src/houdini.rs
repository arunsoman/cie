//! Houdini-style candidate refinement (TASK-041, §7 Flanagan–Leino row) —
//! `cie-contract::houdini`.
//!
//! Generate candidate annotations from mined protocols, then MECHANICALLY
//! check each against ALL traces: refuted candidates are removed and
//! logged with the exact refuting trace (global rule 2). The refuted log
//! is append-only and content-addressed (FR-8 spirit; keys follow §15.1,
//! global rule 7). Termination: bounded refinement rounds; every survivor
//! records the checks it survived.

use crate::protocols::{ProtocolExpr, SurvivingCandidate, Traces};
use crate::{Authority, ContractRegistry};
use cie_core::evidence::Evidence;
use cie_core::types::{hash_hex, RevId, SymbolId};

/// One append-only refutation record: the exact candidate and the exact
/// trace that refuted it, content-addressed for auditability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefutedEntry {
    pub candidate: ProtocolExpr,
    /// The exact refuting trace (sketch: `r.by_trace == fx.trace_commit_after_rollback`).
    pub by_trace: Vec<String>,
    /// Refinement round that removed it (bounded loop counter).
    pub round: usize,
    /// blake3 over (candidate ‖ trace) — content-addressed audit key.
    pub content_hash: String,
}

impl RefutedEntry {
    /// §15.1-style key for the persisted refutation.
    pub fn key(&self) -> String {
        format!("refuted:{}", self.content_hash)
    }
}

/// The Houdini loop (sketch: `Houdini::new(miner.registry_view())`).
pub struct Houdini {
    /// Bounded refinement rounds (termination guarantee).
    max_rounds: usize,
    refuted: Vec<RefutedEntry>,
    rev: RevId,
}

impl Houdini {
    pub fn new(_registry_view: &ContractRegistry) -> Self {
        Houdini { max_rounds: 2, refuted: vec![], rev: 0 }
    }

    pub fn with_rev(rev: RevId) -> Self {
        Houdini { max_rounds: 2, refuted: vec![], rev }
    }

    /// Mechanically refine candidates against all traces. Survivors record
    /// the number of checks they survived; every refutation is logged with
    /// the exact refuting trace. Deterministic: candidates checked in
    /// sorted order, traces in vec order.
    pub fn refine(
        &mut self,
        candidates: Vec<ProtocolExpr>,
        traces: &Traces,
    ) -> Vec<SurvivingCandidate> {
        let mut current = candidates;
        let mut survivors = vec![];
        for round in 0..self.max_rounds {
            if current.is_empty() {
                break;
            }
            current.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            let mut next_round: Vec<ProtocolExpr> = vec![];
            for cand in current.drain(..) {
                let mut refuted_by: Option<Vec<String>> = None;
                let mut checked = 0usize;
                for t in &traces.traces {
                    checked += 1;
                    if !cand.holds_on(t) {
                        refuted_by = Some(t.clone());
                        break;
                    }
                }
                match refuted_by {
                    Some(t) => {
                        // append-only refutation log with the exact trace
                        let payload = format!(
                            "{}|{}",
                            cand.as_str(),
                            t.join("->")
                        );
                        self.refuted.push(RefutedEntry {
                            by_trace: t,
                            round,
                            content_hash: hash_hex(&cie_core::types::hash(
                                payload.as_bytes(),
                            )),
                            candidate: cand,
                        });
                    }
                    None => {
                        if round + 1 == self.max_rounds {
                            let evidence = Evidence {
                                rev: self.rev,
                                provenance: format!(
                                    "houdini: {} survived {} trace check(s) across {} round(s)",
                                    cand.as_str(),
                                    checked,
                                    self.max_rounds
                                ),
                                assumptions: vec![format!(
                                    "bounded refinement: max_rounds={}",
                                    self.max_rounds
                                )],
                                unsupported: vec![],
                                unresolved: traces.unresolved.clone(),
                                generated_at_ns: 0,
                            };
                            survivors.push(SurvivingCandidate {
                                expr: cand,
                                authority: Authority::InferredCandidate,
                                survived_checks: checked,
                                evidence,
                            });
                        } else {
                            // bounded re-check next round (annotation
                            // strengthening happens here in full Houdini;
                            // this layer re-verifies stability)
                            next_round.push(cand);
                        }
                    }
                }
            }
            current = next_round;
        }
        survivors
    }

    /// The append-only refuted log (sketch: `houdini.refuted_log()`).
    pub fn refuted_log(&self) -> &[RefutedEntry] {
        &self.refuted
    }

    /// Register survivors through the guarded registry (the ONLY write
    /// path; `ObligationKind::Protocol` is rejected with any authority
    /// other than `InferredCandidate`).
    pub fn register(
        &self,
        reg: &mut ContractRegistry,
        survivors: &[SurvivingCandidate],
        symbols: Vec<SymbolId>,
    ) -> Result<Vec<u32>, cie_core::protocol::CieError> {
        let mut ids = vec![];
        for s in survivors {
            let mut c = s.to_contract(symbols.clone());
            // carry the survival evidence into the contract expr audit trail
            c.expr = format!("{} [{}]", c.expr, s.evidence.provenance);
            ids.push(reg.insert(c)?);
        }
        Ok(ids)
    }
}
