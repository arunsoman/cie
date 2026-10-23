use cie_storage::{retrieval::*, StoragePolicy};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cie-retrieve-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn ingest(&self, src: &str) {
        std::fs::write(self.0.join("app.py"), src).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_cie"))
            .args([
                "ingest-polyglot",
                self.0.to_str().unwrap(),
                "--web-out",
                self.0.join("bundle.json").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn storage(&self) -> Storage {
        Storage::open(&self.0, StoragePolicy::default()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_file(cie_polyglot::schema::default_db_path(&self.0));
    }
}
fn request(query: Value, outputs: Vec<Value>) -> Request {
    serde_json::from_value(json!({"query":query,"outputs":outputs})).unwrap()
}
fn symbols() -> Value {
    json!({"select":{"kind":"symbol","filter":{"field":{"field":"kind","op":"eq","value":"Function"}}}})
}
fn entities(limit: usize) -> Value {
    json!({"id":"rows","output":{"entities":{"page":{"limit":limit}}}})
}
#[test]
fn pinned_revisions_source_paging_and_forged_cursor() {
    let f = Fixture::new();
    f.ingest("def save(x):\n    return x + 1\ndef call(x):\n    return save(x)\n");
    let reader = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let rev = reader.revision().to_string();
    let result = reader
        .retrieve(request(symbols(), vec![entities(1)]))
        .unwrap();
    assert_eq!(
        result.matches,
        2,
        "{}",
        serde_json::to_string_pretty(
            &reader
                .retrieve(request(symbols(), vec![entities(100)]))
                .unwrap()
        )
        .unwrap()
    );
    let cursor = result.outputs[0].data["continuation"].as_str().unwrap();
    let second=reader.retrieve(request(symbols(),vec![json!({"id":"rows","output":{"entities":{"page":{"limit":1,"continuation":cursor}}}})])).unwrap();
    assert_ne!(
        result.outputs[0].data["items"][0]["id"],
        second.outputs[0].data["items"][0]["id"]
    );
    let mut forged = cursor.to_string();
    forged.push('x');
    assert_eq!(reader.retrieve(request(symbols(),vec![json!({"id":"rows","output":{"entities":{"page":{"limit":1,"continuation":forged}}}})])).unwrap_err().code,"CursorInvalid");
    let id = result.outputs[0].data["items"][0]["id"].as_str().unwrap();
    let select = json!({"select":{"kind":"symbol","filter":{"ids":[id]}}});
    let source = reader
        .retrieve(request(
            select.clone(),
            vec![
                json!({"id":"src","output":{"detail":{"source":{}}}}),
                json!({"id":"ir","output":{"detail":"function_ir"}}),
            ],
        ))
        .unwrap();
    assert!(source.outputs[0].data["text"]
        .as_str()
        .unwrap()
        .starts_with("def "));
    assert!(source.outputs[1].data["blocks"].as_array().unwrap().len() > 0);
    f.ingest("def changed():\n    return 99\n");
    std::fs::remove_file(f.0.join("app.py")).unwrap();
    assert_eq!(
        reader
            .retrieve(request(symbols(), vec![entities(10)]))
            .unwrap()
            .matches,
        2
    );
    assert!(reader
        .retrieve(request(
            select,
            vec![json!({"id":"src","output":{"detail":{"source":{}}}})]
        ))
        .is_ok());
    let latest = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    assert_ne!(latest.revision(), rev);
    assert_eq!(
        latest
            .retrieve(request(symbols(), vec![entities(10)]))
            .unwrap()
            .matches,
        1
    );
    let old = f
        .storage()
        .open_revision(RevisionSelector::Id(rev))
        .unwrap();
    assert_eq!(
        old.retrieve(request(symbols(), vec![entities(10)]))
            .unwrap()
            .matches,
        2
    );
}
#[test]
fn concept_sets_cubes_drill_through_and_views() {
    let f = Fixture::new();
    f.ingest("def charge(x):\n    return x + 1\ndef pay(x):\n    return charge(x)\ndef test_pay():\n    return pay(1)\n");
    let r = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let concepts = r
        .retrieve(request(
            json!({"select":{"kind":"concept"}}),
            vec![entities(100)],
        ))
        .unwrap();
    assert!(concepts.matches > 0);
    let concept = concepts.outputs[0].data["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["fields"]["symbols"].as_array().unwrap().is_empty())
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let membership = json!({"select":{"kind":"symbol","filter":{"concept_membership":{"concepts":[concept],"include_descendants":true}}}});
    let query = json!({"intersect":[symbols(),membership.clone()]});
    let result=r.retrieve(request(query.clone(),vec![entities(100),json!({"id":"cube","output":{"cube":{"dimensions":["file.path","language"],"rows":["file.path"],"columns":["language"]}}}),json!({"id":"view","output":{"view":{"kind":"table","source_output":"cube"}}})])).unwrap();
    assert!(result.matches > 0);
    let cube = &result.outputs[1].data;
    assert_eq!(cube["grand_totals"]["m0"]["value"], result.matches);
    let cell = &cube["items"][0];
    let drill = r
        .retrieve(request(
            json!({"cell_contributors":{"descriptor":cell["contributors"]}}),
            vec![entities(100)],
        ))
        .unwrap();
    assert_eq!(
        drill.matches as u64,
        cell["measures"]["m0"]["value"].as_u64().unwrap()
    );
    let diff = r
        .retrieve(request(
            json!({"difference":{"left":membership.clone(),"right":membership}}),
            vec![entities(100)],
        ))
        .unwrap();
    assert_eq!(diff.matches, 0);
    let graph = r
        .retrieve(request(
            query,
            vec![json!({"id":"v","output":{"view":{"kind":"relationships"}}})],
        ))
        .unwrap();
    assert!(graph.outputs[0].data["spec"]["elements"].is_array());
    let all = r.retrieve(request(symbols(), vec![entities(100)])).unwrap();
    let pay = all.outputs[0].data["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["fields"]["name"] == "pay")
        .unwrap()["id"]
        .clone();
    let impact = r
        .retrieve(request(
            json!({"select":{"kind":"symbol","filter":{"ids":[pay]}}}),
            vec![json!({"id":"impact","output":{"detail":"impact"}})],
        ))
        .unwrap();
    assert!(
        impact.outputs[0].data["affected_symbols"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
}
#[test]
fn errors_budgets_and_missing_artifacts_are_explicit() {
    let f = Fixture::new();
    f.ingest("def hello():\n    return 1\n");
    let r = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let mut req = request(symbols(), vec![entities(1)]);
    req.budget.max_response_bytes = 10;
    assert_eq!(r.retrieve(req).unwrap_err().code, "BudgetExceeded");
    let mut req = request(
        symbols(),
        vec![
            entities(1),
            json!({"id":"missing","output":{"artifact":{"kind":"quality"}}}),
        ],
    );
    req.best_effort = true;
    let result = r.retrieve(req).unwrap();
    assert_eq!(result.outputs[1].status, "failed");
    assert_eq!(
        result.outputs[1].error.as_ref().unwrap().code,
        "ArtifactUnavailable"
    );
    assert!(r
        .diagnostics(Some("files; DROP TABLE symbols"), 10, 0)
        .is_err());
}
