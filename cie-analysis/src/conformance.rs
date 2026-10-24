//! Behavioral conformance (PRD §23 / FR-12) + architecture rules (CAP-1).
//!
//! Distinguishes established violations from inferred suspicions via
//! `Authority`. Implements foundation acceptance #1–#5:
//! print-only authorization, unconditional success, legitimate delegation
//! not flagged, ignored rejection, exceptions converted to success.

use crate::behavior::{is_auth_name, is_destructive_name, is_money_name};
use crate::findings::{Finding, FindingKind};
use cie_contract::{Authority, Contract, ContractSource, ObligationKind};
use cie_core::types::{RevId, SymbolId};
use cie_semantic::facts::{BuiltinKind, Callee, LangFacts, RhsKind, SymbolStore};
use cie_summary::EffectKind;
use std::collections::BTreeMap;

pub struct ConformanceResult {
    pub findings: Vec<Finding>,
    pub contracts: Vec<Contract>,
}

pub fn run(
    store: &SymbolStore,
    per_file: &[LangFacts],
    summaries: &BTreeMap<SymbolId, cie_summary::FunctionSummary>,
    rev: RevId,
) -> ConformanceResult {
    let mut findings: Vec<Finding> = vec![];
    let mut contracts: Vec<Contract> = vec![];
    let mut seq = 0u32;
    let mut emit = |f: &mut Vec<Finding>, finding: Finding| {
        seq += 1;
        let mut finding = finding;
        finding.id = format!("F-{rev}-{seq:04}");
        f.push(finding);
    };

    // ---- PR-001 Print-only authorization (acceptance #1) ----
    let mut print_only_syms: Vec<SymbolId> = vec![];
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() {
            continue;
        }
        let Some(s) = summaries.get(&sym.id) else {
            continue;
        };
        if !is_auth_name(&sym.name) {
            continue;
        }
        // Legitimate delegation check (acceptance #3): any resolved callee
        // with a verification-flavored name or security-relevant effect.
        let delegates_to_verification = s.callees.iter().any(|c| {
            summaries
                .get(c)
                .map(|cs| {
                    store
                        .get(*c)
                        .map(|sym| {
                            let n = sym.name.to_ascii_lowercase();
                            n.contains("verify")
                                || n.contains("check")
                                || n.contains("lookup")
                                || n.contains("token")
                                || n.contains("permission")
                                || n.contains("hash")
                                || n.contains("secret")
                                || n.contains("db")
                                || n.contains("query")
                                || n.contains("validate")
                        })
                        .unwrap_or(false)
                        || cs.effects.iter().any(|e| e.security_relevant())
                })
                .unwrap_or(false)
        });
        let success_only = !s.returns_constants.is_empty()
            && s.returns_constants
                .iter()
                .all(|c| matches!(c.as_str(), "true" | "num" | "str" | "empty"));
        let print_only = s.effects.iter().all(|e| {
            matches!(
                e,
                EffectKind::Print | EffectKind::Log | EffectKind::Time | EffectKind::Assert
            )
        }) && !s.effects.is_empty();

        if delegates_to_verification || !success_only || !print_only {
            continue;
        }
        print_only_syms.push(sym.id);
        emit(
            &mut findings,
            Finding {
                introduced: None,
                id: String::new(),
                kind: FindingKind::PrintOnlyAuthorization,
                severity: FindingKind::PrintOnlyAuthorization.default_severity(),
                symbol: sym.id,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "'{}' is named like an authorization check, has no verification call, prints/logs only, and returns success on every path",
                    sym.name
                ),
                evidence_lines: vec![store.files.line(sym.loc)],
                remediation: "Call a real verification primitive (credential lookup, permission check) and propagate its result".into(),
                authority: Authority::InferredCandidate,
                rule: "conf/PR-001".into(),
                tags: vec!["security".into(), "authz".into()],
            },
        );
    }

    // ---- PR-002 Unconditional success (acceptance #2) ----
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() {
            continue;
        }
        if print_only_syms.contains(&sym.id) {
            continue;
        }
        let Some(s) = summaries.get(&sym.id) else {
            continue;
        };
        let success_only = !s.returns_constants.is_empty()
            && s.returns_constants
                .iter()
                .all(|c| matches!(c.as_str(), "true" | "num" | "str" | "empty"));
        if !success_only || s.has_failure_path {
            continue;
        }
        // Decision-shaped: has branches but all outcomes succeed.
        let has_branches = per_file
            .iter()
            .any(|f| f.ifs.iter().any(|i| i.symbol == sym.id));
        let multi_return = s.returns_constants.len() > 1;
        if !has_branches && !multi_return {
            continue;
        }
        emit(
            &mut findings,
            Finding {
                introduced: None,
                id: String::new(),
                kind: FindingKind::UnconditionalSuccess,
                severity: FindingKind::UnconditionalSuccess.default_severity(),
                symbol: sym.id,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "'{}' has decision branches but every path returns a success value ({:?}) with no failure path",
                    sym.name,
                    s.returns_constants.iter().collect::<Vec<_>>()
                ),
                evidence_lines: vec![store.files.line(sym.loc)],
                remediation: "Return distinct outcomes for failure branches; let callers observe rejection".into(),
                authority: Authority::InferredCandidate,
                rule: "conf/PR-002".into(),
                tags: vec!["robustness".into()],
            },
        );
    }

    // ---- PR-003 Ignored rejection (acceptance #4) — python deep facts ----
    for facts in per_file {
        for a in &facts.assignments {
            if a.rhs_kind != RhsKind::Call {
                continue;
            }
            let Some(callee_name) = a.rhs_data.first() else {
                continue;
            };
            // Is the assigned call auth-flavored?
            let auth_flavored = is_auth_name(callee_name)
                || facts.calls.iter().any(|c| {
                    c.caller == a.symbol && c.line == a.line && is_auth_name(&c.callee_text)
                });
            if !auth_flavored {
                continue;
            }
            let used_in_condition = facts
                .ifs
                .iter()
                .any(|i| i.symbol == a.symbol && i.cond_vars.iter().any(|v| v == &a.var));
            if used_in_condition {
                continue;
            }
            // Sensitive operation after the (unchecked) call?
            let sensitive: Vec<(u32, String)> = facts
                .calls
                .iter()
                .filter(|c| {
                    c.caller == a.symbol
                        && c.line > a.line
                        && match &c.callee {
                            Callee::Builtin(b) => matches!(
                                b,
                                BuiltinKind::DbExecute
                                    | BuiltinKind::FileWrite
                                    | BuiltinKind::ProcessExec
                            ),
                            Callee::Resolved(id) => summaries
                                .get(id)
                                .map(|cs| {
                                    cs.tags.contains("money")
                                        || cs.tags.contains("destructive")
                                        || cs.tags.contains("mutation")
                                })
                                .unwrap_or(false),
                            _ => false,
                        }
                })
                .map(|c| (c.line, c.callee_text.clone()))
                .collect();
            if sensitive.is_empty() {
                continue;
            }
            let Some(sym) = store.get(a.symbol) else {
                continue;
            };
            let mut lines = vec![a.line];
            lines.extend(sensitive.iter().map(|(l, _)| *l));
            emit(
                &mut findings,
                Finding {
                introduced: None,
                    id: String::new(),
                    kind: FindingKind::IgnoredRejection,
                    severity: FindingKind::IgnoredRejection.default_severity(),
                    symbol: a.symbol,
                    symbol_name: sym.qual_name.clone(),
                    message: format!(
                        "result of authorization call '{}' (line {}) is bound to '{}' but never branched on before a sensitive operation ({})",
                        callee_name,
                        a.line,
                        a.var,
                        sensitive
                            .iter()
                            .map(|(l, t)| format!("'{}' @ {}", t, l))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    evidence_lines: lines,
                    remediation: "Check the authorization result and abort the sensitive operation on rejection".into(),
                    authority: Authority::InferredCandidate,
                    rule: "conf/PR-003".into(),
                    tags: vec!["security".into(), "authz".into()],
                },
            );
        }
    }

    // ---- PR-004/PR-005 Swallowed exceptions / exception-to-success (acceptance #5) ----
    for facts in per_file {
        for x in &facts.exceptions {
            if x.handler_end == 0 || x.in_handler {
                continue; // raise facts, or nested handlers
            }
            let is_broad = x.is_bare
                || x.kind_text.is_empty()
                || x.kind_text == "Exception"
                || x.kind_text == "BaseException";
            if !is_broad {
                continue;
            }
            let body_calls: Vec<_> = facts
                .calls
                .iter()
                .filter(|c| c.caller == x.symbol && c.line > x.line && c.line <= x.handler_end)
                .collect();
            let body_returns: Vec<_> = facts
                .returns
                .iter()
                .filter(|r| r.symbol == x.symbol && r.line > x.line && r.line <= x.handler_end)
                .collect();
            let body_raises = facts.exceptions.iter().any(|r| {
                r.in_handler && r.symbol == x.symbol && r.line > x.line && r.line <= x.handler_end
            });
            let only_quiet = body_calls.iter().all(|c| {
                matches!(
                    &c.callee,
                    Callee::Builtin(BuiltinKind::Print) | Callee::Builtin(BuiltinKind::Log)
                )
            });
            let returns_success = body_returns.iter().any(|r| {
                r.constant
                    .as_deref()
                    .is_some_and(|c| matches!(c, "true" | "num" | "str" | "empty"))
            });

            if body_raises {
                continue;
            }
            let Some(sym) = store.get(x.symbol) else {
                continue;
            };
            if returns_success {
                emit(
                    &mut findings,
                    Finding {
                introduced: None,
                        id: String::new(),
                        kind: FindingKind::ExceptionToSuccess,
                        severity: FindingKind::ExceptionToSuccess.default_severity(),
                        symbol: x.symbol,
                        symbol_name: sym.qual_name.clone(),
                        message: format!(
                            "{} exception handler in '{}' (line {}) returns a success value, converting failure into success",
                            if x.is_bare { "bare" } else { "broad" },
                            sym.name,
                            x.line
                        ),
                        evidence_lines: vec![x.line],
                        remediation: "Propagate or wrap the error instead of returning success".into(),
                        authority: Authority::InferredCandidate,
                        rule: "conf/PR-005".into(),
                        tags: vec!["robustness".into()],
                    },
                );
            } else if only_quiet && !body_calls.is_empty()
                || (body_calls.is_empty() && body_returns.is_empty())
            {
                emit(
                    &mut findings,
                    Finding {
                introduced: None,
                        id: String::new(),
                        kind: FindingKind::SwallowedException,
                        severity: FindingKind::SwallowedException.default_severity(),
                        symbol: x.symbol,
                        symbol_name: sym.qual_name.clone(),
                        message: format!(
                            "{} exception handler in '{}' (line {}) swallows the error ({})",
                            if x.is_bare { "bare" } else { "broad" },
                            sym.name,
                            x.line,
                            if body_calls.is_empty() {
                                "empty body"
                            } else {
                                "print/log only"
                            }
                        ),
                        evidence_lines: vec![x.line],
                        remediation:
                            "At minimum log with context and re-raise or convert to a typed error"
                                .into(),
                        authority: Authority::InferredCandidate,
                        rule: "conf/PR-004".into(),
                        tags: vec!["robustness".into()],
                    },
                );
            }
        }
    }

    // ---- ARCH-001 Layering violation (CAP-1.1, acceptance #41) ----
    // Convention: UI/API layer modules must not touch the database directly.
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() {
            continue;
        }
        let ui_layer = [
            "ui",
            "api",
            "view",
            "route",
            "controller",
            "web",
            "http",
            "handler",
        ]
        .iter()
        .any(|p| store.files.path(sym.loc.file).to_ascii_lowercase().contains(p));
        if !ui_layer {
            continue;
        }
        let Some(s) = summaries.get(&sym.id) else {
            continue;
        };
        let direct_db =
            s.effects.contains(&EffectKind::DbWrite) || s.effects.contains(&EffectKind::DbRead);
        if !direct_db {
            continue;
        }
        // Only flag when the db effect is direct (own writes), not via a data module.
        let via_data_module = s.callees.iter().any(|c| {
            store
                .get(*c)
                .map(|cs| {
                    store.files.path(cs.loc.file).to_ascii_lowercase().contains("db")
                        || store.files.path(cs.loc.file).to_ascii_lowercase().contains("repo")
                })
                .unwrap_or(false)
        });
        if via_data_module {
            continue;
        }
        emit(
            &mut findings,
            Finding {
                introduced: None,
                id: String::new(),
                kind: FindingKind::LayeringViolation,
                severity: FindingKind::LayeringViolation.default_severity(),
                symbol: sym.id,
                symbol_name: sym.qual_name.clone(),
                message: format!(
                    "UI/API-layer function '{}' touches the database directly (effects: {:?}); data access should live behind a data/repository module",
                    sym.qual_name,
                    s.effects.iter().map(|e| e.as_str()).collect::<Vec<_>>()
                ),
                evidence_lines: vec![store.files.line(sym.loc)],
                remediation: "Move the query into a data/repository module and call that from the UI layer".into(),
                authority: Authority::Explicit,
                rule: "arch/AR-001".into(),
                tags: vec!["architecture".into()],
            },
        );
    }

    // ---- Contracts (FR-6) ----
    let mut cid: u32 = 0;
    // Explicit docstring contracts
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let Some(doc) = &sym.doc else { continue };
        for line in doc.lines() {
            let l = line.trim().to_ascii_lowercase();
            let obligation = if l.contains("double")
                && (l.contains("charge") || l.contains("spend") || l.contains("pay"))
            {
                Some(ObligationKind::MustNotDoubleSpend)
            } else if l.contains("authoriz") || l.contains("authenticat") {
                Some(ObligationKind::MustAuthorize)
            } else if l.contains("log") {
                Some(ObligationKind::MustLog)
            } else if l.contains("valid") {
                Some(ObligationKind::MustValidateInput)
            } else if l.contains("secret") || l.contains("credential") {
                Some(ObligationKind::MustNotLeakSecrets)
            } else if l.starts_with("must")
                || l.starts_with("requires:")
                || l.starts_with("ensures:")
            {
                Some(ObligationKind::Custom(
                    line.trim().chars().take(80).collect(),
                ))
            } else {
                None
            };
            if let Some(ob) = obligation {
                contracts.push(Contract {
                verified_by: vec![],
                introduced: None,
                    id: cid,
                    obligation: ob,
                    expr: line.trim().chars().take(200).collect(),
                    authority: Authority::Explicit,
                    symbols: vec![sym.id],
                    source: ContractSource::Docstring,
                });
                cid += 1;
            }
        }
    }
    // Naming-convention MustAuthorize contracts were RETIRED (behavioural-
    // evidence redesign): a function name is a lexical hint, never a
    // normative authorization claim. The candidates now surface as typed
    // `lexical_hints` on the function summaries (behavior.rs), so an auditor
    // sees the same lead without the bundle asserting a security obligation
    // the source never made. Docstring contracts above stay: they are
    // explicitly authored.

    // Ordering for deterministic output
    findings.sort_by(|a, b| (&a.kind, &a.symbol).cmp(&(&b.kind, &b.symbol)));
    bind_verifying_tests(store, summaries, &mut contracts);
    ConformanceResult {
        findings,
        contracts,
    }
}


