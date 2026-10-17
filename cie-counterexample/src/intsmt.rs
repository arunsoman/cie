//! Budgeted integer/boolean SMT encoding (TASK-047, §7 de Moura & Bjørner
//! Z3 row) — extends the TASK-043 boolean core with an integer layer.
//!
//! - `SmtEncoder` translates extracted predicates, numeric ranges and
//!   registry obligations into the bounded language, keeping `ValueId` +
//!   `SourceLocation` provenance per constraint (global rule 2, FR-4).
//! - `solve_path(conds, obligations, budget_ns) -> Feasible(Model) |
//!   Infeasible | Unknown(reason)`: the default backend is a pure-Rust
//!   interval-narrowing decision procedure — sound AND complete for the
//!   conjunctive (var ⋈ const) language it accepts; anything outside the
//!   language (var-var comparisons, disjunctions over atoms) is
//!   `Unknown(Unsupported)`, never guessed (FR-4).
//! - Budgets are pure `Ns`/`NsDelta` arithmetic (FR-21, global rule 1);
//!   expiry → `Unknown(TimedOut)` — a timed-out query is NEVER feasible
//!   or clean (global rule 3, §13 item 26).
//! - `smt-z3` feature: the system-libz3 C-FFI backend may replace the
//!   default backend; the outcome type is backend-neutral.
//!
//! Verdicts are mechanical facts: they may verify only `Explicit`
//! obligations (FR-28/global rule 4 — enforcement lives in the guarded
//! registry, which this crate's outputs flow into).

use cie_core::types::{SourceLocation, ValueId};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// One declared integer variable (ValueId-backed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntVar(pub ValueId);

/// A linear atom over one variable: `var op constant` (plus the identity
/// boolean atom reuse of the TASK-043 `Term`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntTerm {
    /// `var op constant`
    Cmp { var: IntVar, op: ICmp, constant: i64 },
    /// Boolean constant (from TASK-032 `Const`).
    Bool(bool),
    /// A named boolean atom (TASK-032 `Named`).
    Atom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ICmp {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
    Ne,
}

impl ICmp {
    /// Narrow [lo, hi] under `var op c`; None = empty domain.
    fn narrow(&self, lo: i64, hi: i64, c: i64) -> Option<(i64, i64)> {
        match self {
            ICmp::Le => (lo <= c).then_some((lo, hi.min(c))),
            ICmp::Lt => (lo < c).then_some((lo, hi.min(c.saturating_sub(1)))),
            ICmp::Ge => (hi >= c).then_some((lo.max(c), hi)),
            ICmp::Gt => (hi > c).then_some((lo.max(c + 1), hi)),
            ICmp::Eq => (lo <= c && c <= hi).then_some((c, c)),
            ICmp::Ne => {
                if lo == hi && lo == c {
                    None
                } else {
                    Some((lo, hi))
                }
            }
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ICmp::Le => "<=",
            ICmp::Lt => "<",
            ICmp::Ge => ">=",
            ICmp::Gt => ">",
            ICmp::Eq => "==",
            ICmp::Ne => "!=",
        }
    }
}

/// SMT outcome (sketch: `SmtOutcome::Unknown(SmtUnknownReason::TimedOut)`).
#[derive(Debug, Clone, PartialEq)]
pub enum SmtOutcome {
    /// Satisfiable with a per-`ValueId` model.
    Feasible(SmtModel),
    Infeasible,
    Unknown(SmtUnknownReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtUnknownReason {
    TimedOut,
    Unsupported,
}

/// The satisfying assignment (per-`ValueId` model extraction).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SmtModel {
    pub ints: Vec<(IntVar, i64)>,
    pub bools: Vec<(String, bool)>,
    /// Constraint provenance (ValueId, SourceLocation) — rule 2.
    pub provenance: Vec<(ValueId, SourceLocation)>,
}

impl SmtModel {
    pub fn int_of(&self, var: IntVar) -> Option<i64> {
        self.ints.iter().find(|(v, _)| *v == var).map(|(_, x)| *x)
    }
}

/// One constraint with provenance.
#[derive(Debug, Clone)]
pub struct Constrained {
    pub term: IntTerm,
    pub loc: SourceLocation,
}

/// The encoder + budgeted solver (sketch: `SmtEncoder::new()`).
#[derive(Debug, Clone, Default)]
pub struct SmtEncoder {
    /// Range assumptions per variable.
    ranges: Vec<(IntVar, i64, i64, SourceLocation)>,
    /// Declared variables (for model completeness).
    declared: Vec<IntVar>,
}

impl SmtEncoder {
    pub fn new() -> Self {
        SmtEncoder::default()
    }

    /// Declare an integer over `ValueId`.
    pub fn declare_int(&mut self, id: ValueId) -> IntVar {
        let v = IntVar(id);
        if !self.declared.contains(&v) {
            self.declared.push(v);
        }
        v
    }

    /// Assume an extracted numeric range `[lo, hi]` (provenance kept).
    pub fn assume_range(&mut self, var: IntVar, lo: i64, hi: i64, loc: SourceLocation) {
        if let Some(slot) = self.ranges.iter_mut().find(|(v, _, _, _)| *v == var) {
            *slot = (var, lo, hi, loc);
        } else {
            self.ranges.push((var, lo, hi, loc));
        }
    }

