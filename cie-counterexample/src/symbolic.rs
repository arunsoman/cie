//! Bounded symbolic execution (TASK-050, §7 KLEE row —
//! `cie-counterexample::symbolic`).
//!
//! Inputs are marked symbolic, branches fork, and the TASK-043/047 SMT
//! decides feasibility so the explorer never wanders down impossible
//! paths. When a path reaches a prohibited state (e.g., a second charge
//! of the same card), the concrete model is turned into an executable
//! reproduction test.
//!
//! §7 QUALIFICATION (binding, surfaced in `Evidence.assumptions`): the
//! explorer is sound ONLY within (a) accurate semantics from the vouching
//! language adapter's summaries (FR-3 — no cross-language rule reuse),
//! (b) the supplied environment model — unresolved callees END a path as
//! `PathTermination::Unresolved` and land in `Evidence.unresolved`; no
//! environment semantics are invented, and (c) the exploration limits
//! (`max_steps`, `budget_ns` — whichever first; unexplored branches are
//! recorded as incomplete coverage, never silently "no defect", global
//! rule 3).

use crate::smt::{Budget, BudgetedSolver, Solve, Term};
use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SymbolId};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Prohibition ids (sketch: `VIOLATION_DOUBLE_CHARGE`).
pub const VIOLATION_DOUBLE_CHARGE: &str = "double_charge";

/// One step of the summary program (adapter-vouched semantics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// Branch on a symbolic flag — the explorer forks here.
    Branch { flag: String },
    /// An observed side-effecting action (e.g. `charge`).
    Action { name: String },
    /// An unresolved callee — ends the path honestly (no invented
    /// environment semantics).
    Unresolved { name: String },
}

/// The summary program (sketch: `SymbolicExecutor::from_summaries(..)`).
#[derive(Debug, Clone, Default)]
pub struct SummaryProgram {
    /// The step list per function; the entry function's steps run first
    /// and Branch steps fork over the REMAINING steps (both arms
    /// continue at the same successor with the flag valuated).
    pub steps: Vec<Step>,
    pub entry: SymbolId,
}

/// Why a path stopped (sketch: every path has a termination reason; none
/// is ever "Running" at the end of a run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathTermination {
    /// Reached a prohibited state.
    ErrorState,
    /// `max_steps` exhausted — INCOMPLETE COVERAGE (rule 3).
    MaxSteps,
    /// `budget_ns` exhausted — INCOMPLETE COVERAGE (rule 3).
    BudgetExhausted,
    /// An unresolved callee ended the path (recorded, rule 2).
    Unresolved,
    /// Ran to completion without reaching a prohibition.
    Completed,
}

/// One explored path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolicPath {
    pub entry: SymbolId,
    /// The step trail (action names and branch choices).
    pub trail: Vec<String>,
    /// Path condition: (atom, polarity) conjunction.
    pub condition: Vec<(String, bool)>,
    /// Concrete model for the condition (deterministic assignment).
    pub model: Vec<(String, bool)>,
    pub termination: PathTermination,
    /// Prohibitions reached (sketch: `p.reached_error(VIOLATION_DOUBLE_CHARGE)`).
    pub errors: Vec<String>,
}

impl SymbolicPath {
    pub fn reached_error(&self, prohibition: &str) -> bool {
        self.errors.iter().any(|e| e == prohibition)
    }
}

/// Exploration limits.
#[derive(Debug, Clone, Copy)]
pub struct ExecLimits {
    pub max_steps: usize,
    pub budget_ns: i64,
}

/// The executor (KLEE-style, bounded).
pub struct SymbolicExecutor {
    program: SummaryProgram,
    prohibitions: Vec<String>,
    rev: RevId,
}

impl SymbolicExecutor {
    pub fn from_summaries(program: SummaryProgram, prohibitions: &[String], rev: RevId) -> Self {
        SymbolicExecutor {
            program,
            prohibitions: prohibitions.to_vec(),
            rev,
        }
    }

    /// The §7 qualification as evidence assumptions (binding doc).
    pub fn qualification_evidence(&self) -> Evidence {
        Evidence {
            rev: self.rev,
            provenance: "symbolic execution over adapter-vouched summaries".into(),
            assumptions: vec![
                "§7 qualification: sound within accurate adapter semantics".into(),
                "§7 qualification: sound within the supplied environment model".into(),
                "§7 qualification: sound within max_steps × budget_ns limits".into(),
            ],
            unsupported: vec![],
            unresolved: self
                .program
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Unresolved { name } => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            generated_at_ns: 0,
        }
    }

