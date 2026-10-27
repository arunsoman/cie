//! One process owns the web ingestion job and exports its compatible artifacts.
//! All structural consumers use the captured facts/snapshot, not another CLI.
use cie_ingest::IngestOutcome;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) fn stage(name: &str, state: &str, detail: &str) {
    use std::io::Write;
    let line = format!(
        "CIE_STAGE {}",
        json!({"key":name,"state":state,"detail":detail})
    );
    eprintln!("{}", line);
    let _ = std::io::stderr().flush();
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

pub fn export(root: &Path, raw: IngestOutcome, out: &Path, _docs_budget_ms: u64) -> std::io::Result<()> {
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
    // RFC-0026 §11: overlap's all-pairs matrix is still the brute-force
    // algorithm (the lattice replacement in §§1-8 is not implemented), but
    // it no longer runs here — it was blocking ingest/publish on it for as
    // long as the matrix took, which is exactly the OOM/hang this RFC
    // started from. Deferred the same way as drift (§10): a placeholder
    // publishes immediately, and the real computation runs the first time
    // something actually requests `overlap`, via the on-demand build in
    // cie-storage::retrieval (see cie_ingest::OnDemandDeferredBuilder).
    stage("overlap", "skipped", "Not checked during this scan.");
    let overlap = json!({"pairs":[],"unmapped":[],"min_shared":1,"incomplete_reason":"deferred_on_demand"});
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
    // RFC-0026 §9/§10: drift is no longer computed here at all. Ingestion
    // publishes a "deferred_on_demand" placeholder immediately; the real
    // documentation-vs-code check runs the first time it's actually
    // requested (via cie-storage::retrieval's coalesced, persisted
    // on-demand build — see cie-storage/src/retrieval/deferred.rs and
    // cie_ingest::DriftDeferredBuilder), under its own liberal background
    // budget rather than this ingest run's. `_docs_budget_ms` is kept on
    // the signature for CLI/API stability but no longer used to bound
    // anything here.
    stage("drift", "skipped", "Not checked during this scan.");
    let drift = json!({"rev":rev,"docs":[],"claims":[],"mappings":[],
        "verifications":[],"reverse_drift":[],"aggregate":{},"complete":false,
        "generated_ns":cie_core::types::now_ns(),"incomplete_reason":"deferred_on_demand"});
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
