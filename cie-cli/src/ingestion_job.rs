//! One process owns the web ingestion job and exports its compatible artifacts.
//! All structural consumers use the captured facts/snapshot, not another CLI.
use cie_core::types::SymbolId;
use cie_ingest::IngestOutcome;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) fn stage(name: &str, state: &str, detail: &str) {
    eprintln!(
        "CIE_STAGE {}",
        json!({"key":name,"state":state,"detail":detail})
    );
}
fn optional<T: serde::Serialize>(name: &str, f: impl FnOnce() -> Result<T, String>) -> Value {
    stage(name, "running", "Processing captured repository evidence…");
    match f() {
        Ok(v) => {
            stage(name, "done", "Ready.");
            serde_json::to_value(v).unwrap_or(Value::Null)
        }
        Err(e) => {
            stage(name, "skipped", &e);
            Value::Null
        }
    }
}

pub fn export(root: &Path, raw: IngestOutcome, out: &Path) -> std::io::Result<()> {
    let outcome = cie_ingest::materialize(raw)?;
    let rev = outcome.rev;
    let artifacts = cie_storage::artifacts::RevisionArtifacts::open(root)?;
    let polyglot = optional("polyglot", || {
        artifacts
            .get::<Value>(rev, cie_storage::artifacts::Artifact::Polyglot)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no polyglot artifact for this revision".to_string())
    });
    let viewspec = optional("map", || {
        cie_viz::produce(&outcome.snapshot, &cie_viz::ProduceOptions::default())
            .map_err(|e| format!("{e:?}"))
    });
    let concepts = serde_json::to_value(&outcome.snapshot.concepts)?;
    let stack = serde_json::to_value(&outcome.snapshot.stack)?;
    let deps = optional("deps", || {
        let table = cie_analysis::deps::scan_manifests(root);
        let imports = cie_ingest::stream::cached_imports(root).map_err(|e| e.to_string())?;
        let usage = cie_analysis::deps::usage(&table, &imports);
        Ok(json!({"table":table,"usage":usage,"what_if":null}))
    });
    let cochange = optional("cochange", || {
        cie_ingest::cochange::co_change_report(root, &Default::default()).map_err(|e| e.reason)
    });
    let overlap = optional("overlap", || {
        let names: BTreeMap<SymbolId, String> = outcome
            .store
            .symbols
            .iter()
            .map(|s| (s.id, s.qual_name.clone()))
            .collect();
        let mappings: BTreeMap<String, BTreeSet<SymbolId>> = outcome
            .snapshot
            .concepts
            .nodes
            .iter()
            .filter(|n| n.kind != cie_concepts::ConceptKind::Domain && !n.symbols.is_empty())
            .map(|n| (n.label.clone(), n.symbols.iter().copied().collect()))
            .collect();
        let matrix = cie_concepts::overlap_matrix(&mappings, 1);
        let name_of = |ids: &BTreeSet<SymbolId>| -> Vec<String> {
            ids.iter().filter_map(|id| names.get(id).cloned()).collect()
        };
        let pairs:Vec<Value>=matrix.pairs.iter().map(|p|json!({"a":p.a,"b":p.b,"shared":name_of(&p.shared),"only_a":name_of(&p.only_a),"only_b":name_of(&p.only_b),"similarity":p.similarity()})).collect();
        Ok(json!({"pairs":pairs,"unmapped":matrix.unmapped,"min_shared":1}))
    });
    let fault = if outcome
        .snapshot
        .state_model
        .as_ref()
        .is_some_and(|m| !m.transitions.is_empty())
    {
        use cie_ingest::state_model_store::StateModelStoreExt;
        let store = cie_store::Store::open(root)?;
        let model = store.load_state_model(rev)?;
        Some(cie_fault::lineage_from_state_model::analyze_model(
            rev,
            model.as_ref(),
            cie_fault::RoundLimits {
                max_faults: 2,
                budget_ns: 500_000_000,
            },
        ))
    } else {
        None
    };
    let mut report = serde_json::to_value(&outcome.snapshot)?;
    if let Some(fault) = fault {
        report["fault_analysis"] = serde_json::to_value(fault)?;
    }
    let docs = documents(root)?;
    let drift = if docs.is_empty() {
        stage("drift", "skipped", "No bounded documentation inputs found.");
        Value::Null
    } else {
        stage(
            "drift",
            "running",
            "Checking documentation against captured facts…",
        );
        match cie_ingest::run_drift_from_outcome(root, &docs, outcome, None, false) {
            Ok((drift, _)) => {
                stage("drift", "done", "Documentation checked.");
                serde_json::to_value(drift)?
            }
            Err(e) => {
                stage("drift", "skipped", &e.to_string());
                Value::Null
            }
        }
    };
    let runtime = root.join("runtime-data/runtime.json");
    let runtime = std::fs::metadata(&runtime)
        .ok()
        .filter(|m| m.len() <= 4 * 1024 * 1024)
        .and_then(|_| std::fs::read(runtime).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    let result = json!({"report":report,"viewspec":viewspec,"concepts":concepts,"stack":stack,
        "polyglotSpecs":polyglot,"deps":deps,"cochange":cochange,"overlap":overlap,"drift":drift,"runtime":runtime});
    stage("publish", "running", "Writing the web artifact bundle…");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    cie_storage::artifacts::put_web_bundle(root, rev, &result)?;
    let tmp = out.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(&result)?)?;
    std::fs::rename(tmp, out)?;
    stage("publish", "done", "Repository artifacts ready for the web app.");
    Ok(())
}

fn documents(root: &Path) -> std::io::Result<Vec<(String, String)>> {
    let mut paths = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        if depth > 14 {
            continue;
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                if !cie_ingest::IGNORED.contains(&entry.file_name().to_string_lossy().as_ref()) {
                    dirs.push((entry.path(), depth + 1));
                }
            } else if ty.is_file()
                && matches!(
                    entry.path().extension().and_then(|x| x.to_str()),
                    Some("md" | "rst" | "adoc")
                )
                && entry.metadata()?.len() <= 256 * 1024
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort_by_key(|p| (p.components().count(), p.clone()));
    Ok(paths
        .into_iter()
        .take(20)
        .filter_map(|p| {
            std::fs::read_to_string(&p)
                .ok()
                .map(|s| (p.to_string_lossy().to_string(), s))
        })
        .collect())
}
