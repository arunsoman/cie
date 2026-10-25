//! Capability-overlap detection (CIE-US US-106 engine close).
//!
//! "Does the requested capability already exist?" — the engine half:
//! concept-level overlap detection over the shipped lattice. Two
//! concepts overlap when their symbol sets intersect; the overlap matrix
//! reports the shared symbols so "B is X plus Y, and existing A is X"
//! becomes a structural comparison: what can be REUSED (the shared X)
//! and what the proposal ADDS (Y) — without duplicating X because names
//! differ.
//!
//! Deterministic: pairs sorted by (overlap desc, a, b); zero-overlap
//! pairs omitted. The verdict stays a comparison input — capability-
//! level equivalence judgement remains the caller's reasoning layer.

use std::collections::{BTreeMap, BTreeSet};

use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

/// One overlap finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overlap {
    pub a: String,
    pub b: String,
    /// Symbols BOTH concepts map to (the reusable X).
    pub shared: BTreeSet<SymbolId>,
    /// Symbols only one side maps to (the distinguishing remainder).
    pub only_a: BTreeSet<SymbolId>,
    pub only_b: BTreeSet<SymbolId>,
}

impl Overlap {
    /// Jaccard-style similarity over the symbol sets (0..1).
    pub fn similarity(&self) -> f64 {
        let union = self
            .shared
            .len()
            .saturating_add(self.only_a.len())
            .saturating_add(self.only_b.len());
        if union == 0 {
            return 0.0;
        }
        self.shared.len() as f64 / union as f64
    }
}

/// The overlap matrix for the mapped concepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapMatrix {
    pub pairs: Vec<Overlap>,
    /// Concepts with NO mappings at all (named gap — they cannot be
    /// compared structurally).
    pub unmapped: Vec<String>,
}

/// Compute the pairwise overlap matrix.
pub fn overlap_matrix(
    mappings: &BTreeMap<String, BTreeSet<SymbolId>>,
    min_shared: usize,
) -> OverlapMatrix {
    let names: Vec<&String> = mappings.keys().collect();
    let mut pairs = Vec::new();
    for i in 0..names.len() {
        let a = &mappings[names[i]];
        for j in (i + 1)..names.len() {
            let b = &mappings[names[j]];
            let shared: BTreeSet<SymbolId> = a.intersection(b).copied().collect();
            if shared.len() < min_shared {
                continue;
            }
            pairs.push(Overlap {
                a: names[i].clone(),
                b: names[j].clone(),
                only_a: a.difference(b).copied().collect(),
                only_b: b.difference(a).copied().collect(),
                shared,
            });
        }
    }
    pairs.sort_by(|x, y| {
        y.shared
            .len()
            .cmp(&x.shared.len())
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });
    let unmapped = mappings
        .iter()
        .filter(|(_, s)| s.is_empty())
        .map(|(n, _)| n.clone())
        .collect();
    OverlapMatrix { pairs, unmapped }
}

/// Reuse/add decomposition for the "B is X plus Y" question: given a
/// proposed capability's symbols and an existing concept's symbols,
/// report the reusable core and the genuine addition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReuseVerdict {
    pub existing: String,
    /// Symbols the proposal shares with the existing capability (reuse).
    pub reusable: BTreeSet<SymbolId>,
    /// Symbols the proposal adds beyond the existing capability (Y).
    pub adds: BTreeSet<SymbolId>,
    /// True when the proposal is fully covered by the existing
    /// capability (nothing new — the rebuild adds nothing).
    pub duplicate: bool,
}

pub fn reuse_verdict(
    existing: &str,
    existing_symbols: &BTreeSet<SymbolId>,
    proposed_symbols: &BTreeSet<SymbolId>,
) -> ReuseVerdict {
    let reusable: BTreeSet<SymbolId> = existing_symbols.intersection(proposed_symbols).copied().collect();
    let adds: BTreeSet<SymbolId> = proposed_symbols.difference(existing_symbols).copied().collect();
    let duplicate = adds.is_empty() && !proposed_symbols.is_empty();
    ReuseVerdict {
        existing: existing.to_string(),
        reusable,
        adds,
        duplicate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mappings() -> BTreeMap<String, BTreeSet<SymbolId>> {
        BTreeMap::from([
            (
                "export customers".into(),
                BTreeSet::from([SymbolId(1), SymbolId(2), SymbolId(3)]),
            ),
            (
                "export user data".into(),
                BTreeSet::from([SymbolId(1), SymbolId(2), SymbolId(9)]),
            ),
            (
                "refund flow".into(),
                BTreeSet::from([SymbolId(7)]),
            ),
            (
                "orphan capability".into(),
                BTreeSet::new(),
            ),
        ])
    }

    #[test]
    fn matrix_reports_shared_symbols_and_remainders() {
        let m = overlap_matrix(&mappings(), 2);
        assert_eq!(m.unmapped, vec!["orphan capability".to_string()]);
        assert_eq!(m.pairs.len(), 1);
        let p = &m.pairs[0];
        assert_eq!(p.a, "export customers");
        assert_eq!(p.b, "export user data");
        assert_eq!(p.shared.len(), 2);
        assert_eq!(p.only_a.len(), 1);
        assert_eq!(p.only_b, BTreeSet::from([SymbolId(9)]));
        assert!((p.similarity() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn min_shared_filters_weak_pairs() {
        assert!(overlap_matrix(&mappings(), 3).pairs.is_empty());
        // with min 1, refund pairs with nothing (no shared symbols)
        let m = overlap_matrix(&mappings(), 1);
        assert!(m.pairs.iter().all(|p| p.a != "refund flow" && p.b != "refund flow"));
    }

    #[test]
    fn reuse_verdict_splits_reusable_from_genuine_addition() {
        let existing = BTreeSet::from([SymbolId(1), SymbolId(2)]);
        // B = X plus Y
        let v = reuse_verdict(
            "export customers",
            &existing,
            &BTreeSet::from([SymbolId(1), SymbolId(2), SymbolId(5)]),
        );
        assert_eq!(v.reusable, BTreeSet::from([SymbolId(1), SymbolId(2)]));
        assert_eq!(v.adds, BTreeSet::from([SymbolId(5)]));
        assert!(!v.duplicate);
        // same symbols, different NAME → duplicate detected structurally
        let v = reuse_verdict("export customers", &existing, &existing);
        assert!(v.duplicate);
    }
}
