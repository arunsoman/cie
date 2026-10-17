//! Bounded predicate checking (TASK-043): extracted predicates conjoined
//! with TASK-032 path conditions, checked under explicit budget semantics.
//!
//! - `ExtractedPredicate` is the shared shape: one contract obligation
//!   over one action pair, with provenance back to its contract (rule 2).
//! - `BoundedChecker` realizes paths (path-condition sets over action
//!   atoms) and decides each predicate per path: violated / clean /
//!   unknown. Bounds (budget + depth) are recorded as assumptions
//!   (FR-4/global rule 1); exhaustion yields `Conclusion`-level
//!   `Unknown/TimedOut` for every remaining predicate (global rule 3).
//! - A fully-clean complete run yields a mechanical proof string the
//!   caller feeds to the guarded registry's `upgrade_to_verified` (§8:
//!   the ONLY InferredCandidate → Verified path).

use crate::smt::{Budget, BudgetedSolver, CheckStatus, Solve, Term};
use cie_core::evidence::Evidence;
use cie_core::types::RevId;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// One extracted predicate: `antecedent requires consequent` (e.g. charge
/// requires authorized). Action names are shared atoms with path
/// conditions — this is the semantic glue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedPredicate {
    /// Provenance back to the source contract (rule 2); `None` = inline.
    pub from_contract: Option<u32>,
    pub antecedent: String,
    pub consequent: String,
    /// Path conditions conjoined with this predicate's obligation, per
    /// realized path (TASK-032 output, action-atom form).
    pub paths: Vec<PathAtomSet>,
    /// Constructs the extractor could not lower (FR-4 honesty).
    pub unsupported: Vec<String>,
}

/// One realized path: the conjunction of action atoms observed along it
/// (`true` = action occurs / condition holds, `false` = it does not) plus
/// the call trail for counterexample rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathAtomSet {
    /// (atom, polarity) — e.g. ("authorized", false) = the unauthorized arm.
    pub atoms: Vec<(String, bool)>,
    /// Call names along the realized path (counterexample trail).
    pub trail: Vec<String>,
}

/// Per-path verdict for one predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conclusion {
    /// The obligation holds along the path.
    Clean,
    /// The obligation is breached along the path (realized).
    Violated,
    /// Budget/opacity: no sound answer (never reported as clean — rule 3).
    Unknown,
}

/// Status of the whole bounded run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Complete,
    TimedOut,
}

/// A realized violation with its counterexample trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counterexample {
    /// Call trail; the final step is the breaching site (sketch:
    /// `path.ends_with(fx.unauthorized_call)`).
    pub path: Vec<String>,
}

/// One verdict record for a predicate.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub antecedent: String,
    pub consequent: String,
    pub from_contract: Option<u32>,
    pub conclusion: Conclusion,
    pub status: Status,
    pub counterexample: Option<Counterexample>,
    pub evidence: Evidence,
}

/// The bounded checker (sketch: `BoundedChecker::new(&fx).budget_ns(..).depth(..)`).
pub struct BoundedChecker {
    budget: Budget,
}

impl BoundedChecker {
    pub fn new() -> Self {
        BoundedChecker { budget: Budget::new(5_000_000_000, 64) }
    }

    pub fn budget_ns(mut self, ns: u64) -> Self {
        self.budget.budget_ns = ns;
        self
    }

    pub fn depth(mut self, d: usize) -> Self {
        self.budget.depth = d;
        self
    }

    /// Check every predicate over its realized paths.
    pub fn check(&self, preds: &[ExtractedPredicate], rev: RevId) -> Vec<Verdict> {
        let mut out = vec![];
        for p in preds {
            out.push(self.check_one(p, rev));
        }
        out
    }