/// Bind only actual cited sites. Bounded history is optional and never changes
/// authority. The name `introduced` is a compatibility label for Git blame.
pub fn bind_history(root: &std::path::Path, store: &SymbolStore, contracts: &mut [Contract], findings: &mut [Finding]) {
    let site = |id: SymbolId, line: Option<u32>| store.get(id).map(|s| (store.files.path(s.loc.file).to_string(), line.unwrap_or_else(||store.files.line(s.loc)) as u64));
    let mut sites: Vec<_> = contracts.iter().flat_map(|c|c.symbols.iter()).filter_map(|s|site(*s,None)).collect();
    sites.extend(findings.iter().flat_map(|f| {
        if f.evidence_lines.is_empty() { vec![site(f.symbol,None)] }
        else { f.evidence_lines.iter().map(|l|site(f.symbol,Some(*l))).collect() }
    }).flatten());
    let history=cie_history::cited_history(root,&sites);
    let chain = |sites: Vec<(String,u64)>| {
        let mut c: Vec<_> = sites.iter().flat_map(|(p,l)|cie_history::archaeology_chain(&history,p,*l)).collect();
        c.sort_by(|a,b|(a.time,&a.id).cmp(&(b.time,&b.id))); c.dedup_by(|a,b|a.id==b.id);
        (!c.is_empty()).then_some(c)
    };
    for c in contracts { c.introduced=chain(c.symbols.iter().filter_map(|s|site(*s,None)).collect()); }
    for f in findings { f.introduced=chain(if f.evidence_lines.is_empty(){site(f.symbol,None).into_iter().collect()}else{f.evidence_lines.iter().filter_map(|l|site(f.symbol,Some(*l))).collect()}); }
}


