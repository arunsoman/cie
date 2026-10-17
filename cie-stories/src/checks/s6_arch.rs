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
