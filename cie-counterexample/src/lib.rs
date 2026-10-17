//! cie-counterexample — the counterexample pipeline (Phase 6):
//! budgeted SMT encoding (TASK-047), CEGAR refinement (TASK-048),
//! SLAM-style protocol checks (TASK-049), symbolic execution with
//! reproduction-test emission (TASK-050) and DerivedRecord packaging
//! (TASK-051).
//!
//! Landed so far: `smt` (TASK-043 substrate / TASK-047 foundation) and
//! `bounded` (TASK-043 bounded predicate checking).

pub mod bounded;
pub mod cegar;
pub mod slam;
pub mod symbolic;
pub mod intsmt;
pub mod package;
pub mod smt;

pub use symbolic::{concretize, ExecLimits, GeneratedTest, PathTermination, Step, SummaryProgram, SymbolicExecutor, SymbolicPath, VIOLATION_DOUBLE_CHARGE};
pub use slam::{CallEdge, Csr, MachEdge, ProtocolMachine, ProtocolMachineBuilder, RuleOutcome, RuleUnknown, TraceHop, check_rule, check_rule_budgeted, trace_evidence};
pub use cegar::{AbstractDomain, AbstractPred, AbstractVal, CegarLoop, CegarMetrics, CegarOutcome, CfaProgram, PredAbstractDomain, RefineLimits, RefinementExhausted};
pub use bounded::{
    BoundedChecker, Conclusion, Counterexample, ExtractedPredicate, PathAtomSet, Status,
    Verdict,
};
pub use package::{package, package_symbolic, shrink, violation_record, PackagedCounterexample, RawCex};
pub use intsmt::{Constrained, ICmp, IntTerm, IntVar, SmtEncoder, SmtModel, SmtOutcome, SmtUnknownReason};
pub use smt::{Budget, BudgetedSolver, CheckStatus, Solve, Term};
