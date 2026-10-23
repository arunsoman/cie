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
                json!({"id":"ir","output":{"detail":{"function_ir":{}}}}),
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
    let membership = json!({"select":{"kind":"symbol","filter":{"concept_membership":{"concepts":[concept],"scope":"include_descendants"}}}});
    let query = json!({"intersect":[symbols(),membership.clone()]});
    let result=r.retrieve(request(query.clone(),vec![entities(100),json!({"id":"cube","output":{"cube":{"dimensions":[{"id":"file","dimension":{"field":"file.path"}},{"id":"lang","dimension":{"field":"language"}}],"layout":{"pivot":{"rows":["file"],"columns":["lang"]}}}}}),json!({"id":"view","output":{"view":{"kind":"table","source":{"output":"cube"}}}})])).unwrap();
    assert!(result.matches > 0);
    let cube = &result.outputs[1].data;
    assert_eq!(cube["grand_totals"]["count"], result.matches);
    let cell = &cube["items"][0];
    let drill = r
        .retrieve(request(
            json!({"cell_contributors":{"descriptor":cell["contributors"]}}),
            vec![entities(100)],
        ))
        .unwrap();
    assert_eq!(
        drill.matches as u64,
        cell["measures"]["count"].as_u64().unwrap()
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
            vec![json!({"id":"impact","output":{"detail":{"impact":{}}}})],
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

#[test]
fn symbol_resolution_name_position_ambiguity_and_non_ascii_bytes() {
    let f = Fixture::new();
    // Leading non-ASCII comment shifts every declaration byte offset; "save"
    // is defined twice so a name lookup must stay ambiguous.
    let source = "# café = 42\ndef save(x):\n    return x\ndef save(x):\n    return x + 1\n";
    f.ingest(source);
    let r = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let file = "app.py".to_string();
    let resolved = r
        .resolve_symbol(SymbolLookup::ByName {
            file: file.clone(),
            qualified_name: "app.save".into(),
        })
        .unwrap();
    match resolved {
        SymbolResolution::Ambiguous { candidates } => {
            assert_eq!(candidates.len(), 2);
            assert_ne!(candidates[0].entity.id, candidates[1].entity.id);
            assert!(candidates.iter().all(|c| c.file == "app.py"));
            assert_eq!(candidates[0].entity.repository.len(), 64);
            assert_eq!(candidates[0].entity.revision, r.revision());
        }
        other => panic!("expected ambiguity, got {other:?}"),
    }
    assert!(matches!(
        r.resolve_symbol(SymbolLookup::ByName {
            file: file.clone(),
            qualified_name: "absent.x".into(),
        })
        .unwrap(),
        SymbolResolution::NotFound
    ));
    // Source byte positions are zero-based UTF-8 offsets into captured source.
    let first = source.find("def save").unwrap() as u64;
    let second = source.rfind("def save").unwrap() as u64;
    let names: Vec<String> = [first + 2, second + 2]
        .into_iter()
        .map(|offset| match r
            .resolve_symbol(SymbolLookup::AtPosition {
                file: file.clone(),
                byte_offset: offset,
            })
            .unwrap()
        {
            SymbolResolution::Found(entity) => entity.id,
            other => panic!("expected resolution at byte {offset}, got {other:?}"),
        })
        .collect();
    assert_ne!(names[0], names[1], "distinct declarations resolve distinctly");
    // Absolute paths and traversal are rejected before any lookup.
    for bad in ["/etc/passwd", "../escape.py", "a/../b.py", ""] {
        assert_eq!(
            r.resolve_symbol(SymbolLookup::ByName {
                file: bad.into(),
                qualified_name: "app.save".into(),
            })
            .unwrap_err()
            .code,
            "InvalidRequest"
        );
    }
}

#[test]
fn quantifiers_min_depth_and_relation_endpoint_validation() {
    let f = Fixture::new();
    f.ingest("def a():\n    return b()\ndef b():\n    return c()\ndef c():\n    return b()\ndef both():\n    x = b()\n    y = c()\n    return x\ndef d():\n    return 1\n");
    let r = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let all_symbols = json!({"select":{"kind":"symbol","filter":{"field":{"field":"kind","op":"eq","value":"Function"}}}});
    let named = |name: &str| {
        json!({"select":{"kind":"symbol","filter":{"field":{"field":"name","op":"eq","value":name}}}})
    };
    let result = r
        .retrieve(
            request(
                all_symbols.clone(),
                vec![json!({"id":"rel","output":{"relationships":{"page":{"limit":50}}}})],
            ),
        )
        .unwrap();
    let calls: usize = result.outputs[0].data["total"].as_u64().unwrap() as usize;
    assert!(calls >= 4, "call edges captured: {calls}");
    // `Any`: at least one related entity in the target set.
    let any = r
        .retrieve(
            request(
                json!({"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"related":{"relation":"calls","target":named("b"),"quantifier":"any"}}]}}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(any.matches, 3, "a, c and both call b");
    // `All` requires every target entity to be related (and a non-empty set).
    let all = r
        .retrieve(
            request(
                json!({"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"related":{"relation":"calls","target":{"union":[named("b"),named("c")]},"quantifier":"all"}}]}}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(all.matches, 1, "only both() calls b and c");
    // `None` means no such related entity.
    let none = r
        .retrieve(
            request(
                json!({"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"related":{"relation":"calls","target":named("b"),"quantifier":"none"}}]}}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(none.matches, 2, "b and d never call b");
    // Bounded traversal with a minimum depth excludes depth-1 hits; a call
    // cycle (b<->c) is visited safely and returned once.
    let deep = r
        .retrieve(
            request(
                json!({"follow":{"input":{"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"field":{"field":"name","op":"eq","value":"a"}}]}}},"relation":"calls","min_depth":2,"max_depth":2}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(deep.matches, 1, "depth 2 from a reaches c only");
    let shallow = r
        .retrieve(
            request(
                json!({"follow":{"input":{"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"field":{"field":"name","op":"eq","value":"a"}}]}}},"relation":"calls","min_depth":1,"max_depth":1}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(shallow.matches, 1);
    let cyclic = r
        .retrieve(
            request(
                json!({"follow":{"input":{"select":{"kind":"symbol","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"field":{"field":"name","op":"eq","value":"a"}}]}}},"relation":"calls","max_depth":8}}),
                vec![entities(50)],
            ),
        )
        .unwrap();
    assert_eq!(cyclic.matches, 2, "b and c, returned once despite the cycle");
    // Incompatible relation endpoints are validation errors, never silent.
    let bad = r
        .retrieve(
            request(
                json!({"select":{"kind":"concept","filter":{"and":[{"field":{"field":"kind","op":"eq","value":"Function"}},{"related":{"relation":"calls","target":named("b"),"quantifier":"any"}}]}}}),
                vec![entities(1)],
            ),
        )
        .unwrap_err();
    assert_eq!(bad.code, "InvalidRequest");
}

#[test]
fn cube_buckets_having_order_pivot_totals_and_facets() {
    let f = Fixture::new();
    std::fs::write(f.0.join("app.py"), "def one():\n    return 1\n").unwrap();
    std::fs::create_dir_all(f.0.join("lib")).unwrap();
    std::fs::write(
        f.0.join("lib/util.py"),
        "def two():\n    return 2\ndef three():\n    return 3\n",
    )
    .unwrap();
    // A function that avoids the exact-duplicate cluster of the trivial ones.
    std::fs::write(
        f.0.join("lib/unique.py"),
        "def unique_one():\n    a = 41\n    b = 43\n    c = 47\n    return a * b - c\n",
    )
    .unwrap();
    f.ingest_tree();
    let r = f.storage().open_revision(RevisionSelector::Latest).unwrap();
    let files = json!({"select":{"kind":"file"}});
    let util_bytes = std::fs::read(f.0.join("lib/util.py")).unwrap().len();
    let result = r
        .retrieve(
            request(
                files.clone(),
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"size","dimension":{"numeric_bucket":{"field":"size_bytes","boundaries":[util_bytes as f64]}}}],
                    "measures":[{"id":"n","measure":"count"}],
                    "order":[{"by":{"measure":"n"},"direction":"desc"}],
                    "totals":{"grand":true,"page":true}}}})],
            ),
        )
        .unwrap();
    let data = &result.outputs[0].data;
    // Lower-inclusive, upper-exclusive buckets: the file exactly at the
    // boundary belongs to the >= bucket.
    let labels: Vec<String> = data["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["members"]["size"].as_str().unwrap().to_string())
        .collect();
    assert!(labels.contains(&format!("<{util_bytes}")));
    assert!(labels.contains(&format!(">={util_bytes}")));
    assert_eq!(data["grand_totals"]["n"], result.matches);
    assert_eq!(data["total_scope"], "all_matches_before_having");
    // HAVING removes every cell while grand totals still cover all matches.
    let filtered = r
        .retrieve(
            request(
                files.clone(),
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"size","dimension":{"field":"size_bytes"}}],
                    "measures":[{"id":"n","measure":"count"}],
                    "cell_filter":{"measure":{"id":"n","op":"gt","value":100.0}}}}})],
            ),
        )
        .unwrap();
    assert_eq!(filtered.outputs[0].data["total"], 0);
    assert_eq!(filtered.outputs[0].data["grand_totals"]["n"], result.matches);
    // Drill-through can be disabled to save response bytes.
    let bare = r
        .retrieve(
            request(
                files,
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"size","dimension":{"field":"size_bytes"}}],
                    "drill_through":"disabled"}}})],
            ),
        )
        .unwrap();
    assert!(bare.outputs[0].data["items"][0].get("contributors").is_none());
    // Unknown members are typed and distinct from null or a user label.
    let symbols = json!({"select":{"kind":"symbol","filter":{"field":{"field":"kind","op":"eq","value":"Function"}}}});
    // No duplicate families exist, so the cluster dimension has no captured
    // members: a distinct function avoids the duplicate cluster and
    // contributes the reserved unknown member.
    let with_unknown = r
        .retrieve(
            request(
                symbols.clone(),
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"cl","dimension":{"related_entity":{"relation_path":[{"relation":"member_of_cluster"}],"attribute":"algorithm"}}}],
                    "measures":[{"id":"n","measure":"count"}],
                    "missing":"include_unknown"}}})],
            ),
        )
        .unwrap();
    let cells = with_unknown.outputs[0].data["items"].as_array().unwrap();
    let unknown_cells: Vec<_> = cells
        .iter()
        .filter(|c| c["members"]["cl"].get("$unknown").is_some())
        .collect();
    assert_eq!(unknown_cells.len(), 1);
    // The unknown cell holds exactly the symbols outside any captured
    // cluster, and per-cell counts stay distinct.
    let member_counts: u64 = cells
        .iter()
        .filter(|c| c["members"]["cl"].get("$unknown").is_none())
        .map(|c| c["measures"]["n"].as_u64().unwrap())
        .sum();
    assert_eq!(
        unknown_cells[0]["measures"]["n"].as_u64().unwrap() + member_counts,
        with_unknown.matches as u64
    );
    // Pivot layouts report row/column keys without allocating a dense matrix.
    let pivot = r
        .retrieve(
            request(
                symbols.clone(),
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"file","dimension":{"field":"file.path"}},{"id":"lang","dimension":{"field":"language"}}],
                    "layout":{"pivot":{"rows":["file"],"columns":["lang"]}}}}})],
            ),
        )
        .unwrap();
    let p = &pivot.outputs[0].data["pivot"];
    assert!(!p["rows"].as_array().unwrap().is_empty());
    for cell in pivot.outputs[0].data["items"].as_array().unwrap() {
        assert!(cell.get("row_key").is_some() && cell.get("column_key").is_some());
    }
    // Unsupported dimensions fail loudly instead of producing an empty cube.
    let err = r
        .retrieve(
            request(
                symbols,
                vec![json!({"id":"cube","output":{"cube":{
                    "dimensions":[{"id":"nope","dimension":{"field":"no_such_field"}}]}}})],
            ),
        )
        .unwrap_err();
    assert_eq!(err.code, "UnsupportedQuery");
    // Facet self-exclusion removes exactly the tagged dimension's filter.
    let filtered_symbols = json!({"select":{"kind":"symbol","filter":{"and":[
        {"field":{"field":"file.path","op":"prefix","value":"lib"}},
        {"field":{"field":"kind","op":"eq","value":"Function"}}]}}});
    let facet = r
        .retrieve(
            request(
                filtered_symbols,
                vec![json!({"id":"f","output":{"facets":{"dimensions":["file.path"],"self_excluding":["file.path"],"page":{"limit":50}}}})],
            ),
        )
        .unwrap();
    let rows = facet.outputs[0].data["items"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|row| row["self_excluded"] == json!(true)
                && row["dimension"] == "file.path"
                && row["member"] == "app.py"
                && row["count"].as_u64().unwrap() >= 1),
        "{}",
        serde_json::to_string(&facet.outputs[0].data).unwrap()
    );
}

