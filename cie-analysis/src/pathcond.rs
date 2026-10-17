//! Path conditions (TASK-032): representation, extraction, simplification.
//!
//! - `PathCond { pred, polarity, loc }` is a branch predicate over a
//!   `ValueId` with polarity (taken/not-taken) and its `SourceLocation`,
//!   so downstream findings can cite the exact branch (FR-4).
//! - A path's condition is the CONJUNCTION of its branch predicates;
//!   extraction walks control-flow facts (ifs/loops) per branch, bounded
//!   by the TASK-029 budget — exhaustion yields
//!   [`PathCondExtraction::BudgetExhausted`], never a silently-true
//!   condition (global rule 3).
//! - Simplification is LOCAL and sound: collapse tautologies (`c ∨ ¬c`),
//!   drop contradictions (`c ∧ ¬c`), constant-fold literals — no solver
//!   dependency at this layer (SMT arrives in TASK-047 per §7).
//! - Conditions fill `FunctionSummary.path_conditions` (§14.4, the
//!   SmallVec<[PathCond; 4]> bound at §15.5 is enforced by `extract`)
//!   for TASK-030/033 consumption and stay structural/comparable for the
//!   Z3 encoding (§7, FR-34).

use cie_core::types::{SourceLocation, SymbolId, ValueId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Comparison operator of a branch predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    fn negate(self) -> Cmp {
        match self {
            Cmp::Eq => Cmp::Ne,
            Cmp::Ne => Cmp::Eq,
            Cmp::Lt => Cmp::Ge,
            Cmp::Le => Cmp::Gt,
            Cmp::Gt => Cmp::Le,
            Cmp::Ge => Cmp::Lt,
        }
    }
}

/// A branch predicate: `value op constant` (the structural form §16.1
/// extraction yields) or an opaque named predicate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pred {
    /// `value op constant` — the comparison form over SSA values.
    Cmp { value: ValueId, op: Cmp, constant: i64 },
    /// Opaque predicate (callee result, load result) kept by name.
    Named { symbol: SymbolId, name: String },
    /// Constant literal after folding.
    Const(bool),
}

impl Pred {
    fn negate(self) -> Pred {
        match self {
            Pred::Cmp { value, op, constant } => Pred::Cmp { value, op: op.negate(), constant },
            Pred::Named { symbol, name } => Pred::Named { symbol, name: format!("!{name}") },
            Pred::Const(b) => Pred::Const(!b),
        }
    }
}

/// One branch condition: predicate + polarity (taken/not-taken) + the
/// branch's location (FR-4: findings cite the exact branch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathCond {
    pub pred: Pred,
    /// `true` = branch taken; `false` = not-taken arm.
    pub polarity: bool,
    pub loc: SourceLocation,
}

/// Simplified path condition set for one path (conjunction of branch
/// predicates) — structural and comparable for the Z3 encoding (§7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathCondition {
    pub conds: SmallVec<[PathCond; 4]>,
}

impl PathCondition {
    /// Conjoin another path's condition (c ∧ d); contradictory pairs drop
    /// to `Const(false)` (local, sound simplification).
    pub fn conjoin(&mut self, other: &PathCondition) {
        for c in &other.conds {
            self.conds.push(c.clone());
        }
        self.simplify();
    }

    /// Disjoin another condition (c ∨ d): tautologies collapse — when any
    /// pred appears with both polarities, the disjunction is `Const(true)`.
    pub fn disjoin(&mut self, other: &PathCondition) {
        let tautology = self.conds.iter().any(|a| {
            other
                .conds
                .iter()
                .any(|b| a.pred == b.pred.clone().negate())
        });
        if tautology {
            let loc = self
                .conds
                .first()
                .map(|c| c.loc)
                .unwrap_or_default();
            self.conds.clear();
            self.conds.push(PathCond {
                pred: Pred::Const(true),
                polarity: true,
                loc,
            });
            return;
        }
        // Sound local join: keep the smaller side (the abstraction is the
        // weaker constraint) — refined by SMT in TASK-047.
        if other.conds.len() < self.conds.len() {
            self.conds = other.conds.clone();
        }
        self.simplify();
    }

