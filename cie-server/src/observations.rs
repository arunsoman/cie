//! Imported observations are caller-supplied evidence, never evidence that CIE
//! executed a workload. Mapping requires the exact retained revision/source hash.
use crate::{api::App, model::*, repository};
use cie_core::types::{content_hash, hash_hex};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|_| {
        Error::new(
            400,
            "INVALID_EVIDENCE",
            "Observation request does not match its schema",
        )
    })
}
fn text(s: &str, max: usize) -> Result<()> {
    if s.trim().is_empty() || s.len() > max || s.contains('\0') {
        return Err(Error::new(
            400,
            "INVALID_EVIDENCE",
            "Evidence text is empty or exceeds limits",
        ));
    }
    Ok(())
}
fn revision<'a>(s: &'a State, repo: &str, rev: &str) -> Result<(&'a Repository, &'a Revision)> {
    let r = s
        .repositories
        .iter()
        .find(|r| r.id == repo)
        .ok_or_else(|| Error::new(404, "EVIDENCE_GONE", "Repository is unavailable"))?;
    let v =
        r.revisions.iter().find(|v| v.id == rev).ok_or_else(|| {
            Error::new(410, "EVIDENCE_GONE", "Exact source snapshot is unavailable")
        })?;
    Ok((r, v))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Import {
    repository_id: String,
    revision_id: String,
    environment: String,
    producer: String,
    kind: String,
    events: Vec<ObservedEvent>,
}
fn validate_events(req: &Import, rev: &Revision) -> Result<()> {
    if !["trace", "profile", "tests"].contains(&req.kind.as_str())
        || req.events.is_empty()
        || req.events.len() > 2000
    {
        return Err(Error::new(
            400,
            "INVALID_OBSERVATIONS",
            "Use trace, profile or tests with 1–2000 records",
        ));
    }
    text(&req.environment, 120)?;
    text(&req.producer, 120)?;
    let mut ids = BTreeMap::new();
    for e in &req.events {
        text(&e.id, 120)?;
        text(&e.name, 500)?;
        if ids.insert(e.id.as_str(), e).is_some() {
            return Err(Error::new(
                400,
                "DUPLICATE_EVENT",
                "Event IDs must be unique within the report",
            ));
        }
        if e.end_micros < e.start_micros
            || e.end_micros > 9_007_199_254_740_991
            || !["ok", "error", "unknown", "passed", "failed", "skipped"]
                .contains(&e.outcome.as_str())
        {
            return Err(Error::new(
                400,
                "INVALID_EVENT",
                "Invalid event interval or outcome",
            ));
        }
        if req.kind == "profile" && e.samples.is_none() {
            return Err(Error::new(
                400,
                "MISSING_SAMPLES",
                "Profile records require observed sample counts",
            ));
        }
        match (&e.file,e.line,&e.source_hash) {
            (None,None,None)=>{},
            (Some(file),Some(line),Some(hash))=>{
                let source=rev.sources.get(file).ok_or_else(||Error::new(400,"UNMAPPED_SOURCE","Source path is not in this exact snapshot"))?;
                if line==0 || line as usize>source.lines().count().max(1) || *hash!=hash_hex(&content_hash(source.as_bytes())) {return Err(Error::new(409,"SOURCE_MISMATCH","Source hash or line differs from the retained snapshot; no name-based mapping was attempted"));}
            },
            _=>return Err(Error::new(400,"INCOMPLETE_MAPPING","Provide file, line and sourceHash together, or omit all three for an unmapped observation")),
        }
    }
    for e in &req.events {
        let mut seen = BTreeSet::new();
        let mut cursor = Some(e.id.as_str());
        while let Some(id) = cursor {
            if !seen.insert(id) {
                return Err(Error::new(400, "CYCLIC_TRACE", "Parent links form a cycle"));
            }
            let event = ids.get(id).ok_or_else(|| {
                Error::new(
                    400,
                    "MISSING_PARENT",
                    "Parent ID is not present in this report",
                )
            })?;
            cursor = event.parent_id.as_deref();
        }
    }
    Ok(())
}
fn render_observation(s: &State, o: &Observation) -> Result<Value> {
    let (repo, rev) = revision(s, &o.repository_id, &o.revision_id)?;
    let mapped = o.events.iter().filter(|e| e.file.is_some()).count();
    let items:Vec<_>=o.events.iter().map(|e|{
        let citation=e.file.as_ref().and_then(|file|rev.sources.get(file).map(|source|json!({"repositoryId":repo.id,"revisionId":rev.id,"file":file,"startLine":e.line,"endLine":e.line,"sourceHash":e.source_hash,"excerpt":crate::answers::secret_safe(source.lines().nth(e.line.unwrap_or(1).saturating_sub(1) as usize).unwrap_or(""))})));
        json!({"id":e.id,"label":e.name,"kind":o.kind,"claimClass":"importedObservation","startMicros":e.start_micros,"endMicros":e.end_micros,"durationMicros":e.end_micros-e.start_micros,"outcome":e.outcome,"samples":e.samples,"mapped":citation.is_some(),"citation":citation})
    }).collect();
    let edges:Vec<_>=o.events.iter().filter_map(|e|e.parent_id.as_ref().map(|p|json!({"from":p,"to":e.id,"kind":"reportedParent","claimClass":"importedObservation"}))).collect();
    Ok(
        json!({"observation":o,"items":items,"edges":edges,"freshness":repository::freshness(repo,rev),"mapped":mapped,"unmapped":o.events.len()-mapped,"provenance":{"class":"importedObservation","producer":o.producer,"executionVerifiedByCie":false},"gaps":["Imported records are supplied by the caller; CIE did not execute or independently attest the producer.","Reported parent links and timing do not establish causal necessity or complete runtime coverage.","Unmapped records remain separate; similar names never establish a source mapping."]}),
    )
}
fn render_investigation(s: &State, i: &Investigation) -> Result<Value> {
    let (repo, rev) = revision(s, &i.repository_id, &i.revision_id)?;
    let references: Vec<_> = i
        .hypotheses
        .iter()
        .flat_map(|h| h.support.iter().chain(h.counter_evidence.iter()))
        .cloned()
        .collect();
    let missing = references
        .iter()
        .filter(|id| {
            !s.answers.iter().any(|a| &a.id == *id) && !s.observations.iter().any(|o| &o.id == *id)
        })
        .count();
    let challenged = missing > 0
        || references.iter().any(|id| {
            s.reviews
                .iter()
                .any(|r| &r.answer_id == id && r.verdict != "confirm")
        });
    Ok(
        json!({"investigation":i,"freshness":repository::freshness(repo,rev),"claimClass":"hypothesis","challenged":challenged,"missingReferences":missing,"gaps":["Hypotheses and conclusions are investigator statements, not runtime proof.","A next step is a proposal; this workspace does not execute it automatically.","Changed, deleted or disputed evidence requires reviewing the hypothesis before resuming."]}),
    )
}
fn valid_refs(s: &State, i: &Investigation, refs: &[String]) -> bool {
    refs.len() <= 50
        && refs.iter().all(|id| {
            s.answers.iter().any(|a| {
                a.id == *id && a.repository_id == i.repository_id && a.revision_id == i.revision_id
            }) || s.observations.iter().any(|o| {
                o.id == *id && o.repository_id == i.repository_id && o.revision_id == i.revision_id
            })
        })
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    if !parts
        .get(1)
        .is_some_and(|p| ["observations", "investigations"].contains(p))
    {
        return None;
    }
    Some((|| match (method, parts.as_slice()) {
        ("GET", ["api", "observations"]) => {
            let s = app.storage.read()?;
            Ok(json!(s.observations.iter().map(|o|json!({"id":o.id,"repositoryId":o.repository_id,"revisionId":o.revision_id,"environment":o.environment,"kind":o.kind,"producer":o.producer,"events":o.events.len()})).collect::<Vec<_>>()))
        }
        ("POST", ["api", "observations"]) => {
            let req: Import = parse(body)?;
            let o = app.storage.transaction(|s| {
                let (_, rev) = revision(s, &req.repository_id, &req.revision_id)?;
                validate_events(&req, rev)?;
                if let Some(existing) = s.observations.iter().find(|o| {
                    o.repository_id == req.repository_id
                        && o.revision_id == req.revision_id
                        && o.environment == req.environment
                        && o.producer == req.producer
                        && o.kind == req.kind
                        && o.events == req.events
                }) {
                    return Ok(existing.clone());
                }
                if s.observations.len() >= 100 {
                    return Err(Error::new(
                        409,
                        "OBSERVATION_QUOTA",
                        "Remove older reports before importing more",
                    ));
                }
                let o = Observation {
                    id: s.id("observation"),
                    repository_id: req.repository_id,
                    revision_id: req.revision_id,
                    environment: req.environment,
                    producer: req.producer,
                    kind: req.kind,
                    events: req.events,
                    created_ns: now(),
                };
                s.observations.push(o.clone());
                Ok(o)
            })?;
            render_observation(&app.storage.read()?, &o)
        }
        ("GET", ["api", "observations", id]) => {
            let s = app.storage.read()?;
            let o = s
                .observations
                .iter()
                .find(|o| o.id == *id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Observation unavailable"))?;
            render_observation(&s, o)
        }
        ("DELETE", ["api", "observations", id]) => {
            app.storage.transaction(|s| {
                if !s.observations.iter().any(|o| o.id == *id) {
                    return Err(Error::new(404, "NOT_FOUND", "Observation unavailable"));
                }
                s.observations.retain(|o| o.id != *id);
                for i in &mut s.investigations {
                    if i.hypotheses.iter().any(|h| {
                        h.support.contains(&id.to_string())
                            || h.counter_evidence.contains(&id.to_string())
                    }) {
                        i.status = "evidenceMissing".into();
                        i.notes.push(
                            "Referenced observation was deleted; conclusions need review.".into(),
                        );
                    }
                }
                Ok(())
            })?;
            Ok(json!({"deleted":true}))
        }
        ("GET", ["api", "investigations"]) => Ok(json!(app.storage.read()?.investigations)),
        ("POST", ["api", "investigations"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct New {
                repository_id: String,
                revision_id: String,
                symptom: String,
            }
            let req: New = parse(body)?;
            text(&req.symptom, 4000)?;
            let i = app.storage.transaction(|s| {
                revision(s, &req.repository_id, &req.revision_id)?;
                if s.investigations.len() >= 100 {
                    return Err(Error::new(
                        409,
                        "INVESTIGATION_QUOTA",
                        "Investigation limit reached",
                    ));
                }
                let i = Investigation {
                    id: s.id("investigation"),
                    repository_id: req.repository_id,
                    revision_id: req.revision_id,
                    symptom: req.symptom,
                    status: "open".into(),
                    hypotheses: vec![],
                    notes: vec![],
                    created_ns: now(),
                };
                s.investigations.push(i.clone());
                Ok(i)
            })?;
            render_investigation(&app.storage.read()?, &i)
        }
        ("GET", ["api", "investigations", id]) => {
            let s = app.storage.read()?;
            let i = s
                .investigations
                .iter()
                .find(|i| i.id == *id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Investigation unavailable"))?;
            render_investigation(&s, i)
        }
        ("POST", ["api", "investigations", id, "hypotheses"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct New {
                statement: String,
                support: Vec<String>,
                counter_evidence: Vec<String>,
                next_step: String,
            }
            let req: New = parse(body)?;
            text(&req.statement, 2000)?;
            text(&req.next_step, 2000)?;
            let i = app.storage.transaction(|s| {
                let pos = s
                    .investigations
                    .iter()
                    .position(|i| i.id == *id)
                    .ok_or_else(|| Error::new(404, "NOT_FOUND", "Investigation unavailable"))?;
                if !valid_refs(s, &s.investigations[pos], &req.support)
                    || !valid_refs(s, &s.investigations[pos], &req.counter_evidence)
                {
                    return Err(Error::new(
                        400,
                        "INVALID_REFERENCE",
                        "Evidence must belong to this exact repository and revision",
                    ));
                }
                if s.investigations[pos].hypotheses.len() >= 30 {
                    return Err(Error::new(
                        409,
                        "HYPOTHESIS_QUOTA",
                        "At most 30 hypotheses per investigation",
                    ));
                }
                let h = Hypothesis {
                    id: s.id("hypothesis"),
                    statement: req.statement,
                    status: "proposed".into(),
                    support: req.support,
                    counter_evidence: req.counter_evidence,
                    next_step: req.next_step,
                };
                s.investigations[pos].hypotheses.push(h);
                Ok(s.investigations[pos].clone())
            })?;
            render_investigation(&app.storage.read()?, &i)
        }
        ("POST", ["api", "investigations", id, "checkpoint"]) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Change {
                status: String,
                note: String,
            }
            let req: Change = parse(body)?;
            text(&req.note, 2000)?;
            if !["open", "paused", "closedUnresolved"].contains(&req.status.as_str()) {
                return Err(Error::new(
                    400,
                    "INVALID_STATUS",
                    "Use open, paused or closedUnresolved; hypotheses are not promoted to proof",
                ));
            }
            let i = app.storage.transaction(|s| {
                let i = s
                    .investigations
                    .iter_mut()
                    .find(|i| i.id == *id)
                    .ok_or_else(|| Error::new(404, "NOT_FOUND", "Investigation unavailable"))?;
                if i.notes.len() >= 100 {
                    return Err(Error::new(
                        409,
                        "NOTE_QUOTA",
                        "Investigation note limit reached",
                    ));
                }
                i.status = req.status;
                i.notes.push(req.note);
                Ok(i.clone())
            })?;
            render_investigation(&app.storage.read()?, &i)
        }
        _ => Err(Error::new(
            404,
            "NOT_FOUND",
            "Unsupported observation operation",
        )),
    })())
}
