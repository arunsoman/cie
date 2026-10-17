//! Testing & quality slice (CAP-3, v1 scope):
//! - Test-to-code mapping via call-graph reachability (CAP-3.5)
//! - Coverage gap analysis (CAP-3.1)
//! - Minimum test set for a change (CAP-17.3, acceptance #46)

use crate::findings::{Finding, FindingKind};
use cie_core::types::SymbolId;
use cie_semantic::facts::{SymbolKind, SymbolStore};
use std::collections::{BTreeMap, BTreeSet};

pub struct TestMap {
    /// test symbol id -> set of function symbols reachable from it
    pub reach: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// function symbol id -> tests that reach it
    pub tests_of: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
}

pub fn build_test_map(
    store: &SymbolStore,
    callees_of: &BTreeMap<SymbolId, Vec<SymbolId>>,
) -> TestMap {
    let mut reach: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();
    let mut tests_of: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();

    let tests: Vec<SymbolId> = store
        .symbols
        .iter()
        .filter(|s| s.is_test() || s.kind == SymbolKind::Test)
        .map(|s| s.id)
        .collect();

    for t in tests {
        // Forward closure over the call graph.
        let mut seen: BTreeSet<SymbolId> = BTreeSet::new();
        let mut queue = vec![t];
        while let Some(cur) = queue.pop() {
            for callee in callees_of.get(&cur).cloned().unwrap_or_default() {
                if seen.insert(callee) {
                    queue.push(callee);
                }
            }
        }
        seen.remove(&t);
        for f in &seen {
            tests_of.entry(*f).or_default().insert(t);
        }
        reach.insert(t, seen);
    }

    TestMap { reach, tests_of }
}

/// Coverage gap analysis: functions with no reaching test (CAP-3.1).
pub fn coverage_gaps(store: &SymbolStore, map: &TestMap, rev: u64) -> Vec<Finding> {
    let mut findings = vec![];
    let mut seq = 0u32;
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() {
            continue;
        }
        // Skip entry points (not directly testable) and dunder methods.
        if sym.name.starts_with("__") {
            continue;
        }
        if map.tests_of.contains_key(&sym.id) {
            continue;
        }
        seq += 1;
        findings.push(Finding {
            id: format!("F-{rev}-T{seq:04}"),
            kind: FindingKind::CoverageGap,
            severity: FindingKind::CoverageGap.default_severity(),
            symbol: sym.id,
            symbol_name: sym.qual_name.clone(),
            message: format!(
                "'{}' is not reached by any test in the analyzed set",
                sym.qual_name
            ),
            evidence_lines: vec![sym.loc.start_line],
            remediation: "Add a test that exercises this function".into(),
            authority: cie_contract::Authority::Verified,
            rule: "test/TQ-001".into(),
            tags: vec!["testing".into()],
        });
    }
    findings
}

/// Minimum test set for changed symbols (CAP-17.3): greedy set cover over
/// the tests' reachability sets. Empty changed set -> empty plan.
pub fn minimum_test_set(map: &TestMap, changed: &[SymbolId]) -> Vec<SymbolId> {
    let changed: BTreeSet<SymbolId> = changed.iter().copied().collect();
    if changed.is_empty() {
        return vec![];
    }
    let mut uncovered = changed.clone();
    let mut chosen: Vec<SymbolId> = vec![];
    loop {
        if uncovered.is_empty() {
            break;
        }
        // Pick the test covering the most uncovered symbols.
        let mut best: Option<(SymbolId, usize)> = None;
        for (t, reach) in &map.reach {
            let covers = reach.intersection(&uncovered).count();
            if covers > 0 && best.map(|(_, c)| covers > c).unwrap_or(true) {
                best = Some((*t, covers));
            }
        }
        match best {
            Some((t, _)) => {
                if let Some(r) = map.reach.get(&t) {
                    uncovered.retain(|s| !r.contains(s));
                }
                chosen.push(t);
            }
            None => break,
        }
    }
    chosen
}

/// "What breaks if I change this?" (CAP-10.2) — transitive callers.
pub fn affected_by_change(
    reverse_deps: &BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    seeds: &[SymbolId],
) -> BTreeSet<SymbolId> {
    let rd = cie_incremental::ReverseDeps {
        callers_of: reverse_deps.clone(),
    };
    rd.transitive_callers(seeds)
}
