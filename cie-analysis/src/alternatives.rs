//! Architecture alternatives comparison (CIE-US US-059 engine close).
//!
//! "Compare extending a module, introducing a service or retaining
//! compatibility support — trade-offs explicit before commitment." The
//! impact ingredients ship; this module assembles the *comparison*: each
//! alternative lists affected capabilities/symbols (from the reverse-dep
//! impact), contract surfaces touched, data-boundary crossings, risks and
//! unresolved assumptions — with proposed consequences permanently
//! distinct from measured outcomes (a proposal is `proposed`; nothing
//! here is `measured` unless the caller attaches a workload-backed
//! citation from the `cie-evidence::workload` protocol).

use std::collections::BTreeSet;

use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

/// One alternative under comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alternative {
    pub name: String,
    /// Symbols directly affected (impact input; the engine computed set).
    pub affected_symbols: BTreeSet<SymbolId>,
    /// Contract surfaces touched (interface names / endpoints).
    pub contract_changes: Vec<String>,
    /// Data boundaries crossed (stores, queues, external systems).
    pub data_boundaries: Vec<String>,
    /// Risks the caller records (cited, not vibes).
    pub risks: Vec<String>,
    /// Unresolved assumptions (each must stay visible per the story).
    pub assumptions: Vec<String>,
}

/// One row of the comparison (all dimensions side by side).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonRow {
    pub alternative: String,
    pub affected_symbol_count: usize,
    pub contract_changes: Vec<String>,
    pub data_boundaries: Vec<String>,
    pub risks: Vec<String>,
    pub assumptions: Vec<String>,
    /// `proposed` — or a citation string when the caller attaches a
    /// workload-backed measurement (never generated here).
    pub outcome_class: OutcomeClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeClass {
    /// Predicted from structure; not measured.
    Proposed,
    /// Backed by an imported workload timing (citation carried).
    Measured(String),
}

impl OutcomeClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutcomeClass::Proposed => "proposed",
            OutcomeClass::Measured(_) => "measured",
        }
    }
}

/// The assembled comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlternativesComparison {
    pub rows: Vec<ComparisonRow>,
    /// Permanent disclosure: what is and is not measured here.
    pub disclosure: String,
    /// Shared assumptions across all alternatives (the comparison frame).
    pub frame_assumptions: Vec<String>,
}

/// Assemble the comparison. Deterministic: rows in input order, sets
/// sorted. `measurements` maps an alternative name to a workload-backed
/// citation; anything absent stays `proposed`.
pub fn compare_alternatives(
    alternatives: &[Alternative],
    measurements: &std::collections::BTreeMap<String, String>,
) -> AlternativesComparison {
    let rows = alternatives
        .iter()
        .map(|a| ComparisonRow {
            alternative: a.name.clone(),
            affected_symbol_count: a.affected_symbols.len(),
            contract_changes: {
                let mut v = a.contract_changes.clone();
                v.sort();
                v
            },
            data_boundaries: {
                let mut v = a.data_boundaries.clone();
                v.sort();
                v
            },
            risks: {
                let mut v = a.risks.clone();
                v.sort();
                v
            },
            assumptions: {
                let mut v = a.assumptions.clone();
                v.sort();
                v
            },
            outcome_class: match measurements.get(&a.name) {
                Some(citation) => OutcomeClass::Measured(citation.clone()),
                None => OutcomeClass::Proposed,
            },
        })
        .collect();
    AlternativesComparison {
        rows,
        disclosure: "proposed consequences are structural predictions; a measured \
                     outcome requires an imported workload run (cie-evidence::workload)"
            .into(),
        frame_assumptions: vec![
            "impact counts come from the parsed call graph of the analysed revision"
                .into(),
        ],
    }
}

/// Convenience: build an alternative from the shipped impact output
/// (reverse deps of a seed symbol) so callers do not hand-roll sets.
pub fn alternative_from_impact(
    name: &str,
    seed: SymbolId,
    reverse_deps: &std::collections::BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    contract_changes: Vec<String>,
    data_boundaries: Vec<String>,
    risks: Vec<String>,
    assumptions: Vec<String>,
) -> Alternative {
    let mut affected: BTreeSet<SymbolId> = BTreeSet::new();
    affected.insert(seed);
    if let Some(callers) = reverse_deps.get(&seed) {
        affected.extend(callers.iter().copied());
        // second hop: callers-of-callers (transitive, still structural)
        for c in callers {
            if let Some(cc) = reverse_deps.get(c) {
                affected.extend(cc.iter().copied());
            }
        }
    }
    Alternative {
        name: name.into(),
        affected_symbols: affected,
        contract_changes,
        data_boundaries,
        risks,
        assumptions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alts() -> Vec<Alternative> {
        let mut rd = std::collections::BTreeMap::new();
        rd.insert(SymbolId(1), BTreeSet::from([SymbolId(2), SymbolId(3)]));
        rd.insert(SymbolId(2), BTreeSet::from([SymbolId(4)]));
        vec![
            alternative_from_impact(
                "extend module",
                SymbolId(1),
                &rd,
                vec!["auth::authorize signature".into()],
                vec!["orders-db".into()],
                vec!["blast radius grows".into()],
                vec!["callers stay in-process".into()],
            ),
            Alternative {
                name: "extract service".into(),
                affected_symbols: BTreeSet::from([SymbolId(1)]),
                contract_changes: vec!["new network API".into()],
                data_boundaries: vec!["orders-db", "payments-queue"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                risks: vec!["distributed failure modes".into()],
                assumptions: vec!["latency budget holds".into()],
            },
        ]
    }

    #[test]
    fn comparison_lists_every_dimension_per_alternative() {
        let cmp = compare_alternatives(&alts(), &Default::default());
        assert_eq!(cmp.rows.len(), 2);
        // extend-module impact is transitive: seed + callers + callers-of-callers
        assert_eq!(cmp.rows[0].affected_symbol_count, 4);
        assert_eq!(cmp.rows[1].affected_symbol_count, 1);
        assert!(cmp.rows[0].data_boundaries.contains(&"orders-db".to_string()));
        assert_eq!(cmp.rows[0].outcome_class, OutcomeClass::Proposed);
        // assumptions stay visible
        assert!(cmp.rows[1].assumptions.contains(&"latency budget holds".to_string()));
        assert!(cmp.disclosure.contains("measured"));
    }

    #[test]
    fn measured_requires_citation_and_never_generated() {
        let mut m = std::collections::BTreeMap::new();
        m.insert(
            "extract service".to_string(),
            "workload order_listing_bench @ r41".to_string(),
        );
        let cmp = compare_alternatives(&alts(), &m);
        assert_eq!(
            cmp.rows[1].outcome_class,
            OutcomeClass::Measured("workload order_listing_bench @ r41".into())
        );
        assert_eq!(cmp.rows[0].outcome_class, OutcomeClass::Proposed);
        assert_eq!(cmp.rows[1].outcome_class.as_str(), "measured");
    }
}
