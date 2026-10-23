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
        self.ingest_tree();
    }
    /// Ingest the directory as it stands, without writing any file.
    fn ingest_tree(&self) {
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
fn audit_forward_view_dependency_must_work() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\n");let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let req=request(symbols(),vec![json!({"id":"chart","output":{"view":{"kind":"table","source":{"output":"cube"}}}}),json!({"id":"cube","output":{"cube":{"dimensions":[]}}})]);
    let result=r.retrieve(req);assert!(result.is_ok(),"Acyclic forward reference failed: {result:?}");
}
#[test]
fn audit_failed_cube_must_not_produce_ready_chart() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\n");let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let mut req=request(symbols(),vec![json!({"id":"cube","output":{"cube":{"dimensions":[{"id":"bad","dimension":{"field":"nonexistent_field"}}]}}}),json!({"id":"chart","output":{"view":{"kind":"table","source":{"output":"cube"}}}})]);req.best_effort=true;
    let result=r.retrieve(req).unwrap();assert_ne!(result.outputs[1].status,"ready","Failed cube became a ready chart: {:?}",result.outputs);
}
#[test]
fn audit_non_cube_source_must_be_rejected() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\n");let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let req=request(symbols(),vec![entities(20),json!({"id":"chart","output":{"view":{"kind":"table","source":{"output":"rows"}}}})]);
    assert!(r.retrieve(req).is_err(),"Entity records were accepted as a cube chart source");
}
#[test]
fn audit_unicode_casefold_must_match_sharp_s() {
    let f=Fixture::new();f.ingest("def Straße():\n    return 1\n");let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let result=r.retrieve(request(json!({"select":{"kind":"symbol","filter":{"field":{"field":"name","op":"contains_fold","value":"STRASSE"}}}}),vec![entities(20)])).unwrap();
    assert!(result.matches>0,"Unicode case-folding must match Straße with STRASSE");
}
#[test]
fn audit_existing_fault_data_must_be_retrievable() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\n");let bundle=f.0.join(".cie/revs/1/web-bundle.json");let mut data:Value=serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
    data["report"]["fault_analysis"]=json!({"audit_marker":"stored_fault"});std::fs::write(bundle,serde_json::to_vec(&data).unwrap()).unwrap();
    let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();let result=r.retrieve(request(symbols(),vec![json!({"id":"fault","output":{"artifact":{"kind":"fault"}}})]));
    assert!(result.is_ok(),"Fault data exists in persisted report but retrieval failed: {result:?}");
}
#[test]
fn audit_simple_file_query_must_not_load_all_duplicate_pairs() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\ndef two():\n    return 2\n");
    let db=cie_polyglot::schema::open_db(&f.0.join(".cie/revs/1/repository.sqlite")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM duplicates; WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<100001) INSERT INTO duplicates(id,symbol_a,symbol_b,similarity,tier,detection,epistemic) SELECT x,1,2,1.0,'canonical','ir-hash-match','Fact' FROM n;").unwrap();drop(db);
    let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();let result=r.retrieve(request(json!({"select":{"kind":"file","filter":{"field":{"field":"path","op":"eq","value":"app.py"}}}}),vec![entities(1)]));
    assert!(result.is_ok(),"Unrelated duplicate cardinality broke a file lookup: {result:?}");
}
#[test]
fn audit_cube_source_with_overlapping_concepts_must_not_be_donut() {
    let f=Fixture::new();f.ingest("def one():\n    return 1\n");let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let snapshot=f.0.join(".cie/revs/1/snapshot.json");let mut data:Value=serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let mut first=data["concepts"]["nodes"][0].clone();first["id"]=json!(100);first["label"]=json!("A");first["symbols"]=json!([1]);first["parent"]=Value::Null;first["children"]=json!([]);
    let mut second=first.clone();second["id"]=json!(101);second["label"]=json!("B");data["concepts"]["nodes"]=json!([first,second]);std::fs::write(snapshot,serde_json::to_vec(&data).unwrap()).unwrap();
    let r=f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let req=request(symbols(),vec![json!({"id":"cube","output":{"cube":{"dimensions":[{"id":"concept","dimension":{"related_entity":{"relation_path":[{"relation":"member_of_concept"}],"attribute":"id"}}}]}}}),json!({"id":"chart","output":{"view":{"kind":"donut","source":{"output":"cube"}}}})]);
    let result=r.retrieve(req);assert!(result.is_err(),"Unverified multi-valued categories accepted as a whole/parts donut: {result:?}");
}
