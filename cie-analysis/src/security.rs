//! Security slice (CAP-2, v1 scope):
//! - SEC-1 hardcoded secrets (CAP-2.3)
//! - SEC-2 intra-procedural taint-lite source→sink (CAP-2.1/2.2, acceptance #42)
//! - PERF-1 N+1 queries in loops (CAP-4.2, acceptance #43)
//!
//! Evidence carries the limitation explicitly: taint-lite is
//! intra-procedural without heap modeling (PRD §74) — results are
//! InferredCandidate findings, never verified proofs.

use crate::findings::{Finding, FindingKind};
use cie_core::types::{RevId, SymbolId};
use cie_semantic::facts::{BuiltinKind, Callee, LangFacts, RhsKind, SinkKind, SymbolStore};
use regex::Regex;
use std::collections::BTreeMap;

pub fn run(store: &SymbolStore, per_file: &[LangFacts], rev: RevId) -> Vec<Finding> {
    let mut findings: Vec<Finding> = vec![];
    let mut seq = 0u32;

    // ---- SEC-1 hardcoded secrets (CAP-2.3) ----
    let secret_patterns: Vec<Regex> = [
        r#"AKIA[0-9A-Z]{16}"#,
        r#"ghp_[A-Za-z0-9]{30,}"#,
        r#"sk_live_[A-Za-z0-9]{10,}"#,
        r#"(?i)\b(api[_-]?key|secret|passwd|password|pwd|token|access[_-]?key)\b\s*['\"]?\s*[:=]\s*['\"][A-Za-z0-9_\-/+=.]{12,}['\"]"#,
        r#"-----BEGIN [A-Z ]*PRIVATE KEY-----"#,
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect();

    for facts in per_file {
        for lit in &facts.literals {
            if !lit.is_string {
                continue;
            }
            let v = lit.value.to_ascii_lowercase();
            if v.contains("xxx")
                || v.contains("changeme")
                || v.contains("example")
                || v.contains("placeholder")
            {
                continue;
            }
            if let Some(_m) = secret_patterns.iter().find(|re| re.is_match(&lit.value)) {
                let Some(sym) = store.get(lit.symbol) else {
                    continue;
                };
                seq += 1;
                findings.push(Finding {
                    id: format!("F-{rev}-S{seq:04}"),
                    kind: FindingKind::HardcodedSecret,
                    severity: FindingKind::HardcodedSecret.default_severity(),
                    symbol: lit.symbol,
                    symbol_name: sym.qual_name.clone(),
                    message: format!(
                        "possible hardcoded secret in string literal (line {})",
                        lit.line
                    ),
                    evidence_lines: vec![lit.line],
                    remediation: "Move the secret to a secret manager / environment variable"
                        .into(),
                    authority: cie_contract::Authority::InferredCandidate,
                    rule: "sec/SEC-001".into(),
                    tags: vec!["security".into(), "secrets".into()],
                });
            }
        }
    }

    // ---- SEC-3 recovery-lite (TASK-054 subset): ignored error values ----
    // A call whose value is assigned to a throwaway (`_`, `_var`) discards
    // the error path. Derived from existing AssignmentFact facts — no
    // obligation solver (that stays v0.3/v1.0).
    for facts in per_file {
        for a in &facts.assignments {
            if a.rhs_kind != RhsKind::Call {
                continue;
            }
            let throwaway = a.var == "_" || (a.var.starts_with('_') && a.var.len() > 1);
            if !throwaway {
                continue;
            }
            let Some(sym) = store.get(a.symbol) else {
                continue;
            };
            seq += 1;
            findings.push(Finding {
                id: format!("F-{rev}-S{seq:04}"),
                kind: FindingKind::IgnoredErrorValue,
                severity: FindingKind::IgnoredErrorValue.default_severity(),
                symbol: a.symbol,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "result of a call is assigned to throwaway '{}' (line {}) — error path ignored",
                    a.var, a.line
                ),
                evidence_lines: vec![a.line],
                remediation: "Match/unwrap the error explicitly, or document why ignoring is safe"
                    .into(),
                authority: cie_contract::Authority::InferredCandidate,
                rule: "sec/SEC-004".into(),
                tags: vec!["robustness".into(), "recovery".into()],
            });
        }
    }

    // SEC-3b: bare except whose handler cannot recover — an exception fact
    // with is_bare and NO re-raise inside the same enclosing function.
    for facts in per_file {
        let re_raises: std::collections::BTreeSet<SymbolId> = facts
            .exceptions
            .iter()
            .filter(|x| x.in_handler)
            .map(|x| x.symbol)
            .collect();
        for x in &facts.exceptions {
            if !x.is_bare || re_raises.contains(&x.symbol) {
                continue;
            }
            let Some(sym) = store.get(x.symbol) else {
                continue;
            };
            seq += 1;
            findings.push(Finding {
                id: format!("F-{rev}-S{seq:04}"),
                kind: FindingKind::SwallowedException,
                severity: FindingKind::SwallowedException.default_severity(),
                symbol: x.symbol,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "bare except in '{}' (line {}) has no re-raise — swallowed error has no recovery path",
                    sym.name, x.line
                ),
                evidence_lines: vec![x.line],
                remediation: "Catch typed errors and re-raise or convert; never swallow bare".into(),
                authority: cie_contract::Authority::InferredCandidate,
                rule: "sec/SEC-005".into(),
                tags: vec!["robustness".into(), "recovery".into()],
            });
        }
    }

    // ---- SEC-2 taint-lite (CAP-2.1/2.2) ----
    for facts in per_file {
        // Group per enclosing function.
        let mut tainted: BTreeMap<SymbolId, std::collections::BTreeSet<String>> = BTreeMap::new();
        for src in &facts.sources {
            tainted
                .entry(src.symbol)
                .or_default()
                .insert(src.var.clone());
        }
        // Propagate through assignments (bounded rounds).
        for _ in 0..8 {
            let mut changed = false;
            for a in &facts.assignments {
                let Some(t) = tainted.get(&a.symbol) else {
                    continue;
                };
                if t.contains(&a.var) {
                    continue;
                }
                let infected = match a.rhs_kind {
                    RhsKind::Vars | RhsKind::FString => a.rhs_data.iter().any(|v| t.contains(v)),
                    RhsKind::Input => true,
                    _ => false,
                };
                if infected {
                    tainted.entry(a.symbol).or_default().insert(a.var.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for sink in &facts.sinks {
            let Some(t) = tainted.get(&sink.symbol) else {
                continue;
            };
            let tainted_vars: Vec<String> = sink
                .arg_vars
                .iter()
                .filter(|v| t.contains(*v))
                .cloned()
                .collect();
            if tainted_vars.is_empty() {
                continue;
            }
            // Parameterized SQL sink is not injectable through the placeholder.
            if sink.kind == SinkKind::SqlExecute && sink.parameterized {
                continue;
            }
            let kind = match sink.kind {
                SinkKind::SqlExecute => FindingKind::SqlInjection,
                SinkKind::ShellExec => FindingKind::CommandInjection,
                SinkKind::CodeEval => FindingKind::CodeInjection,
                SinkKind::FileWrite | SinkKind::HtmlRender => FindingKind::TaintFlow,
            };
            let Some(sym) = store.get(sink.symbol) else {
                continue;
            };
            seq += 1;
            findings.push(Finding {
                id: format!("F-{rev}-S{seq:04}"),
                kind,
                severity: kind.default_severity(),
                symbol: sink.symbol,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "tainted input ({}) reaches {} sink '{}' at line {}; no sanitizer observed on this path",
                    tainted_vars.join(", "),
                    match sink.kind {
                        SinkKind::SqlExecute => "SQL execute",
                        SinkKind::ShellExec => "shell execute",
                        SinkKind::CodeEval => "code evaluation",
                        SinkKind::FileWrite => "file write",
                        SinkKind::HtmlRender => "HTML render",
                    },
                    sink.call_text,
                    sink.line
                ),
                evidence_lines: vec![sink.line],
                remediation: "Use parameterized queries / safe APIs; validate and encode the input".into(),
                authority: cie_contract::Authority::InferredCandidate,
                rule: "sec/SEC-002".into(),
                tags: vec!["security".into(), "taint".into()],
            });
        }

        // ---- PERF-1 N+1 queries in loops (CAP-4.2) ----
        for call in &facts.calls {
            if !call.in_loop {
                continue;
            }
            let is_query = match &call.callee {
                Callee::Builtin(b) => {
                    matches!(b, BuiltinKind::DbQuery | BuiltinKind::DbExecute)
                }
                Callee::Resolved(id) => store
                    .get(*id)
                    .map(|s| {
                        let n = s.name.to_ascii_lowercase();
                        n.contains("query")
                            || n.contains("fetch")
                            || n.contains("find")
                            || n.contains("select")
                            || n.contains("get_")
                    })
                    .unwrap_or(false),
                _ => false,
            };
            if !is_query {
                continue;
            }
            let Some(sym) = store.get(call.caller) else {
                continue;
            };
            seq += 1;
            findings.push(Finding {
                id: format!("F-{rev}-P{seq:04}"),
                kind: FindingKind::NPlusOneQuery,
                severity: FindingKind::NPlusOneQuery.default_severity(),
                symbol: call.caller,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "query call '{}' inside a loop body (line {}) — N+1 pattern",
                    call.callee_text, call.line
                ),
                evidence_lines: vec![call.line],
                remediation: "Batch the query outside the loop or use a bulk API".into(),
                authority: cie_contract::Authority::InferredCandidate,
                rule: "perf/PF-001".into(),
                tags: vec!["performance".into()],
            });
        }
    }

    findings.sort_by(|a, b| (&a.kind, &a.symbol).cmp(&(&b.kind, &b.symbol)));
    findings
}
