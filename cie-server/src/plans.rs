//! Explicit bounded read-only analysis plans. Each completed step is durable;
//! failures never discard earlier answers or imply a successful dependent step.
use crate::{answers, api::App, model::*, repository};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Instant};
fn missing() -> Error {
    Error::new(404, "PLAN_UNAVAILABLE", "Analysis plan is unavailable")
}
fn render(s: &State, id: &str) -> Result<Value> {
    let p = s.plans.iter().find(|p| p.id == id).ok_or_else(missing)?;
    let repo = s
        .repositories
        .iter()
        .find(|r| r.id == p.repository_id)
        .ok_or_else(missing)?;
    let rev = repo
        .revisions
        .iter()
        .find(|v| v.id == p.revision_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Plan source snapshot unavailable"))?;
    let mut missing_count = 0;
    let results: Vec<_> = p
        .steps
        .iter()
        .map(|step| match &step.answer_id {
            Some(id) => match answers::render(s, id) {
                Ok(r) => json!({"answer":r}),
                Err(_) => {
                    missing_count += 1;
                    json!({"error":"Exact saved answer unavailable; no substitute was shown"})
                }
            },
            None => json!({"status":step.status,"issue":step.issue}),
        })
        .collect();
    Ok(
        json!({"plan":p,"results":results,"freshness":repository::freshness(repo,rev),"missingAnswers":missing_count,"gaps":["This is an explicit deterministic read-only analysis sequence, not an autonomous plan inferred from arbitrary intent.","Completed steps are saved independently. Cancellation stops future steps and does not undo saved answers.","Static answers and test reachability do not prove runtime behavior."]}),
    )
}
fn execute(app: &Arc<App>, id: &str) -> Result<Value> {
    let start = Instant::now();
    let proceed = app.storage.transaction(|s| {
        let p = s
            .plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(missing)?;
        if p.status == "cancelling" {
            p.status = "cancelled".into();
            return Ok(false);
        }
        if p.status == "running" {
            return Err(Error::new(409, "PLAN_RUNNING", "Plan already running"));
        }
        p.status = "running".into();
        Ok(true)
    })?;
    if !proceed {
        return render(&app.storage.read()?, id);
    }
    loop {
        let state = app.storage.read()?;
        let p = state
            .plans
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(missing)?
            .clone();
        if p.status == "cancelling" {
            app.storage.transaction(|s| {
                s.plans
                    .iter_mut()
                    .find(|p| p.id == id)
                    .ok_or_else(missing)?
                    .status = "cancelled".into();
                Ok(())
            })?;
            break;
        }
        let Some(index) = p
            .steps
            .iter()
            .position(|step| ["queued", "interrupted"].contains(&step.status.as_str()))
        else {
            break;
        };
        if start.elapsed().as_secs() >= 10 {
            app.storage.transaction(|s| {
                s.plans
                    .iter_mut()
                    .find(|p| p.id == id)
                    .ok_or_else(missing)?
                    .status = "interrupted".into();
                Ok(())
            })?;
            return render(&app.storage.read()?, id);
        }
        let step = &p.steps[index];
        let attempt = (|| -> Result<Value> {
            let repo = state
                .repositories
                .iter()
                .find(|r| r.id == p.repository_id)
                .ok_or_else(missing)?;
            if repo.revisions.last().is_none_or(|v| v.id != p.revision_id) {
                return Err(Error::new(
                    409,
                    "REVISION_CHANGED",
                    "Evidence changed; remaining step was not run",
                ));
            }
            let mut symbol = step.symbol_id.clone();
            if let Some(previous) = step.from_step {
                let aid = p
                    .steps
                    .get(previous)
                    .and_then(|step| step.answer_id.as_ref())
                    .ok_or_else(|| {
                        Error::new(
                            409,
                            "DEPENDENCY_FAILED",
                            "Required earlier step has no saved answer",
                        )
                    })?;
                let prior = answers::render(&state, aid)?;
                if prior.challenged || prior.answer.items.len() != 1 {
                    return Err(Error::new(400,"AMBIGUOUS_DEPENDENCY","Earlier step must have exactly one unchallenged symbol; no target was guessed"));
                }
                symbol = Some(
                    prior.answer.items[0]
                        .id
                        .strip_prefix("symbol:")
                        .ok_or_else(|| {
                            Error::new(
                                400,
                                "NOT_A_SYMBOL",
                                "Earlier step selected an interpreted aggregate",
                            )
                        })?
                        .to_string(),
                );
            }
            app.dispatch("POST","/api/questions",&serde_json::to_vec(&json!({"repositoryId":p.repository_id,"question":step.question,"intent":step.intent,"symbolId":symbol,"expectedRevision":p.revision_id,"requestId":format!("{}:step:{index}",p.id)})).map_err(|_|Error::new(500,"SERIALIZE","Cannot build analysis step"))?)
        })();
        app.storage.transaction(|s| {
            let p = s
                .plans
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(missing)?;
            let step = &mut p.steps[index];
            match attempt {
                Ok(result) => {
                    step.answer_id = result["answer"]["id"].as_str().map(str::to_string);
                    step.status = result["answer"]["status"]
                        .as_str()
                        .unwrap_or("partial")
                        .into();
                    step.issue = None;
                }
                Err(error) => {
                    step.status = "failed".into();
                    step.issue = Some(format!("{}: {}", error.code, error.message));
                }
            }
            Ok(())
        })?;
    }
    app.storage.transaction(|s| {
        let p = s
            .plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(missing)?;
        if p.status != "cancelled" {
            p.status = if p.steps.iter().any(|s| s.status != "ready") {
                "partial"
            } else {
                "ready"
            }
            .into();
        }
        Ok(())
    })?;
    render(&app.storage.read()?, id)
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    if parts.get(1) != Some(&"plans") {
        return None;
    }
    Some((|| match (method, parts.as_slice()) {
        ("GET", ["api", "plans"]) => Ok(json!(app.storage.read()?.plans)),
        ("POST", ["api", "plans"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Step {
                question: String,
                intent: Option<Intent>,
                symbol_id: Option<String>,
                from_step: Option<usize>,
            }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct New {
                repository_id: String,
                expected_revision: Option<String>,
                steps: Vec<Step>,
            }
            let req: New = serde_json::from_slice(body)
                .map_err(|_| Error::new(400, "INVALID_PLAN", "Plan schema is invalid"))?;
            if req.steps.is_empty() || req.steps.len() > 6 {
                return Err(Error::new(
                    400,
                    "PLAN_LIMIT",
                    "Provide 1–6 read-only analysis steps",
                ));
            }
            for (index, step) in req.steps.iter().enumerate() {
                if step.question.trim().is_empty()
                    || step.question.len() > 4000
                    || step.from_step.is_some_and(|i| i >= index)
                {
                    return Err(Error::new(
                        400,
                        "INVALID_STEP",
                        "Each step needs a bounded question and can reference only an earlier step",
                    ));
                }
            }
            let p = app.storage.transaction(|s| {
                if s.plans.len() >= 100 {
                    return Err(Error::new(409, "PLAN_QUOTA", "Plan limit reached"));
                }
                let repo = s
                    .repositories
                    .iter()
                    .find(|r| r.id == req.repository_id)
                    .ok_or_else(missing)?;
                let revision_id = repo
                    .revisions
                    .last()
                    .ok_or_else(|| Error::new(409, "NOT_INDEXED", "Index repository first"))?
                    .id
                    .clone();
                if req.expected_revision.as_ref().is_some_and(|id|*id!=revision_id){return Err(Error::new(409,"REVISION_CHANGED","Selected plan snapshot is no longer latest. Choose the current snapshot explicitly."));}
                let p = AnalysisPlan {
                    id: s.id("plan"),
                    repository_id: req.repository_id,
                    revision_id,
                    status: "queued".into(),
                    steps: req
                        .steps
                        .into_iter()
                        .map(|r| AnalysisStep {
                            question: r.question,
                            intent: r.intent,
                            symbol_id: r.symbol_id,
                            from_step: r.from_step,
                            status: "queued".into(),
                            answer_id: None,
                            issue: None,
                        })
                        .collect(),
                    created_ns: now(),
                };
                s.plans.push(p.clone());
                Ok(p)
            })?;
            execute(app, &p.id)
        }
        ("GET", ["api", "plans", id]) => render(&app.storage.read()?, id),
        ("POST", ["api", "plans", id, "cancel"]) => {
            app.storage.transaction(|s| {
                let p = s
                    .plans
                    .iter_mut()
                    .find(|p| p.id == *id)
                    .ok_or_else(missing)?;
                if ["running", "queued"].contains(&p.status.as_str()) {
                    p.status = "cancelling".into();
                }
                Ok(())
            })?;
            render(&app.storage.read()?, id)
        }
        ("POST", ["api", "plans", id, "resume"]) => {
            let s = app.storage.read()?;
            let p = s.plans.iter().find(|p| p.id == *id).ok_or_else(missing)?;
            if !["interrupted", "cancelled"].contains(&p.status.as_str()) {
                return Err(Error::new(
                    409,
                    "NOT_RESUMABLE",
                    "Only interrupted or cancelled plans have unfinished steps to resume",
                ));
            }
            let repo = s
                .repositories
                .iter()
                .find(|r| r.id == p.repository_id)
                .ok_or_else(missing)?;
            let rev = repo
                .revisions
                .iter()
                .find(|v| v.id == p.revision_id)
                .ok_or_else(missing)?;
            if repository::freshness(repo, rev) != "current" {
                return Err(Error::new(409,"STALE_PLAN","Sources are stale, historical or unavailable. Start a new plan; old results remain retained."));
            }
            execute(app, id)
        }
        _ => Err(missing()),
    })())
}