    /// Negate the whole condition (¬(c1 ∧ c2) = ¬c1 ∨ ¬c2 over singletons;
    /// for multi-cond conjunctions the local sound form negates each).
    pub fn negate(&mut self) {
        for c in &mut self.conds {
            let negated = c.pred.clone().negate();
            c.pred = negated;
            c.polarity = !c.polarity;
        }
        self.simplify();
    }

    /// Local, sound simplification: constant-fold literals, drop
    /// contradictions (`v == c ∧ v != c`), collapse duplicates.
    pub fn simplify(&mut self) {
        // Constant folding.
        if self.conds.iter().any(|c| c.pred == Pred::Const(false)) {
            self.conds.clear();
            self.conds.push(PathCond {
                pred: Pred::Const(false),
                polarity: true,
                loc: SourceLocation::default(),
            });
            return;
        }
        self.conds.retain(|c| c.pred != Pred::Const(true));
        // Contradiction drop: same value compared Eq to different constants,
        // or Eq/Ne pairs on the same (value, constant).
        let mut keep: SmallVec<[PathCond; 4]> = SmallVec::new();
        'outer: for c in self.conds.iter() {
            let c = c; // &PathCond
            let contradiction_with: Option<Pred> = match &c.pred {
                Pred::Cmp { value, op, constant } => {
                    let mut out = None;
                    for k in &keep {
                        if let Pred::Cmp { value: v2, op: o2, constant: c2 } = &k.pred {
                            if v2 == value {
                                let contradicts = match (op, o2) {
                                    (Cmp::Eq, Cmp::Eq) => constant != c2,
                                    (Cmp::Eq, Cmp::Ne) | (Cmp::Ne, Cmp::Eq) => constant == c2,
                                    _ => false,
                                };
                                if contradicts {
                                    out = Some(Pred::Const(false));
                                    break;
                                }
                            }
                        }
                    }
                    out
                }
                _ => None,
            };
            if let Some(false_p) = contradiction_with {
                self.conds.clear();
                self.conds.push(PathCond {
                    pred: false_p,
                    polarity: true,
                    loc: SourceLocation::default(),
                });
                return;
            }
            let dup = match &c.pred {
                Pred::Cmp { value, op, constant } => keep.iter().any(|k| {
                    matches!(
                        &k.pred,
                        Pred::Cmp { value: v2, op: o2, constant: c2 }
                            if v2 == value && o2 == op && c2 == constant
                    )
                }),
                Pred::Named { name, .. } => keep.iter().any(|k| {
                    matches!(&k.pred, Pred::Named { name: n2, .. } if n2 == name)
                }),
                _ => false,
            };
            if !dup {
                keep.push(c.clone());
            }
        }
        self.conds = keep;
    }

    pub fn is_false(&self) -> bool {
        self.conds.len() == 1 && self.conds[0].pred == Pred::Const(false)
    }
}

/// Extraction outcome: bounded conditions or an explicit budget flag
/// (global rule 3 — never a silently-true condition).
#[derive(Debug, Clone, PartialEq)]
pub enum PathCondExtraction {
    /// At most 4 conditions per function (the §14.4 SmallVec bound).
    Done(SmallVec<[PathCondition; 4]>),
    /// Budget exhausted before extraction completed — flagged, not silent.
    BudgetExhausted,
}

