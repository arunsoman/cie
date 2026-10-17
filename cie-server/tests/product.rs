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
        "cie-restart-{}",
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
    let dir = std::env::temp_dir().join(format!("cie-lock-{}", SEQ.fetch_add(1, Ordering::SeqCst)));
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
