//! Evidence-preserving inspection of retained parser facts. These records are
//! source observations, not invented business rules or confirmed runtime paths.
use crate::{answers, api::App, model::*};
use cie_core::types::SymbolId;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
fn safe_value(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(answers::secret_safe(&s)),
        Value::Array(a) => Value::Array(a.into_iter().map(safe_value).collect()),
        Value::Object(a) => Value::Object(a.into_iter().map(|(k, v)| (k, safe_value(v))).collect()),
        other => other,
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Query {
    repository_id: String,
    revision_id: String,
    kind: String,
    symbol_id: Option<String>,
}
fn execute(app: &Arc<App>, req: Query) -> Result<Value> {
    if ![
        "guards",
        "loops",
        "calls",
        "returns",
        "assignments",
        "exceptions",
        "sources",
        "sinks",
        "imports",
        "async",
        "resources",
    ]
    .contains(&req.kind.as_str())
    {
        return Err(Error::new(
            400,
            "FACT_KIND",
            "Choose a supported parser fact family",
        ));
    }
    let state = app.storage.read()?;
    let repo = state
        .repositories
        .iter()
        .find(|r| r.id == req.repository_id)
        .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
    let rev = repo
        .revisions
        .iter()
        .find(|v| v.id == req.revision_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Exact source snapshot unavailable"))?;
    let selected = req
        .symbol_id
        .as_ref()
        .map(|s| {
            s.parse::<u32>().map(SymbolId).map_err(|_| {
                Error::new(
                    400,
                    "INVALID_SYMBOL",
                    "Symbol ID must be a snapshot-local numeric ID",
                )
            })
        })
        .transpose()?;
    if selected.is_some_and(|id| !rev.snapshot.symbols.iter().any(|s| s.id == id)) {
        return Err(Error::new(
            400,
            "UNKNOWN_SYMBOL",
            "Symbol does not belong to this exact snapshot",
        ));
    }
    let mut items = vec![];
    let mut matched = 0;
    let mut unmapped = 0;
    for (file_index, facts) in rev.snapshot.facts.iter().enumerate() {
        let mut add = |index: usize, sym: SymbolId, line: u32, fact: Value, label: String| {
            if selected.is_some_and(|id| id != sym) {
                return;
            }
            matched += 1;
            if items.len() >= 500 {
                return;
            }
            let symbol = rev.snapshot.symbols.iter().find(|s| s.id == sym);
            let mapped = symbol.is_some_and(|symbol| {
                rev.snapshot
                    .files
                    .get(symbol.loc.file.0 as usize)
                    .is_some_and(|p| p == &facts.path)
            }) && line > 0
                && rev
                    .sources
                    .get(&facts.path)
                    .is_some_and(|source| line as usize <= source.lines().count().max(1));
            let citation = if mapped {
                symbol.and_then(|s| answers::citation(repo, rev, s, Some(line)))
            } else {
                None
            };
            if citation.is_none() {
                unmapped += 1;
            }
            items.push(Item{id:format!("ir:{}:{file_index}:{index}",req.kind),label:answers::secret_safe(&label),kind:format!("parser:{}",req.kind),claim_class:if !mapped || ["sources","sinks","resources","async"].contains(&req.kind.as_str()) {"analyzerInference"} else {"codeObservation"}.into(),citations:citation.into_iter().collect(),detail:json!({"symbolId":sym.0.to_string(),"file":facts.path,"line":line,"language":format!("{:?}",facts.lang),"analyzer":facts.analyzer,"fact":safe_value(fact),"displayRedactionPossible":true,"scope":"A parser observation of supported source syntax. Not an approved policy, complete control/data-flow path, or observed runtime effect."})});
        };
        match req.kind.as_str() {
            "guards" => {
                for (index, f) in facts.ifs.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · guard {}", facts.path, f.line, f.cond),
                    );
                }
            }
            "loops" => {
                for (index, f) in facts.loops.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · {:?} loop", facts.path, f.line, f.kind),
                    );
                }
            }
            "calls" => {
                for (index, f) in facts.calls.iter().enumerate() {
                    add(
                        index,
                        f.caller,
                        f.line,
                        json!(f),
                        format!("{}:{} · call {}", facts.path, f.line, f.callee_text),
                    );
                }
            }
            "returns" => {
                for (index, f) in facts.returns.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · return {}", facts.path, f.line, f.expr),
                    );
                }
            }
            "assignments" => {
                for (index, f) in facts.assignments.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · assignment {}", facts.path, f.line, f.var),
                    );
                }
            }
            "exceptions" => {
                for (index, f) in facts.exceptions.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · exception {}", facts.path, f.line, f.kind_text),
                    );
                }
            }
            "sources" => {
                for (index, f) in facts.sources.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · input {:?}", facts.path, f.line, f.kind),
                    );
                }
            }
            "sinks" => {
                for (index, f) in facts.sinks.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · sink {:?}", facts.path, f.line, f.kind),
                    );
                }
            }
            "imports" => {
                for (index, f) in facts.imports.iter().enumerate() {
                    add(
                        index,
                        f.module_symbol,
                        0,
                        json!(f),
                        format!("{} · import", facts.path),
                    );
                }
            }
            "async" => {
                for (index, f) in facts.async_facts.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · async site", facts.path, f.line),
                    );
                }
            }
            "resources" => {
                for (index, f) in facts.resource_effects.iter().enumerate() {
                    add(
                        index,
                        f.symbol,
                        f.line,
                        json!(f),
                        format!("{}:{} · resource {}", facts.path, f.line, f.kind_text),
                    );
                }
            }
            _ => {}
        }
    }
    let mut gaps=vec!["Parser facts cover supported constructs only; unsupported/dynamic constructs remain gaps.".into(),"Source order is not runtime chronology. Source/sink sites do not establish a complete taint or data-flow path.".into(),"Graph relationships are not synthesized for this record population; the source table and outline retain the same facts.".into()];
    if rev.snapshot.facts.is_empty() {
        gaps.push("This retained snapshot has no detailed IR. Reindex explicitly; facts were not reconstructed from names or summaries.".into());
    }
    if matched > 500 {
        gaps.push(format!(
            "Matched {matched} parser records; display capped at 500."
        ));
    }
    if unmapped > 0 {
        gaps.push(format!(
            "{unmapped} displayed records have no usable source line; no position was invented."
        ));
    }
    let mut answer=Answer{id:String::new(),repository_id:repo.id.clone(),revision_id:rev.id.clone(),question:format!("Source facts: {}{}",req.kind,req.symbol_id.as_ref().map(|id|format!(" for symbol {id}")).unwrap_or_default()),interpretation:format!("Explicit {} parser records from the selected source snapshot",req.kind),intent:Intent::Search,summary:format!("{matched} matched parser records; {} displayed. These are syntax observations and analyzer classifications, not approved business meaning or runtime proof.",items.len()),status:if items.is_empty(){"unknown"}else{"partial"}.into(),gaps,items,edges:vec![],preferred_view:"table".into(),parent_answer:None,request_id:None,request_fingerprint:None,created_ns:now()};
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
                "Exact source snapshot disappeared before saving",
            ));
        }
        answer.id = s.id("answer");
        s.answers.push(answer.clone());
        s.workspaces[0].answer_ids.push(answer.id.clone());
        s.workspaces[0].active_answer = Some(answer.id.clone());
        Ok(())
    })?;
    Ok(json!(answers::render(&app.storage.read()?, &answer.id)?))
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    if method != "POST" || path != "/api/source-facts" {
        return None;
    }
    Some(
        serde_json::from_slice(body)
            .map_err(|_| Error::new(400, "FACT_QUERY", "Parser-fact query schema is invalid"))
            .and_then(|req| execute(app, req)),
    )
}