pub const MAX_VERIFY_DEPTH: usize = 6;
/// Build the reverse index once, then walk each obligation's bound symbols.
/// This says a parsed test reaches a symbol, not that the rule was asserted.
pub fn bind_verifying_tests(store: &SymbolStore, summaries: &BTreeMap<SymbolId, cie_summary::FunctionSummary>, contracts: &mut [Contract]) {
    use std::collections::{BTreeSet, VecDeque};
    let tests: BTreeSet<_> = store.symbols.iter().filter(|s|s.is_test()).map(|s|s.id).collect();
    let mut callers: BTreeMap<SymbolId, BTreeSet<SymbolId>>=BTreeMap::new();
    for (from,s) in summaries { for to in &s.callees { callers.entry(*to).or_default().insert(*from); } }
    for contract in contracts {
        let mut seen=BTreeSet::new(); let mut found=BTreeSet::new();
        let mut queue: VecDeque<_> = contract.symbols.iter().map(|s|(*s,0)).collect();
        while let Some((symbol,depth))=queue.pop_front() {
            if !seen.insert(symbol) { continue; }
            if tests.contains(&symbol) { found.insert(symbol); }
            if depth>=MAX_VERIFY_DEPTH { continue; }
            for caller in callers.get(&symbol).into_iter().flatten() { queue.push_back((*caller,depth+1)); }
        }
        contract.verified_by=found.into_iter().collect();
    }
}

