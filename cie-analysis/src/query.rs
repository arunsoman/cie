//! Condition composition + scoped selection queries (CIE-US US-010 /
//! US-024 engine close).
//!
//! US-010 — combine several conditions in one question: the composed
//! query is engine-side data (not routed through a model): each condition
//! is applied over the symbol universe and its inclusion decision is
//! reported per condition (the "each inclusion condition explained"
//! obligation). Missing audit-style evidence renders as a *suspected
//! gap* unless absence is established within scope — never a silent
//! drop.
//!
//! US-024 — ask about "these": an explicit selection set (the scoped
//! query). The answer identifies the selected entities, answers within
//! that scope, and explains an EMPTY selection (why nothing is there) —
//! the engine half of "even an empty cell can be selected".

use std::collections::BTreeSet;

use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

/// One row of the symbol universe the query runs over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRow {
    pub symbol: SymbolId,
    pub name: String,
    pub path: String,
    pub has_tests: bool,
    pub changed: bool,
    /// Audit-style evidence (e.g. "audit check mapped"); `None` means no
    /// evidence was FOUND — absence must then be established in scope.
    pub audit_evidence: Option<String>,
}

/// Composable conditions (the shipped engine queries, composed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Name contains (case-insensitive).
    NameContains(String),
    /// Reachable tests exist / do not exist.
    HasTests(bool),
    /// Changed in this revision's plan.
    Changed(bool),
    /// File path prefix.
    PathPrefix(String),
    /// Audit evidence present (the "lacks an identified audit check"
    /// condition composes as Not(AuditPresent)).
    AuditPresent,
}

/// Composition: all conditions must hold (AND) or any (OR). The result
/// explains per-condition inclusion either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Composition {
    All,
    Any,
}

/// One condition's decision for one row (the explanation ledger).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionVerdict {
    pub condition: Condition,
    pub matched: bool,
    /// Why: the engine fact that decided it.
    pub why: String,
}

/// The composed query result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub included: Vec<SymbolRow>,
    /// Per-row, per-condition verdicts (aligned with `included`).
    pub explanations: Vec<Vec<ConditionVerdict>>,
    /// Suspected gaps: rows excluded ONLY by unevidenced absence — the
    /// audit condition could not be established within scope, so the row
    /// is named as a suspected gap rather than silently dropped.
    pub suspected_gaps: Vec<String>,
}

fn decide(cond: &Condition, row: &SymbolRow) -> ConditionVerdict {
    let (matched, why) = match cond {
        Condition::NameContains(s) => {
            let m = row.name.to_lowercase().contains(&s.to_lowercase());
            (m, format!("name {} contains {s:?}", row.name))
        }
        Condition::HasTests(want) => {
            (row.has_tests == *want, format!("tests reaching symbol: {}", row.has_tests))
        }
        Condition::Changed(want) => (row.changed == *want, format!("changed in plan: {}", row.changed)),
        Condition::PathPrefix(p) => {
            let m = row.path.starts_with(p.as_str());
            (m, format!("path {} prefix {p:?}", row.path))
        }
        Condition::AuditPresent => match &row.audit_evidence {
            Some(e) => (true, format!("audit evidence: {e}")),
            None => (
                false,
                "no audit evidence found in scope — absence NOT established".into(),
            ),
        },
    };
    ConditionVerdict {
        condition: cond.clone(),
        matched,
        why,
    }
}

/// Run a composed query (US-010). Deterministic: rows in input order.
pub fn run_query(
    conditions: &[Condition],
    op: Composition,
    universe: &[SymbolRow],
) -> QueryResult {
    let mut included = Vec::new();
    let mut explanations = Vec::new();
    let mut suspected_gaps = Vec::new();
    for row in universe {
        let verdicts: Vec<ConditionVerdict> =
            conditions.iter().map(|c| decide(c, row)).collect();
        let holds = match op {
            Composition::All => verdicts.iter().all(|v| v.matched),
            Composition::Any => verdicts.iter().any(|v| v.matched),
        };
        if holds {
            included.push(row.clone());
            explanations.push(verdicts);
        } else if let Some(v) = verdicts.iter().find(|v| !v.matched) {
            // suspected gap: the deciding miss is an unevidenced absence
            if v.why.contains("NOT established") {
                suspected_gaps.push(format!(
                    "{} — excluded only by unevidenced absence: {}",
                    row.name, v.why
                ));
            }
        }
    }
    QueryResult {
        included,
        explanations,
        suspected_gaps,
    }
}

