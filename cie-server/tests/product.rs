use cie_server::{api::App, model::*, storage::Storage};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    app: Arc<App>,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cie-product-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let root = dir.join("source");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("auth.py"),"def allowed(value):\n    return value\n\ndef authorize(value):\n    return allowed(value)\n\ndef test_authorize():\n    assert authorize(True)\n").unwrap();
        let app = App::open(&dir.join("data"), vec![root.clone()]).unwrap();
        Self { dir, root, app }
    }
    fn call(&self, method: &str, path: &str, body: Value) -> Value {
        self.app
            .dispatch(method, path, &serde_json::to_vec(&body).unwrap())
            .unwrap()
    }
    fn connect(&self) -> String {
        self.call(
            "POST",
            "/api/repositories",
            json!({"path":self.root,"name":"sample"}),
        )["id"]
            .as_str()
            .unwrap()
            .into()
    }
    fn index(&self, id: &str) -> String {
        let j = self.call("POST", &format!("/api/repositories/{id}/index"), json!({}));
        let id = j["id"].as_str().unwrap().to_string();
        wait_job(&self.app, &id);
        let s = self.app.storage.read().unwrap();
        let j = s.jobs.iter().find(|j| j.id == id).unwrap();
        assert_eq!(j.state, "ready", "{:?}", j.issue);
        j.revision_id.clone().unwrap()
    }
    fn ask(&self, repo: &str, q: &str) -> Value {
        self.call(
            "POST",
            "/api/questions",
            json!({"repositoryId":repo,"question":q}),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self
            .app
            .storage
            .read()
            .map(|s| s.jobs.iter().any(|j| j.active()))
            .unwrap_or(false)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn wait_job(app: &Arc<App>, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let s = app.storage.read().unwrap();
        if !s.jobs.iter().find(|j| j.id == id).unwrap().active() {
            break;
        }
        assert!(Instant::now() < deadline, "job timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn index_publishes_immutable_sources_without_modifying_repository() {
    let f = Fixture::new();
    let before = fs::read(f.root.join("auth.py")).unwrap();
    let id = f.connect();
    f.index(&id);
    assert!(!f.root.join(".cie").exists());
    assert_eq!(fs::read(f.root.join("auth.py")).unwrap(), before);
    let s = f.app.storage.read().unwrap();
    let rev = &s.repositories[0].revisions[0];
    assert!(!rev.snapshot.symbols.is_empty());
    assert_eq!(rev.sources["auth.py"], String::from_utf8(before).unwrap());
}
#[test]
fn answers_have_real_citations_and_shared_graph_population() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let a = f.ask(&id, "overview");
    assert_ne!(a["answer"]["status"], "unknown");
    let items = a["answer"]["items"].as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert_eq!(item["claimClass"], "codeObservation");
        for c in item["citations"].as_array().unwrap() {
            assert_eq!(c["file"], "auth.py");
            assert!(!c["sourceHash"].as_str().unwrap().is_empty());
        }
    }
    let ids: Vec<_> = items.iter().map(|i| &i["id"]).collect();
    for e in a["answer"]["edges"].as_array().unwrap() {
        assert!(ids.contains(&&e["from"]) && ids.contains(&&e["to"]));
    }
}
#[test]
fn unknown_question_does_not_return_unrelated_finding_as_answer() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let a = f.ask(&id, "Did a user pay for topup and never receive a refund?");
    assert_eq!(a["answer"]["status"], "unknown");
    assert!(a["answer"]["items"].as_array().unwrap().is_empty());
}
#[test]
fn changed_sources_are_visibly_stale_and_export_keeps_the_same_caveats() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let a = f.ask(&id, "overview");
    let aid = a["answer"]["id"].as_str().unwrap();
    assert_eq!(a["freshness"], "current");
    fs::write(f.root.join("auth.py"), "def changed():\n    return False\n").unwrap();
    let view = f.call("GET", &format!("/api/answers/{aid}"), json!({}));
    assert_eq!(view["freshness"], "stale");
    let export = f.call("GET", &format!("/api/answers/{aid}/export"), json!({}));
    assert_eq!(view, export);
}
#[test]
fn refutation_hides_claim_and_edges_without_upgrading_confirmation() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let a = f.ask(&id, "overview");
    let aid = a["answer"]["id"].as_str().unwrap();
    let item = &a["answer"]["items"][0];
    let review=f.call("POST","/api/reviews",json!({"answerId":aid,"itemId":item["id"],"verdict":"refute","reason":"This interpretation is unsupported"}));
    assert_eq!(review["hiddenClaims"], 1);
    assert!(review["challenged"].as_bool().unwrap());
    assert!(review["answer"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["id"] != item["id"]));
}
#[test]
fn explicit_route_and_symbol_selection_are_honored() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let s = f.app.storage.read().unwrap();
    let rev = &s.repositories[0].revisions[0];
    let symbol = rev
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "allowed")
        .unwrap();
    let a=f.call("POST","/api/questions",json!({"repositoryId":id,"question":"ignore confusing wording","intent":"callers","symbolId":symbol.id.0.to_string()}));
    assert_eq!(a["answer"]["intent"], "callers");
    assert!(a["answer"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["label"].as_str().unwrap().contains("authorize")));
}
#[test]
fn ambiguous_or_absent_target_is_not_guessed() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    assert_eq!(
        f.ask(&id, "callers of imaginary")["answer"]["status"],
        "unknown"
    );
    assert_eq!(f.ask(&id, "callers")["answer"]["status"], "unknown");
}
#[test]
fn static_test_map_never_claims_executed_coverage() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let a = f.ask(&id, "tests for allowed");
    assert!(a["answer"]["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g.as_str().unwrap().contains("not executed")));
}
#[test]
fn missing_index_and_outside_roots_are_distinct_errors() {
    let f = Fixture::new();
    let id = f.connect();
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/questions",
                &serde_json::to_vec(&json!({"repositoryId":id,"question":"overview"})).unwrap()
            )
            .unwrap_err()
            .code,
        "NOT_INDEXED"
    );
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/repositories",
                &serde_json::to_vec(&json!({"path":f.dir})).unwrap()
            )
            .unwrap_err()
            .status,
        403
    );
}
#[test]
fn cancel_queued_job_blocks_publication() {
    let f = Fixture::new();
    let id = f.connect();
    let j = f.call("POST", &format!("/api/repositories/{id}/index"), json!({}));
    let jid = j["id"].as_str().unwrap();
    f.call("POST", &format!("/api/jobs/{jid}/cancel"), json!({}));
    wait_job(&f.app, jid);
    let s = f.app.storage.read().unwrap();
    assert!(s.repositories[0].revisions.is_empty());
    assert_eq!(s.jobs[0].state, "cancelled");
}
#[test]
fn restart_marks_interrupted_jobs_failed_without_automatic_resume() {
    let dir = std::env::temp_dir().join(format!(
        "cie-restart-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let storage = Storage::open(&dir).unwrap();
    storage
        .transaction(|s| {
            s.jobs.push(Job {
                id: "job-1".into(),
                repository_id: "r".into(),
                state: "running".into(),
                stage: "analysis".into(),
                completed_units: 1,
                total_units: 3,
                issue: None,
                revision_id: None,
                created_ns: 0,
            });
            Ok(())
        })
        .unwrap();
    drop(storage);
    let storage = Storage::open(&dir).unwrap();
    let s = storage.read().unwrap();
    assert_eq!(s.jobs[0].state, "failed");
    assert_eq!(s.jobs[0].stage, "interrupted");
    drop(storage);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn storage_rejects_concurrent_owners_and_corruption() {
    let dir = std::env::temp_dir().join(format!(
        "cie-lock-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let storage = Storage::open(&dir).unwrap();
    assert!(Storage::open(&dir).is_err());
    drop(storage);
    fs::write(dir.join("workspace.json"), b"bad").unwrap();
    assert!(Storage::open(&dir).is_err());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn failed_transaction_does_not_change_persisted_or_in_memory_state() {
    let f = Fixture::new();
    let before = f.app.storage.read().unwrap().next_id;
    let result: Result<()> = f.app.storage.transaction(|s| {
        s.next_id += 100;
        Err(Error::new(400, "TEST", "rejected"))
    });
    assert!(result.is_err());
    assert_eq!(f.app.storage.read().unwrap().next_id, before);
}
#[test]
fn deletion_removes_all_derived_views_but_not_connected_source() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    f.ask(&id, "overview");
    f.call("DELETE", &format!("/api/repositories/{id}"), json!({}));
    let s = f.app.storage.read().unwrap();
    assert!(s.repositories.is_empty() && s.answers.is_empty() && s.jobs.is_empty());
    assert!(s.workspaces[0].answer_ids.is_empty());
    assert!(f.root.join("auth.py").exists());
}
#[test]
fn feature_request_records_criteria_without_pretending_to_build() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let draft=f.call("POST","/api/features",json!({"repositoryId":id,"prompt":"Improve authorize handling","criteria":["Reject invalid input"]}));
    assert_eq!(draft["status"], "planningOnly");
    assert!(draft["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str().unwrap().contains("No code was generated")));
}
#[test]
fn cross_repository_search_keeps_identical_names_separate() {
    let f = Fixture::new();
    let id = f.connect();
    f.index(&id);
    let other = f.root.join("nested");
    fs::create_dir_all(&other).unwrap();
    fs::write(
        other.join("auth.py"),
        "def allowed(value):\n    return value\n",
    )
    .unwrap();
    let other = f.call(
        "POST",
        "/api/repositories",
        json!({"path":other,"name":"other"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    f.index(&other);
    let results = f.call("POST", "/api/search", json!({"query":"allowed"}));
    let ids: std::collections::BTreeSet<_> = results["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["repositoryId"].as_str().unwrap())
        .collect();
    assert!(ids.contains(id.as_str()) && ids.contains(other.as_str()));
}
#[cfg(unix)]
#[test]
fn symlinked_sources_outside_scope_are_not_read() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let outside = f.dir.join("secret.py");
    fs::write(&outside, "outside confidential content").unwrap();
    symlink(&outside, f.root.join("escape.py")).unwrap();
    let capture = cie_server::repository::capture(&fs::canonicalize(&f.root).unwrap()).unwrap();
    assert!(!capture.sources.contains_key("escape.py"));
}
#[test]
fn source_text_and_input_limits_fail_explicitly() {
    let f = Fixture::new();
    let id = f.connect();
    let result = f.app.dispatch(
        "POST",
        "/api/questions",
        &serde_json::to_vec(&json!({"repositoryId":id,"question":"x".repeat(5000)})).unwrap(),
    );
    assert!(result.is_err());
    assert_eq!(
        f.app
            .dispatch("POST", "/api/search", &vec![b'x'; 1024 * 1024 + 1])
            .unwrap_err()
            .status,
        413
    );
}
#[test]
fn saved_workspace_validates_answer_references() {
    let f = Fixture::new();
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/workspaces",
                br#"{"name":"Bad","answerIds":["missing"],"activeAnswer":null}"#
            )
            .unwrap_err()
            .code,
        "INVALID_WORKSPACE"
    );
}
#[test]
fn unknown_schema_fields_are_rejected() {
    let f = Fixture::new();
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/repositories",
                &serde_json::to_vec(&json!({"path":f.root,"cloud":true})).unwrap()
            )
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}

#[test]
fn deleting_a_tracked_file_does_not_make_future_indexing_fail() {
    use std::process::Command;
    let f = Fixture::new();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&f.root)
        .arg("init")
        .output()
        .unwrap()
        .status
        .success());
    fs::write(f.root.join("deleted.py"), "def gone(): pass\n").unwrap();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&f.root)
        .args(["add", "deleted.py"])
        .output()
        .unwrap()
        .status
        .success());
    fs::remove_file(f.root.join("deleted.py")).unwrap();
    let capture = cie_server::repository::capture(&fs::canonicalize(&f.root).unwrap()).unwrap();
    assert!(capture.sources.contains_key("auth.py"));
    assert!(!capture.sources.contains_key("deleted.py"));
}

fn observation_body(f: &Fixture, repo: &str, revision: &str) -> Value {
    let state = f.app.storage.read().unwrap();
    let rev = state
        .repositories
        .iter()
        .find(|r| r.id == repo)
        .unwrap()
        .revisions
        .iter()
        .find(|r| r.id == revision)
        .unwrap();
    let hash = cie_core::types::hash_hex(&cie_core::types::content_hash(
        rev.sources["auth.py"].as_bytes(),
    ));
    json!({"repositoryId":repo,"revisionId":revision,"environment":"test","producer":"test-recorder","kind":"trace","events":[{"id":"root","name":"request","startMicros":0,"endMicros":200,"outcome":"error"},{"id":"child","parentId":"root","name":"allowed","startMicros":20,"endMicros":100,"outcome":"ok","file":"auth.py","line":1,"sourceHash":hash}]})
}
#[test]
fn imported_observations_map_only_exact_source_and_never_claim_execution() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let body = observation_body(&f, &repo, &rev);
    let o = f.call("POST", "/api/observations", body.clone());
    assert_eq!(o["mapped"], 1);
    assert_eq!(o["unmapped"], 1);
    assert_eq!(o["provenance"]["executionVerifiedByCie"], false);
    assert_eq!(o["items"][1]["durationMicros"], 80);
    assert_eq!(o["edges"][0]["kind"], "reportedParent");
    let repeated = f.call("POST", "/api/observations", body);
    assert_eq!(o["observation"]["id"], repeated["observation"]["id"]);
    assert_eq!(f.app.storage.read().unwrap().observations.len(), 1);
}
#[test]
fn observations_reject_mismatch_cycles_duplicates_and_invalid_intervals() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let body = observation_body(&f, &repo, &rev);
    for (field, value) in [
        ("sourceHash", json!("wrong")),
        ("line", json!(9999)),
        ("endMicros", json!(1)),
        ("parentId", json!("absent")),
    ] {
        let mut b = body.clone();
        b["events"][1][field] = value;
        assert!(f
            .app
            .dispatch(
                "POST",
                "/api/observations",
                &serde_json::to_vec(&b).unwrap()
            )
            .is_err());
    }
    let mut b = body.clone();
    b["events"][0]["parentId"] = json!("child");
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/observations",
                &serde_json::to_vec(&b).unwrap()
            )
            .unwrap_err()
            .code,
        "CYCLIC_TRACE"
    );
    let mut b = body;
    b["events"][1]["id"] = json!("root");
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/observations",
                &serde_json::to_vec(&b).unwrap()
            )
            .unwrap_err()
            .code,
        "DUPLICATE_EVENT"
    );
    assert!(f.app.storage.read().unwrap().observations.is_empty());
}
#[test]
fn investigations_checkpoint_competing_hypotheses_without_promoting_them() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let answer = f.ask(&repo, "overview");
    let aid = answer["answer"]["id"].as_str().unwrap();
    let i=f.call("POST","/api/investigations",json!({"repositoryId":repo,"revisionId":rev,"symptom":"Authorization intermittently fails"}));
    let id = i["investigation"]["id"].as_str().unwrap();
    let route = format!("/api/investigations/{id}/hypotheses");
    let result=f.call("POST",&route,json!({"statement":"Input varies","support":[aid],"counterEvidence":[],"nextStep":"Record failing input with permission"}));
    assert_eq!(result["claimClass"], "hypothesis");
    assert_eq!(
        result["investigation"]["hypotheses"][0]["status"],
        "proposed"
    );
    f.call(
        "POST",
        &format!("/api/investigations/{id}/checkpoint"),
        json!({"status":"paused","note":"Waiting for recorder output"}),
    );
    let stored = f.app.storage.read().unwrap();
    assert_eq!(stored.investigations[0].status, "paused");
    assert_eq!(stored.investigations[0].notes.len(), 1);
    let bad = json!({"statement":"Wrong scope","support":["missing"],"counterEvidence":[],"nextStep":"Read exact evidence"});
    assert_eq!(
        f.app
            .dispatch("POST", &route, &serde_json::to_vec(&bad).unwrap())
            .unwrap_err()
            .code,
        "INVALID_REFERENCE"
    );
}
#[test]
fn deleting_observation_challenges_dependents_and_repository_delete_cascades() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let o = f.call(
        "POST",
        "/api/observations",
        observation_body(&f, &repo, &rev),
    );
    let oid = o["observation"]["id"].as_str().unwrap();
    let i = f.call(
        "POST",
        "/api/investigations",
        json!({"repositoryId":repo,"revisionId":rev,"symptom":"Failure"}),
    );
    let id = i["investigation"]["id"].as_str().unwrap();
    f.call("POST",&format!("/api/investigations/{id}/hypotheses"),json!({"statement":"Recorder shows failure","support":[oid],"counterEvidence":[],"nextStep":"Inspect cause independently"}));
    f.call("DELETE", &format!("/api/observations/{oid}"), json!({}));
    assert_eq!(
        f.app.storage.read().unwrap().investigations[0].status,
        "evidenceMissing"
    );
    f.call("DELETE", &format!("/api/repositories/{repo}"), json!({}));
    let s = f.app.storage.read().unwrap();
    assert!(s.observations.is_empty());
    assert!(s.investigations.is_empty());
}
#[test]
fn imported_observations_become_stale_when_source_changes() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let o = f.call(
        "POST",
        "/api/observations",
        observation_body(&f, &repo, &rev),
    );
    let id = o["observation"]["id"].as_str().unwrap();
    fs::write(f.root.join("auth.py"), "def changed():\n    pass\n").unwrap();
    let view = f.call("GET", &format!("/api/observations/{id}"), json!({}));
    assert_eq!(view["freshness"], "stale");
    assert_eq!(
        view["items"][1]["citation"]["excerpt"],
        "def allowed(value):"
    );
}

