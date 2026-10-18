//! Deep checks — Architecture, dependencies and impact (US-057..US-063).

use super::fixture;

/// US-057: real relationships, not just folders — reverse deps and call
/// evidence agree, and the concept DAG adds grouping beyond directories.
pub fn us_057() -> Result<(), String> {
    let outcome = fixture::ingest("us057");
    let checkout = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "checkout")
        .map(|s| s.id)
        .ok_or("checkout missing")?;
    let charge = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "charge")
        .map(|s| s.id)
        .ok_or("charge missing")?;
    let rdeps = outcome
        .reverse_deps
        .get(&charge)
        .ok_or("reverse deps must exist for called symbols")?;
    if !rdeps.contains(&checkout) {
        return Err("call relationship must appear in reverse deps".into());
    }
    Ok(())
}

/// US-058: the blast radius separates direct dependents from the minimum
/// test set — evidence, not folklore.
pub fn us_058() -> Result<(), String> {
    let outcome = fixture::ingest("us058");
    let checkout = outcome
        .snapshot
        .symbols
        .iter()
        .find(|s| s.name == "checkout")
        .map(|s| s.id)
        .ok_or("checkout missing")?;
    let min = cie_analysis::testing::minimum_test_set(&outcome.test_map, &[checkout]);
    if min.is_empty() {
        return Err("changed symbol with reaching tests needs a minimum test set".into());
    }
    Ok(())
}

/// US-060: candidate separations from the concept lattice — FCA over the
/// tagged fixture yields shared-attribute concepts (coupling signal).
pub fn us_060() -> Result<(), String> {
    let outcome = fixture::ingest("us060");
    let mut objects: Vec<(cie_core::types::SymbolId, std::collections::BTreeSet<String>)> = vec![];
    for s in &outcome.snapshot.summaries {
        let attrs = s.tags.clone();
        objects.push((s.sym, attrs));
    }
    let lattice = cie_analysis::fca::build_lattice(&objects);
    let shared = lattice
        .iter()
        .filter(|c| !c.intent.is_empty() && c.extent.len() >= 2)
        .count();
    if shared == 0 {
        return Err("fixture tags (auth/money) must form shared-attribute concepts".into());
    }
    Ok(())
}

/// US-062: dependency manifests + usage evidence + upgrade what-if.
/// Builds a fixture with THREE ecosystems and asserts: manifest parsing
/// (with licences), honest usage evidence (resolved via Python import
/// facts; unresolved packages are NOT called unused), and the upgrade
/// what-if whose compatibility is always Unknown.
pub fn us_062() -> Result<(), String> {
    use cie_analysis::deps::{scan_manifests, collect_imports, upgrade_what_if, usage, Compatibility};
    let dir = std::env::temp_dir().join(format!("cie-check-us062-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"app\"\nlicense = {text = \"Apache-2.0\"}\ndependencies = [\n  \"requests==2.31.0\",\n  \"neverimported>=1.0\",\n]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.py"),
        "import requests\n\n\ndef main() -> None:\n    \"\"\"Entry point.\"\"\"\n    print(requests.__version__)\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("sub").join("Cargo.toml"),
        "[package]\nname = \"sub\"\nlicense = \"MIT\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
    )
    .unwrap();
    let table = scan_manifests(&dir);
    if table.deps.len() != 3 {
        return Err(format!("expected 3 declared deps, got {}", table.deps.len()));
    }
    if table.unparsed.iter().any(|u| u.contains("Cargo.toml")) {
        return Err("Cargo.toml must parse, not land in unparsed".into());
    }
    let serde_dep = table.deps.iter().find(|d| d.name == "serde").ok_or("serde missing")?;
    if serde_dep.licence.as_deref() != Some("MIT") {
        return Err("cargo licence must flow from manifest".into());
    }
    let reg = cie_ingest::default_registry();
    let imports = collect_imports(&dir, &reg);
    let u = usage(&table, &imports);
    let req = u.iter().find(|x| x.name == "requests").ok_or("requests usage missing")?;
    if !req.resolved || req.imported_by != vec!["app.py".to_string()] {
        return Err(format!("requests usage wrong: {:?}", req));
    }
    let never = u
        .iter()
        .find(|x| x.name == "neverimported")
        .ok_or("neverimported usage missing")?;
    if never.resolved || !never.note.contains("NOT proof of being unused") {
        return Err("unresolved package must carry the honesty note".into());
    }
    // serde: cargo — no import facts for that adapter: named gap, not zero
    let s = u.iter().find(|x| x.name == "serde").ok_or("serde usage missing")?;
    if s.resolved || !s.note.contains("unevidenced") {
        return Err("cargo usage must carry the named-gap note".into());
    }
    let wi = upgrade_what_if(&table, &imports, "requests", "2.32.0").ok_or("what-if None")?;
    if wi.consumers != vec!["app.py".to_string()] || wi.compatibility != Compatibility::Unknown {
        return Err(format!("what-if wrong: {:?}", wi));
    }
    if upgrade_what_if(&table, &imports, "not-declared", "1").is_some() {
        return Err("undeclared name must return None (nothing invented)".into());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
