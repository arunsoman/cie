//! Counterexample-guided abstraction refinement (TASK-048, §7 Clarke et
//! al. CEGAR row — `cie-counterexample::cegar`).
//!
//! Loop: model-check the (cheap, over-approximating) predicate
//! abstraction → if the abstract run reaches the error state, SMT
//! concretization (TASK-047/043 backend) decides real vs spurious →
//! spurious traces contribute their distinguishing predicate back into
//! the domain, so each round strictly sharpens the check and the
//! false-alarm rate measurably drops (`spurious_traces`, `rounds_used`).
//!
//! Rules honored:
//! - Bounded refinement: `max_rounds` + per-round `budget_ns`;
//!   exhaustion → `CegarOutcome::Unknown(RefinementExhausted)` — a
//!   possibly-spurious alarm is NEVER emitted as a violation (global
//!   rule 3, §8).
//! - A counterexample is reported only after concretization PROVES the
//!   trace feasible; spurious traces never leak out as findings.
//! - Every added predicate is recorded with provenance into
//!   `Evidence.assumptions` (global rule 2) and refinement order is
//!   deterministic (predicates sorted by `(SymbolId, SourceLocation)`).
//! - `AbstractDomain` is language-neutral; per-language adapters vouch
//!   that domain operations are valid for their semantics (FR-3, global
//!   rule 6).

use crate::smt::{Budget, BudgetedSolver, Solve, Term};
use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SourceLocation, SymbolId};
use serde::{Deserialize, Serialize};

/// A predicate of the abstraction, with its provenance (deterministic
/// refinement sorts by `(SymbolId, SourceLocation)`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AbstractPred {
    pub symbol: SymbolId,
    pub name: String,
    pub loc: SourceLocation,
}

/// Language-neutral abstraction seam (FR-3/global rule 6: language
/// adapters vouch that the domain operations are valid for their
/// semantics — this trait is the vouching point).
pub trait AbstractDomain {
    /// Least upper bound of two abstract values.
    fn join(&self, a: &AbstractVal, b: &AbstractVal) -> AbstractVal;
    /// Greatest lower bound.
    fn meet(&self, a: &AbstractVal, b: &AbstractVal) -> AbstractVal;
    /// The concrete set described by an abstract value (for SMT
    /// concretization).
    fn concretize(&self, v: &AbstractVal) -> Vec<Term>;
    /// Encode an abstract value into the SMT backend.
    fn encode_to_smt(&self, v: &AbstractVal) -> Vec<Term>;
}

/// Predicate-abstraction value: the valuation of the tracked predicates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AbstractVal {
    /// (predicate name, polarity) — sorted for determinism.
    pub atoms: Vec<(String, bool)>,
}

/// Predicate abstraction (sketch: `PredAbstractDomain::new(vec![pred_p()])`).
#[derive(Debug, Clone)]
pub struct PredAbstractDomain {
    /// Sorted by (SymbolId, SourceLocation) — deterministic refinement.
    pub predicates: Vec<AbstractPred>,
}

impl PredAbstractDomain {
    pub fn new(mut preds: Vec<AbstractPred>) -> Self {
        preds.sort();
        PredAbstractDomain { predicates: preds }
    }

    /// The distinguishing predicate for a spurious trace: the first
    /// trace-guard atom (sorted) missing from the domain.
    pub fn distinguishing(&self, trace_guards: &[(String, bool)]) -> Option<AbstractPred> {
        let mut missing: Vec<(String, bool)> = trace_guards
            .iter()
            .filter(|(n, _)| !self.predicates.iter().any(|p| &p.name == n))
            .cloned()
            .collect();
        missing.sort();
        missing
            .into_iter()
            .next()
            .map(|(name, _)| AbstractPred {
                symbol: SymbolId(0), // assigned by the caller's fact table
                name,
                loc: SourceLocation::point(0.into(), 0),
            })
    }
}

impl AbstractDomain for PredAbstractDomain {
    fn join(&self, a: &AbstractVal, b: &AbstractVal) -> AbstractVal {
        // pointwise join: differing polarities drop to unconstrained
        let mut atoms = vec![];
        for (n, pa) in &a.atoms {
            match b.atoms.iter().find(|(m, _)| m == n) {
                Some((_, pb)) if pb == pa => atoms.push((n.clone(), *pa)),
                _ => {} // join removes the constraint
            }
        }
        atoms.sort();
        AbstractVal { atoms }
    }

