//! cie-counterexample — the counterexample pipeline (Phase 6):
//! budgeted SMT encoding (TASK-047), CEGAR refinement (TASK-048),
//! SLAM-style protocol checks (TASK-049), symbolic execution with
//! reproduction-test emission (TASK-050) and DerivedRecord packaging
//! (TASK-051).
//!
//! Landed so far: `smt` (TASK-043 substrate / TASK-047 foundation) and
//! `bounded` (TASK-043 bounded predicate checking).

pub mod bounded;
pub mod smt;

pub use bounded::{
    BoundedChecker, Conclusion, Counterexample, ExtractedPredicate, PathAtomSet, Status,
    Verdict,
};
pub use smt::{Budget, BudgetedSolver, CheckStatus, Solve, Term};
