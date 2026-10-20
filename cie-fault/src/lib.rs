//! cie-fault — recovery obligations support (TASK-052 lives in
//! `cie-contract::obligations`), lineage-driven fault injection planning
//! (TASK-053, `ldfi`) and safety/liveness-ranked error-handling failures
//! (TASK-054, `error_handling`).

pub mod error_handling;
pub mod ldfi;
pub mod next_steps;
pub mod trace;

pub use error_handling::{
    DetectorKind, ErrorHandlingDetectors, ErrorHandlingFinding, ModelProjection,
    ModelTransitionRef, ThreatClass,
};
pub use ldfi::{
    ExperimentOutcome, ExperimentResult, Fault, LdfiError, LdfiPlanner, LdfiUnknown,
    LineageEdge, LineageGraph, LineageKind, Property, RoundLimits, RoundResults, RuntimeFaultInjector,
    SimInjector, SimRun, TimedOutReason,
};
pub use trace::{
    parse_trace, suspects_from_trace, ExceptionsInbox, InboxEntry, InboxState, Suspect,
    TraceFrame, TraceFrames, TraceParseError,
};
pub use next_steps::{next_steps, EvidenceState, NextStep};

pub mod lineage_from_state_model;