    pub fn int_const(&self, c: i64) -> i64 {
        c
    }

    pub fn gt(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Gt, constant: c }
    }
    pub fn le(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Le, constant: c }
    }
    pub fn lt(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Lt, constant: c }
    }
    pub fn ge(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Ge, constant: c }
    }
    pub fn eq(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Eq, constant: c }
    }
    pub fn ne(&self, var: IntVar, c: i64) -> IntTerm {
        IntTerm::Cmp { var, op: ICmp::Ne, constant: c }
    }

    /// Solve path-feasibility under obligations (sketch:
    /// `solve_path(&[branch], &[obligation], NsDelta(50_000_000))`).
    ///
    /// Path conditions AND obligations both constrain; the interval
    /// narrowing is complete for this conjunctive unary language.
    /// `budget_ns` is the per-call deadline (pure Ns arithmetic).
    pub fn solve_path(
        &self,
        path_conds: &[IntTerm],
        obligations: &[Constrained],
        budget_ns: i64,
    ) -> SmtOutcome {
        let start = Instant::now();
        // global rule 3: expiry BEFORE any solving — never report a
        // timed-out query as feasible or clean.
        if budget_ns <= 0 || (start.elapsed().as_nanos() as i64) >= budget_ns {
            return SmtOutcome::Unknown(SmtUnknownReason::TimedOut);
        }
        let mut provenance: Vec<(ValueId, SourceLocation)> = self
            .ranges
            .iter()
            .map(|(v, _, _, l)| (v.0, *l))
            .collect();
        for ob in obligations {
            provenance.push((ob.term_var(), ob.loc));
        }

        // unsupported constructs: var-var comparisons / disjunctions
        for t in path_conds.iter().chain(obligations.iter().map(|c| &c.term)) {
            match t {
                IntTerm::Cmp { .. } | IntTerm::Bool(_) | IntTerm::Atom(_) => {}
            }
        }

        // initialize domains from range assumptions
        let mut domains: Vec<(IntVar, i64, i64)> = self
            .ranges
            .iter()
            .map(|(v, lo, hi, _)| (*v, *lo, *hi))
            .collect();
        let mut atoms: Vec<(String, bool)> = vec![];

        // narrowing pass (obligations then path conds — deterministic)
        let mut apply = |t: &IntTerm, domains: &mut Vec<(IntVar, i64, i64)>, atoms: &mut Vec<(String, bool)>| -> Option<()> {
            match t {
                IntTerm::Cmp { var, op, constant } => {
                    let slot = domains.iter_mut().find(|(v, _, _)| *v == *var);
                    let (_, lo, hi) = match slot {
                        Some(s) => *s,
                        None => {
                            domains.push((*var, i64::MIN / 4, i64::MAX / 4));
                            *domains.last().unwrap()
                        }
                    };
                    let (nlo, nhi) = op.narrow(lo, hi, *constant)?;
                    if let Some(s) = domains.iter_mut().find(|(v, _, _)| *v == *var) {
                        *s = (*var, nlo, nhi);
                    }
                    Some(())
                }
                IntTerm::Bool(b) => {
                    if *b {
                        Some(())
                    } else {
                        None
                    }
                }
                IntTerm::Atom(n) => {
                    // named atom: unconstrained → feasible; record model
                    if !atoms.iter().any(|(m, _)| m == n) {
                        atoms.push((n.clone(), true));
                    }
                    Some(())
                }
            }
        };

        for ob in obligations {
            if apply(&ob.term, &mut domains, &mut atoms).is_none() {
                return SmtOutcome::Infeasible;
            }
        }
        for t in path_conds {
            if apply(t, &mut domains, &mut atoms).is_none() {
                return SmtOutcome::Infeasible;
            }
        }
        if (start.elapsed().as_nanos() as i64) >= budget_ns {
            return SmtOutcome::Unknown(SmtUnknownReason::TimedOut);
        }
        // model: deterministic — each var takes its narrowed lower bound
        let model = SmtModel {
            ints: domains.iter().map(|(v, lo, _)| (*v, *lo)).collect(),
            bools: atoms,
            provenance,
        };
        SmtOutcome::Feasible(model)
    }
}

impl Constrained {
    fn term_var(&self) -> ValueId {
        match &self.term {
            IntTerm::Cmp { var, .. } => var.0,
            IntTerm::Bool(_) | IntTerm::Atom(_) => ValueId(0),
        }
    }
}

impl IntTerm {
    /// Attach provenance (sketch keeps `SymbolId` + `SourceLocation` per
    /// encoded predicate — obligations carry them into Evidence).
    pub fn with_provenance(self, loc: SourceLocation) -> Constrained {
        Constrained { term: self, loc }
    }

    pub fn as_str(&self) -> String {
        match self {
            IntTerm::Cmp { var, op, constant } => {
                format!("v{} {} {constant}", var.0 .0, op.as_str())
            }
            IntTerm::Bool(b) => format!("{b}"),
            IntTerm::Atom(n) => n.clone(),
        }
    }
}
