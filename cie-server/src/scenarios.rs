//! Finite, user-declared transition graphs. These are conditional model analyses,
//! never runtime experiments, code proofs or quantitative capacity predictions.
use crate::{api::App, model::*, repository};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|_| {
        Error::new(
            400,
            "INVALID_SCENARIO",
            "Scenario does not match its declared schema",
        )
    })
}
fn text(s: &str, max: usize) -> Result<()> {
    if s.trim().is_empty() || s.len() > max || s.contains('\0') {
        return Err(Error::new(
            400,
            "MODEL_TEXT",
            "Model text is empty or exceeds limits",
        ));
    }
    Ok(())
}
fn model<'a>(s: &'a State, id: &str) -> Result<&'a WorkflowModel> {
    s.workflow_models
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::new(404, "MODEL_UNAVAILABLE", "Workflow model unavailable"))
}
fn render(s: &State, m: &WorkflowModel, run: Option<&ScenarioRun>) -> Result<Value> {
    let repo = s
        .repositories
        .iter()
        .find(|r| r.id == m.repository_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Repository unavailable"))?;
    let rev = repo
        .revisions
        .iter()
        .find(|v| v.id == m.revision_id)
        .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Model source snapshot unavailable"))?;
    let challenged = m.evidence_ids.iter().any(|id| {
        if let Some(a) = s.answers.iter().find(|a| a.id == *id) {
            crate::answers::render(s, &a.id)
                .map(|a| a.challenged)
                .unwrap_or(true)
        } else {
            !s.observations.iter().any(|o| o.id == *id)
        }
    });
    let items:Vec<_>=m.states.iter().map(|id|json!({"id":id,"label":id,"kind":"declaredState","claimClass":"modelAssumption","citations":[]})).collect();
    let edges:Vec<_>=m.transitions.iter().map(|t|json!({"from":t.from,"to":t.to,"kind":t.event,"claimClass":"modelAssumption","transitionId":t.id,"guardStatus":t.guard_status})).collect();
    Ok(
        json!({"model":m,"run":run,"items":items,"edges":edges,"freshness":repository::freshness(repo,rev),"freshnessScope":repository::FRESHNESS_SCOPE,"challenged":challenged,"executionPerformed":false,"claimClass":"conditionalModelResult","gaps":["States, events and guards were declared by the user, not automatically verified against implementation or production.","Unknown guards are treated as possibly enabled; witnesses may depend on assumptions that are false at runtime.","Transition disabling is a counterfactual graph operation. No provider call, scheduler action, ledger mutation or external effect occurs.","No liveness, fairness, capacity, throughput, latency or real-world recovery guarantee follows from this finite reachability analysis.","No path in the declared graph is a conditional graph result, not proof that no implementation path exists."]}),
    )
}
fn explore(
    m: &WorkflowModel,
    target: &str,
    max_steps: u32,
    disabled: &[String],
) -> Result<ScenarioRun> {
    if !m.states.iter().any(|s| s == target)
        || max_steps == 0
        || max_steps > 128
        || disabled.len() > 256
        || disabled
            .iter()
            .any(|id| !m.transitions.iter().any(|t| &t.id == id))
    {
        return Err(Error::new(
            400,
            "SCENARIO_SCOPE",
            "Choose a declared target, 1–128 steps and existing transition IDs",
        ));
    }
    let mut visited = BTreeSet::from([m.initial_state.clone()]);
    let mut queue = VecDeque::from([(m.initial_state.clone(), 0)]);
    let mut parent: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut found = false;
    let mut cut = false;
    while let Some((state, depth)) = queue.pop_front() {
        if state == target {
            found = true;
            break;
        }
        for t in m.transitions.iter().filter(|t| {
            t.from == state && t.guard_status != "disabled" && !disabled.contains(&t.id)
        }) {
            if visited.contains(&t.to) {
                continue;
            }
            if depth >= max_steps {
                cut = true;
                continue;
            }
            visited.insert(t.to.clone());
            parent.insert(t.to.clone(), (state.clone(), t.id.clone()));
            queue.push_back((t.to.clone(), depth + 1));
        }
    }
    let mut witness = vec![];
    if found {
        let mut state = target.to_string();
        while state != m.initial_state {
            let (previous, transition) = parent.get(&state).ok_or_else(|| {
                Error::new(
                    500,
                    "MODEL_TRACE",
                    "Cannot reconstruct declared-model witness",
                )
            })?;
            witness.push(transition.clone());
            state = previous.clone();
        }
        witness.reverse();
    }
    let unknown_guards = witness
        .iter()
        .filter(|id| {
            m.transitions
                .iter()
                .any(|t| &t.id == *id && t.guard_status == "unknown")
        })
        .cloned()
        .collect();
    Ok(ScenarioRun {
        id: String::new(),
        model_id: m.id.clone(),
        target_state: target.into(),
        max_steps,
        disabled_transitions: disabled.to_vec(),
        classification: if found {
            "conditionalWitness"
        } else if cut {
            "notFoundWithinBound"
        } else {
            "notReachableInDeclaredGraph"
        }
        .into(),
        witness,
        reachable_states: visited.into_iter().collect(),
        unknown_guards,
        created_ns: now(),
    })
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    if !parts
        .get(1)
        .is_some_and(|s| ["workflow-models", "scenario-runs"].contains(s))
    {
        return None;
    }
    Some((|| match (method, parts.as_slice()) {
        ("GET", ["api", "workflow-models"]) => Ok(json!(app.storage.read()?.workflow_models)),
        ("POST", ["api", "workflow-models"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct New {
                repository_id: String,
                revision_id: String,
                name: String,
                states: Vec<String>,
                initial_state: String,
                transitions: Vec<ModelTransition>,
                evidence_ids: Vec<String>,
            }
            let req: New = parse(body)?;
            text(&req.name, 120)?;
            if req.states.is_empty()
                || req.states.len() > 64
                || req.transitions.len() > 256
                || req.evidence_ids.len() > 50
            {
                return Err(Error::new(
                    400,
                    "MODEL_LIMIT",
                    "Use 1–64 states, at most 256 transitions and 50 evidence references",
                ));
            }
            let mut unique = BTreeSet::new();
            for state in &req.states {
                text(state, 120)?;
                if !unique.insert(state) {
                    return Err(Error::new(
                        400,
                        "DUPLICATE_STATE",
                        "State names must be unique",
                    ));
                }
            }
            if !unique.contains(&req.initial_state) {
                return Err(Error::new(
                    400,
                    "INITIAL_STATE",
                    "Initial state must be declared",
                ));
            }
            let mut transitions = BTreeSet::new();
            for t in &req.transitions {
                text(&t.id, 120)?;
                text(&t.event, 500)?;
                if t.guard_note.len() > 2000
                    || !transitions.insert(&t.id)
                    || !unique.contains(&t.from)
                    || !unique.contains(&t.to)
                    || !["enabled", "disabled", "unknown"].contains(&t.guard_status.as_str())
                {
                    return Err(Error::new(400,"INVALID_TRANSITION","Use unique transition IDs, declared endpoints and enabled/disabled/unknown guards"));
                }
            }
            let m = app.storage.transaction(|s| {
                if s.workflow_models.len() >= 100 {
                    return Err(Error::new(409, "MODEL_QUOTA", "Model limit reached"));
                }
                let repo = s
                    .repositories
                    .iter()
                    .find(|r| r.id == req.repository_id)
                    .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
                if !repo.revisions.iter().any(|v| v.id == req.revision_id) {
                    return Err(Error::new(
                        410,
                        "EVIDENCE_GONE",
                        "Source snapshot unavailable",
                    ));
                }
                if req.evidence_ids.iter().any(|id| {
                    !s.answers.iter().any(|a| {
                        a.id == *id
                            && a.repository_id == req.repository_id
                            && a.revision_id == req.revision_id
                    }) && !s.observations.iter().any(|o| {
                        o.id == *id
                            && o.repository_id == req.repository_id
                            && o.revision_id == req.revision_id
                    })
                }) {
                    return Err(Error::new(
                        400,
                        "FOREIGN_EVIDENCE",
                        "Evidence references must match this exact repository and snapshot",
                    ));
                }
                let m = WorkflowModel {
                    id: s.id("model"),
                    repository_id: req.repository_id,
                    revision_id: req.revision_id,
                    name: req.name,
                    states: req.states,
                    initial_state: req.initial_state,
                    transitions: req.transitions,
                    evidence_ids: req.evidence_ids,
                    created_ns: now(),
                };
                s.workflow_models.push(m.clone());
                Ok(m)
            })?;
            render(&app.storage.read()?, &m, None)
        }
        ("GET", ["api", "workflow-models", id]) => {
            let s = app.storage.read()?;
            render(&s, model(&s, id)?, None)
        }
        ("POST", ["api", "workflow-models", id, "explore"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Run {
                target_state: String,
                max_steps: u32,
                disabled_transitions: Vec<String>,
            }
            let req: Run = parse(body)?;
            let run = app.storage.transaction(|s| {
                if s.scenario_runs.len() >= 500 {
                    return Err(Error::new(409, "SCENARIO_QUOTA", "Scenario limit reached"));
                }
                let mut r = explore(
                    model(s, id)?,
                    &req.target_state,
                    req.max_steps,
                    &req.disabled_transitions,
                )?;
                r.id = s.id("scenario");
                s.scenario_runs.push(r.clone());
                Ok(r)
            })?;
            let s = app.storage.read()?;
            render(&s, model(&s, id)?, Some(&run))
        }
        ("GET", ["api", "scenario-runs"]) => Ok(json!(app.storage.read()?.scenario_runs)),
        ("GET", ["api", "scenario-runs", id]) => {
            let s = app.storage.read()?;
            let r = s
                .scenario_runs
                .iter()
                .find(|r| r.id == *id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Scenario unavailable"))?;
            render(&s, model(&s, &r.model_id)?, Some(r))
        }
        ("DELETE", ["api", "workflow-models", id]) => {
            app.storage.transaction(|s| {
                if !s.workflow_models.iter().any(|m| m.id == *id) {
                    return Err(Error::new(404, "NOT_FOUND", "Model unavailable"));
                }
                s.workflow_models.retain(|m| m.id != *id);
                s.scenario_runs.retain(|r| r.model_id != *id);
                Ok(())
            })?;
            Ok(json!({"deleted":true}))
        }
        _ => Err(Error::new(
            404,
            "NOT_FOUND",
            "Scenario operation unavailable",
        )),
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reachability_matches_independent_transitive_closure_oracle() {
        for seed in 0..20 {
            let states: Vec<_> = (0..6).map(|i| format!("s{i}")).collect();
            let mut transitions = vec![];
            let mut oracle = [[false; 6]; 6];
            for i in 0..6 {
                oracle[i][i] = true;
                for j in 0..6 {
                    let value = (seed * 31 + i * 17 + j * 13) % 7;
                    if value < 3 {
                        let enabled = value != 0;
                        transitions.push(ModelTransition {
                            id: format!("t{i}_{j}"),
                            from: states[i].clone(),
                            to: states[j].clone(),
                            event: "declared event".into(),
                            guard_status: if enabled { "unknown" } else { "disabled" }.into(),
                            guard_note: "No runtime assertion".into(),
                        });
                        oracle[i][j] |= enabled;
                    }
                }
            }
            // Floyd-Warshall is deliberately independent from the BFS under test.
            for k in 0..6 {
                for i in 0..6 {
                    for j in 0..6 {
                        oracle[i][j] |= oracle[i][k] && oracle[k][j];
                    }
                }
            }
            let m = WorkflowModel {
                id: "m".into(),
                repository_id: "r".into(),
                revision_id: "v".into(),
                name: "oracle fixture".into(),
                states: states.clone(),
                initial_state: states[0].clone(),
                transitions,
                evidence_ids: vec![],
                created_ns: 0,
            };
            for target in 0..6 {
                let result = explore(&m, &states[target], 128, &[]).unwrap();
                assert_eq!(
                    result.classification == "conditionalWitness",
                    oracle[0][target],
                    "seed {seed}, target {target}"
                );
                let mut cursor = m.initial_state.clone();
                for id in &result.witness {
                    let t = m.transitions.iter().find(|t| &t.id == id).unwrap();
                    assert_eq!(t.from, cursor);
                    assert_ne!(t.guard_status, "disabled");
                    cursor = t.to.clone();
                }
                if oracle[0][target] {
                    assert_eq!(cursor, states[target]);
                }
            }
        }
    }
}