#[test]
fn investigation_with_deleted_answer_reference_remains_challenged_after_resume() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let answer = f.ask(&repo, "overview");
    let aid = answer["answer"]["id"].as_str().unwrap();
    let i = f.call(
        "POST",
        "/api/investigations",
        json!({"repositoryId":repo,"revisionId":rev,"symptom":"Failure"}),
    );
    let id = i["investigation"]["id"].as_str().unwrap();
    f.call("POST",&format!("/api/investigations/{id}/hypotheses"),json!({"statement":"Static calls may explain failure","support":[aid],"counterEvidence":[],"nextStep":"Observe failing execution"}));
    f.call("DELETE", &format!("/api/answers/{aid}"), json!({}));
    f.call(
        "POST",
        &format!("/api/investigations/{id}/checkpoint"),
        json!({"status":"open","note":"Continuing with missing evidence disclosed"}),
    );
    let view = f.call("GET", &format!("/api/investigations/{id}"), json!({}));
    assert_eq!(view["challenged"], true);
    assert_eq!(view["missingReferences"], 1);
}

#[test]
fn generated_sample_has_real_dated_synthetic_history_without_mutating_user_source() {
    let f = Fixture::new();
    let before = fs::read(f.root.join("auth.py")).unwrap();
    let sample = f.call("POST", "/api/samples", json!({"kind":"checkout"}));
    assert_eq!(sample["synthetic"], true);
    let id = sample["repository"]["id"].as_str().unwrap();
    let root = PathBuf::from(sample["repository"]["path"].as_str().unwrap());
    assert!(root.join(".git").exists());
    assert!(root.join("domain/cart.py").exists());
    assert!(root.join("web/checkout.ts").exists());
    assert!(root.join("web/format.js").exists());
    assert!(root.join("gateway/PaymentGateway.java").exists());
    assert!(!f.root.join(".git").exists());
    assert_eq!(fs::read(f.root.join("auth.py")).unwrap(), before);
    let history = f.call("GET", &format!("/api/repositories/{id}/history"), json!({}));
    assert_eq!(history["commits"].as_array().unwrap().len(), 2);
    assert_eq!(history["authorshipCollected"], false);
    assert!(
        history["commits"][0]["timestampSeconds"].as_u64().unwrap()
            > history["commits"][1]["timestampSeconds"].as_u64().unwrap()
    );
    assert!(!serde_json::to_string(&history)
        .unwrap()
        .contains("fixture@localhost"));
    let repeated = f.call("POST", "/api/samples", json!({"kind":"checkout"}));
    assert_eq!(repeated["repository"]["id"], id);
    f.index(id);
}
#[test]
fn snapshot_comparison_reports_text_changes_without_claiming_semantic_compatibility() {
    let f = Fixture::new();
    let id = f.connect();
    let before = f.index(&id);
    fs::write(
        f.root.join("new.ts"),
        "export function added() { return 3; }\n",
    )
    .unwrap();
    fs::write(f.root.join("auth.py"), "def changed():\n    return False\n").unwrap();
    let after = f.index(&id);
    let diff = f.call(
        "POST",
        &format!("/api/repositories/{id}/compare"),
        json!({"before":before,"after":after}),
    );
    let rows = diff["differences"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().find(|r| r["file"] == "new.ts").unwrap()["status"],
        "added"
    );
    assert!(diff["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g.as_str().unwrap().contains("semantic compatibility")));
}
#[test]
fn source_view_uses_snapshot_not_live_path_and_removes_secret_shapes() {
    let f = Fixture::new();
    fs::write(
        f.root.join("auth.py"),
        "def old():\n    password = 'sensitivefixturevalue'\n    return True\n",
    )
    .unwrap();
    let id = f.connect();
    let revision = f.index(&id);
    fs::write(f.root.join("auth.py"), "def live():\n    return False\n").unwrap();
    let source = f.call(
        "POST",
        "/api/source",
        json!({"repositoryId":id,"revisionId":revision,"file":"auth.py"}),
    );
    assert!(source["text"].as_str().unwrap().contains("def old"));
    assert!(!source["text"]
        .as_str()
        .unwrap()
        .contains("sensitivefixturevalue"));
    assert_eq!(source["freshness"], "stale");
    assert!(f
        .app
        .dispatch(
            "POST",
            "/api/source",
            &serde_json::to_vec(
                &json!({"repositoryId":id,"revisionId":revision,"file":"../outside"})
            )
            .unwrap()
        )
        .is_err());
}

#[test]
fn compound_queries_preserve_unknown_under_negative_missing_effect() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    f.app
        .storage
        .transaction(|s| {
            for summary in &mut s.repositories[0].revisions[0].snapshot.summaries {
                summary.effects.clear();
                summary.complete = true;
            }
            Ok(())
        })
        .unwrap();
    let q=f.call("POST","/api/queries",json!({"repositoryId":repo,"revisionId":rev,"expression":{"op":"all","args":[{"op":"name","value":"authorize"},{"op":"not","arg":{"op":"effect","value":"network"}}]}}));
    assert_eq!(q["population"]["matched"], 0);
    assert!(q["population"]["unknown"].as_u64().unwrap() > 0);
    for i in q["answer"]["items"].as_array().unwrap() {
        assert_eq!(i["detail"]["queryDecision"], "unknown");
        assert!(i["label"].as_str().unwrap().contains("authorize"));
        assert_eq!(i["claimClass"], "analyzerInference");
    }
    let id = q["answer"]["id"].as_str().unwrap();
    let exported = f.call("GET", &format!("/api/answers/{id}/export"), json!({}));
    assert_eq!(q["answer"]["items"], exported["answer"]["items"]);
}
#[test]
fn compound_query_union_and_complement_use_one_exact_population() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let q=f.call("POST","/api/queries",json!({"repositoryId":repo,"revisionId":rev,"expression":{"op":"any","args":[{"op":"name","value":"authorize"},{"op":"name","value":"allowed"}]},"includeUnknown":false}));
    assert!(q["population"]["matched"].as_u64().unwrap() >= 2);
    assert_eq!(q["population"]["unknown"], 0);
    for i in q["answer"]["items"].as_array().unwrap() {
        assert_eq!(i["detail"]["queryDecision"], "true");
        assert!(
            i["label"].as_str().unwrap().contains("authorize")
                || i["label"].as_str().unwrap().contains("allowed")
        );
    }
    let selected: std::collections::BTreeSet<_> = q["answer"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    for edge in q["answer"]["edges"].as_array().unwrap() {
        assert!(selected.contains(edge["from"].as_str().unwrap()));
        assert!(selected.contains(edge["to"].as_str().unwrap()));
    }
}
#[test]
fn compound_queries_reject_unbounded_expressions_and_foreign_comparison() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let mut expr = json!({"op":"name","value":"authorize"});
    for _ in 0..10 {
        expr = json!({"op":"not","arg":expr});
    }
    for expression in [
        expr,
        json!({"op":"changed","beforeRevisionId":"foreign"}),
        json!({"op":"effect","value":"made_up_effect"}),
        json!({"op":"all","args":[]}),
    ] {
        let body = json!({"repositoryId":repo,"revisionId":rev,"expression":expression});
        assert!(f
            .app
            .dispatch("POST", "/api/queries", &serde_json::to_vec(&body).unwrap())
            .is_err());
    }
}

