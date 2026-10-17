//! Error-handling failure detectors (TASK-054, §7 Yuan et al. OSDI 2014
//! row — `cie-fault::error_handling`).
//!
//! Static detectors targeting the error-handling defects that dominate
//! real outages: swallowed exceptions, empty handlers, log-and-continue,
//! misleading success, and early returns that break retry loops. Findings
//! are classified by FR-15 threat class (Safety vs Liveness — distinct,
//! never conflated) and prioritized accordingly.
//!
//! Rules honored:
//! - Findings carry an explicit `ThreatClass::Safety | Liveness`; a
//!   swallowed refund failure is a POTENTIAL INEFFECTIVE REFUND, never
//!   "recovery handled" (global rule 3 spirit, FR-15 note).
//! - "Misleading success" requires two-sided evidence — error path taken
//!   AND success value returned — never naming alone (global rule 2).
//! - Detector output is `Authority::InferredCandidate`; detectors never
//!   upgrade contract authority (FR-28, global rule 4).
//! - Exception semantics are per-language via adapter-provided
//!   `ExceptionSite`s (Go error values, C errno, Rust `Result`); no
//!   cross-language rule reuse (FR-3, global rule 6).


use cie_contract::Authority;
use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_semantic::facts::LangFacts;
use serde::{Deserialize, Serialize};

/// Detector catalogue (Yuan et al.-derived).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DetectorKind {
    SwallowedException,
    EmptyHandler,
    LogAndContinue,
    MisleadingSuccess,
    EarlyReturnInRetryLoop,
}

impl DetectorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectorKind::SwallowedException => "swallowed_exception",
            DetectorKind::EmptyHandler => "empty_handler",
            DetectorKind::LogAndContinue => "log_and_continue",
            DetectorKind::MisleadingSuccess => "misleading_success",
            DetectorKind::EarlyReturnInRetryLoop => "early_return_in_retry_loop",
        }
    }
}

/// FR-15 threat class — Safety or Liveness, explicitly distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatClass {
    Safety,
    Liveness,
}

/// Severity rank 1 (highest) .. 5.
pub type Severity = u8;

/// One detector finding, evidence-ready for TASK-051 packaging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorHandlingFinding {
    pub detector: DetectorKind,
    pub subject: SymbolId,
    pub classification: ThreatClass,
    pub severity: Severity,
    /// SourceLocation chain (sketch: `evidence.locs` — carried on the
    /// finding; the Evidence provenance cites them textually too).
    pub locs: Vec<SourceLocation>,
    pub authority: Authority,
    pub evidence: Evidence,
}

impl ErrorHandlingFinding {
    pub fn about(&self) -> SymbolId {
        self.subject
    }
}

/// Detector input: per-language facts (adapter-provided exception
/// semantics) + the TASK-052 obligation model (for safety/liveness
/// prioritization: findings touching recovery obligations rank first).
pub struct ErrorHandlingDetectors<'a> {
    facts: &'a [LangFacts],
    refund_symbols: Vec<SymbolId>,
    debit_symbols: Vec<SymbolId>,
    rev: RevId,
}

/// Projection of the TASK-052 obligation model used for prioritization:
/// the model's transitions (event + provenance symbols), decoupled from
/// cie-contract's full StateModel to keep this module's dependency
/// surface small.
pub struct ModelProjection {
    pub transitions: Vec<ModelTransitionRef>,
}

/// One projected transition.
#[derive(Debug, Clone)]
pub struct ModelTransitionRef {
    pub event: String,
    pub provenance_symbols: Vec<SymbolId>,
}

impl<'a> ErrorHandlingDetectors<'a> {
    /// Build from facts + the 052 model projection (refund/debit
    /// participants come from the model's transitions so prioritization is
    /// obligation-aware).
    pub fn new(
        facts: &'a [LangFacts],
        model: &ModelProjection,
        rev: RevId,
    ) -> Self {
        let mut refund_symbols = vec![];
        let mut debit_symbols = vec![];
        for t in &model.transitions {
            if t.event == "refund_called" || t.event == "refund_effective" {
                refund_symbols.extend(t.provenance_symbols.iter().copied());
            }
            if t.event == "debit" || t.event == "charge" {
                debit_symbols.extend(t.provenance_symbols.iter().copied());
            }
        }
        refund_symbols.sort();
        refund_symbols.dedup();
        debit_symbols.sort();
        debit_symbols.dedup();
        ErrorHandlingDetectors { facts, refund_symbols, debit_symbols, rev }
    }

    /// Run every detector; findings ranked by (threat impact, severity).
    pub fn run(&self) -> Vec<ErrorHandlingFinding> {
        let mut out = vec![];
        for f in self.facts {
            self.detect_in_file(f, &mut out);
        }
        out.sort_by(|a, b| {
            (threat_rank(a), a.severity).cmp(&(threat_rank(b), b.severity))
        });
        out
    }

