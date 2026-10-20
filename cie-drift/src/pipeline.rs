//! Drift analysis (PRD §29 / FR-31–39).
//!
//! Pipeline: extract claims from documents (rule-based, LLM-optional) →
//! map claims to symbols (lexical + tag scoring; LLM proposals are
//! candidate-only) → verify mechanically → per-claim status with evidence.
//! Status rules (FR-35): Implemented requires evidence; timeout ≠
//! Implemented; inferred mapping alone ≠ Implemented; Unverifiable is
//! explicit; Unknown when the verifier could not finish.

use cie_claims::model::*;
use cie_core::types::{ClaimId, Ns, RevId, SymbolId};
use cie_semantic::facts::SymbolStore;
use cie_summary::EffectKind;
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- claims

/// Extract claims from a Markdown/plain-text document (FR-32).
/// Rule-based extractor: `rule:regex-v1`. Works with LLM disabled
/// (acceptance #25 requires >= 60% recall without LLM).
pub fn extract_claims(doc_name: &str, text: &str, now: Ns) -> Vec<Claim> {
    let mut claims = vec![];
    let mut id: ClaimId = 0;
    let mut section = String::new();
    let mut char_off: u64 = 0;

    for (ln, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            section = trimmed.trim_start_matches('#').trim().to_string();
        }
        let is_bullet = trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || (trimmed.len() > 2
                && trimmed.as_bytes()[0].is_ascii_digit()
                && trimmed.contains('.'));
        let body = trimmed
            .trim_start_matches(['-', '*', '>', ' '])
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches(". ")
            .trim();
        let lower = body.to_ascii_lowercase();
        let has_modality = [
            "must", "must not", "shall", "should", "may", "will", "never", "always",
        ]
        .iter()
        .any(|m| lower.contains(m));

        if !body.is_empty() && (is_bullet || has_modality) && body.len() > 12 {
            if let Some((kind, modality, conf)) = classify_claim(body) {
                claims.push(Claim {
                introduced: None,
                    id,
                    doc: doc_name.to_string(),
                    text: body.chars().take(500).collect(),
                    kind,
                    modality,
                    loc: DocSourceLocation {
                        doc: doc_name.to_string(),
                        section: section.clone(),
                        line: (ln + 1) as u32,
                        char_start: char_off,
                        char_end: char_off + body.len() as u64,
                    },
                    confidence: conf,
                    extractor: "rule:regex-v1".into(),
                    created_ns: now,
                });
                id += 1;
            }
        }
        char_off += line.len() as u64 + 1;
    }
    claims
}

fn classify_claim(text: &str) -> Option<(ClaimKind, Modality, f32)> {
    let t = text.to_ascii_lowercase();
    let modality = if t.contains("must not")
        || t.contains("never")
        || t.contains("shall not")
        || t.contains("must")
    {
        Modality::Must
    } else if t.contains("shall") {
        Modality::Shall
    } else if t.contains("should") {
        Modality::Should
    } else if t.contains("may") {
        Modality::May
    } else if t.contains("will") {
        Modality::Will
    } else if t.contains("always") {
        Modality::Must
    } else {
        return None;
    };

    let kind = if t.contains("never") || t.contains("must not") || t.contains("shall not") {
        ClaimKind::Prohibition
    } else if [
        "p99",
        "latency",
        " ms",
        "seconds",
        "uptime",
        "qps",
        "throughput",
        "99.9",
        "99.99",
        "response time",
        "scalable",
        "scale",
    ]
    .iter()
    .any(|w| t.contains(w))
    {
        ClaimKind::NonFunctional
    } else if [
        "endpoint", "api", "returns", "payload", "request", "response", "json", "http",
    ]
    .iter()
    .any(|w| t.contains(w))
    {
        ClaimKind::Interface
    } else if [
        "schema", "table", "field", "column", "model", "stored", "database",
    ]
    .iter()
    .any(|w| t.contains(w))
    {
        ClaimKind::DataModel
    } else if [
        "authoriz",
        "authenticat",
        "permission",
        "secret",
        "encrypt",
        "security",
        "audit",
    ]
    .iter()
    .any(|w| t.contains(w))
        || ["log", "metrics", "trace"].iter().any(|w| t.contains(w))
    {
        ClaimKind::Constraint
    } else {
        ClaimKind::Capability
    };

    let conf = if t.contains("must") || t.contains("shall") || t.contains("never") {
        0.9
    } else {
        0.6
    };
    Some((kind, modality, conf))
}

// ---------------------------------------------------------------- mapping

