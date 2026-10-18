use cie_incremental::snapshot::SnapshotStoreExt;
use cie_ingest::{materialize, run_ingest, IngestOptions};
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "cie-ir-{}-{}",
            std::process::id(),
            cie_core::types::now_ns()
        ));
        fs::create_dir_all(&p).unwrap();
        for (path,src)in [("a.py","def first(n):\n    return n\n"),("b.py","def guarded(n):\n    if n <= 0:\n        return False\n    while n > 0:\n        n = n - 1\n    return True\n")]{fs::write(p.join(path),src).unwrap();}
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn raw_ir_is_retained_reused_and_uses_the_correct_cross_file_locations() {
    let f = Fixture::new();
    let first = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    assert_eq!(first.snapshot.facts.len(), 2);
    let facts = first
        .snapshot
        .facts
        .iter()
        .find(|f| f.path == "b.py")
        .unwrap();
    assert!(!facts.ifs.is_empty());
    assert!(!facts.loops.is_empty());
    let symbol = first
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "guarded")
        .unwrap();
    assert_eq!(symbol.loc.file.0, 1);
    let container = first
        .snapshot
        .symbols
        .iter()
        .find(|s| {
            s.kind == cie_semantic::facts::SymbolKind::Module && s.loc.file == symbol.loc.file
        })
        .unwrap();
    assert_eq!(symbol.parent, Some(container.id));
    assert_eq!(facts.ifs[0].symbol, symbol.id);
    assert_eq!(facts.ifs[0].loc.file, symbol.loc.file);
    assert_eq!(facts.loops[0].loc.file, symbol.loc.file);
    assert_eq!(facts.ifs[0].line, 2);
    let expected = serde_json::to_value(&first.snapshot.facts).unwrap();
    let noop = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    assert!(noop.snapshot.incremental);
    let loaded = materialize(noop).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.snapshot.facts).unwrap(),
        expected
    );
}
#[test]
fn tombstoned_ir_is_removed_and_surviving_source_locations_are_remapped() {
    let f = Fixture::new();
    let first = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    let mut snapshot = first.snapshot;
    let store = cie_store::Store::open(&f.0).unwrap();
    store.mark_tombstone("a.py").unwrap();
    store.purge_tombstoned(&mut snapshot).unwrap();
    assert_eq!(snapshot.files, vec!["b.py"]);
    assert_eq!(snapshot.facts.len(), 1);
    assert_eq!(snapshot.facts[0].path, "b.py");
    assert!(snapshot.symbols.iter().all(|s| s.loc.file.0 == 0));
    assert!(snapshot.facts[0].ifs.iter().all(|s| s.loc.file.0 == 0));
    assert!(snapshot.facts[0].loops.iter().all(|s| s.loc.file.0 == 0));
}
#[test]
fn legacy_snapshot_without_raw_ir_remains_readable_without_invented_facts() {
    let f = Fixture::new();
    let first = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    let mut value = serde_json::to_value(first.snapshot).unwrap();
    value.as_object_mut().unwrap().remove("facts");
    value.as_object_mut().unwrap().remove("file_table");
    let legacy: cie_incremental::snapshot::RevSnapshot = serde_json::from_value(value).unwrap();
    assert!(legacy.facts.is_empty());
    assert!(!legacy.symbols.is_empty());
}

#[test]
fn resource_effects_in_second_file_reference_its_own_global_function() {
    let registry = cie_ingest::default_registry();
    let files=vec![("a.c".into(),cie_semantic::LangId::C,"int first(void) { return 0; }\n".into()),("b.c".into(),cie_semantic::LangId::C,"#include <stdlib.h>\nint second(void) {\n char *buf = malloc(64);\n free(buf);\n return 0;\n}\n".into())];
    let extracted = cie_adapters::extract_project(&files, &registry);
    let symbol = extracted
        .store
        .symbols
        .iter()
        .find(|s| s.name == "second")
        .unwrap();
    let facts = extracted.per_file.iter().find(|f| f.path == "b.c").unwrap();
    assert!(!facts.resource_effects.is_empty());
    assert!(facts.resource_effects.iter().all(|r| r.symbol == symbol.id));
}

#[test]
fn evidence_schema_upgrade_reanalyzes_unchanged_sources_without_stale_summary_reuse() {
    let f = Fixture::new();
    let first = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    let before = fs::read(f.0.join("b.py")).unwrap();
    let manifest = f.0.join(".cie/extraction-schema.json");
    assert!(manifest.exists());
    fs::write(manifest, b"0").unwrap();
    let upgraded = run_ingest(&f.0, &IngestOptions::default()).unwrap();
    assert!(!upgraded.snapshot.incremental);
    assert!(upgraded.rev > first.rev);
    assert!(!upgraded.snapshot.facts.is_empty());
    assert_eq!(upgraded.summary_reuse.reused_files, 0);
    assert_eq!(fs::read(f.0.join("b.py")).unwrap(), before);
    assert!(upgraded
        .snapshot
        .warnings
        .iter()
        .any(|w| w.contains("schema changed")));
}

#[test]
fn tombstones_remove_bound_contracts_unattached_rules_and_reuse_keys() {
    let f = Fixture::new();
    let mut snapshot = run_ingest(&f.0, &IngestOptions::default())
        .unwrap()
        .snapshot;
    let subject = snapshot
        .symbols
        .iter()
        .find(|s| s.name == "first")
        .unwrap()
        .id;
    snapshot.contracts.push(cie_contract::Contract {
        id: 999,
        obligation: cie_contract::ObligationKind::MustLog,
        expr: "first must log".into(),
        authority: cie_contract::Authority::Explicit,
        symbols: vec![subject],
        source: cie_contract::ContractSource::Docstring,
    });
    let rule = cie_concepts::RuleRef {
        contract: 999,
        kind: "log".into(),
        authority: cie_contract::Authority::Explicit,
    };
    snapshot.concepts.unattached_rules.push(rule.clone());
    snapshot.concepts.nodes[0].rules.push(rule);
    snapshot
        .concepts
        .unattached_behaviors
        .push(cie_concepts::BehaviorRef {
            summary: subject,
            kind: "first".into(),
            confidence: 0.5,
        });
    let store = cie_store::Store::open(&f.0).unwrap();
    store.mark_tombstone("a.py").unwrap();
    store.purge_tombstoned(&mut snapshot).unwrap();
    assert!(!snapshot.contracts.iter().any(|c| c.id == 999));
    assert!(!snapshot
        .concepts
        .unattached_rules
        .iter()
        .any(|r| r.contract == 999));
    assert!(!snapshot
        .concepts
        .nodes
        .iter()
        .any(|n| n.rules.iter().any(|r| r.contract == 999)));
    assert!(!snapshot
        .concepts
        .unattached_behaviors
        .iter()
        .any(|b| b.summary == subject));
    assert!(!snapshot.summary_cache.iter().any(|k| k.path == "a.py"));
}