    fn detect_in_file(&self, f: &LangFacts, out: &mut Vec<ErrorHandlingFinding>) {
        // symbol name lookup for obligation-aware classification
        let names: Vec<(SymbolId, String)> =
            f.symbols.iter().map(|s| (s.id, s.name.to_ascii_lowercase())).collect();
        let name_of = |id: SymbolId| -> Option<String> {
            names.iter().find(|(i, _)| *i == id).map(|(_, n)| n.clone())
        };
        let loc_of = |id: SymbolId| -> Vec<SourceLocation> {
            f.symbols
                .iter()
                .filter(|s| s.id == id)
                .map(|s| s.loc)
                .collect()
        };
        let is_recovery_flavored = |id: SymbolId| -> bool {
            let n = name_of(id).unwrap_or_default();
            self.refund_symbols.contains(&id)
                || n.contains("refund")
                || self.debit_symbols.contains(&id)
                || n.contains("debit")
                || n.contains("charge")
        };

        // ---- exception-site detectors (per-language ExceptionSites) ----
        for ex in &f.exceptions {
            let subject = ex.symbol;
            let rec_flavored = is_recovery_flavored(subject);
            let locs = loc_of(subject);

            // EmptyHandler: handler body spans zero lines
            if ex.handler_end > 0 && ex.handler_end <= ex.line + 1 && !ex.is_bare {
                out.push(self.finding(
                    DetectorKind::EmptyHandler,
                    subject,
                    if rec_flavored { ThreatClass::Safety } else { ThreatClass::Liveness },
                    if rec_flavored { 1 } else { 3 },
                    locs.clone(),
                    format!(
                        "empty handler at line {} swallows the failure silently",
                        ex.line
                    ),
                ));
                continue;
            }
            // SwallowedException: caught inside a handler, never re-raised,
            // and the handler is not bare-pass (two-sided: in_handler true
            // means a raise happened INSIDE the handler — i.e. it was
            // caught and re-raised, which is NOT a swallow; a catch with
            // no raise inside and no propagation is).
            if !ex.in_handler {
                // handler records: does anything after it re-raise? The
                // adapter marks re-raises with in_handler=true raises; if
                // none exists for this handler span, the error died here.
                let re_raised = f.exceptions.iter().any(|other| {
                    other.in_handler
                        && other.symbol == subject
                        && other.line >= ex.line
                        && other.line <= ex.handler_end.max(ex.line)
                });
                if !re_raised && !ex.is_bare {
                    let class = if rec_flavored { ThreatClass::Safety } else { ThreatClass::Liveness };
                    out.push(self.finding(
                        DetectorKind::SwallowedException,
                        subject,
                        class,
                        if rec_flavored { 1 } else { 3 },
                        locs.clone(),
                        format!(
                            "exception caught at line {} is never re-raised or propagated",
                            ex.line
                        ),
                    ));
                }
                // LogAndContinue: bare handler = catch-everything then move on
                if ex.is_bare {
                    out.push(self.finding(
                        DetectorKind::LogAndContinue,
                        subject,
                        if rec_flavored { ThreatClass::Safety } else { ThreatClass::Liveness },
                        2,
                        locs.clone(),
                        format!(
                            "bare handler at line {} catches everything and continues",
                            ex.line
                        ),
                    ));
                }
            }
        }

        // ---- MisleadingSuccess: two-sided evidence (global rule 2) ----
        for s in &f.symbols {
            if !s.is_function_like() {
                continue;
            }
            let error_path_taken = f.exceptions.iter().any(|e| e.symbol == s.id);
            let success_returned = f.returns.iter().any(|r| {
                r.symbol == s.id
                    && r.constant
                        .as_deref()
                        .map(|c| matches!(c, "true" | "num" | "str" | "empty"))
                        .unwrap_or(false)
            });
            if error_path_taken && success_returned {
                out.push(self.finding(
                    DetectorKind::MisleadingSuccess,
                    s.id,
                    if is_recovery_flavored(s.id) {
                        ThreatClass::Safety
                    } else {
                        ThreatClass::Liveness
                    },
                    2,
                    loc_of(s.id),
                    format!(
                        "error path taken AND success value returned (two-sided evidence) in {}",
                        s.name
                    ),
                ));
            }
        }

        // ---- EarlyReturnInRetryLoop: loop + immediate success return ----
        for lp in &f.loops {
            let subject = lp.symbol;
            if !is_retry_flavored(&name_of(subject).unwrap_or_default()) {
                continue;
            }
            let early_return = f
                .returns
                .iter()
                .any(|r| r.symbol == subject && r.line >= lp.body_start && r.line <= lp.body_end);
            if early_return {
                out.push(self.finding(
                    DetectorKind::EarlyReturnInRetryLoop,
                    subject,
                    ThreatClass::Liveness,
                    1,
                    loc_of(subject),
                    format!(
                        "early return inside retry loop (lines {}-{}) interrupts the recovery schedule",
                        lp.body_start, lp.body_end
                    ),
                ));
            }
        }
    }

    fn finding(
        &self,
        detector: DetectorKind,
        subject: SymbolId,
        classification: ThreatClass,
        severity: Severity,
        locs: Vec<SourceLocation>,
        detail: String,
    ) -> ErrorHandlingFinding {
        ErrorHandlingFinding {
            detector,
            subject,
            classification,
            severity,
            locs: locs.clone(),
            authority: Authority::InferredCandidate,
            evidence: Evidence {
                rev: self.rev,
                provenance: format!("{}: {}", detector.as_str(), detail),
                assumptions: vec![
                    "yuan-et-al static detector; confirm dynamically via TASK-053 injection".into(),
                ],
                unsupported: vec![],
                unresolved: vec![],
                generated_at_ns: 0,
            },
        }
    }
}

fn threat_rank(f: &ErrorHandlingFinding) -> u8 {
    match (f.classification, f.severity) {
        (ThreatClass::Safety, s) => s,
        (ThreatClass::Liveness, s) => s + 1,
    }
}

fn is_retry_flavored(name: &str) -> bool {
    name.contains("retry") || name.contains("scheduler") || name.contains("attempt")
}