fn tokenize(s: &str) -> Vec<String> {
    let mut out = vec![];
    for part in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if part.len() < 3 {
            continue;
        }
        // split camelCase / snake_case
        let mut cur = String::new();
        for ch in part.chars() {
            if ch.is_ascii_uppercase() && !cur.is_empty() {
                out.push(cur.to_ascii_lowercase());
                cur = String::new();
            }
            if ch != '_' {
                cur.push(ch);
            }
        }
        if !cur.is_empty() {
            out.push(cur.to_ascii_lowercase());
        }
    }
    out
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "must", "should", "shall", "will", "have",
    "has", "are", "was", "when", "user", "users", "system", "all", "any", "not", "into", "onto",
    "each", "per", "its", "his", "her", "their", "been", "being", "than", "then", "they", "them",
    "there", "here", "where", "which", "while", "before", "after", "every", "all", "any", "the",
    "a", "an", "of", "to", "in", "on", "is", "be",
];

pub fn map_claims(
    claims: &[Claim],
    store: &SymbolStore,
    summaries: &BTreeMap<SymbolId, cie_summary::FunctionSummary>,
) -> Vec<ClaimMapping> {
    // Precompute symbol token sets.
    let mut symbol_tokens: Vec<(SymbolId, Vec<String>, Vec<String>)> = vec![];
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let mut name_tokens = tokenize(&sym.name);
        let mut content_tokens = tokenize(&sym.qual_name);
        if let Some(doc) = &sym.doc {
            content_tokens.extend(tokenize(doc));
        }
        if let Some(s) = summaries.get(&sym.id) {
            for t in &s.tags {
                content_tokens.push(t.clone());
            }
        }
        name_tokens.retain(|t| !STOPWORDS.contains(&t.as_str()));
        content_tokens.retain(|t| !STOPWORDS.contains(&t.as_str()));
        symbol_tokens.push((sym.id, name_tokens, content_tokens));
    }

    let mut mappings = vec![];
    for claim in claims {
        let tokens: Vec<String> = tokenize(&claim.text)
            .into_iter()
            .filter(|t| !STOPWORDS.contains(&t.as_str()) && t.len() > 2)
            .collect();
        let tokens: BTreeSet<String> = tokens.into_iter().collect();
        let mut scored: Vec<(SymbolId, f32)> = vec![];
        for (id, name_tokens, content_tokens) in &symbol_tokens {
            let name_set: BTreeSet<String> = name_tokens.iter().cloned().collect();
            let content_set: BTreeSet<String> = content_tokens.iter().cloned().collect();
            let name_hits = tokens.intersection(&name_set).count() as f32;
            let content_hits = tokens.intersection(&content_set).count() as f32;
            if name_hits == 0.0 && content_hits == 0.0 {
                continue;
            }
            let score = (name_hits * 3.0 + content_hits) / (tokens.len() as f32).max(1.0);
            if score >= 0.28 {
                scored.push((*id, score.min(1.0)));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(5);
        if !scored.is_empty() {
            mappings.push(ClaimMapping {
                claim: claim.id,
                symbols: scored.iter().map(|(id, _)| *id).collect(),
                confidence: scored[0].1,
                proposer: "rule:lexical-v1".into(),
            });
        }
    }
    mappings
}

// ---------------------------------------------------------------- verify

pub struct VerifyConfig {
    pub deadline_ns: Option<Ns>,
    /// Test hook for acceptance #26: pretend the verifier timed out.
    pub inject_timeout: bool,
}

pub fn verify_claims(
    claims: &[Claim],
    mappings: &[ClaimMapping],
    store: &SymbolStore,
    per_file: &[cie_semantic::facts::LangFacts],
    summaries: &BTreeMap<SymbolId, cie_summary::FunctionSummary>,
    findings: &[cie_contract::findings::Finding],
    cfg: &VerifyConfig,
) -> Vec<ClaimVerification> {
    let mut out = vec![];
    let search_space = store.function_ids().len();

    for claim in claims {
        let start = cie_core::types::now_ns();
        let timed_out = match cfg.deadline_ns {
            Some(dl) => cie_core::types::now_ns() > dl,
            None => false,
        };
        if timed_out || cfg.inject_timeout {
            out.push(ClaimVerification {
                claim: claim.id,
                status: DriftStatus::Unknown,
                checker: "verify/behavioral-v1".into(),
                evidence: vec![
                    "verification budget exceeded — no conclusion (never Implemented)".into(),
                ],
                counterexamples: vec![],
                symbols: vec![],
                reason: Some("verification_timeout".into()),
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        }

        let mapping = mappings.iter().find(|m| m.claim == claim.id);
        let symbols: Vec<SymbolId> = mapping.map(|m| m.symbols.clone()).unwrap_or_default();

        // 1. Non-functional measurable claims -> Unverifiable (acceptance #20)
        if claim.kind == ClaimKind::NonFunctional {
            out.push(ClaimVerification {
                claim: claim.id,
                status: DriftStatus::Unverifiable,
                checker: "verify/behavioral-v1".into(),
                evidence: vec![
                    "latency/throughput claims require runtime evidence (OTel ingestion not present in this revision)".into(),
                ],
                counterexamples: vec![],
                symbols,
                reason: Some(UnverifiableReason::RequiresRuntimeEvidence.as_str().into()),
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        }

        // 2. Vague claims without operational definition
        let lower = claim.text.to_ascii_lowercase();
        let vague = [
            "user-friendly",
            "fast",
            "easy",
            "intuitive",
            "robust",
            "modern",
        ]
        .iter()
        .any(|w| lower.contains(w))
            && !["ms", "p99", "seconds", "%"]
                .iter()
                .any(|w| lower.contains(w));
        if vague {
            out.push(ClaimVerification {
                claim: claim.id,
                status: DriftStatus::Unverifiable,
                checker: "verify/behavioral-v1".into(),
                evidence: vec!["no operational definition — vague quality claim".into()],
                counterexamples: vec![],
                symbols,
                reason: Some(UnverifiableReason::NoOperationalDefinition.as_str().into()),
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        }

        // 3. No mapping -> Missing with evidence of complete search (acceptance #19)
        let Some(m) = mapping else {
            out.push(ClaimVerification {
                claim: claim.id,
                status: DriftStatus::Missing,
                checker: "verify/behavioral-v1".into(),
                evidence: vec![format!(
                    "complete search over {search_space} symbols: no candidate reached the mapping threshold"
                )],
                counterexamples: vec![],
                symbols: vec![],
                reason: None,
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        };

        // 4. Low-confidence mapping alone cannot establish status
        if m.confidence < 0.34 {
            out.push(ClaimVerification {
                claim: claim.id,
                status: DriftStatus::Unverifiable,
                checker: "verify/behavioral-v1".into(),
                evidence: vec![format!(
                    "best mapping confidence {:.2} below verification threshold",
                    m.confidence
                )],
                counterexamples: vec![],
                symbols,
                reason: Some(UnverifiableReason::LowConfidenceMapping.as_str().into()),
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        }

        let primary = symbols[0];
        let summary = summaries.get(&primary).cloned();

        // Gather findings attached to the mapped symbol set.
        let related_findings: Vec<&cie_contract::findings::Finding> = findings
            .iter()
            .filter(|f| symbols.contains(&f.symbol))
            .collect();

        // ---- Prohibition checks (acceptance #18) ----
        if claim.kind == ClaimKind::Prohibition {
            let double_spend = lower.contains("double") || lower.contains("twice");
            let mut counterexamples: Vec<String> = vec![];
            let contradicts = if double_spend {
                // count direct db-write call sites in the mapped function
                let write_sites: Vec<u32> = per_file
                    .iter()
                    .flat_map(|f| f.calls.iter())
                    .filter(|c| {
                        c.caller == primary
                            && matches!(
                                &c.callee,
                                cie_semantic::facts::Callee::Builtin(b)
                                    if matches!(b, cie_semantic::facts::BuiltinKind::DbExecute)
                            )
                    })
                    .map(|c| c.line)
                    .collect();
                if write_sites.len() >= 2 {
                    counterexamples = write_sites
                        .iter()
                        .map(|l| format!("charge effect at line {l}"))
                        .collect();
                    true
                } else if let Some(s) = &summary {
                    if s.has_loops && s.effects.contains(&EffectKind::DbWrite) {
                        counterexamples.push("db write inside loop — repeat spend possible".into());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                // "never X": check for the forbidden effect
                let forbidden: Option<EffectKind> = if lower.contains("log") {
                    None // "never log" is about presence, handled below
                } else if lower.contains("delete") || lower.contains("drop") {
                    Some(EffectKind::DbWrite)
                } else {
                    None
                };
                match forbidden {
                    Some(eff) => summary
                        .as_ref()
                        .map(|s| s.effects.contains(&eff))
                        .unwrap_or(false),
                    None => false,
                }
            };

            // authz-related prohibitions: "must never return success without authorization"
            let authz_fail = related_findings.iter().any(|f| {
                matches!(
                    f.kind,
                    cie_contract::findings::FindingKind::PrintOnlyAuthorization
                        | cie_contract::findings::FindingKind::IgnoredRejection
                )
            });

            out.push(ClaimVerification {
                claim: claim.id,
                status: if contradicts || authz_fail {
                    DriftStatus::Contradicting
                } else {
                    DriftStatus::Implemented
                },
                checker: "verify/behavioral-v1".into(),
                evidence: if contradicts || authz_fail {
                    counterexamples
                        .iter()
                        .cloned()
                        .chain(
                            related_findings
                                .iter()
                                .filter(|f| {
                                    matches!(
                                        f.kind,
                                        cie_contract::findings::FindingKind::PrintOnlyAuthorization
                                            | cie_contract::findings::FindingKind::IgnoredRejection
                                    )
                                })
                                .map(|f| format!("finding {}: {}", f.id, f.message)),
                        )
                        .collect()
                } else {
                    vec![format!(
                        "mapped to '{}' ({:?}); no violating pattern found by mechanical check",
                        store
                            .get(primary)
                            .map(|s| s.qual_name.clone())
                            .unwrap_or_default(),
                        summary
                            .map(|s| s.effects.iter().map(|e| e.as_str()).collect::<Vec<_>>())
                            .unwrap_or_default()
                    )]
                },
                counterexamples,
                symbols,
                reason: None,
                verified_ns: cie_core::types::now_ns(),
                compute_ns: 0,
            });
            continue;
        }

        // ---- Requirement checks ----
        let wants_auth = lower.contains("authoriz")
            || lower.contains("authenticat")
            || lower.contains("permission");
        let wants_log = lower.contains("log");
        let wants_validation = lower.contains("valid");

        let transitive = summary
            .as_ref()
            .map(|s| s.effects.clone())
            .unwrap_or_default();
        let auth_broken = related_findings.iter().any(|f| {
            matches!(
                f.kind,
                cie_contract::findings::FindingKind::PrintOnlyAuthorization
                    | cie_contract::findings::FindingKind::IgnoredRejection
                    | cie_contract::findings::FindingKind::AuthzBypass
            )
        });

        let status = if wants_auth {
            if auth_broken {
                DriftStatus::Contradicting
            } else if summary
                .as_ref()
                .map(|s| s.tags.contains("auth"))
                .unwrap_or(false)
            {
                DriftStatus::Implemented
            } else {
                DriftStatus::Partial
            }
        } else if wants_log {
            if transitive.contains(&EffectKind::Log) {
                DriftStatus::Implemented
            } else {
                DriftStatus::Missing
            }
        } else if wants_validation {
            if transitive.contains(&EffectKind::Eval)
                || summary
                    .as_ref()
                    .map(|s| !s.path_conditions.is_empty())
                    .unwrap_or(false)
            {
                DriftStatus::Partial
            } else {
                DriftStatus::Missing
            }
        } else {
            // Generic capability/behavior claim: presence of mapped code with
            // any behavior signal counts as Implemented; bare existence = Partial.
            let has_signal = summary
                .as_ref()
                .map(|s| !s.effects.is_empty() || !s.callees.is_empty())
                .unwrap_or(false);
            if has_signal {
                DriftStatus::Implemented
            } else {
                DriftStatus::Partial
            }
        };

        let mut evidence = vec![format!(
            "mapping confidence {:.2} to '{}'",
            m.confidence,
            store
                .get(primary)
                .map(|s| s.qual_name.clone())
                .unwrap_or_default()
        )];
        if auth_broken {
            for f in related_findings.iter().filter(|f| {
                matches!(
                    f.kind,
                    cie_contract::findings::FindingKind::PrintOnlyAuthorization
                        | cie_contract::findings::FindingKind::IgnoredRejection
                        | cie_contract::findings::FindingKind::AuthzBypass
                )
            }) {
                evidence.push(format!("counterexample finding {}: {}", f.id, f.message));
            }
        }

        out.push(ClaimVerification {
            claim: claim.id,
            status,
            checker: "verify/behavioral-v1".into(),
            evidence,
            counterexamples: vec![],
            symbols,
            reason: None,
            verified_ns: cie_core::types::now_ns(),
            compute_ns: (cie_core::types::now_ns() - start) as i64,
        });
    }
    out
}

// ---------------------------------------------------------------- report

/// Assemble the drift report with multi-doc reconciliation (FR-38) and
/// reverse drift (FR-36, acceptance #21).
#[allow(clippy::too_many_arguments)] // §9 report assembly is one seam; params mirror §11 fields
pub fn build_report(
    rev: RevId,
    docs: &[String],
    claims: Vec<Claim>,
    mappings: Vec<ClaimMapping>,
    mut verifications: Vec<ClaimVerification>,
    store: &SymbolStore,
    complete: bool,
    now: Ns,
) -> DriftReport {
    // Multi-doc reconciliation: same normalized text in different docs with
    // different statuses -> Contradicting citing both docs.
    let mut by_text: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, c) in claims.iter().enumerate() {
        let norm = conflict_key(&c.text);
        by_text.entry(norm).or_default().push(i);
    }
    for idxs in by_text.values() {
        if idxs.len() < 2 {
            continue;
        }
        let docs_involved: BTreeSet<String> = idxs.iter().map(|i| claims[*i].doc.clone()).collect();
        if docs_involved.len() < 2 {
            continue;
        }
        let statuses: BTreeSet<DriftStatus> = idxs
            .iter()
            .filter_map(|i| {
                verifications
                    .iter()
                    .find(|v| v.claim == claims[*i].id)
                    .map(|v| v.status)
            })
            .collect();
        if statuses.len() > 1 {
            for i in idxs {
                let cid = claims[*i].id;
                if let Some(v) = verifications.iter_mut().find(|v| v.claim == cid) {
                    v.status = DriftStatus::Contradicting;
                    v.evidence.push(format!(
                        "multi-document conflict: same claim in {} with statuses {:?}",
                        docs_involved.iter().cloned().collect::<Vec<_>>().join(", "),
                        statuses.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                    ));
                }
            }
        }
    }

    // Reverse drift: function-like symbols never mapped by any claim.
    let mapped: BTreeSet<SymbolId> = mappings
        .iter()
        .flat_map(|m| m.symbols.iter().copied())
        .collect();
    let mut reverse_drift = vec![];
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() || sym.name.starts_with("__") {
            continue;
        }
        if sym
            .doc
            .as_ref()
            .map(|d| d.contains("entrypoint"))
            .unwrap_or(false)
        {
            continue;
        }
        let is_entry = summaries_entrypoint(sym.id, store);
        if is_entry {
            continue;
        }
        if !mapped.contains(&sym.id) {
            reverse_drift.push(ReverseDriftEntry {
                symbol: sym.id,
                qual_name: sym.qual_name.clone(),
                reason: "no document claim maps to this symbol".into(),
            });
        }
    }

    let mut report = DriftReport {
        rev,
        docs: docs.to_vec(),
        claims,
        mappings,
        verifications,
        reverse_drift,
        aggregate: BTreeMap::new(),
        complete,
        generated_ns: now,
    };
    report.recount();
    report
}

/// FR-38 reconciliation key: modality- and number-insensitive core tokens,
/// so "must log every refund" and "must never log refunds" conflict.
fn conflict_key(text: &str) -> String {
    let drop_words = [
        "must", "shall", "should", "may", "will", "never", "not", "always",
    ];
    let mut toks: Vec<String> = tokenize(text)
        .into_iter()
        .filter(|t| !STOPWORDS.contains(&t.as_str()) && !drop_words.contains(&t.as_str()))
        .map(|t| {
            if t.len() > 3 && t.ends_with('s') {
                t[..t.len() - 1].to_string()
            } else {
                t
            }
        })
        .collect();
    toks.sort();
    toks.dedup();
    toks.join("+")
}

fn summaries_entrypoint(id: SymbolId, store: &SymbolStore) -> bool {
    store
        .get(id)
        .map(|s| {
            matches!(
                s.name.as_str(),
                "main" | "index" | "handler" | "app" | "run" | "serve"
            ) || s.name.starts_with("handle_")
        })
        .unwrap_or(false)
}


/// Bind document claims only when the extracted line still contains their text.
/// Normalization can change line numbers; such sites remain unbound.
pub fn bind_history(root: &std::path::Path, claims: &mut [Claim]) {
    let Ok(base)=root.canonicalize() else {return};
    let sites: Vec<Option<(String,u64)>>=claims.iter().map(|c| {
        let path=root.join(&c.doc).canonicalize().or_else(|_|std::path::Path::new(&c.doc).canonicalize()).ok()?;
        let relative=path.strip_prefix(&base).ok()?.to_str()?.replace('\\',"/");
        let text=std::fs::read_to_string(&path).ok()?;
        if !text.lines().nth(c.loc.line.saturating_sub(1) as usize)?.contains(&c.text) {return None;}
        Some((relative,c.loc.line as u64))
    }).collect();
    let requested:Vec<_>=sites.iter().flatten().cloned().collect();
    let history=cie_history::cited_history(root,&requested);
    for (claim,site) in claims.iter_mut().zip(sites) {
        claim.introduced=site.and_then(|(path,line)| {
            let chain=cie_history::archaeology_chain(&history,&path,line);
            (!chain.is_empty()).then_some(chain)
        });
    }
}
