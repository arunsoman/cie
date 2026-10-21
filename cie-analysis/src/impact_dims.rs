//! Multi-dimension impact (CIE-US US-109 engine close).
//!
//! "The expected changes to code, permissions, data, interfaces,
//! operations and performance": code impact ships (`cli:impact`); the
//! permission/data/operations dimensions were unmodelled. This module
//! models them from the effect classes the adapters already classify —
//! every dimension entry cites the effect site that grounded it, and the
//! whole report is marked planned (planned changes are never confused
//! with applied or validated work).
//!
//! Dimension grounding (deterministic, over BuiltinKind classes):
//! * Permission — auth/check-style calls (deny-by-default: a seed that
//!   reaches no auth surface states that explicitly);
//! * Data — DB read/write and file-write effects;
//! * Operations — env access, process exec, network, sleep/config;
//! * Performance — effects inside loops (the N+1 hint is already typed);
//! * Interface — exported symbols (no caller restriction) among the
//!   affected set.

use std::collections::{BTreeMap, BTreeSet};

use cie_core::types::SymbolId;
use cie_semantic::facts::{BuiltinKind, Callee, SymbolStore, TypedFacts};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Code,
    Permission,
    Data,
    Interface,
    Operations,
    Performance,
}

impl Dimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Dimension::Code => "code",
            Dimension::Permission => "permission",
            Dimension::Data => "data",
            Dimension::Interface => "interface",
            Dimension::Operations => "operations",
            Dimension::Performance => "performance",
        }
    }
}

/// One dimension hit with its citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionHit {
    pub symbol: SymbolId,
    pub line: u32,
    /// The classified effect (e.g. `db_write`, `env_read`).
    pub effect: String,
}

/// The report: dimension → cited hits; planned label permanent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionReport {
    pub seed: SymbolId,
    pub affected: BTreeSet<SymbolId>,
    pub hits: BTreeMap<Dimension, Vec<DimensionHit>>,
    /// Dimensions with NO hits say so explicitly (deny-by-default
    /// honesty: an empty permission surface is a statement, not a blank).
    pub empty_dimensions: Vec<Dimension>,
    pub label: &'static str,
}

pub const PLANNED_LABEL: &str = "planned impact model — never confused with applied or validated changes";

const AUTH_HINTS: &[&str] = &[
    "authorize", "check_permission", "has_role", "verify_token", "require_auth", "authenticate",
];

