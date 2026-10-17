use crate::model::*;
use cie_core::types::{content_hash, hash_hex, SymbolId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Question {
    pub repository_id: String,
    pub question: String,
    pub intent: Option<Intent>,
    pub symbol_id: Option<String>,
    pub parent_answer: Option<String>,
}
pub fn route(question: &str) -> Intent {
    let q = question.to_lowercase();
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
    let first = line
        .unwrap_or_else(|| cie_core::types::line_of(source.as_bytes(), symbol.loc.start))
        .max(1);
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
fn symbol_item(repo: &Repository, rev: &Revision, s: &cie_semantic::facts::SymbolFact) -> Item {
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
                for c in &rev.snapshot.concepts.nodes {
                    let citations = c
                        .symbols
                        .iter()
                        .take(3)
                        .filter_map(|id| rev.snapshot.symbols.iter().find(|s| s.id == *id))
                        .filter_map(|s| citation(repo, rev, s, None))
                        .collect();
                    answer.items.push(Item{id:format!("concept:{}",c.id),label:c.label.clone(),kind:c.kind.as_str().into(),claim_class:"analyzerInference".into(),citations,detail:json!({"symbols":c.symbols,"behaviors":c.behaviors,"rules":c.rules,"scope":"Inferred concept; source membership does not verify business meaning"})});
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
            Intent::Unknown => {
                selected.clear();
                answer.status = "unknown".into();
                gaps.push("This deterministic mode cannot support a definitive answer to that question. Try overview, findings, concepts, search, callers, callees, impact or tests. Runtime and financial outcomes need additional evidence.".into());
            }
        }
    }
    if !matches!(intent, Intent::Findings | Intent::Concepts) && answer.status != "unknown" {
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
    if !matches!(intent, Intent::Findings | Intent::Concepts) {
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
    let hidden: BTreeSet<_> = decisions
        .iter()
        .filter(|(_, v)| v.as_str() == "refute")
        .map(|(k, _)| (*k).clone())
        .collect();
    answer.items.retain(|i| !hidden.contains(&i.id));
    answer
        .edges
        .retain(|e| !hidden.contains(&e.from) && !hidden.contains(&e.to));
    let challenged = decisions.values().any(|v| v.as_str() != "confirm")
        || answer.parent_answer.as_ref().is_some_and(|parent| {
            state
                .reviews
                .iter()
                .any(|r| r.answer_id == *parent && r.verdict != "confirm")
        });
    if challenged {
        answer.status = "challenged".into();
        answer.gaps.push(
            "A user challenged an interpretation. Confirmation never upgrades it to a proven fact."
                .into(),
        );
    }
    // Regenerate the count summary after refutation so hidden claims are not leaked by an old summary.
    if !hidden.is_empty() {
        answer.summary=format!("{} visible records; {} refuted records are withheld. Historical evidence is preserved.",answer.items.len(),hidden.len());
    }
    Ok(RenderedAnswer {
        freshness: crate::repository::freshness(repo, rev).into(),
        answer,
        challenged,
        hidden_claims: hidden.len(),
    })
}
