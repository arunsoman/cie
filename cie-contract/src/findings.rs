//! Findings — the shared result protocol for conformance and security
//! analyses (PRD §22–24, §31).

use crate::Authority;
use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FindingKind {
    PrintOnlyAuthorization,
    UnconditionalSuccess,
    IgnoredRejection,
    SwallowedException,
    ExceptionToSuccess,
    AuthzBypass,
    HardcodedSecret,
    SqlInjection,
    CommandInjection,
    CodeInjection,
    TaintFlow,
    NPlusOneQuery,
    DeadCode,
    OrphanCode,
    LayeringViolation,
    CoverageGap,
    UnresolvedExternal,
    /// TASK-054 recovery-lite: an error-producing call's value is assigned
    /// to a throwaway (`_`, `_var`) — the error path is silently ignored.
    IgnoredErrorValue,
}

impl FindingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingKind::PrintOnlyAuthorization => "print_only_authorization",
            FindingKind::UnconditionalSuccess => "unconditional_success",
            FindingKind::IgnoredRejection => "ignored_rejection",
            FindingKind::SwallowedException => "swallowed_exception",
            FindingKind::ExceptionToSuccess => "exception_to_success",
            FindingKind::AuthzBypass => "authz_bypass",
            FindingKind::HardcodedSecret => "hardcoded_secret",
            FindingKind::SqlInjection => "sql_injection",
            FindingKind::CommandInjection => "command_injection",
            FindingKind::CodeInjection => "code_injection",
            FindingKind::TaintFlow => "taint_flow",
            FindingKind::NPlusOneQuery => "n_plus_one_query",
            FindingKind::DeadCode => "dead_code",
            FindingKind::OrphanCode => "orphan_code",
            FindingKind::LayeringViolation => "layering_violation",
            FindingKind::CoverageGap => "coverage_gap",
            FindingKind::UnresolvedExternal => "unresolved_external",
            FindingKind::IgnoredErrorValue => "ignored_error_value",
        }
    }

    pub fn default_severity(&self) -> Severity {
        match self {
            FindingKind::PrintOnlyAuthorization
            | FindingKind::SqlInjection
            | FindingKind::CommandInjection
            | FindingKind::CodeInjection
            | FindingKind::HardcodedSecret => Severity::Critical,
            FindingKind::IgnoredRejection
            | FindingKind::ExceptionToSuccess
            | FindingKind::AuthzBypass
            | FindingKind::TaintFlow => Severity::High,
            FindingKind::UnconditionalSuccess
            | FindingKind::SwallowedException
            | FindingKind::NPlusOneQuery
            | FindingKind::LayeringViolation => Severity::Medium,
            FindingKind::DeadCode | FindingKind::OrphanCode | FindingKind::CoverageGap => {
                Severity::Low
            }
            FindingKind::UnresolvedExternal => Severity::Info,
            // Recovery-lite (TASK-054): ignored error values are robustness
            // hypotheses until a checker proves the error path matters.
            FindingKind::IgnoredErrorValue => Severity::Info,
        }
    }
}

/// A concrete finding with evidence lines, remediation and authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable-ish id: `F-{rev}-{seq}`.
    pub id: String,
    pub kind: FindingKind,
    pub severity: Severity,
    pub symbol: SymbolId,
    pub symbol_name: String,
    pub message: String,
    pub evidence_lines: Vec<u32>,
    pub remediation: String,
    pub authority: Authority,
    /// Rule id that produced this finding, e.g. `conf/PR-001`.
    pub rule: String,
    pub tags: Vec<String>,
}
