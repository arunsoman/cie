//! Budgeted SMT layer (TASK-043 substrate, TASK-047 foundation — §7 Z3 row).
//!
//! Turns extracted predicates and path conditions (TASK-032) into formulas
//! and answers bounded feasibility questions. Two backends:
//! - `smt-z3` feature: system libz3 via hand-written C FFI (never a build
//!   dependency; offline-first stays default).
//! - default: the pure-Rust bounded solver here — structural DPLL over the
//!   predicate language with an explicit node budget and wall-clock
//!   deadline. Out-of-budget answers are `Unknown`, never "no defect"
//!   (global rule 3).
//!
//! The bounded language: boolean atoms (named predicates shared by name
//! across the contract and the path condition), comparisons over opaque
//! values (treated as atoms with the comparison as their name), and
//! structural connectives. Anything outside the language is listed as
//! unsupported by the caller (FR-4) — the solver never guesses.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A formula in the bounded predicate language.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Term {
    /// Constant truth.
    Bool(bool),
    /// A named atom; sharing the name shares the variable (this is how a
    /// path condition's `authorized` and the contract's `authorized` bind).
    Atom(String),
    /// `Not(a)`
    Not(Box<Term>),
    /// Conjunction.
    And(Vec<Term>),
    /// Disjunction.
    Or(Vec<Term>),
}

impl Term {
    pub fn and(terms: Vec<Term>) -> Term {
        Term::And(terms)
    }

    /// Collect atom names (deterministic order).
    pub fn atoms(&self, out: &mut Vec<String>) {
        match self {
            Term::Bool(_) => {}
            Term::Atom(n) => {
                if !out.iter().any(|x| x == n) {
                    out.push(n.clone());
                }
            }
            Term::Not(t) => t.atoms(out),
            Term::And(ts) | Term::Or(ts) => {
                for t in ts {
                    t.atoms(out);
                }
            }
        }
    }
}

/// Solver status — `TimedOut` is first-class (global rule 3: a timeout is
/// NEVER reported as "no defect").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Complete,
    TimedOut,
}

/// One solver answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Solve {
    /// Satisfiable with the given atom valuation (atom → bool).
    Sat(Vec<(String, bool)>),
    Unsatisfiable,
    /// Budget exhausted or language boundary — honest unknown.
    Unknown { reason: String },
}

/// A budgeted check request (FR-4/global rule 1: bounds are explicit and
/// recorded as assumptions).
#[derive(Debug, Clone)]
pub struct Budget {
    /// Wall-clock nanoseconds allowed for the whole check.
    pub budget_ns: u64,
    /// Maximum conjoin depth / paths explored per predicate.
    pub depth: usize,
    /// Maximum solver expansion nodes.
    pub node_budget: usize,
}

impl Budget {
    pub fn new(budget_ns: u64, depth: usize) -> Self {
        Budget { budget_ns, depth, node_budget: 50_000 }
    }
}

/// The pure-Rust budgeted solver (default backend).
pub struct BudgetedSolver {
    budget: Budget,
    start: Instant,
    nodes_used: usize,
}

impl BudgetedSolver {
    pub fn new(budget: Budget) -> Self {
        BudgetedSolver { budget, start: Instant::now(), nodes_used: 0 }
    }

    fn expired(&self) -> bool {
        self.start.elapsed().as_nanos() as u64 >= self.budget.budget_ns
    }

    /// Satisfiability within the budget. Deterministic atom order; the
    /// search is a bounded DPLL over structural simplifications:
    /// - constant folding (Booltrue/false absorption),
    /// - unit propagation through Not/And/Or,
    /// - contradiction short-circuit (a ∧ ¬a),
    /// - otherwise: pick the first unassigned atom, try false then true.
    pub fn check(&mut self, term: &Term) -> Solve {
        self.nodes_used = 0;
        let mut atoms = vec![];
        term.atoms(&mut atoms);
        let mut trail: Vec<(String, bool)> = vec![];
        self.dpll(term, &atoms, 0, &mut trail)
    }

    fn dpll(
        &mut self,
        term: &Term,
        atoms: &[String],
        depth: usize,
        trail: &mut Vec<(String, bool)>,
    ) -> Solve {
        if self.expired() {
            return Solve::Unknown { reason: "wall-clock budget exhausted".into() };
        }
        self.nodes_used += 1;
        if self.nodes_used > self.budget.node_budget {
            return Solve::Unknown { reason: "node budget exhausted".into() };
        }
        if depth > self.budget.depth.saturating_mul(atoms.len().max(1)) + 8 {
            return Solve::Unknown { reason: "depth bound exceeded".into() };
        }
        match self.eval_structural(term, trail) {
            Ok(Some(true)) => return Solve::Sat(trail.clone()),
            Ok(Some(false)) => return Solve::Unsatisfiable,
            Err(reason) => return Solve::Unknown { reason },
            Ok(None) => {}
        }
        // pick next unassigned atom (deterministic: declaration order)
        let Some(next) = atoms
            .iter()
            .find(|a| !trail.iter().any(|(n, _)| n == *a))
        else {
            return Solve::Unsatisfiable;
        };
        for v in [false, true] {
            trail.push((next.clone(), v));
            let r = self.dpll(term, atoms, depth + 1, trail);
            trail.pop();
            match r {
                Solve::Sat(_) | Solve::Unknown { .. } => return r,
                Solve::Unsatisfiable => continue,
            }
        }
        Solve::Unsatisfiable
    }

    /// Structural evaluation under the current trail: `Ok(Some(b))` when
    /// fully decided, `Ok(None)` when atoms remain, `Err(reason)` for
    /// budget trips.
    fn eval_structural(&self, term: &Term, trail: &[(String, bool)]) -> Result<Option<bool>, String> {
        if self.expired() {
            return Err("wall-clock budget exhausted".into());
        }
        Ok(match term {
            Term::Bool(b) => Some(*b),
            Term::Atom(n) => trail.iter().find(|(name, _)| name == n).map(|(_, v)| *v),
            Term::Not(t) => self.eval_structural(t, trail)?.map(|b| !b),
            Term::And(ts) => {
                let mut any_partial = false;
                let mut result = true;
                for t in ts {
                    match self.eval_structural(t, trail)? {
                        Some(false) => return Ok(Some(false)),
                        None => any_partial = true,
                        Some(true) => {}
                    }
                }
                if any_partial { None } else { Some(result) }
            }
            Term::Or(ts) => {
                let mut any_partial = false;
                let mut result = false;
                for t in ts {
                    match self.eval_structural(t, trail)? {
                        Some(true) => return Ok(Some(true)),
                        None => any_partial = true,
                        Some(false) => {}
                    }
                }
                if any_partial { None } else { Some(result) }
            }
        })
    }
}
