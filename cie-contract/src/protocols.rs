//! Protocol mining (TASK-041, §7 Ammons/Garland/Larus row) —
//! `cie-contract::protocols`.
//!
//! Mines candidate sequencing obligations as k-bounded automata over
//! summary/relation-derived traces, entering the registry as
//! `ObligationKind::Protocol` contracts with `Authority::InferredCandidate`
//! ONLY (global rule 4 — the guarded registry enforces this).
//!
//! The protocol expression DSL (shared shape with TASK-043's
//! `requires(a, b)`):
//! - `before(then, prior)` — every `then` has a `prior` before it.
//! - `never_after(a, b)` — `a` never occurs after `b` in a trace.
//!
//! Traces exclude unresolved calls (FR-4); the unresolved set rides on
//! [`Traces`] and lands in every candidate's `Evidence.unresolved`.

use crate::{Authority, Contract, ObligationKind};
use cie_core::evidence::Evidence;

use serde::{Deserialize, Serialize};

/// A mined protocol expression.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProtocolExpr {
    /// `before(then, prior)`: every occurrence of `then` has a `prior`
    /// strictly before it in the same trace.
    Before { then: String, prior: String },
    /// `never_after(a, b)`: `a` never occurs after `b` within one trace.
    NeverAfter { a: String, b: String },
}

impl ProtocolExpr {
    pub fn before(then: &str, prior: &str) -> Self {
        ProtocolExpr::Before {
            then: then.to_ascii_lowercase(),
            prior: prior.to_ascii_lowercase(),
        }
    }
    pub fn never_after(a: &str, b: &str) -> Self {
        ProtocolExpr::NeverAfter {
            a: a.to_ascii_lowercase(),
            b: b.to_ascii_lowercase(),
        }
    }

    /// The DSL spelling used in contract `expr`s.
    pub fn as_str(&self) -> String {
        match self {
            ProtocolExpr::Before { then, prior } => format!("before({then}, {prior})"),
            ProtocolExpr::NeverAfter { a, b } => format!("never_after({a}, {b})"),
        }
    }

    /// Mechanically check the expression against one trace.
    pub fn holds_on(&self, trace: &[String]) -> bool {
        let t: Vec<String> = trace.iter().map(|e| e.to_ascii_lowercase()).collect();
        match self {
            ProtocolExpr::Before { then, prior } => {
                for (i, e) in t.iter().enumerate() {
                    if e == then && !t[..i].contains(prior) {
                        return false;
                    }
                }
                true
            }
            ProtocolExpr::NeverAfter { a, b } => {
                for (i, e) in t.iter().enumerate() {
                    if e == a && t[..i].contains(b) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// Derived traces (sketch: `fx.traces()`). Unresolved calls are already
/// excluded by the trace builder and recorded here (FR-4).
#[derive(Debug, Clone, Default)]
pub struct Traces {
    pub traces: Vec<Vec<String>>,
    /// Unresolved call texts excluded from traces — recorded in evidence.
    pub unresolved: Vec<String>,
}

impl Traces {
    pub fn alphabet(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for t in &self.traces {
            for e in t {
                let e = e.to_ascii_lowercase();
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out.sort();
        out
    }
}

/// A candidate that survived Houdini refinement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurvivingCandidate {
    pub expr: ProtocolExpr,
    pub authority: Authority,
    /// How many traces the candidate was mechanically checked against.
    pub survived_checks: usize,
    pub evidence: Evidence,
}

impl SurvivingCandidate {
    /// Registered as `ObligationKind::Protocol` — the guarded registry
    /// accepts this ONLY with `Authority::InferredCandidate` (§8 rule 4).
    pub fn to_contract(&self, symbols: Vec<cie_core::types::SymbolId>) -> Contract {
        Contract {
                introduced: None,
            id: 0, // assigned by the registry
            obligation: ObligationKind::Protocol,
            expr: self.expr.as_str(),
            authority: Authority::InferredCandidate,
            symbols,
            source: crate::ContractSource::NamingConvention,
        }
    }
}

/// The k-bounded Ammons-style miner (sketch: `ProtocolMiner::bounded(2)`).
///
/// `k` bounds the context window for candidate generation: ordered pairs
/// are considered when the two actions co-occur within a `k`-window in
/// some trace. Candidates are GUESSES — possibly vacuously true on the
/// generating trace — and Houdini refutes them against all traces.
#[derive(Debug, Clone)]
pub struct ProtocolMiner {
    k: usize,
}

impl ProtocolMiner {
    pub fn bounded(k: usize) -> Self {
        ProtocolMiner { k: k.max(1) }
    }

    /// Mine candidates: `before(x, y)` when y's occurrences in some trace
    /// all have x within the preceding k-window; `never_after(x, y)` when
    /// some trace shows no x after y (vacuous holds count — Houdini's
    /// business is the refutation).
    pub fn mine(&self, traces: &Traces) -> Vec<ProtocolExpr> {
        let alpha = traces.alphabet();
        let mut out: Vec<ProtocolExpr> = vec![];
        for x in &alpha {
            for y in &alpha {
                if x == y {
                    continue;
                }
                // ---- before(x, y): y reached with x in the k-window ----
                let supported = traces.traces.iter().any(|t| {
                    let low: Vec<String> =
                        t.iter().map(|e| e.to_ascii_lowercase()).collect();
                    low.iter().enumerate().any(|(i, e)| {
                        e == y
                            && low[..i]
                                .iter()
                                .rev()
                                .take(self.k)
                                .any(|p| p == x)
                    })
                });
                if supported {
                    out.push(ProtocolExpr::Before {
                        then: y.clone(),
                        prior: x.clone(),
                    });
                }
                // ---- never_after(x, y): some trace without x-after-y ----
                let vacuous = traces.traces.iter().any(|t| {
                    let low: Vec<String> =
                        t.iter().map(|e| e.to_ascii_lowercase()).collect();
                    // y occurs (or not) and x never follows any y
                    !low.iter().enumerate().any(|(i, e)| {
                        e == x && low[..i].contains(y)
                    })
                });
                if vacuous {
                    out.push(ProtocolExpr::NeverAfter {
                        a: x.clone(),
                        b: y.clone(),
                    });
                }
            }
        }
        out.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        out.dedup();
        out
    }

    /// Read-only view of the registry the miner would feed (sketch:
    /// `miner.registry_view()`); the Houdini pass performs the guarded
    /// inserts via [`crate::registry::ContractRegistry::insert`].
    pub fn registry_view(&self) -> crate::registry::ContractRegistry {
        crate::registry::ContractRegistry::new()
    }
}