#[test]
fn revisions_list_source_ranges_context_and_stored_only_views() {
    let f = Fixture::new();
    f.ingest("def save(x):\n    return x\n");
    std::fs::write(f.0.join("u.py"), "text = \"café\"\n").unwrap();
    f.ingest("def other():\n    return 2\n");
    let storage = f.storage();
    let page = storage
        .list_revisions(RevisionListRequest {
            limit: 1,
            after: None,
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.continuation.is_some());
    let rest = storage
        .list_revisions(RevisionListRequest {
            limit: 10,
            after: page.continuation.clone(),
        })
        .unwrap();
    assert_eq!(page.total, rest.total);
    // A stored-only relationship view is explicitly unavailable, never
    // substituted from derivation.
    let r = storage.open_revision(RevisionSelector::Latest).unwrap();
    let symbols = json!({"select":{"kind":"symbol"}});
    assert_eq!(
        r.retrieve(request(
            symbols.clone(),
            vec![json!({"id":"v","output":{"view":{"kind":"relationships","availability":"stored_only"}}})],
        ))
        .unwrap_err()
        .code,
        "ProjectionUnavailable"
    );
    let first_symbol = r
        .retrieve(request(symbols.clone(), vec![entities(50)]))
        .unwrap()
        .outputs[0]
        .data["items"][0]["id"]
        .clone();
    // Byte ranges are half-open and echoed; context expansion preserves the
    // requested range while reporting the expanded returned range.
    let src = r
        .retrieve(request(
            json!({"select":{"kind":"symbol","filter":{"ids":[first_symbol]}}}),
            vec![json!({"id":"s","output":{"detail":{"source":{"range":{"bytes":{"start":8,"end":20}},"context_lines":1}}}})],
        ))
        .unwrap();
    let data = &src.outputs[0].data;
    assert_eq!(data["requested"]["start"], 8);
    assert_eq!(data["requested"]["end"], 20);
    assert_eq!(data["returned"]["end"], 26, "context extends to the line end");
    // The café character occupies two bytes: a range cutting inside it is
    // unaligned UTF-8 and rejected rather than returning damaged text.
    let u = "text = \"café\"\n";
    let idx = u.find("caf").unwrap() as u64;
    let select_file = json!({"select":{"kind":"file","filter":{"field":{"field":"path","op":"eq","value":"u.py"}}}});
    let bad = r
        .retrieve(request(
            select_file.clone(),
            vec![json!({"id":"s","output":{"detail":{"source":{"range":{"bytes":{"start":idx,"end":idx+4}}}}}})],
        ))
        .unwrap_err();
    assert_eq!(bad.code, "InvalidRequest");
    // Whole-file detail echoes captured source with a content hash.
    let wf = r
        .retrieve(request(
            select_file,
            vec![json!({"id":"s","output":{"detail":{"source":{"range":"whole_file","context_lines":0}}}})],
        ))
        .unwrap();
    assert_eq!(wf.outputs[0].data["requested"]["whole_file"], json!(true));
    assert!(wf.outputs[0].data["content_hash"].as_str().is_some());
}
