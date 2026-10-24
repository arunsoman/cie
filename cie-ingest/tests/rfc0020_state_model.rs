use cie_incremental::snapshot::{RevSnapshot, SnapshotStoreExt};
use cie_ingest::{materialize, run_ingest, state_model_store::StateModelStoreExt, IngestOptions};
#[test]
fn filtered_model_persists_roundtrips_and_survives_noop_without_fabrication() {
    let root = std::env::temp_dir().join(format!(
        "cie-rfc20-{}-{}",
        std::process::id(),
        cie_core::types::now_ns()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("app.py"),"def authorize():\n    \"\"\"must authorize the caller before returning.\"\"\"\n    print('hello')\n    return True\n\ndef test_authorize():\n    assert authorize()\n").unwrap();
    let out = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(out
        .snapshot
        .contracts
        .iter()
        .any(|c| !c.verified_by.is_empty()));
    let view = out.snapshot.state_model.as_ref().unwrap();
    assert!(view.transitions.iter().all(|t| t.display_mode == "Fact"));
    assert!(view
        .transitions
        .iter()
        .flat_map(|t| &t.evidence)
        .all(|p| p.symbol.0 != u32::MAX));
    let store = cie_store::Store::open(&root).unwrap();
    assert!(store.load_state_model(out.rev).unwrap().is_some());
    let saved = store.load_snapshot(out.rev).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&saved.state_model).unwrap(),
        serde_json::to_value(view).unwrap()
    );
    let noop = materialize(run_ingest(&root, &IngestOptions::default()).unwrap()).unwrap();
    assert!(noop.snapshot.state_model.is_some());
    let mut wire = serde_json::to_value(saved).unwrap();
    wire.as_object_mut().unwrap().remove("state_model");
    assert!(serde_json::from_value::<RevSnapshot>(wire)
        .unwrap()
        .state_model
        .is_none());
    std::fs::write(root.join("app.py"), "def greet():\n    print('hello')\n").unwrap();
    assert!(run_ingest(&root, &IngestOptions::default())
        .unwrap()
        .snapshot
        .state_model
        .is_none());
    std::fs::remove_dir_all(root).unwrap();
}