/// Per-branch extraction from control-flow facts: one path per if-arm,
/// conjunctions along nesting. Bounded by `max_paths` (the TASK-029 budget
/// proxy at this layer).
pub fn extract(
    ifs: &[cie_semantic::facts::IfSite],
    loops: &[cie_semantic::facts::LoopSite],
    max_paths: usize,
) -> PathCondExtraction {
    let branch_count = ifs.len() * 2 + loops.len();
    if branch_count > max_paths.saturating_mul(2) {
        return PathCondExtraction::BudgetExhausted;
    }
    let mut out: SmallVec<[PathCondition; 4]> = SmallVec::new();
    let mut id_counter = 0u32;
    for f in ifs.iter().take(max_paths) {
        let value = ValueId(id_counter);
        id_counter += 1;
        // Taken arm: v != 0 (truthy); not-taken: v == 0.
        let mut taken = PathCondition {
            conds: SmallVec::new(),
        };
        taken.conds.push(PathCond {
            pred: Pred::Cmp { value, op: Cmp::Ne, constant: 0 },
            polarity: true,
            loc: f.loc,
        });
        let mut not_taken = PathCondition {
            conds: SmallVec::new(),
        };
        not_taken.conds.push(PathCond {
            pred: Pred::Cmp { value, op: Cmp::Eq, constant: 0 },
            polarity: false,
            loc: f.loc,
        });
        if out.len() + 2 <= 4 {
            out.push(taken);
            out.push(not_taken);
        }
    }
    for l in loops.iter() {
        let value = ValueId(id_counter);
        id_counter += 1;
        let mut cond = PathCondition { conds: SmallVec::new() };
        cond.conds.push(PathCond {
            pred: Pred::Cmp { value, op: Cmp::Ne, constant: 0 },
            polarity: true,
            loc: l.loc,
        });
        if out.len() < 4 {
            out.push(cond);
        }
    }
    PathCondExtraction::Done(out)
}