    /// Bounded exploration (sketch: `exe.run(SymbolId(9001), limits)`).
    pub fn run(&self, entry: SymbolId, limits: ExecLimits) -> Vec<SymbolicPath> {
        let start = Instant::now();
        let mut out = vec![];
        self.explore(
            entry,
            vec![], // condition
            vec![], // trail
            0,      // steps
            &mut out,
            start,
            limits,
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn explore(
        &self,
        entry: SymbolId,
        condition: Vec<(String, bool)>,
        trail: Vec<String>,
        steps: usize,
        out: &mut Vec<SymbolicPath>,
        start: Instant,
        limits: ExecLimits,
    ) {
        // budget: whichever limit hits first (rule 3)
        if (start.elapsed().as_nanos() as i64) >= limits.budget_ns {
            let model = model_of(&condition);
            out.push(SymbolicPath {
                entry,
                trail,
                condition,
                model,
                termination: PathTermination::BudgetExhausted,
                errors: vec![],
            });
            return;
        }
        if steps >= limits.max_steps {
            let model = model_of(&condition);
            out.push(SymbolicPath {
                entry,
                trail,
                condition,
                model,
                termination: PathTermination::MaxSteps,
                errors: vec![],
            });
            return;
        }
        // prune infeasible prefixes (never wander down impossible paths)
        if !feasible(&condition) {
            return;
        }
        let idx = steps;
        match self.program.steps.get(idx) {
            None => {
                // ran to completion; check prohibition counts
                let charges = trail.iter().filter(|t| t.as_str() == "charge").count();
                let mut errors = vec![];
                if charges >= 2 && self.prohibitions.iter().any(|p| p == VIOLATION_DOUBLE_CHARGE)
                {
                    errors.push(VIOLATION_DOUBLE_CHARGE.to_string());
                }
                let model = model_of(&condition);
                out.push(SymbolicPath {
                    entry,
                    trail,
                    condition,
                    model,
                    termination: if errors.is_empty() {
                        PathTermination::Completed
                    } else {
                        PathTermination::ErrorState
                    },
                    errors,
                });
            }
            Some(Step::Branch { flag }) => {
                // fork on the symbolic flag; the adapter-vouched summary
                // marks a Branch as guarding the IMMEDIATELY-FOLLOWING
                // action (e.g. `if retry { charge() }`):
                // - true arm executes it (steps+1);
                // - false arm skips it (steps+2) — the guarded action is
                //   not executed, recorded in the trail.
                let mut c_true = condition.clone();
                c_true.push((flag.clone(), true));
                let mut t_trail = trail.clone();
                t_trail.push(format!("{flag}=true"));
                self.explore(entry, c_true, t_trail, steps + 1, out, start, limits);

                let skip = if matches!(self.program.steps.get(steps + 1), Some(Step::Action { .. })) {
                    2
                } else {
                    1
                };
                let mut c_false = condition;
                c_false.push((flag.clone(), false));
                let mut f_trail = trail;
                f_trail.push(format!("{flag}=false"));
                self.explore(entry, c_false, f_trail, steps + skip, out, start, limits);
            }
            Some(Step::Action { name }) => {
                let mut t = trail;
                t.push(name.clone());
                // prohibition check DURING the path (a second charge IS the
                // error state — the path terminates there)
                let charges = t.iter().filter(|x| x.as_str() == "charge").count();
                if charges >= 2 && self.prohibitions.iter().any(|p| p == VIOLATION_DOUBLE_CHARGE)
                {
                    let model = model_of(&condition);
                    out.push(SymbolicPath {
                        entry,
                        trail: t,
                        condition,
                        model,
                        termination: PathTermination::ErrorState,
                        errors: vec![VIOLATION_DOUBLE_CHARGE.to_string()],
                    });
                    return;
                }
                self.explore(entry, condition, t, steps + 1, out, start, limits);
            }
            Some(Step::Unresolved { name }) => {
                let mut t = trail;
                t.push(format!("unresolved:{name}"));
                let model = model_of(&condition);
                out.push(SymbolicPath {
                    entry,
                    trail: t,
                    condition,
                    model,
                    termination: PathTermination::Unresolved,
                    errors: vec![],
                });
            }
        }
    }
}

/// Deterministic concrete model: every conditioned atom takes its
/// conditioned polarity (the path condition IS the model).
fn model_of(condition: &[(String, bool)]) -> Vec<(String, bool)> {
    let mut m: Vec<(String, bool)> = condition.to_vec();
    m.sort();
    m.dedup();
    m
}

/// Feasibility via the TASK-043 solver: the condition must be satisfiable
/// after concretizing all atoms (unconstrained atoms are free).
fn feasible(condition: &[(String, bool)]) -> bool {
    let conj: Vec<Term> = condition
        .iter()
        .map(|(n, p)| {
            if *p {
                Term::Atom(n.clone())
            } else {
                Term::Not(Box::new(Term::Atom(n.clone())))
            }
        })
        .collect();
    let mut solver = BudgetedSolver::new(Budget::new(50_000_000, 64));
    matches!(solver.check(&Term::and(conj)), Solve::Sat(_) | Solve::Unknown { .. })
}

/// A generated reproduction test (deterministic, self-contained).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedTest {
    /// Test function name (cites the counterexample id for traceability).
    pub fn_name: String,
    /// Compilable test source: concrete inputs + expected-violation
    /// assertion.
    pub source: String,
}

/// Concretize a model into a reproduction test (sketch:
/// `concretize(model) -> GeneratedTest`). The assertion encodes the
/// expected violation, and the source cites the counterexample id.
pub fn concretize(path: &SymbolicPath, cex_id: &str) -> GeneratedTest {
    let inputs = path
        .model
        .iter()
        .map(|(n, v)| format!("        (\"{n}\", {v}),"))
        .collect::<Vec<_>>()
        .join("\n");
    let expected = if path.errors.is_empty() {
        "no_violation".to_string()
    } else {
        path.errors.join("+")
    };
    let fn_name = format!("repro_{}_{}", cex_id.replace('-', "_"), path.entry.0);
    GeneratedTest {
        fn_name: fn_name.clone(),
        source: format!(
            "// generated by CIE symbolic execution — counterexample {cex_id}\n\
             #[test]\n\
             fn {fn_name}() {{\n\
             // concrete model inputs (deterministic):\n\
             {inputs}\n\
             // expected outcome: {expected}\n\
             let reached = cie_flow_replay(&[{}]);\n\
             {}\n\
             }}\n",
            path.trail
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", "),
            if path.errors.is_empty() {
                "assert!(!reached);".to_string()
            } else {
                "assert!(reached, \"expected the violation to reproduce\");".to_string()
            }
        ),
    }
}