    fn meet(&self, a: &AbstractVal, b: &AbstractVal) -> AbstractVal {
        let mut atoms = a.atoms.clone();
        for (n, p) in &b.atoms {
            if !atoms.iter().any(|(m, _)| m == n) {
                atoms.push((n.clone(), *p));
            }
        }
        atoms.sort();
        AbstractVal { atoms }
    }

    fn concretize(&self, v: &AbstractVal) -> Vec<Term> {
        self.encode_to_smt(v)
    }

    fn encode_to_smt(&self, v: &AbstractVal) -> Vec<Term> {
        v.atoms
            .iter()
            .map(|(n, p)| {
                if *p {
                    Term::Atom(n.clone())
                } else {
                    Term::Not(Box::new(Term::Atom(n.clone())))
                }
            })
            .collect()
    }
}

/// A miniature CFA program: edges with boolean guard atoms, an error
/// node, and the extracted FACT base (TASK-032 predicates — what is
/// concretely true).
#[derive(Debug, Clone, Default)]
pub struct CfaProgram {
    /// (from, guard atom, polarity, to) — deterministic edge order.
    pub edges: Vec<(usize, String, bool, usize)>,
    pub error_node: usize,
    /// Extracted facts: concretely-true atoms (e.g. q == false).
    pub facts: Vec<(String, bool)>,
}

/// The outcome of a bounded CEGAR run.
#[derive(Debug, Clone, PartialEq)]
pub enum CegarOutcome {
    /// The abstraction proves the error unreachable.
    Safe(CegarMetrics),
    /// A CONCRETE counterexample (concretization proved feasibility).
    Counterexample(Vec<(String, bool)>),
    /// Bounded refinement exhausted — the alarm stays UNKNOWN (rule 3).
    Unknown(RefinementExhausted),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefinementExhausted {
    RefinementExhausted,
}

/// False-alarm-reduction metrics (sketch: `cegar.metrics()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CegarMetrics {
    pub spurious_traces: usize,
    pub rounds_used: usize,
}

/// The CEGAR loop (sketch: `CegarLoop::new(domain, smt_backend)`).
pub struct CegarLoop {
    pub domain: PredAbstractDomain,
    backend_facts: Vec<(String, bool)>,
    metrics: CegarMetrics,
    added: Vec<AbstractPred>,
    rev: RevId,
}

impl CegarLoop {
    pub fn new(domain: PredAbstractDomain, _smt_backend: ()) -> Self {
        CegarLoop {
            domain,
            backend_facts: vec![],
            metrics: CegarMetrics::default(),
            added: vec![],
            rev: 0,
        }
    }

    pub fn with_rev(mut self, rev: RevId) -> Self {
        self.rev = rev;
        self
    }

    pub fn metrics(&self) -> CegarMetrics {
        self.metrics
    }

    /// Refinement evidence (rule 2): every added predicate recorded.
    pub fn evidence(&self) -> Evidence {
        Evidence {
            rev: self.rev,
            provenance: format!(
                "cegar: {} round(s), {} spurious trace(s), {} predicate(s) added",
                self.metrics.rounds_used,
                self.metrics.spurious_traces,
                self.added.len()
            ),
            assumptions: self
                .added
                .iter()
                .map(|p| format!("added predicate `{}` ({:?})", p.name, p.loc))
                .collect(),
            unsupported: vec![],
            unresolved: vec![],
            generated_at_ns: 0,
        }
    }

    /// The predicates currently in the domain (sketch:
    /// `refinement_predicates()`).
    pub fn refinement_predicates(&self) -> &[AbstractPred] {
        &self.domain.predicates
    }

