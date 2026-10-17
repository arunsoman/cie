//! What-if change-set simulation (CIE-US US-099 engine close).
//!
//! The deletion counterfactual (US-058 oracle) answers "what if this is
//! removed". This module broadens the dry run to whole change-sets —
//! component removal and module splits — over the same miniature-but-
//! faithful incremental world, and types every consequence:
//! * [`ConsequenceKind::Structural`] — read directly off the simulated
//!   world diff (symbols gone, edges broken, findings dropped);
//! * [`ConsequenceKind::Inferred`] — required adaptations the simulation
//!   implies (callers of removed symbols need new targets);
//! * **measured** consequences are deliberately NOT emitted here: a
//!   measured number requires an imported workload run (the
//!   `cie-evidence::workload` protocol). A what-if never borrows one.
//!
//! The scenario result is permanently labelled: never presented as
//! already implemented (the simulation runs on a clone; the caller's
//! world is untouched).

use std::collections::BTreeSet;

use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

use crate::proptests::{Edit, IncrWorld, RepoSnapshot};

/// One hypothetical change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum HypotheticalChange {
    /// Remove every file under a component prefix.
    RemoveComponent { prefix: String },
    /// Split a file: remove it, add the named new files with contents.
    SplitModule {
        path: String,
        new_files: Vec<(String, String)>,
    },
}

/// Consequence typing (US-099 obligation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsequenceKind {
    /// Read directly off the simulated diff.
    Structural,
    /// Implied required adaptation.
    Inferred,
}

impl ConsequenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConsequenceKind::Structural => "structural",
            ConsequenceKind::Inferred => "inferred",
        }
    }
}

/// One typed consequence with its citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consequence {
    pub kind: ConsequenceKind,
    /// What happened (cites symbols/edges/files).
    pub detail: String,
}

/// The dry-run result. Current/proposed separation is by construction:
/// `before`/`after` facts are both reported, and the label is permanent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhatIfResult {
    pub scenario: String,
    pub before_symbols: usize,
    pub after_symbols: usize,
    pub before_edges: usize,
    pub after_edges: usize,
    pub consequences: Vec<Consequence>,
    /// Assumptions the simulation made (stated, not hidden).
    pub assumptions: Vec<String>,
    pub label: &'static str,
}

pub const WHATIF_LABEL: &str = "what-if simulation on a clone — never presented as implemented";