/// Assemble dimensions for an affected symbol set (the output of the
/// shipped impact computation).
pub fn impact_dimensions(
    seed: SymbolId,
    affected: &BTreeSet<SymbolId>,
    facts: &TypedFacts,
    store: &SymbolStore,
) -> DimensionReport {
    let mut hits: BTreeMap<Dimension, Vec<DimensionHit>> = BTreeMap::new();
    let affected_or_seed = |s: SymbolId| s == seed || affected.contains(&s);

    for c in &facts.calls {
        if !affected_or_seed(c.caller) {
            continue;
        }
        match &c.callee {
            Callee::Builtin(kind) => {
                let (dim, effect) = match kind {
                    BuiltinKind::DbExecute | BuiltinKind::DbQuery => {
                        (Dimension::Data, "db_access")
                    }
                    BuiltinKind::FileWrite => (Dimension::Data, "file_write"),
                    BuiltinKind::FileRead => (Dimension::Operations, "file_read"),
                    BuiltinKind::Env => (Dimension::Operations, "env_read"),
                    BuiltinKind::ProcessExec => (Dimension::Operations, "process_exec"),
                    BuiltinKind::Network => (Dimension::Operations, "network"),
                    BuiltinKind::Crypto | BuiltinKind::Random => {
                        (Dimension::Permission, "crypto_material")
                    }
                    BuiltinKind::Sleep | BuiltinKind::Time => (Dimension::Operations, "timing"),
                    BuiltinKind::Lock => (Dimension::Performance, "lock"),
                    _ => continue,
                };
                hits.entry(dim).or_default().push(DimensionHit {
                    symbol: c.caller,
                    line: c.line,
                    effect: effect.into(),
                });
            }
            Callee::Resolved(_) | Callee::Unresolved(_) => {
                // auth-shaped resolved/unresolved calls → permission
                let lower = c.callee_text.to_lowercase();
                if AUTH_HINTS.iter().any(|h| lower.contains(h)) {
                    hits.entry(Dimension::Permission).or_default().push(DimensionHit {
                        symbol: c.caller,
                        line: c.line,
                        effect: format!("auth_call:{}", c.callee_text),
                    });
                }
                if c.in_loop {
                    hits.entry(Dimension::Performance).or_default().push(DimensionHit {
                        symbol: c.caller,
                        line: c.line,
                        effect: "call_in_loop".into(),
                    });
                }
            }
        }
    }

    // interface dimension: affected symbols that are exported (appear as
    // callee targets from outside the affected set)
    let mut interface_hits = Vec::new();
    for c in &facts.calls {
        if let Callee::Resolved(target) = c.callee {
            if affected.contains(&target) && !affected_or_seed(c.caller) {
                let name = store
                    .get(target)
                    .map(|s| s.qual_name.clone())
                    .unwrap_or_default();
                interface_hits.push(DimensionHit {
                    symbol: target,
                    line: c.line,
                    effect: format!("external_caller:{}", name),
                });
            }
        }
    }
    if !interface_hits.is_empty() {
        hits.insert(Dimension::Interface, interface_hits);
    }

    for d in hits.values_mut() {
        d.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.line.cmp(&b.line)));
        d.dedup();
    }

    let empty_dimensions = [
        Dimension::Code,
        Dimension::Permission,
        Dimension::Data,
        Dimension::Interface,
        Dimension::Operations,
        Dimension::Performance,
    ]
    .iter()
    .copied()
    .filter(|d| !hits.contains_key(d))
    .collect();

    DimensionReport {
        seed,
        affected: affected.clone(),
        hits,
        empty_dimensions,
        label: PLANNED_LABEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::{FileId, SourceLocation};
    use cie_semantic::facts::{CallSite, SymbolFact, SymbolKind};
    use cie_semantic::lang::LangId;

    fn fact(id: u32, qual: &str) -> SymbolFact {
        SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(id),
            kind: SymbolKind::Function,
            name: qual.rsplit("::").next().unwrap().into(),
            qual_name: qual.into(),
            loc: SourceLocation::span_bytes(FileId(0), 0, 10),
            lang: LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        }
    }

    fn call(caller: u32, text: &str, callee: Callee, line: u32, in_loop: bool) -> CallSite {
        CallSite {
            caller: SymbolId(caller),
            callee_text: text.into(),
            callee,
            line,
            in_loop,
            arg_vars: vec![],
        }
    }

    #[test]
    fn dimensions_are_cited_and_empty_ones_state_themselves() {
        let symbols = vec![fact(1, "pay::charge")];
        let facts = TypedFacts {
            symbols,
            calls: vec![
                call(1, "db_exec", Callee::Builtin(BuiltinKind::DbExecute), 10, false),
                call(1, "authorize", Callee::Unresolved("auth".into()), 11, false),
                call(1, "os_env", Callee::Builtin(BuiltinKind::Env), 12, false),
                call(1, "helper", Callee::Resolved(SymbolId(2)), 13, true),
            ],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let mut affected = BTreeSet::new();
        affected.insert(SymbolId(2));
        let rep = impact_dimensions(SymbolId(1), &affected, &facts, &store);
        assert_eq!(rep.label, PLANNED_LABEL);
        let data = rep.hits.get(&Dimension::Data).unwrap();
        assert!(data.iter().any(|h| h.effect == "db_access" && h.line == 10));
        let perm = rep.hits.get(&Dimension::Permission).unwrap();
        assert!(perm.iter().any(|h| h.effect.contains("authorize")));
        let ops = rep.hits.get(&Dimension::Operations).unwrap();
        assert!(ops.iter().any(|h| h.effect == "env_read"));
        let perf = rep.hits.get(&Dimension::Performance).unwrap();
        assert!(perf.iter().any(|h| h.effect == "call_in_loop"));
        // interface + file-write are absent → explicitly stated
        assert!(rep.empty_dimensions.contains(&Dimension::Interface));
        assert!(rep.empty_dimensions.contains(&Dimension::Data) == false);
        assert!(rep.hits.get(&Dimension::Interface).is_none());
    }

    #[test]
    fn interface_dimension_cites_external_callers() {
        let symbols = vec![fact(1, "pay::charge"), fact(5, "web::handler")];
        let facts = TypedFacts {
            symbols,
            calls: vec![call(5, "charge", Callee::Resolved(SymbolId(1)), 2, false)],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let mut affected = BTreeSet::new();
        affected.insert(SymbolId(1));
        let rep = impact_dimensions(SymbolId(1), &affected, &facts, &store);
        let iface = rep.hits.get(&Dimension::Interface).unwrap();
        assert!(iface.iter().any(|h| h.effect.contains("web::handler")));
        assert!(rep.empty_dimensions.is_empty() || !rep.empty_dimensions.contains(&Dimension::Interface));
    }

    #[test]
    fn no_auth_surface_is_a_statement_not_a_blank() {
        let symbols = vec![fact(1, "util::pure")];
        let facts = TypedFacts {
            symbols,
            calls: vec![],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let affected = BTreeSet::new();
        let rep = impact_dimensions(SymbolId(1), &affected, &facts, &store);
        assert!(rep.hits.is_empty());
        assert_eq!(rep.empty_dimensions.len(), 6);
        assert!(rep.empty_dimensions.contains(&Dimension::Permission));
    }
}