    fn check_one(&self, p: &ExtractedPredicate, rev: RevId) -> Verdict {
        let start = Instant::now();
        let bounds = vec![
            format!("budget_ns={}", self.budget.budget_ns),
            format!("depth={}", self.budget.depth),
            format!("node_budget={}", self.budget.node_budget),
        ];
        let mut assumptions = bounds;
        assumptions.push(format!(
            "obligation: {} requires {}",
            p.antecedent, p.consequent
        ));

        // Paths are checked in order; the FIRST budget trip converts the
        // remaining decision to Unknown/TimedOut for the whole predicate
        // (rule 3: partial coverage never reads as clean).
        let mut violated: Option<Counterexample> = None;
        let mut all_clean = true;
        let mut timed_out = false;
        let mut reasons: Vec<String> = vec![];
        let mut solver = BudgetedSolver::new(self.budget.clone());

        for path in &p.paths {
            if timed_out {
                break;
            }
            if start.elapsed().as_nanos() as u64 >= self.budget.budget_ns {
                timed_out = true;
                break;
            }
            // Path atoms: does this path realize the antecedent?
            let antecedent_true = path
                .atoms
                .iter()
                .any(|(a, v)| *a == p.antecedent && *v);
            let consequent_atom = path
                .atoms
                .iter()
                .find(|(a, _)| *a == p.consequent)
                .map(|(_, v)| *v);
            if !antecedent_true {
                continue; // obligation not triggered on this path
            }
            match consequent_atom {
                Some(true) => {} // authorized: obligation satisfied here
                Some(false) => {
                    // breached: counterexample is this path's trail
                    if violated.is_none() {
                        violated = Some(Counterexample { path: path.trail.clone() });
                    }
                    all_clean = false;
                }
                None => {
                    // opaque: encode path ∧ ¬consequent and ask the solver
                    let mut conj: Vec<Term> = path
                        .atoms
                        .iter()
                        .map(|(a, v)| if *v { Term::Atom(a.clone()) } else { Term::Not(Box::new(Term::Atom(a.clone()))) })
                        .collect();
                    conj.push(Term::Not(Box::new(Term::Atom(p.consequent.clone()))));
                    let term = Term::and(conj);
                    match solver.check(&term) {
                        Solve::Sat(_) => {
                            if violated.is_none() {
                                violated = Some(Counterexample { path: path.trail.clone() });
                            }
                            all_clean = false;
                        }
                        Solve::Unsatisfiable => {} // provably safe on this path
                        Solve::Unknown { reason } => {
                            timed_out = true;
                            reasons.push(reason);
                        }
                    }
                }
            }
        }

        let status = if timed_out { Status::TimedOut } else { Status::Complete };
        let conclusion = if timed_out {
            Conclusion::Unknown // rule 3: never clean on timeout
        } else if violated.is_some() {
            Conclusion::Violated
        } else if all_clean {
            Conclusion::Clean
        } else {
            Conclusion::Unknown
        };

        let mut unsupported = p.unsupported.clone();
        for r in &reasons {
            unsupported.push(r.clone());
        }
        // rule 3 store-guard consistency: never claim Clean while a
        // timeout marker rides along (mirror of the record store's check).
        let evidence = Evidence {
            rev,
            provenance: format!(
                "bounded check: {} requires {} over {} path(s); outcome {:?}",
                p.antecedent,
                p.consequent,
                p.paths.len(),
                conclusion
            ),
            assumptions,
            unsupported,
            unresolved: vec![],
            generated_at_ns: 0,
        };
        Verdict {
            antecedent: p.antecedent.clone(),
            consequent: p.consequent.clone(),
            from_contract: p.from_contract,
            conclusion,
            status,
            counterexample: violated,
            evidence,
        }
    }

    /// The mechanical proof string for a fully-clean complete run — the
    /// ONLY input the registry accepts for `upgrade_to_verified`.
    pub fn proof_string(verdicts: &[Verdict]) -> Option<String> {
        if verdicts.is_empty() {
            return None;
        }
        let all_clean = verdicts
            .iter()
            .all(|v| v.conclusion == Conclusion::Clean && v.status == Status::Complete);
        if all_clean {
            Some(format!(
                "bounded check: {} predicate(s) clean under stated bounds",
                verdicts.len()
            ))
        } else {
            None
        }
    }
}

impl Default for BoundedChecker {
    fn default() -> Self {
        Self::new()
    }
}