/// Simulate a change-set (US-099). Deterministic; the input world is not
/// mutated (the simulation runs on a clone).
pub fn simulate(world: &IncrWorld, changes: &[HypotheticalChange]) -> WhatIfResult {
    let scenario = changes
        .iter()
        .map(|c| match c {
            HypotheticalChange::RemoveComponent { prefix } => format!("remove:{prefix}"),
            HypotheticalChange::SplitModule { path, new_files } => {
                format!("split:{}→{}", path, new_files.len())
            }
        })
        .collect::<Vec<_>>()
        .join("+");

    let before_symbols = world.symbols.len();
    let before_edges: usize = world.deps_ns.values().map(|v| v.len()).sum();
    let before_findings = world.findings_stripped();

    let mut sim = world.clone();
    let mut consequences: Vec<Consequence> = Vec::new();
    let mut assumptions: Vec<String> = vec![
        "miniature fact model: fn/call segments (the US-058 oracle world)".into(),
        "measured consequences require an imported workload run — none borrowed here".into(),
    ];

    // callers snapshot for adaptation inference
    let mut callers_of: std::collections::BTreeMap<SymbolId, BTreeSet<SymbolId>> =
        std::collections::BTreeMap::new();
    for (s, deps) in &sim.deps_ns {
        for d in deps {
            callers_of.entry(*d).or_default().insert(*s);
        }
    }

    for change in changes {
        match change {
            HypotheticalChange::RemoveComponent { prefix } => {
                let paths: Vec<String> = sim
                    .files
                    .keys()
                    .filter(|p| p.starts_with(prefix.as_str()))
                    .cloned()
                    .collect();
                for p in &paths {
                    let ids = sim.ids_in_file(p);
                    // structural: symbols + their edges disappear
                    for id in &ids {
                        if let Some(info) = sim.symbols.get(id) {
                            consequences.push(Consequence {
                                kind: ConsequenceKind::Structural,
                                detail: format!("symbol {} ({}) removed", info.name, p),
                            });
                        }
                        let callers = callers_of.get(id).cloned().unwrap_or_default();
                        for caller in callers {
                            consequences.push(Consequence {
                                kind: ConsequenceKind::Inferred,
                                detail: format!(
                                    "caller symbol#{caller} loses its edge to symbol#{id} — adaptation required"
                                ),
                            });
                        }
                    }
                    sim.apply(Edit::DeleteFile { path: p.clone() });
                }
                if paths.is_empty() {
                    consequences.push(Consequence {
                        kind: ConsequenceKind::Structural,
                        detail: format!("no files matched prefix {prefix} — nothing changed"),
                    });
                }
                assumptions.push(format!(
                    "remove:{prefix} assumes no dynamic dispatch to the removed symbols"
                ));
            }
            HypotheticalChange::SplitModule { path, new_files } => {
                let ids = sim.ids_in_file(path);
                let names: Vec<String> = ids
                    .iter()
                    .filter_map(|id| sim.symbols.get(id).map(|i| i.name.clone()))
                    .collect();
                sim.apply(Edit::DeleteFile { path: path.clone() });
                for (np, content) in new_files {
                    sim.apply(Edit::AddFile {
                        path: np.clone(),
                        content: content.clone(),
                    });
                }
                consequences.push(Consequence {
                    kind: ConsequenceKind::Structural,
                    detail: format!(
                        "file {path} removed; symbols {:?} re-homed into {} new files",
                        names,
                        new_files.len()
                    ),
                });
                assumptions.push(format!(
                    "split:{path} assumes import sites update to the new paths (no re-export shim)"
                ));
            }
        }
    }

    // findings dropped by the change (evidence-bearing consequence)
    let after_findings = sim.findings_stripped();
    let dropped: Vec<_> = before_findings.difference(&after_findings).collect();
    for (sym, h) in dropped {
        consequences.push(Consequence {
            kind: ConsequenceKind::Structural,
            detail: format!("finding on symbol#{sym} (facts {h}) invalidated by the change"),
        });
    }
    // integrity: the simulated world stays deletion-correct
    debug_assert!(sim.no_orphan_records());

    consequences.sort_by(|a, b| {
        a.kind
            .as_str()
            .cmp(b.kind.as_str())
            .then(a.detail.cmp(&b.detail))
    });

    WhatIfResult {
        scenario,
        before_symbols,
        after_symbols: sim.symbols.len(),
        before_edges,
        after_edges: sim.deps_ns.values().map(|v| v.len()).sum(),
        consequences,
        assumptions,
        label: WHATIF_LABEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proptests::RepoSnapshot;

    fn repo() -> RepoSnapshot {
        // NB: the miniature world resolves same-file callees only
        // (content-addressed ids are path-scoped), so the caller lives
        // beside its callee here.
        RepoSnapshot::from([
            (
                "svc/pay.rs".into(),
                "fn charge(order):\n    call ledger_post\n    ok\n\n\
                 fn handle(req):\n    call charge\n    ok\n".into(),
            ),
            (
                "svc/ledger.rs".into(),
                "fn ledger_post(x):\n    ok\n".into(),
            ),
        ])
    }

    #[test]
    fn removal_produces_structural_and_inferred_consequences() {
        let w = IncrWorld::new(repo());
        let r = simulate(
            &w,
            &[HypotheticalChange::RemoveComponent {
                prefix: "svc/pay.rs".into(),
            }],
        );
        assert_eq!(r.label, WHATIF_LABEL);
        assert!(r.after_symbols < r.before_symbols);
        assert!(r.after_edges < r.before_edges);
        // structural: charge + ledger_post gone (pay.rs removed)
        let structural: Vec<&Consequence> = r
            .consequences
            .iter()
            .filter(|c| c.kind == ConsequenceKind::Structural)
            .collect();
        assert!(structural.iter().any(|c| c.detail.contains("charge")));
        // inferred: web/api.rs handle loses its edge to charge
        assert!(r
            .consequences
            .iter()
            .any(|c| c.kind == ConsequenceKind::Inferred
                && c.detail.contains("adaptation required")));
        // stated assumptions include the no-measurement rule
        assert!(r
            .assumptions
            .iter()
            .any(|a| a.contains("workload run")));
    }

    #[test]
    fn split_rehomes_symbols_and_assumes_import_updates() {
        let w = IncrWorld::new(repo());
        let r = simulate(
            &w,
            &[HypotheticalChange::SplitModule {
                path: "svc/ledger.rs".into(),
                new_files: vec![(
                    "svc/ledger_core.rs".into(),
                    "fn ledger_post(x):\n    ok\n".into(),
                )],
            }],
        );
        assert_eq!(r.after_symbols, r.before_symbols); // re-homed, not lost
        assert!(r
            .consequences
            .iter()
            .any(|c| c.detail.contains("re-homed into 1 new files")));
        assert!(r
            .assumptions
            .iter()
            .any(|a| a.contains("no re-export shim")));
    }

    #[test]
    fn input_world_is_untouched_and_empty_prefix_is_honest() {
        let w = IncrWorld::new(repo());
        let before = w.symbols.len();
        let r = simulate(
            &w,
            &[HypotheticalChange::RemoveComponent { prefix: "nope/".into() }],
        );
        assert_eq!(w.symbols.len(), before);
        assert!(r
            .consequences
            .iter()
            .any(|c| c.detail.contains("nothing changed")));
    }
}