#[cfg(test)]
mod reachability_tests {
    use super::*;
    use cie_semantic::facts::{SymbolFact,SymbolKind};
    fn symbol(id:u32,kind:SymbolKind)->SymbolFact { SymbolFact { annotations: Vec::new(), id:SymbolId(id),kind,name:format!("arbitrary_{id}"),qual_name:format!("arbitrary_{id}"),loc:cie_core::SourceLocation::span_bytes(cie_core::FileId(0),0,1),lang:cie_semantic::LangId::Java,parent:None,rev:1,doc:None } }
    fn contract()->Contract { Contract { introduced:None,verified_by:vec![],id:0,obligation:ObligationKind::MustAuthorize,expr:"requires authorization".into(),authority:Authority::Explicit,symbols:vec![SymbolId(0)],source:ContractSource::Docstring } }
    #[test]
    fn direct_transitive_annotation_tests_depth_cap_and_cycles() {
        let mut symbols:Vec<_>=(0..9).map(|id|symbol(id,if [1,6,7].contains(&id){SymbolKind::Test}else{SymbolKind::Function})).collect();
        symbols.push(symbol(20,SymbolKind::Test));
        let store=SymbolStore::build(symbols);
        let mut summaries=BTreeMap::new();
        for i in 1..9 { summaries.insert(SymbolId(i),cie_summary::FunctionSummary{sym:SymbolId(i),callees:vec![SymbolId(i-1)],..Default::default()}); }
        summaries.insert(SymbolId(0),cie_summary::FunctionSummary{sym:SymbolId(0),callees:vec![SymbolId(1)],..Default::default()});
        let mut contracts=vec![contract()];bind_verifying_tests(&store,&summaries,&mut contracts);
        assert_eq!(contracts[0].verified_by,vec![SymbolId(1),SymbolId(6)]);
        contracts[0].symbols=vec![SymbolId(20)];bind_verifying_tests(&store,&BTreeMap::new(),&mut contracts);assert_eq!(contracts[0].verified_by,vec![SymbolId(20)]);
        contracts[0].symbols=vec![SymbolId(8)];bind_verifying_tests(&store,&summaries,&mut contracts);assert!(contracts[0].verified_by.is_empty());
        let mut wire=serde_json::to_value(&contracts[0]).unwrap();wire.as_object_mut().unwrap().remove("verified_by");wire.as_object_mut().unwrap().remove("introduced");
        let legacy:Contract=serde_json::from_value(wire).unwrap();assert!(legacy.verified_by.is_empty());assert!(legacy.introduced.is_none());
    }
}