    /// Bounded refinement (sketch: `cegar.check(&program, limits)`).
    pub fn check(
        &mut self,
        program: &CfaProgram,
        limits: RefineLimits,
    ) -> CegarOutcome {
        self.backend_facts = program.facts.clone();
        for round in 0..limits.max_rounds {
            self.metrics.rounds_used = round + 1;
            // ---- abstract model check (deterministic BFS) ----
            let Some(abstract_trace) = self.abstract_reach_error(program) else {
                return CegarOutcome::Safe(self.metrics);
            };
            // ---- concretization (TASK-047/043 backend) ----
            let mut conj: Vec<Term> = abstract_trace
                .iter()
                .map(|(n, p)| {
                    if *p {
                        Term::Atom(n.clone())
                    } else {
                        Term::Not(Box::new(Term::Atom(n.clone())))
                    }
                })
                .collect();
            for (n, p) in &program.facts {
                conj.push(if *p {
                    Term::Atom(n.clone())
                } else {
                    Term::Not(Box::new(Term::Atom(n.clone())))
                });
            }
            let term = Term::and(conj);
            let mut solver = BudgetedSolver::new(Budget::new(
                limits.round_budget_ns.max(0) as u64,
                64,
            ));
            match solver.check(&term) {
                Solve::Sat(_) => {
                    // concretization proved the trace feasible — REAL
                    return CegarOutcome::Counterexample(abstract_trace);
                }
                Solve::Unsatisfiable => {
                    // spurious: refine with the distinguishing predicate
                    self.metrics.spurious_traces += 1;
                    if let Some(pred) = self.domain.distinguishing(&abstract_trace) {
                        self.added.push(pred.clone());
                        self.domain.predicates.push(pred);
                        self.domain.predicates.sort();
                    } else {
                        // nothing left to distinguish with: honest unknown
                        return CegarOutcome::Unknown(RefinementExhausted::RefinementExhausted);
                    }
                }
                Solve::Unknown { .. } => {
                    return CegarOutcome::Unknown(RefinementExhausted::RefinementExhausted);
                }
            }
        }
        CegarOutcome::Unknown(RefinementExhausted::RefinementExhausted)
    }

    /// Deterministic BFS over (node, tracked-valuation) states; returns
    /// the guard trace to the error node when abstractly reachable.
    ///
    /// The initial valuation SEEDES the tracked extracted facts (TASK-032
    /// predicates that are concretely known) — that is what lets a
    /// refined domain block a spurious trace. A guard is abstractly
    /// BLOCKED only when its atom is tracked AND the valuation fixes it
    /// to the opposite polarity (untracked guards stay enabled — the
    /// over-approximation that fuels refinement).
    fn abstract_reach_error(&self, program: &CfaProgram) -> Option<Vec<(String, bool)>> {
        use std::collections::{BTreeSet, VecDeque};
        // seed: tracked facts only (untracked facts are invisible to the
        // abstraction — they are what concretization uses to refute)
        let mut start_val: Vec<(String, bool)> = program
            .facts
            .iter()
            .filter(|(n, _)| self.domain.predicates.iter().any(|p| &p.name == n))
            .cloned()
            .collect();
        start_val.sort();
        let mut visited: BTreeSet<(usize, String)> = BTreeSet::new();
        let mut queue: VecDeque<(usize, Vec<(String, bool)>, Vec<(String, bool)>)> =
            VecDeque::new();
        let key0 = val_key(&start_val);
        queue.push_back((0usize, start_val.clone(), vec![]));
        visited.insert((0usize, key0));
        while let Some((node, val, trace)) = queue.pop_front() {
            if node == program.error_node {
                return Some(trace);
            }
            for (from, atom, polarity, to) in &program.edges {
                if *from != node {
                    continue;
                }
                // blocked iff tracked and contradicted by the valuation
                let tracked = self.domain.predicates.iter().any(|p| &p.name == atom);
                let tracked_conflict =
                    tracked && val.iter().any(|(n, v)| n == atom && v != polarity);
                if tracked_conflict {
                    continue;
                }
                let mut next_val = val.clone();
                if tracked && !next_val.iter().any(|(n, _)| n == atom) {
                    next_val.push((atom.clone(), *polarity));
                    next_val.sort();
                }
                let key = val_key(&next_val);
                if visited.insert((*to, key)) {
                    let mut next_trace = trace.clone();
                    next_trace.push((atom.clone(), *polarity));
                    queue.push_back((*to, next_val, next_trace));
                }
            }
        }
        None
    }
}

fn val_key(val: &[(String, bool)]) -> String {
    val.iter().map(|(n, p)| format!("{n}={p}")).collect::<Vec<_>>().join(",")
}

/// Bounded refinement limits (sketch: `RefineLimits`).
#[derive(Debug, Clone, Copy)]
pub struct RefineLimits {
    pub max_rounds: usize,
    pub round_budget_ns: i64,
}
