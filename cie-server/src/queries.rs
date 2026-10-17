//! Bounded set expressions use strong Kleene logic. Missing analyzer evidence
//! never becomes a false fact (and therefore never becomes true under NOT).
use crate::{
    answers::{self},
    api::App,
    model::*,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Expression {
    All { args: Vec<Expression> },
    Any { args: Vec<Expression> },
    Not { arg: Box<Expression> },
    Name { value: String },
    Kind { value: String },
    Language { value: String },
    Effect { value: String },
    Tag { value: String },
    HasTest,
    Changed { before_revision_id: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Truth {
    True,
    False,
    Unknown,
}
fn combine(values: &[Truth], all: bool) -> Truth {
    if all {
        if values.contains(&Truth::False) {
            Truth::False
        } else if values.contains(&Truth::Unknown) {
            Truth::Unknown
        } else {
            Truth::True
        }
    } else if values.contains(&Truth::True) {
        Truth::True
    } else if values.contains(&Truth::Unknown) {
        Truth::Unknown
    } else {
        Truth::False
    }
}
fn validate(
    expr: &Expression,
    rev: &Revision,
    repo: &Repository,
    depth: usize,
    nodes: &mut usize,
) -> Result<()> {
    *nodes += 1;
    if depth > 8 || *nodes > 32 {
        return Err(Error::new(
            400,
            "QUERY_LIMIT",
            "Use at most 32 predicates and 8 nesting levels",
        ));
    }
    match expr {
        Expression::All { args } | Expression::Any { args } => {
            if args.is_empty() || args.len() > 16 {
                return Err(Error::new(
                    400,
                    "QUERY_ARITY",
                    "Boolean groups require 1–16 expressions",
                ));
            }
            for arg in args {
                validate(arg, rev, repo, depth + 1, nodes)?;
            }
        }
        Expression::Not { arg } => validate(arg, rev, repo, depth + 1, nodes)?,
        Expression::Changed { before_revision_id } => {
            if !repo.revisions.iter().any(|r| r.id == *before_revision_id) {
                return Err(Error::new(
                    410,
                    "EVIDENCE_GONE",
                    "Comparison snapshot is not retained in this repository",
                ));
            }
        }
        Expression::Name { value }
        | Expression::Kind { value }
        | Expression::Language { value }
        | Expression::Effect { value }
        | Expression::Tag { value } => {
            if value.trim().is_empty() || value.len() > 500 {
                return Err(Error::new(
                    400,
                    "QUERY_VALUE",
                    "Predicate values require 1–500 bytes",
                ));
            }
            if matches!(expr, Expression::Effect { .. })
                && !rev
                    .snapshot
                    .summaries
                    .iter()
                    .flat_map(|s| s.effects.iter())
                    .any(|e| e.as_str() == value)
                && ![
                    "read_file",
                    "write_file",
                    "db_read",
                    "db_write",
                    "network",
                    "print",
                    "log",
                    "spawn",
                    "crypto",
                    "random",
                    "env_read",
                    "eval",
                    "sleep",
                    "lock",
                    "assert",
                    "time",
                ]
                .contains(&value.as_str())
            {
                return Err(Error::new(
                    400,
                    "UNKNOWN_EFFECT",
                    "Unknown effect kind; no predicate was guessed",
                ));
            }
        }
        Expression::HasTest => {}
    }
    Ok(())
}
fn evaluate(
    expr: &Expression,
    symbol: &cie_semantic::facts::SymbolFact,
    rev: &Revision,
    repo: &Repository,
    test_reachable: &BTreeSet<cie_core::types::SymbolId>,
) -> Truth {
    use Expression::*;
    let yes = |b| if b { Truth::True } else { Truth::False };
    let summary = rev.snapshot.summaries.iter().find(|s| s.sym == symbol.id);
    match expr {
        All { args } => combine(
            &args
                .iter()
                .map(|e| evaluate(e, symbol, rev, repo, test_reachable))
                .collect::<Vec<_>>(),
            true,
        ),
        Any { args } => combine(
            &args
                .iter()
                .map(|e| evaluate(e, symbol, rev, repo, test_reachable))
                .collect::<Vec<_>>(),
            false,
        ),
        Not { arg } => match evaluate(arg, symbol, rev, repo, test_reachable) {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        },
        Name { value } => yes(symbol
            .qual_name
            .to_lowercase()
            .contains(&value.to_lowercase())),
        Kind { value } => yes(symbol.kind_str().eq_ignore_ascii_case(value)),
        Language { value } => yes(format!("{:?}", symbol.lang).eq_ignore_ascii_case(value)),
        Effect { value } => {
            if summary.is_some_and(|s| s.effects.iter().any(|e| e.as_str() == value)) {
                Truth::True
            } else {
                Truth::Unknown
            }
        }
        Tag { value } => {
            if summary.is_some_and(|s| s.tags.contains(value)) {
                Truth::True
            } else {
                Truth::Unknown
            }
        }
        HasTest => {
            if test_reachable.contains(&symbol.id) {
                Truth::True
            } else {
                Truth::Unknown
            }
        }
        Changed { before_revision_id } => {
            let Some(file) = rev.snapshot.files.get(symbol.loc.file.0 as usize) else {
                return Truth::Unknown;
            };
            let Some(before) = repo.revisions.iter().find(|r| r.id == *before_revision_id) else {
                return Truth::Unknown;
            };
            yes(before.sources.get(file) != rev.sources.get(file))
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Query {
    repository_id: String,
    revision_id: String,
    expression: Expression,
    #[serde(default = "include_unknown")]
    include_unknown: bool,
}
fn include_unknown() -> bool {
    true
}
fn execute(app: &Arc<App>, req: Query) -> Result<Value> {
    let s = app.storage.read()?;
    let repo = s
        .repositories
        .iter()
        .find(|r| r.id == req.repository_id)
        .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
    let rev = repo
        .revisions
        .iter()
        .find(|v| v.id == req.revision_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Exact snapshot unavailable"))?;
    validate(&req.expression, rev, repo, 0, &mut 0)?;
    let mut tests: BTreeSet<_> = rev
        .snapshot
        .symbols
        .iter()
        .filter(|s| s.is_test())
        .map(|s| s.id)
        .collect();
    loop {
        let n = tests.len();
        for summary in &rev.snapshot.summaries {
            if tests.contains(&summary.sym) {
                tests.extend(summary.callees.iter().copied());
            }
        }
        if tests.len() == n {
            break;
        }
    }
    let mut matched = 0;
    let mut unknown = 0;
    let mut excluded = 0;
    let mut items = vec![];
    for symbol in &rev.snapshot.symbols {
        let decision = evaluate(&req.expression, symbol, rev, repo, &tests);
        match decision {
            Truth::True => matched += 1,
            Truth::Unknown => unknown += 1,
            Truth::False => excluded += 1,
        }
        if decision == Truth::False
            || decision == Truth::Unknown && !req.include_unknown
            || items.len() >= 500
        {
            continue;
        }
        let mut item = answers::symbol_item(repo, rev, symbol);
        item.claim_class = "analyzerInference".into();
        item.detail["queryDecision"] = json!(decision);
        item.detail["expression"] = json!(req.expression);
        item.detail["membershipLimit"]=json!("Membership is a deterministic evaluation of static observations/inferences. Unknown is not false; absence of a recorded effect/test is not runtime absence.");
        items.push(item);
    }
    let ids: BTreeSet<_> = items.iter().map(|i| i.id.clone()).collect();
    let edges = rev
        .snapshot
        .summaries
        .iter()
        .flat_map(|sum| {
            sum.callees.iter().map(move |to| Edge {
                from: format!("symbol:{}", sum.sym.0),
                to: format!("symbol:{}", to.0),
                kind: "resolvedStaticCall".into(),
                claim_class: "codeObservation".into(),
            })
        })
        .filter(|e| ids.contains(&e.from) && ids.contains(&e.to))
        .collect();
    let expression = serde_json::to_string(&req.expression)
        .map_err(|_| Error::new(500, "SERIALIZE", "Cannot serialize query"))?;
    let mut answer=Answer{id:String::new(),repository_id:repo.id.clone(),revision_id:rev.id.clone(),question:format!("Compound query: {expression}"),interpretation:"Explicit set expression; true/false/unknown are distinct. Your chosen expression is not reinterpreted.".into(),intent:Intent::Search,summary:format!("{matched} matched, {unknown} unknown and {excluded} excluded static symbols; {} shown. Unknown candidates are {}.",items.len(),if req.include_unknown{"included"}else{"withheld by your explicit filter"}),status:if matched+unknown==0{"unknown"}else{"partial"}.into(),gaps:vec!["Missing analyzer effects/tags or test reachability remain unknown, including under NOT. A missing record does not prove runtime absence.".into(),"Changed means supported file text differs between retained snapshots; it does not establish behavioral change.".into(),"Display capped at 500 items; counts describe the whole indexed symbol population, not the whole live system.".into()],items,edges,preferred_view:"table".into(),parent_answer:None,request_id:None,request_fingerprint:None,created_ns:now()};
    app.storage.transaction(|s| {
        if s.answers.len() >= 1000 {
            return Err(Error::new(409, "ANSWER_QUOTA", "Answer limit reached"));
        }
        if !s.repositories.iter().any(|r| {
            r.id == req.repository_id && r.revisions.iter().any(|v| v.id == req.revision_id)
        }) {
            return Err(Error::new(
                410,
                "EVIDENCE_GONE",
                "Exact snapshot disappeared before saving",
            ));
        }
        answer.id = s.id("answer");
        s.answers.push(answer.clone());
        s.workspaces[0].answer_ids.push(answer.id.clone());
        s.workspaces[0].active_answer = Some(answer.id.clone());
        Ok(())
    })?;
    let mut result = serde_json::to_value(answers::render(&app.storage.read()?, &answer.id)?)
        .map_err(|_| Error::new(500, "SERIALIZE", "Cannot render query"))?;
    result["population"] = json!({"matched":matched,"unknown":unknown,"excluded":excluded,"shown":answer.items.len(),"expression":req.expression,"unknownIncluded":req.include_unknown});
    Ok(result)
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    if method != "POST" || path != "/api/queries" {
        return None;
    }
    Some(
        serde_json::from_slice(body)
            .map_err(|_| {
                Error::new(
                    400,
                    "INVALID_QUERY",
                    "Query does not match its expression schema",
                )
            })
            .and_then(|q| execute(app, q)),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kleene_logic_never_promotes_missing_evidence_through_negation() {
        assert_eq!(
            combine(&[Truth::True, Truth::Unknown], true),
            Truth::Unknown
        );
        assert_eq!(combine(&[Truth::False, Truth::Unknown], true), Truth::False);
        assert_eq!(
            combine(&[Truth::False, Truth::Unknown], false),
            Truth::Unknown
        );
        assert_eq!(combine(&[Truth::True, Truth::Unknown], false), Truth::True);
    }
}