#[test]
fn composed_analysis_retains_completed_steps_when_dependency_is_ambiguous() {
    let f = Fixture::new();
    let repo = f.connect();
    f.index(&repo);
    let p=f.call("POST","/api/plans",json!({"repositoryId":repo,"steps":[{"question":"overview"},{"question":"show callers","intent":"callers","fromStep":0},{"question":"findings","intent":"findings"}]}));
    assert_eq!(p["plan"]["status"], "partial");
    assert!(p["plan"]["steps"][0]["answerId"].is_string());
    assert_eq!(p["plan"]["steps"][1]["status"], "failed");
    assert!(p["plan"]["steps"][1]["issue"]
        .as_str()
        .unwrap()
        .contains("AMBIGUOUS_DEPENDENCY"));
    assert!(p["plan"]["steps"][2]["answerId"].is_string());
    let aid = p["plan"]["steps"][0]["answerId"].as_str().unwrap();
    let id = p["plan"]["id"].as_str().unwrap();
    f.call("DELETE", &format!("/api/answers/{aid}"), json!({}));
    let view = f.call("GET", &format!("/api/plans/{id}"), json!({}));
    assert_eq!(view["missingAnswers"], 1);
    assert!(view["results"][0]["error"].is_string());
}
#[test]
fn resuming_after_answer_publication_before_step_checkpoint_is_idempotent() {
    let f = Fixture::new();
    let repo = f.connect();
    f.index(&repo);
    let p=f.call("POST","/api/plans",json!({"repositoryId":repo,"steps":[{"question":"overview"},{"question":"find allowed","intent":"search"}]}));
    let id = p["plan"]["id"].as_str().unwrap();
    let aid = p["plan"]["steps"][1]["answerId"].clone();
    let count = f.app.storage.read().unwrap().answers.len();
    f.app
        .storage
        .transaction(|s| {
            let p = &mut s.plans[0];
            p.status = "interrupted".into();
            p.steps[1].status = "interrupted".into();
            p.steps[1].answer_id = None;
            Ok(())
        })
        .unwrap();
    let resumed = f.call("POST", &format!("/api/plans/{id}/resume"), json!({}));
    assert_eq!(resumed["plan"]["steps"][1]["answerId"], aid);
    assert_eq!(f.app.storage.read().unwrap().answers.len(), count);
    f.app
        .storage
        .transaction(|s| {
            s.plans[0].status = "interrupted".into();
            Ok(())
        })
        .unwrap();
    fs::write(f.root.join("auth.py"), "def changed():\n    return False\n").unwrap();
    assert_eq!(
        f.app
            .dispatch("POST", &format!("/api/plans/{id}/resume"), b"{}")
            .unwrap_err()
            .code,
        "STALE_PLAN"
    );
}
#[test]
fn question_request_ids_deduplicate_and_refuse_changed_input_or_revision() {
    let f = Fixture::new();
    let repo = f.connect();
    let revision = f.index(&repo);
    let q = json!({"repositoryId":repo,"question":"overview","expectedRevision":revision,"requestId":"stable-request"});
    let first = f.call("POST", "/api/questions", q.clone());
    let second = f.call("POST", "/api/questions", q.clone());
    assert_eq!(first["answer"]["id"], second["answer"]["id"]);
    assert_eq!(f.app.storage.read().unwrap().answers.len(), 1);
    let mut conflict = q.clone();
    conflict["question"] = json!("findings");
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/questions",
                &serde_json::to_vec(&conflict).unwrap()
            )
            .unwrap_err()
            .code,
        "REQUEST_ID_CONFLICT"
    );
    let mut wrong = q;
    wrong["requestId"] = json!("fresh-request");
    wrong["expectedRevision"] = json!("missing-snapshot");
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/questions",
                &serde_json::to_vec(&wrong).unwrap()
            )
            .unwrap_err()
            .code,
        "REVISION_CHANGED"
    );
}