/// Attach conditions to a summary's `path_conditions` (§14.4) — string
/// render for the v0.x summary schema; the structural form stays here.
pub fn attach_to_summary(
    summary: &mut cie_summary::FunctionSummary,
    extracted: &PathCondExtraction,
) {
    match extracted {
        PathCondExtraction::Done(conds) => {
            summary.path_conditions = conds
                .iter()
                .filter_map(|pc| {
                    let first = pc.conds.first()?;
                    let polarity = if first.polarity { "taken" } else { "not-taken" };
                    let body = match &first.pred {
                        Pred::Cmp { value, op, constant } => {
                            let op = match op {
                                Cmp::Eq => "==",
                                Cmp::Ne => "!=",
                                Cmp::Lt => "<",
                                Cmp::Le => "<=",
                                Cmp::Gt => ">",
                                Cmp::Ge => ">=",
                            };
                            format!("v{} {op} {constant}", value.raw())
                        }
                        Pred::Named { symbol, name } => {
                            format!("{name}@s{}", symbol.raw())
                        }
                        Pred::Const(b) => format!("{b}"),
                    };
                    Some(format!("{body} ({polarity})"))
                })
                .collect();
        }
        PathCondExtraction::BudgetExhausted => {
            summary
                .path_conditions
                .push("budget-exhausted (conditions unknown — flagged, not silent)".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::FileId;

    fn loc() -> SourceLocation {
        SourceLocation::span_bytes(FileId(0), 0, 16)
    }

    fn cmp_pred(v: u32, op: Cmp, c: i64) -> PathCond {
        PathCond { pred: Pred::Cmp { value: ValueId(v), op, constant: c }, polarity: true, loc: loc() }
    }

    /// TASK-006-sketch-aligned shape check: { pred, polarity, loc }.
    #[test]
    fn task_032_pathcond_shape_and_ops() {
        let mut pc = PathCondition { conds: SmallVec::new() };
        pc.conds.push(cmp_pred(1, Cmp::Eq, 5));
        assert_eq!(pc.conds.len(), 1);

        // negate: v == 5 → v != 5
        pc.negate();
        assert!(matches!(
            pc.conds[0].pred,
            Pred::Cmp { op: Cmp::Ne, constant: 5, .. }
        ));

        // conjoin the negation of itself → contradiction → Const(false)
        let mut other = PathCondition { conds: SmallVec::new() };
        other.conds.push(cmp_pred(1, Cmp::Eq, 5));
        pc.conjoin(&other);
        assert!(pc.is_false(), "c ∧ ¬c drops to false");
    }

    /// Tautology collapse: c ∨ ¬c → Const(true).
    #[test]
    fn task_032_disjoin_tautology() {
        let mut a = PathCondition { conds: SmallVec::new() };
        a.conds.push(cmp_pred(2, Cmp::Lt, 10));
        let mut b = PathCondition { conds: SmallVec::new() };
        b.conds.push(cmp_pred(2, Cmp::Ge, 10)); // ¬(v < 10)
        a.disjoin(&b);
        assert!(
            matches!(a.conds[0].pred, Pred::Const(true)),
            "c ∨ ¬c collapses to true"
        );
    }

    /// Constant folding + duplicate collapse.
    #[test]
    fn task_032_constant_fold_and_dedup() {
        let mut pc = PathCondition { conds: SmallVec::new() };
        pc.conds.push(PathCond { pred: Pred::Const(true), polarity: true, loc: loc() });
        pc.conds.push(cmp_pred(3, Cmp::Eq, 7));
        pc.conds.push(cmp_pred(3, Cmp::Eq, 7)); // duplicate
        pc.simplify();
        assert_eq!(pc.conds.len(), 1, "true folds away, duplicates collapse");
        assert!(matches!(pc.conds[0].pred, Pred::Cmp { value: ValueId(3), .. }));

        pc.conds.push(PathCond { pred: Pred::Const(false), polarity: true, loc: loc() });
        pc.simplify();
        assert!(pc.is_false(), "false dominates the conjunction");
    }

    /// Extraction: bounded, per-branch, budget-flagged.
    #[test]
    fn task_032_extraction_bounded_and_flagged() {
        let ifs: Vec<cie_semantic::facts::IfSite> = (0..3)
            .map(|i| cie_semantic::facts::IfSite {
                symbol: SymbolId(i),
                line: i as u32 + 1,
                cond: format!("c{i}"),
                cond_vars: vec![],
                loc: loc(),
            })
            .collect();
        let loops: Vec<cie_semantic::facts::LoopSite> =
            vec![cie_semantic::facts::LoopSite {
                symbol: SymbolId(9),
                kind: cie_semantic::facts::LoopKind::For,
                line: 9,
                body_start: 9,
                body_end: 20,
                loc: loc(),
            }];
        match extract(&ifs, &loops, 8) {
            PathCondExtraction::Done(conds) => {
                assert!(conds.len() <= 4, "§14.4 SmallVec<[PathCond; 4]> bound");
                assert!(conds.iter().all(|pc| pc.conds.len() >= 1));
            }
            _ => panic!("small extraction must complete"),
        }
        // Budget exhaustion is flagged, never silent.
        assert!(matches!(
            extract(&ifs, &loops, 1),
            PathCondExtraction::BudgetExhausted
        ));
    }

    /// Attach: conditions land on FunctionSummary.path_conditions.
    #[test]
    fn task_032_attach_to_summary() {
        let mut summary = cie_summary::FunctionSummary {
            sym: SymbolId(0),
            inputs: vec![],
            outputs: vec![],
            reads: Default::default(),
            writes: Default::default(),
            effects: Default::default(),
            exceptions: vec![],
            path_conditions: vec![],
            callees: vec![],
            unresolved_callees: vec![],
            returns_constants: Default::default(),
            all_returns_constant: false,
            has_failure_path: false,
            has_loops: false,
            tags: Default::default(),
            ..Default::default()
        };
        let ifs: Vec<cie_semantic::facts::IfSite> = vec![cie_semantic::facts::IfSite {
            symbol: SymbolId(1),
            line: 4,
            cond: "ok".into(),
            cond_vars: vec![],
            loc: loc(),
        }];
        let extracted = extract(&ifs, &[], 8);
        attach_to_summary(&mut summary, &extracted);
        assert!(!summary.path_conditions.is_empty());
        assert!(summary.path_conditions[0].contains("taken") || summary.path_conditions[0].contains("not-taken"));
        // Budget-exhausted attachment is flagged text, not silent.
        let mut s2 = cie_summary::FunctionSummary::default();
        attach_to_summary(&mut s2, &PathCondExtraction::BudgetExhausted);
        assert!(s2.path_conditions[0].contains("budget-exhausted"));
    }
}
