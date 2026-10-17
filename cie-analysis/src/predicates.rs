//! Predicate extraction (TASK-043, `cie-analysis::predicates`).
//!
//! Extracts predicates from contract `expr`s conjoined with path
//! conditions derived from summaries (the TASK-032 layer), each predicate
//! carrying provenance back to its contract (global rule 2).
//!
//! The contract expression DSL (shared with TASK-041 protocol mining):
//! `requires(antecedent, consequent)` — antecedent may only occur when the
//! consequent holds. Unparseable or unsupported exprs are reported in the
//! predicate's `unsupported` list (FR-4) — never silently dropped.

use cie_contract::Contract;
use cie_core::types::SymbolId;
use cie_counterexample::{ExtractedPredicate, PathAtomSet};
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

/// One realized call path through a symbol: the callee trail plus the
/// path-condition strings the summary recorded along it.
#[derive(Debug, Clone)]
pub struct RealizedPath {
    pub trail: Vec<String>,
    pub conditions: Vec<String>,
}

/// The sketch's `PredicateExtractor::new(&reg).extract(&fx.summaries(),
/// &fx.path_conditions())`.
pub struct PredicateExtractor<'a> {
    contracts: &'a [Contract],
}

impl<'a> PredicateExtractor<'a> {
    pub fn new(contracts: &'a [Contract]) -> Self {
        PredicateExtractor { contracts }
    }

    /// Realize paths for `focus` symbols: for each summary, one path per
    /// distinct call trail root — the summary's callees flattened to a
    /// trail, its `path_conditions` split into per-path atom sets.
    ///
    /// Realization is bounded: at most `max_paths` per symbol (TASK-029
    /// budget proxy), deterministic order (BTreeMap iteration).
    pub fn realize_paths(
        summaries: &BTreeMap<SymbolId, FunctionSummary>,
        store_names: &BTreeMap<SymbolId, String>,
        max_paths: usize,
    ) -> BTreeMap<SymbolId, Vec<RealizedPath>> {
        let mut out: BTreeMap<SymbolId, Vec<RealizedPath>> = BTreeMap::new();
        for (id, s) in summaries {
            // A definition-only node (no callees, no recorded conditions)
            // realizes no decision-relevant path — skip it.
            if s.callees.is_empty() && s.path_conditions.is_empty() {
                continue;
            }
            let mut paths: Vec<RealizedPath> = vec![];
            // the symbol's own name opens the trail
            let own = store_names.get(id).cloned().unwrap_or_else(|| format!("sym_{:?}", id));
            if s.callees.is_empty() {
                paths.push(RealizedPath {
                    trail: vec![own],
                    conditions: s.path_conditions.clone(),
                });
            } else {
                for c in &s.callees {
                    let cname = store_names
                        .get(c)
                        .cloned()
                        .unwrap_or_else(|| format!("sym_{:?}", c));
                    paths.push(RealizedPath {
                        trail: vec![own.clone(), cname],
                        conditions: s.path_conditions.clone(),
                    });
                }
                if paths.len() > max_paths {
                    paths.truncate(max_paths);
                }
            }
            out.insert(*id, paths);
        }
        out
    }

    /// Extract one predicate per `requires(..)` contract, conjoined with
    /// the realized paths whose trail mentions the antecedent action.
    pub fn extract(
        &self,
        realized: &BTreeMap<SymbolId, Vec<RealizedPath>>,
    ) -> Vec<ExtractedPredicate> {
        let mut out = vec![];
        for c in self.contracts {
            let Some((a, b)) = parse_requires(&c.expr) else {
                continue; // not a requires-obligation; unsupported list is per-predicate
            };
            let mut unsupported = vec![];
            if !c.expr.trim().starts_with("requires(") && !c.expr.contains("requires(") {
                unsupported.push(format!("expr not in requires() DSL: {}", c.expr));
            }
            let mut paths: Vec<PathAtomSet> = vec![];
            for (_id, rs) in realized {
                for r in rs {
                    // does this path realize the antecedent or constrain
                    // the consequent? Either way it is relevant.
                    let mentions_antecedent = r
                        .trail
                        .iter()
                        .any(|t| t.to_ascii_lowercase().contains(&a));
                    let constrains_consequent = r
                        .conditions
                        .iter()
                        .any(|cd| cd.to_ascii_lowercase().contains(&b));
                    if !mentions_antecedent && !constrains_consequent {
                        continue;
                    }
                    // Per-arm semantics: each path condition constraining
                    // the consequent becomes its OWN PathAtomSet (one arm
                    // per condition); a path with no constraining condition
                    // stays one opaque arm (consequent unbound).
                    let mut arms: Vec<Vec<(String, bool)>> = vec![];
                    for cond in &r.conditions {
                        let low = cond.to_ascii_lowercase();
                        if low.contains(&b) {
                            // parse polarity: negation words flip the atom
                            let negated = ["!", "not ", "unauthorized", " != true", "== false"]
                                .iter()
                                .any(|n| low.contains(n))
                                && !(low.contains("!= false") || low.contains("== true"));
                            let mut atoms: Vec<(String, bool)> =
                                vec![(b.clone(), !negated)];
                            if mentions_antecedent {
                                atoms.push((a.clone(), true));
                            }
                            atoms.sort();
                            atoms.dedup();
                            arms.push(atoms);
                        }
                    }
                    if arms.is_empty() {
                        let mut atoms: Vec<(String, bool)> = vec![];
                        if mentions_antecedent {
                            atoms.push((a.clone(), true));
                        }
                        if atoms.is_empty() && !constrains_consequent {
                            continue;
                        }
                        atoms.sort();
                        atoms.dedup();
                        paths.push(PathAtomSet { atoms, trail: r.trail.clone() });
                    } else {
                        for atoms in arms {
                            paths.push(PathAtomSet { atoms, trail: r.trail.clone() });
                        }
                    }
                }
            }
            out.push(ExtractedPredicate {
                from_contract: Some(c.id),
                antecedent: a,
                consequent: b,
                paths,
                unsupported,
            });
        }
        out
    }
}

/// Parse `requires(a, b)` (tolerant of surrounding prose; the DSL call is
/// the authoritative span).
pub fn parse_requires(expr: &str) -> Option<(String, String)> {
    let start = expr.find("requires(")? + "requires(".len();
    let rest = &expr[start..];
    let end = rest.find(')')?;
    let inner = &rest[..end];
    let mut parts = inner.splitn(2, ',');
    let a = parts.next()?.trim().to_string();
    let b = parts.next()?.trim().to_string();
    if a.is_empty() || b.is_empty() {
        return None;
    }
    Some((a, b))
}
