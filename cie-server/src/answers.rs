use crate::model::*;
use cie_core::types::{content_hash, hash_hex, SymbolId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Question {
    pub repository_id: String,
    pub question: String,
    pub intent: Option<Intent>,
    pub symbol_id: Option<String>,
    pub parent_answer: Option<String>,
    pub expected_revision: Option<String>,
    pub request_id: Option<String>,
}
pub fn route(question: &str) -> Intent {
    let q = question.to_lowercase();
    if q.contains("misleading name") || q.contains("naming mismatch") {
        return Intent::Naming;
    }
    if q.contains("business rule") || q.contains("invariant") || q.trim() == "rules" {
        return Intent::Rules;
    }
    if q.contains("behaviors")
        || q.contains("behaviours")
        || q.contains("what does") && !q.contains("call")
    {
        return Intent::Behaviors;
    }
    if ["caller", "who calls", "called by"]
        .iter()
        .any(|s| q.contains(s))
    {
        Intent::Callers
    } else if q.contains("callee") || q.contains("what does") && q.contains("call") {
        Intent::Callees
    } else if ["impact", "blast radius", "affected"]
        .iter()
        .any(|s| q.contains(s))
    {
        Intent::Impact
    } else if q.contains("test") || q.contains("coverage") {
        Intent::Tests
    } else if ["finding", "bugs", "defect", "vulnerab", "security"]
        .iter()
        .any(|s| q.contains(s))
    {
        Intent::Findings
    } else if q.contains("concept") || q.contains("hierarchy") {
        Intent::Concepts
    } else if [
        "overview",
        "architecture",
        "explain this repository",
        "explain the project",
    ]
    .iter()
    .any(|s| q.contains(s))
    {
        Intent::Overview
    } else if ["find ", "search ", "locate "]
        .iter()
        .any(|s| q.contains(s))
    {
        Intent::Search
    } else {
        Intent::Unknown
    }
}
fn terms(question: &str) -> Vec<String> {
    let stop = [
        "rules",
        "rule",
        "business",
        "invariants",
        "invariant",
        "behaviors",
        "behaviours",
        "behavior",
        "behaviour",
        "does",
        "do",
        "naming",
        "mismatch",
        "misleading",
        "names",
        "name",
        "who",
        "what",
        "where",
        "which",
        "are",
        "is",
        "the",
        "a",
        "an",
        "of",
        "for",
        "to",
        "show",
        "me",
        "find",
        "search",
        "locate",
        "in",
        "this",
        "repository",
        "project",
        "function",
        "method",
        "symbol",
        "calls",
        "callers",
        "callee",
        "callees",
        "impact",
        "blast",
        "radius",
        "affected",
        "by",
        "test",
        "tests",
        "coverage",
        "findings",
        "finding",
        "bugs",
        "defect",
        "defects",
        "security",
        "vulnerabilities",
        "and",
        "or",
        "does",
        "any",
        "there",
        "all",
    ];
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .filter(|s| !s.is_empty() && !stop.contains(s))
        .map(str::to_string)
        .collect()
}
pub(crate) fn secret_safe(text: &str) -> String {
    static SECRET: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern=SECRET.get_or_init(||regex::Regex::new(r#"(?i)(?:gh[pousr]_[a-z0-9_]{20,}|sk-(?:live-)?[a-z0-9_-]{16,}|AKIA[A-Z0-9]{16}|(?:api[_-]?key|password|secret|token)\s*[:=]\s*[\"'][^\"'\r\n]{6,}[\"'])"#).expect("constant secret regex"));
    pattern
        .replace_all(text, "[secret-shaped value removed]")
        .into_owned()
}
pub fn citation(
    repo: &Repository,
    revision: &Revision,
    symbol: &cie_semantic::facts::SymbolFact,
    line: Option<u32>,
) -> Option<Citation> {
    let file = revision.snapshot.files.get(symbol.loc.file.0 as usize)?;
    let source = revision.sources.get(file)?;
    let first =
        line.unwrap_or_else(|| cie_core::types::line_of(source.as_bytes(), symbol.loc.start));
    if first == 0 || first as usize > source.split('\n').count() {
        return None;
    }
    let lines: Vec<_> = source.lines().collect();
    let start = (first as usize - 1).min(lines.len());
    let end = (start + 6).min(lines.len());
    let excerpt = secret_safe(&lines[start..end].join("\n"));
    let excerpt: String = excerpt.chars().take(2000).collect();
    Some(Citation {
        repository_id: repo.id.clone(),
        revision_id: revision.id.clone(),
        file: file.clone(),
        start_line: first,
        end_line: end.max(first as usize) as u32,
        source_hash: hash_hex(&content_hash(source.as_bytes())),
        excerpt,
    })
}
pub(crate) fn symbol_item(
    repo: &Repository,
    rev: &Revision,
    s: &cie_semantic::facts::SymbolFact,
) -> Item {
    let summary = rev.snapshot.summaries.iter().find(|sum| sum.sym == s.id);
    Item {
        id: format!("symbol:{}", s.id.0),
        label: s.qual_name.clone(),
        kind: s.kind_str().into(),
        claim_class: "codeObservation".into(),
        citations: citation(repo, rev, s, None).into_iter().collect(),
        detail: json!({"symbolId":s.id.0.to_string(),"language":format!("{:?}",s.lang),"summary":summary,"interpretation":"Summary effects and tags are analyzer inferences, not proof of successful runtime effects"}),
    }
}
pub fn generate(repo: &Repository, rev: &Revision, request: &Question) -> Result<Answer> {
    if request.question.trim().is_empty() || request.question.len() > 4000 {
        return Err(Error::new(
            400,
            "INVALID_QUESTION",
            "Enter a question of at most 4000 bytes",
        ));
    }
    let intent = request
        .intent
        .clone()
        .unwrap_or_else(|| route(&request.question));
    let query = terms(&request.question);
    let mut selected: BTreeSet<SymbolId> = BTreeSet::new();
    let mut gaps = vec![];
    if let Some(id) = &request.symbol_id {
        let id = id
            .parse::<u32>()
            .map(SymbolId)
            .map_err(|_| Error::new(400, "INVALID_SYMBOL", "Invalid selected symbol"))?;
        if !rev.snapshot.symbols.iter().any(|s| s.id == id) {
            return Err(Error::new(
                400,
                "UNKNOWN_SYMBOL",
                "Selected symbol is not in this evidence snapshot",
            ));
        }
        selected.insert(id);
    } else if !query.is_empty() {
        for s in &rev.snapshot.symbols {
            if query.iter().all(|q| {
                s.qual_name.to_lowercase().contains(q) || s.name.to_lowercase().contains(q)
            }) {
                selected.insert(s.id);
            }
        }
    }
    let targeted = matches!(
        intent,
        Intent::Callers | Intent::Callees | Intent::Impact | Intent::Tests
    );
    let mut answer = Answer {
        id: String::new(),
        repository_id: repo.id.clone(),
        revision_id: rev.id.clone(),
        question: request.question.clone(),
        interpretation: format!(
            "{:?}: deterministic local routing over the selected static evidence snapshot",
            intent
        ),
        intent: intent.clone(),
        summary: String::new(),
        status: "ready".into(),
        gaps: vec![],
        items: vec![],
        edges: vec![],
        preferred_view: "table".into(),
        parent_answer: request.parent_answer.clone(),
        request_id: request.request_id.clone(),
        request_fingerprint: Some(request_fingerprint(request)?),
        created_ns: now(),
    };
    let summaries: BTreeMap<_, _> = rev.snapshot.summaries.iter().map(|s| (s.sym, s)).collect();
    if targeted && selected.len() != 1 {
        answer.status = "unknown".into();
        gaps.push(
            if selected.is_empty() {
                "No indexed symbol matches this request. Select a symbol or use its qualified name."
            } else {
                "Several symbols match. Select one explicitly; no target was guessed."
            }
            .into(),
        );
    } else {
        match intent {
            Intent::Overview => {
                selected = rev.snapshot.symbols.iter().map(|s| s.id).collect();
                answer.preferred_view = "graph".into();
            }
            Intent::Search => {
                if selected.is_empty() {
                    gaps.push("No matching indexed symbol; this is not proof the requested feature is absent.".into());
                }
            }
            Intent::Callers | Intent::Impact | Intent::Callees | Intent::Tests => {
                let seed = *selected.iter().next().unwrap();
                let mut population = BTreeSet::from([seed]);
                if intent == Intent::Callees {
                    if let Some(sum) = summaries.get(&seed) {
                        population.extend(&sum.callees);
                    }
                } else {
                    let mut frontier = vec![seed];
                    while let Some(current) = frontier.pop() {
                        for sum in &rev.snapshot.summaries {
                            if sum.callees.contains(&current)
                                && population.insert(sum.sym)
                                && intent != Intent::Callers
                            {
                                frontier.push(sum.sym);
                            }
                        }
                    }
                }
                if intent == Intent::Tests {
                    population.retain(|id| {
                        *id == seed
                            || rev
                                .snapshot
                                .symbols
                                .iter()
                                .any(|s| s.id == *id && s.is_test())
                    });
                    gaps.push("Test relationships are static call reachability, not executed assertion or runtime coverage evidence.".into());
                }
                selected = population;
                answer.preferred_view = "graph".into();
            }
            Intent::Findings => {
                for finding in &rev.snapshot.findings {
                    if (!query.is_empty() || request.symbol_id.is_some())
                        && !selected.contains(&finding.symbol)
                    {
                        continue;
                    }
                    let symbol = rev.snapshot.symbols.iter().find(|s| s.id == finding.symbol);
                    let citations = symbol
                        .and_then(|s| {
                            citation(repo, rev, s, finding.evidence_lines.first().copied())
                        })
                        .into_iter()
                        .collect();
                    answer.items.push(Item{id:format!("finding:{}",finding.id),label:format!("{}: {}",finding.severity.as_str(),secret_safe(&finding.message)),kind:finding.kind.as_str().into(),claim_class:"analyzerInference".into(),citations,detail:json!({"rule":finding.rule,"severity":finding.severity.as_str(),"engineAuthority":finding.authority,"remediation":finding.remediation,"scope":"Static analyzer result; inspect evidence and supported language semantics before treating this as a runtime defect"})});
                }
            }
            Intent::Concepts => {
                for c in rev.snapshot.concepts.nodes.iter().take(500) {
                    let citations = c
                        .symbols
                        .iter()
                        .take(3)
                        .filter_map(|id| rev.snapshot.symbols.iter().find(|s| s.id == *id))
                        .filter_map(|s| citation(repo, rev, s, None))
                        .collect();
                    answer.items.push(Item{id:format!("concept:{}",c.id),label:c.label.clone(),kind:c.kind.as_str().into(),claim_class:"analyzerInference".into(),citations,detail:json!({"symbols":c.symbols,"behaviors":c.behaviors,"rules":c.rules,"confidenceScore":c.confidence,"provenance":c.provenance,"primaryEdgeReason":c.primary_edge,"extraParents":c.extra_edges,"scope":"Inferred concept; source membership does not verify business meaning. Confidence is an uncalibrated engine score, not a probability of business truth."})});
                    if let Some(parent) = c.parent {
                        answer.edges.push(Edge {
                            from: format!("concept:{}", parent),
                            to: format!("concept:{}", c.id),
                            kind: "inferredParent".into(),
                            claim_class: "analyzerInference".into(),
                        });
                    }
                }
                answer.preferred_view = "graph".into();
            }
            Intent::Rules => {
                for contract in &rev.snapshot.contracts {
                    if (!query.is_empty() || request.symbol_id.is_some())
                        && !contract.symbols.iter().any(|s| selected.contains(s))
                    {
                        continue;
                    }
                    let citations = contract
                        .symbols
                        .iter()
                        .filter_map(|id| rev.snapshot.symbols.iter().find(|s| s.id == *id))
                        .take(3)
                        .filter_map(|s| citation(repo, rev, s, None))
                        .collect();
                    answer.items.push(Item{id:format!("rule:{}",contract.id),label:secret_safe(&contract.expr),kind:contract.obligation.as_str(),claim_class:"analyzerInference".into(),citations,detail:json!({"engineAuthority":contract.authority,"source":contract.source,"symbols":contract.symbols,"scope":"Extracted declaration or candidate obligation. Engine authority does not establish business approval or universal runtime enforcement."})});
                    if answer.items.len() >= 500 {
                        break;
                    }
                }
                for (file_index, facts) in rev.snapshot.facts.iter().enumerate() {
                    for (index, guard) in facts.ifs.iter().enumerate() {
                        if answer.items.len() >= 500 {
                            break;
                        }
                        if (!query.is_empty() || request.symbol_id.is_some())
                            && !selected.contains(&guard.symbol)
                        {
                            continue;
                        }
                        let symbol = rev.snapshot.symbols.iter().find(|s| s.id == guard.symbol);
                        let citations = symbol
                            .and_then(|s| citation(repo, rev, s, Some(guard.line)))
                            .into_iter()
                            .collect();
                        answer.items.push(Item{id:format!("guard-rule:{file_index}:{index}"),label:format!("Guard candidate: {}",secret_safe(&guard.cond)),kind:"guardCandidate".into(),claim_class:"analyzerInference".into(),citations,detail:json!({"symbolId":guard.symbol.0.to_string(),"condition":secret_safe(&guard.cond),"scope":"Observed guard candidate; business approval and control-flow enforcement are not established. A guard condition is not automatically an invariant."})});
                    }
                }
                gaps.push("This is an extracted obligation catalogue, not an approved policy register. Missing or inconsistent paths require examination before claiming a confirmed defect.".into());
            }
            Intent::Behaviors => {
                for summary in &rev.snapshot.summaries {
                    if (!query.is_empty() || request.symbol_id.is_some())
                        && !selected.contains(&summary.sym)
                    {
                        continue;
                    }
                    let Some(symbol) = rev.snapshot.symbols.iter().find(|s| s.id == summary.sym)
                    else {
                        continue;
                    };
                    let effects: Vec<_> = summary.effects.iter().map(|e| e.as_str()).collect();
                    answer.items.push(Item{id:format!("behavior:{}",summary.sym.0),label:format!("{}: {}",symbol.qual_name,if effects.is_empty(){"no effects recorded".into()}else{effects.join(", ")}),kind:"staticBehaviorSummary".into(),claim_class:"analyzerInference".into(),citations:citation(repo,rev,symbol,None).into_iter().collect(),detail:json!({"symbolId":summary.sym.0.to_string(),"summary":summary,"scope":"Recorded effects are static analyzer interpretations. No recorded effect is not proof of runtime absence; unresolved/dynamic behavior remains unknown."})});
                    if answer.items.len() >= 500 {
                        break;
                    }
                }
                gaps.push("Static summaries describe the analyzer's supported model; they are not observations of successful execution or exhaustive behavioral specifications.".into());
            }
            Intent::Naming => {
                let store = cie_semantic::facts::SymbolStore::build(rev.snapshot.symbols.clone());
                let summaries = rev
                    .snapshot
                    .summaries
                    .iter()
                    .map(|s| (s.sym, s.clone()))
                    .collect();
                for record in
                    cie_contract::NamingAdapter::default().run(&store, &summaries, rev.snapshot.rev)
                {
                    if (!query.is_empty() || request.symbol_id.is_some())
                        && !selected.contains(&record.subject)
                    {
                        continue;
                    }
                    let Some(symbol) = rev.snapshot.symbols.iter().find(|s| s.id == record.subject)
                    else {
                        continue;
                    };
                    let related: Vec<_> = rev
                        .snapshot
                        .contracts
                        .iter()
                        .filter(|c| c.symbols.contains(&record.subject))
                        .collect();
                    answer.items.push(Item{id:format!("naming:{}",record.subject.0),label:format!("{}: {}",symbol.qual_name,record.kind.as_str()),kind:record.kind.as_str().into(),claim_class:"analyzerInference".into(),citations:citation(repo,rev,symbol,None).into_iter().collect(),detail:json!({"symbolId":record.subject.0.to_string(),"promisedByName":record.promised,"observedEffects":record.observed,"missingInModel":record.missing,"relatedObligations":related,"scope":"A naming lexicon expectation is a concern, not an established business requirement. Missing modeled actions are not proof of runtime absence. A confirmed violation needs an established obligation and adequate evidence."})});
                    if answer.items.len() >= 500 {
                        break;
                    }
                }
                gaps.push("Name/implementation mismatches are candidates. The lexicon can be wrong, and unresolved or dynamic calls can hide effects. No confirmed business violation is claimed by naming alone.".into());
            }
            Intent::Unknown => {
                selected.clear();
                answer.status = "unknown".into();
                gaps.push("This deterministic mode cannot support a definitive answer to that question. Try overview, findings, concepts, search, callers, callees, impact or tests. Runtime and financial outcomes need additional evidence.".into());
            }
        }
    }
    if !matches!(
        intent,
        Intent::Findings | Intent::Concepts | Intent::Rules | Intent::Behaviors | Intent::Naming
    ) && answer.status != "unknown"
    {
        answer.items = rev
            .snapshot
            .symbols
            .iter()
            .filter(|s| selected.contains(&s.id))
            .take(500)
            .map(|s| symbol_item(repo, rev, s))
            .collect();
    }
    if answer.items.len() >= 500 {
        gaps.push("Display population capped at 500 items; refine the question.".into());
    }
    if !matches!(
        intent,
        Intent::Findings | Intent::Concepts | Intent::Rules | Intent::Behaviors | Intent::Naming
    ) {
        let nodes: BTreeSet<_> = answer.items.iter().map(|i| i.id.clone()).collect();
        for sum in &rev.snapshot.summaries {
            for callee in &sum.callees {
                let (from, to) = (
                    format!("symbol:{}", sum.sym.0),
                    format!("symbol:{}", callee.0),
                );
                if nodes.contains(&from) && nodes.contains(&to) {
                    answer.edges.push(Edge {
                        from,
                        to,
                        kind: "resolvedStaticCall".into(),
                        claim_class: "codeObservation".into(),
                    });
                }
            }
        }
    }
    let nodes: BTreeSet<_> = answer.items.iter().map(|i| i.id.clone()).collect();
    answer
        .edges
        .retain(|e| nodes.contains(&e.from) && nodes.contains(&e.to));
    if !rev.snapshot.coverage.complete {
        gaps.push(
            "The engine reported incomplete analysis; no complete absence claim is supported."
                .into(),
        );
    }
    if !rev.snapshot.evidence.unresolved.is_empty() {
        gaps.push(format!(
            "{} unresolved call records remain in this snapshot.",
            rev.snapshot.evidence.unresolved.len()
        ));
    }
    if !rev.snapshot.evidence.unsupported.is_empty() {
        gaps.push(format!(
            "{} unsupported construct records remain in this snapshot.",
            rev.snapshot.evidence.unsupported.len()
        ));
    }
    if answer.items.is_empty() && answer.status != "unknown" {
        answer.status = "unknown".into();
        gaps.push(
            "No matching records in the indexed scope; this is not evidence of global absence."
                .into(),
        );
    }
    answer.summary = if answer.status == "unknown" {
        "A definitive answer is not determined from the available evidence.".into()
    } else {
        format!("{} scoped {:?} records with {} represented relationships. Source observations and analyzer interpretations are distinguished below.",answer.items.len(),intent,answer.edges.len())
    };
    if !gaps.is_empty() && answer.status == "ready" {
        answer.status = "partial".into();
    }
    answer.gaps = gaps;
    Ok(answer)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedAnswer {
    pub answer: Answer,
    pub freshness: String,
    pub freshness_scope: &'static str,
    pub challenged: bool,
    pub hidden_claims: usize,
}
pub fn render(state: &State, id: &str) -> Result<RenderedAnswer> {
    let mut answer = state
        .answers
        .iter()
        .find(|a| a.id == id)
        .cloned()
        .ok_or_else(|| Error::new(404, "ANSWER_NOT_FOUND", "Answer not found"))?;
    let repo = state
        .repositories
        .iter()
        .find(|r| r.id == answer.repository_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Repository evidence was deleted"))?;
    let rev = repo
        .revisions
        .iter()
        .find(|r| r.id == answer.revision_id)
        .ok_or_else(|| {
            Error::new(
                410,
                "EVIDENCE_GONE",
                "The exact evidence snapshot is no longer available",
            )
        })?;
    let mut decisions = BTreeMap::new();
    for review in state.reviews.iter().filter(|r| r.answer_id == id) {
        decisions.insert(&review.item_id, &review.verdict);
    }
    let mut concept_decisions = BTreeMap::new();
    for review in state
        .concept_reviews
        .iter()
        .filter(|r| r.repository_id == answer.repository_id && r.revision_id == answer.revision_id)
    {
        concept_decisions.insert(&review.item_id, &review.verdict);
    }
    for (item, verdict) in &concept_decisions {
        if answer.items.iter().any(|i| &i.id == *item) {
            decisions.insert(*item, *verdict);
        }
    }
    let hidden: BTreeSet<_> = decisions
        .iter()
        .filter(|(_, v)| v.as_str() == "refute")
        .map(|(k, _)| (*k).clone())
        .collect();
    answer.items.retain(|i| !hidden.contains(&i.id));
    answer
        .edges
        .retain(|e| !hidden.contains(&e.from) && !hidden.contains(&e.to));
    let incompatible_context = answer.parent_answer.as_ref().is_some_and(|id| {
        state.answers.iter().find(|a| a.id == *id).is_none_or(|a| {
            a.repository_id != answer.repository_id || a.revision_id != answer.revision_id
        })
    });
    if incompatible_context {
        answer.gaps.push("Parent context is missing or belongs to another source snapshot. Re-run with cleared context; old symbol identities must not be treated as current.".into());
    }
    let challenged = incompatible_context
        || decisions.values().any(|v| v.as_str() != "confirm")
        || answer.parent_answer.as_ref().is_some_and(|parent| {
            let global_challenge =
                state
                    .answers
                    .iter()
                    .find(|a| a.id == *parent)
                    .is_some_and(|a| {
                        a.items.iter().any(|i| {
                            concept_decisions
                                .get(&i.id)
                                .is_some_and(|v| v.as_str() != "confirm")
                        })
                    });
            global_challenge
                || state
                    .reviews
                    .iter()
                    .any(|r| r.answer_id == *parent && r.verdict != "confirm")
        });
    if challenged {
        answer.status = "challenged".into();
        answer.gaps.push(
            "This answer has challenged or incompatible context. Confirmation never upgrades an interpretation to a proven fact."
                .into(),
        );
    }
    // Regenerate the count summary after refutation so hidden claims are not leaked by an old summary.
    if !hidden.is_empty() {
        answer.summary=format!("{} visible records; {} refuted records are withheld. Historical evidence is preserved.",answer.items.len(),hidden.len());
    }
    Ok(RenderedAnswer {
        freshness: crate::repository::freshness(repo, rev).into(),
        freshness_scope: crate::repository::FRESHNESS_SCOPE,
        answer,
        challenged,
        hidden_claims: hidden.len(),
    })
}

pub(crate) fn request_fingerprint(request: &Question) -> Result<String> {
    serde_json::to_vec(request)
        .map(|bytes| hash_hex(&content_hash(&bytes)))
        .map_err(|_| Error::new(500, "SERIALIZE", "Cannot fingerprint request"))
}