// -------------------------------------------------- US-024 scoped query

/// An explicit selection ("these"). Empty selection is first-class.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedQuery {
    pub selection: BTreeSet<SymbolId>,
}

/// The scoped answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopedAnswer {
    /// The selected entities, identified (no ambiguity).
    pub members: Vec<SymbolRow>,
    /// How many universe rows were outside the scope.
    pub outside: usize,
    /// For an empty selection: why nothing is there (engine facts about
    /// the universe, not a shrug).
    pub empty_reason: Option<String>,
}

/// Answer within the selection scope (US-024).
pub fn scoped_answer(sel: &ScopedQuery, universe: &[SymbolRow]) -> ScopedAnswer {
    let members: Vec<SymbolRow> = universe
        .iter()
        .filter(|r| sel.selection.contains(&r.symbol))
        .cloned()
        .collect();
    let empty_reason = if members.is_empty() {
        Some(if universe.is_empty() {
            "the universe itself is empty — nothing was ingested for this revision".into()
        } else if sel.selection.is_empty() {
            format!(
                "the selection is empty; the universe has {} rows — refine or clear the selection",
                universe.len()
            )
        } else {
            format!(
                "selection ids match no universe rows (universe has {} rows; ids may be stale)",
                universe.len()
            )
        })
    } else {
        None
    };
    ScopedAnswer {
        members,
        outside: universe.len().saturating_sub(sel.selection.len()),
        empty_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<SymbolRow> {
        vec![
            SymbolRow {
                symbol: SymbolId(1),
                name: "customer_export".into(),
                path: "web/export.py".into(),
                has_tests: false,
                changed: true,
                audit_evidence: Some("audit:export-logged".into()),
            },
            SymbolRow {
                symbol: SymbolId(2),
                name: "customer_query".into(),
                path: "web/query.py".into(),
                has_tests: true,
                changed: true,
                audit_evidence: None,
            },
            SymbolRow {
                symbol: SymbolId(3),
                name: "health_check".into(),
                path: "web/health.py".into(),
                has_tests: true,
                changed: false,
                audit_evidence: None,
            },
        ]
    }

    #[test]
    fn composed_conditions_explain_each_inclusion() {
        let q = vec![
            Condition::NameContains("customer".into()),
            Condition::Changed(true),
            Condition::AuditPresent,
        ];
        let r = run_query(&q, Composition::All, &rows());
        // row 1 matches all three; row 2 fails AuditPresent
        assert_eq!(r.included.len(), 1);
        assert_eq!(r.included[0].name, "customer_export");
        let exp = &r.explanations[0];
        assert_eq!(exp.len(), 3);
        assert!(exp.iter().all(|v| v.matched));
        // row 2's exclusion is a SUSPECTED GAP (absence not established),
        // not a silent drop
        assert_eq!(r.suspected_gaps.len(), 1);
        assert!(r.suspected_gaps[0].contains("customer_query"));
        assert!(r.suspected_gaps[0].contains("NOT established"));
    }

    #[test]
    fn any_composition_includes_on_first_match() {
        let q = vec![Condition::HasTests(false), Condition::Changed(false)];
        let r = run_query(&q, Composition::Any, &rows());
        assert_eq!(r.included.len(), 2); // rows 1 (no tests) and 3 (unchanged)
    }

    #[test]
    fn scoped_answer_identifies_members_and_explains_empty() {
        let universe = rows();
        let sel = ScopedQuery {
            selection: BTreeSet::from([SymbolId(2)]),
        };
        let a = scoped_answer(&sel, &universe);
        assert_eq!(a.members.len(), 1);
        assert_eq!(a.members[0].name, "customer_query");
        assert_eq!(a.outside, 2);
        assert!(a.empty_reason.is_none());

        // empty selection is first-class with a stated reason
        let a = scoped_answer(&ScopedQuery::default(), &universe);
        assert!(a.members.is_empty());
        assert!(a.empty_reason.as_deref().unwrap().contains("selection is empty"));

        // stale ids are explained too
        let a = scoped_answer(
            &ScopedQuery {
                selection: BTreeSet::from([SymbolId(99)]),
            },
            &universe,
        );
        assert!(a.empty_reason.as_deref().unwrap().contains("stale"));
    }
}