#[test]
fn naming_anomaly_compares_promised_permission_check_with_observed_printing() {
    let f = Fixture::new();
    fs::write(f.root.join("auth.py"),"def authorize(amount):\n    print('hello world')\n\ndef checkout(amount):\n    return authorize(amount)\n").unwrap();
    let repo = f.connect();
    f.index(&repo);
    let naming = f.ask(&repo, "naming mismatch");
    let items = naming["answer"]["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let issue = items
        .iter()
        .find(|i| i["label"].as_str().unwrap().contains("authorize"))
        .unwrap();
    assert_eq!(issue["claimClass"], "analyzerInference");
    assert!(issue["detail"]["promisedByName"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "CheckPermission"));
    assert!(issue["detail"]["observedEffects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "print"));
    assert!(issue["detail"]["scope"]
        .as_str()
        .unwrap()
        .contains("confirmed violation needs an established obligation"));
    assert!(issue["citations"][0]["excerpt"]
        .as_str()
        .unwrap()
        .contains("hello world"));
    let behavior = f.ask(&repo, "what does authorize do");
    assert!(behavior["answer"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["label"].as_str().unwrap().contains("print")));
    let rules = f.ask(&repo, "rules");
    for item in rules["answer"]["items"].as_array().unwrap() {
        assert_eq!(item["claimClass"], "analyzerInference");
        assert!(item["detail"]["scope"]
            .as_str()
            .unwrap()
            .contains("business approval"));
    }
}
#[test]
fn refuted_concept_remains_withheld_across_answers_and_answer_deletion() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let first = f.ask(&repo, "concepts");
    let second = f.ask(&repo, "concepts");
    let item = first["answer"]["items"][0]["id"].as_str().unwrap();
    let first_id = first["answer"]["id"].as_str().unwrap();
    let second_id = second["answer"]["id"].as_str().unwrap();
    f.call("POST","/api/reviews",json!({"answerId":first_id,"itemId":item,"verdict":"refute","reason":"This grouping does not match the approved domain"}));
    let view = f.call("GET", &format!("/api/answers/{second_id}"), json!({}));
    assert_eq!(view["hiddenClaims"], 1);
    assert!(view["answer"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["id"] != item));
    f.call("DELETE", &format!("/api/answers/{first_id}"), json!({}));
    let fresh = f.ask(&repo, "concepts");
    assert_eq!(fresh["hiddenClaims"], 1);
    let unrelated = f.ask(&repo, "overview");
    assert_eq!(unrelated["hiddenClaims"], 0);
    assert_eq!(unrelated["challenged"], false);
    let statistics = f.call(
        "GET",
        &format!("/api/concept-reviews/{repo}/{rev}"),
        json!({}),
    );
    assert_eq!(statistics["reviewedUniqueCards"], 1);
    assert_eq!(statistics["agreementSummaryAvailable"], false);
    assert_eq!(statistics["calibratedProbabilityAvailable"], false);
    let restored=f.call("POST","/api/reviews",json!({"answerId":second_id,"itemId":item,"verdict":"confirm","reason":"Restored after checking domain membership"}));
    assert_eq!(restored["hiddenClaims"], 0);
    for i in restored["answer"]["items"].as_array().unwrap() {
        assert_eq!(i["claimClass"], "analyzerInference");
    }
}
#[test]
fn concept_answer_respects_display_cap() {
    let f = Fixture::new();
    let repo = f.connect();
    f.index(&repo);
    f.app
        .storage
        .transaction(|s| {
            let nodes = &mut s.repositories[0].revisions[0].snapshot.concepts.nodes;
            let node = nodes[0].clone();
            *nodes = (0..501)
                .map(|id| {
                    let mut n = node.clone();
                    n.id = id;
                    n.parent = None;
                    n
                })
                .collect();
            Ok(())
        })
        .unwrap();
    let answer = f.ask(&repo, "concepts");
    assert_eq!(answer["answer"]["items"].as_array().unwrap().len(), 500);
    assert!(answer["answer"]["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g.as_str().unwrap().contains("500")));
}

fn workflow_body(repo: &str, revision: &str) -> Value {
    json!({"repositoryId":repo,"revisionId":revision,"name":"Declared recovery model","states":["Paid","Pending","Fulfilled","Stranded","Refunded"],"initialState":"Paid","evidenceIds":[],"transitions":[{"id":"submit","from":"Paid","to":"Pending","event":"submit","guardStatus":"enabled","guardNote":"Assumed model edge"},{"id":"success","from":"Pending","to":"Fulfilled","event":"provider success","guardStatus":"unknown","guardNote":"Provider behavior not observed"},{"id":"failure","from":"Pending","to":"Stranded","event":"failure without refund","guardStatus":"unknown","guardNote":"Hypothetical failure branch"},{"id":"retry","from":"Stranded","to":"Refunded","event":"refund retry","guardStatus":"unknown","guardNote":"Assumed recovery path"}]})
}
#[test]
fn declared_scenarios_expose_unknown_guards_and_never_execute_source_or_refunds() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let before = fs::read(f.root.join("auth.py")).unwrap();
    let m = f.call("POST", "/api/workflow-models", workflow_body(&repo, &rev));
    let id = m["model"]["id"].as_str().unwrap();
    let r = f.call(
        "POST",
        &format!("/api/workflow-models/{id}/explore"),
        json!({"targetState":"Stranded","maxSteps":8,"disabledTransitions":[]}),
    );
    assert_eq!(r["executionPerformed"], false);
    assert_eq!(r["claimClass"], "conditionalModelResult");
    assert_eq!(r["run"]["classification"], "conditionalWitness");
    assert_eq!(r["run"]["witness"], json!(["submit", "failure"]));
    assert_eq!(r["run"]["unknownGuards"], json!(["failure"]));
    assert_eq!(fs::read(f.root.join("auth.py")).unwrap(), before);
    let cutoff = f.call(
        "POST",
        &format!("/api/workflow-models/{id}/explore"),
        json!({"targetState":"Stranded","maxSteps":1,"disabledTransitions":[]}),
    );
    assert_eq!(cutoff["run"]["classification"], "notFoundWithinBound");
    let counter = f.call(
        "POST",
        &format!("/api/workflow-models/{id}/explore"),
        json!({"targetState":"Stranded","maxSteps":8,"disabledTransitions":["failure"]}),
    );
    assert_eq!(
        counter["run"]["classification"],
        "notReachableInDeclaredGraph"
    );
    assert!(counter["gaps"].as_array().unwrap().iter().any(|g| g
        .as_str()
        .unwrap()
        .contains("not proof that no implementation path")));
}
#[test]
fn scenario_models_reject_foreign_evidence_invalid_edges_and_unknown_disabled_actions() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let body = workflow_body(&repo, &rev);
    for (field, value) in [
        ("to", json!("undeclared")),
        ("guardStatus", json!("definitely_safe")),
    ] {
        let mut invalid = body.clone();
        invalid["transitions"][0][field] = value;
        assert!(f
            .app
            .dispatch(
                "POST",
                "/api/workflow-models",
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err());
    }
    let mut foreign = body.clone();
    foreign["evidenceIds"] = json!(["missing-answer"]);
    assert_eq!(
        f.app
            .dispatch(
                "POST",
                "/api/workflow-models",
                &serde_json::to_vec(&foreign).unwrap()
            )
            .unwrap_err()
            .code,
        "FOREIGN_EVIDENCE"
    );
    let m = f.call("POST", "/api/workflow-models", body);
    let id = m["model"]["id"].as_str().unwrap();
    assert_eq!(f.app.dispatch("POST",&format!("/api/workflow-models/{id}/explore"),&serde_json::to_vec(&json!({"targetState":"Stranded","maxSteps":8,"disabledTransitions":["invented"]})).unwrap()).unwrap_err().code,"SCENARIO_SCOPE");
}
#[test]
fn scenario_evidence_deletion_is_challenged_and_repository_deletion_cascades_runs() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let answer = f.ask(&repo, "overview");
    let aid = answer["answer"]["id"].as_str().unwrap();
    let mut body = workflow_body(&repo, &rev);
    body["evidenceIds"] = json!([aid]);
    let m = f.call("POST", "/api/workflow-models", body);
    let id = m["model"]["id"].as_str().unwrap();
    f.call(
        "POST",
        &format!("/api/workflow-models/{id}/explore"),
        json!({"targetState":"Stranded","maxSteps":8,"disabledTransitions":[]}),
    );
    f.call("DELETE", &format!("/api/answers/{aid}"), json!({}));
    let challenged = f.call("GET", &format!("/api/workflow-models/{id}"), json!({}));
    assert_eq!(challenged["challenged"], true);
    f.call("DELETE", &format!("/api/repositories/{repo}"), json!({}));
    let s = f.app.storage.read().unwrap();
    assert!(s.workflow_models.is_empty());
    assert!(s.scenario_runs.is_empty());
}

fn profile_body(repo: &str, revision: &str, elapsed: u64) -> Value {
    json!({"repositoryId":repo,"revisionId":revision,"environment":"test","producer":"fixture-profiler","kind":"profile","events":[{"id":"cpu","name":"work","startMicros":0,"endMicros":elapsed,"outcome":"ok","samples":5}],"measurement":{"workloadKey":"ten-authorizations","environmentKey":"fixed-fixture-env","expectedOperations":10,"recordedOperations":10,"failedOperations":0,"omittedOperations":0,"lostSamples":0,"elapsedMicros":elapsed,"samplingIntervalMicros":100,"warmupMicros":0,"captureComplete":true}})
}
#[test]
fn equivalent_profile_comparison_describes_recorded_delta_without_claiming_improvement() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let a = f.call("POST", "/api/observations", profile_body(&repo, &rev, 2000));
    let b = f.call("POST", "/api/observations", profile_body(&repo, &rev, 1000));
    let r=f.call("POST","/api/profile-comparisons",json!({"baselineId":a["observation"]["id"],"candidateId":b["observation"]["id"],"equivalenceNote":"Same declared fixture workload and environment"}));
    assert_eq!(r["comparisonEligible"], true);
    assert_eq!(r["verdict"], "recordedElapsedLower");
    assert_eq!(r["recordedElapsedDeltaMicros"], -1000);
    assert_eq!(r["performanceImprovementEstablished"], false);
    assert_eq!(r["equivalenceVerifiedByCie"], false);
    assert_eq!(r["rows"].as_array().unwrap().len(), 2);
}
#[test]
fn profile_comparison_rejects_dropped_work_or_failures_but_retains_every_row() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let a = f.call("POST", "/api/observations", profile_body(&repo, &rev, 2000));
    for (field, value) in [
        ("failedOperations", json!(1)),
        ("recordedOperations", json!(5)),
        ("lostSamples", json!(2)),
        ("environmentKey", json!("different-environment")),
        ("samplingIntervalMicros", json!(200)),
    ] {
        let mut body = profile_body(&repo, &rev, 100);
        body["measurement"][field] = value;
        body["events"][0]["outcome"] = json!("error");
        let b = f.call("POST", "/api/observations", body);
        let comparison=f.call("POST","/api/profile-comparisons",json!({"baselineId":a["observation"]["id"],"candidateId":b["observation"]["id"],"equivalenceNote":"User declares equivalence but metadata differs"}));
        assert_eq!(comparison["verdict"], "notComparable", "{field}");
        assert_eq!(comparison["rows"].as_array().unwrap().len(), 2);
        assert_eq!(comparison["rows"][1]["reportedErrorRecords"], 1);
        assert!(!comparison["reasons"].as_array().unwrap().is_empty());
    }
}
#[test]
fn profiles_without_workload_metadata_remain_importable_but_not_fairly_comparable() {
    let f = Fixture::new();
    let repo = f.connect();
    let rev = f.index(&repo);
    let a = f.call("POST", "/api/observations", profile_body(&repo, &rev, 2000));
    let mut body = profile_body(&repo, &rev, 1000);
    body.as_object_mut().unwrap().remove("measurement");
    let b = f.call("POST", "/api/observations", body);
    let r=f.call("POST","/api/profile-comparisons",json!({"baselineId":a["observation"]["id"],"candidateId":b["observation"]["id"],"equivalenceNote":"Metadata unavailable for this capture"}));
    assert_eq!(r["verdict"], "notComparable");
    assert!(r["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("metadata is missing")));
}

#[test]
fn deleted_sample_connection_can_be_recreated_without_overwriting_edited_source() {
    let f = Fixture::new();
    let first = f.call("POST", "/api/samples", json!({"kind":"checkout"}));
    let id = first["repository"]["id"].as_str().unwrap();
    let path = PathBuf::from(first["repository"]["path"].as_str().unwrap());
    fs::write(
        path.join("checkout.py"),
        "def manually_edited():\n    return True\n",
    )
    .unwrap();
    f.call("DELETE", &format!("/api/repositories/{id}"), json!({}));
    let recreated = f.call("POST", "/api/samples", json!({"kind":"checkout"}));
    assert_ne!(recreated["repository"]["id"], id);
    assert_eq!(recreated["repository"]["path"], first["repository"]["path"]);
    assert!(fs::read_to_string(path.join("checkout.py"))
        .unwrap()
        .contains("manually_edited"));
}
