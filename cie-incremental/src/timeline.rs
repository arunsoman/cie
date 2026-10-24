//! Interface contract timelines (CIE-US US-089 engine close).
//!
//! The differential layer ships relation-level change traces; this module
//! assembles the *named-interface* timeline: for a public symbol (an
//! interface in the story's sense — any exported contract), the per-
//! revision presence/absence record, the revisions where it appeared,
//! changed or disappeared, the breaking-change concerns those deltas
//! raise, and the affected clients (importers recorded in the per-
//! revision import map, which the ingest pass produces from import
//! facts).
//!
//! Honesty: a "breaking change" is a *concern* citing the revisions and
//! contracts involved — never a verdict about downstream builds, which
//! the engine does not run. Unknown consumers remain a visible coverage
//! gap (revisions without importer facts are named, not hidden).

use std::collections::{BTreeMap, BTreeSet};

use cie_core::types::{RevId, SymbolId};
use crate::snapshot::RevSnapshot;
use cie_semantic::facts::ImportFact;
use serde::{Deserialize, Serialize};

/// Per-revision import map: source file → its import facts (as extracted
/// by the adapters). Produced by the ingest pass per revision.
pub type ImportMap = BTreeMap<String, Vec<ImportFact>>;

/// One revision's state of the tracked interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceRevision {
    pub rev: RevId,
    pub present: bool,
    /// Signature-relevant text (kind + qualified name) when present.
    pub contract: Option<String>,
}

/// A breaking-change concern (cited, labelled — not a downstream verdict).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakingConcern {
    pub rev: RevId,
    pub kind: BreakingKind,
    /// What exactly changed (contract texts before/after or presence).
    pub detail: String,
    /// Importers of the interface at the previous revision (affected).
    pub affected_clients: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BreakingKind {
    /// Interface absent at this revision (was present before).
    Removed,
    /// Qualified contract text changed (rename/signature surface).
    ContractChanged,
    /// Re-appeared after absence (announces history, not a break).
    Reinstated,
}

impl BreakingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakingKind::Removed => "removed",
            BreakingKind::ContractChanged => "contract_changed",
            BreakingKind::Reinstated => "reinstated",
        }
    }
}

/// The assembled timeline for one named interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractTimeline {
    /// Qualified name tracked (e.g. `auth::authorize`).
    pub interface: String,
    pub revisions: Vec<InterfaceRevision>,
    /// Breaking concerns, oldest-first, each citing revisions + clients.
    pub breaking_concerns: Vec<BreakingConcern>,
    /// All clients seen importing the interface across revisions.
    pub known_clients: BTreeSet<String>,
    /// Honest coverage gap: revisions where the interface existed but no
    /// importer facts named it (unknown consumers, not "unused").
    pub unknown_consumer_revs: Vec<RevId>,
}

/// Importers of `interface` in one import map: files whose imports name
/// the interface or its module path.
fn importers_of(interface: &str, map: &ImportMap) -> Vec<String> {
    let module = interface
        .rsplit_once("::")
        .map(|(m, _)| m)
        .unwrap_or(interface);
    let mut out = Vec::new();
    for (file, facts) in map {
        if facts
            .iter()
            .any(|imp| imp.module.contains(module) || imp.names.iter().any(|n| n == interface))
        {
            out.push(file.clone());
        }
    }
    out.sort();
    out
}

/// Build the timeline for `interface` across snapshots + per-revision
/// import maps (oldest-first output; sorted by rev internally).
pub fn contract_timeline(
    interface: &str,
    snapshots: &[RevSnapshot],
    import_maps: &BTreeMap<RevId, ImportMap>,
) -> ContractTimeline {
    let mut snaps: Vec<&RevSnapshot> = snapshots.iter().collect();
    snaps.sort_by_key(|s| s.rev);

    let mut revisions = Vec::new();
    let mut concerns = Vec::new();
    let mut known_clients: BTreeSet<String> = BTreeSet::new();
    let mut unknown_consumer_revs: Vec<RevId> = Vec::new();

    let mut prev: Option<(RevId, bool, Option<String>, Vec<String>)> = None;
    for snap in &snaps {
        let hit = snap
            .symbols
            .iter()
            .find(|s| s.qual_name == interface || s.name == interface);
        let present = hit.is_some();
        let contract = hit.map(|s| format!("{:?}:{}", s.kind, s.qual_name));

        let empty_map: ImportMap = BTreeMap::new();
        let map = import_maps.get(&snap.rev).unwrap_or(&empty_map);
        let mut clients = Vec::new();
        if present {
            clients = importers_of(interface, map);
            if clients.is_empty() {
                // importer facts absent for this revision — a named gap,
                // not an "unused" verdict (US-089: unknown consumers stay
                // visible)
                unknown_consumer_revs.push(snap.rev);
            }
            known_clients.extend(clients.iter().cloned());
        }

        // concern detection vs previous revision
        if let Some((prev_rev, prev_present, prev_contract, prev_clients)) = &prev {
            if *prev_present && !present {
                concerns.push(BreakingConcern {
                    rev: snap.rev,
                    kind: BreakingKind::Removed,
                    detail: format!(
                        "interface {interface} present at r{prev_rev}, absent at r{}",
                        snap.rev
                    ),
                    affected_clients: prev_clients.clone(),
                });
            } else if !*prev_present && present {
                concerns.push(BreakingConcern {
                    rev: snap.rev,
                    kind: BreakingKind::Reinstated,
                    detail: format!(
                        "interface {interface} absent at r{prev_rev}, present again at r{}",
                        snap.rev
                    ),
                    affected_clients: clients.clone(),
                });
            } else if present && prev_contract.as_deref() != contract.as_deref() {
                concerns.push(BreakingConcern {
                    rev: snap.rev,
                    kind: BreakingKind::ContractChanged,
                    detail: format!(
                        "contract changed r{prev_rev}: {} → r{}: {}",
                        prev_contract.clone().unwrap_or_default(),
                        snap.rev,
                        contract.clone().unwrap_or_default()
                    ),
                    affected_clients: prev_clients.clone(),
                });
            }
        }

        revisions.push(InterfaceRevision {
            rev: snap.rev,
            present,
            contract: contract.clone(),
        });
        prev = Some((snap.rev, present, contract, clients));
    }

    ContractTimeline {
        interface: interface.to_string(),
        revisions,
        breaking_concerns: concerns,
        known_clients,
        unknown_consumer_revs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::{FileId, SourceLocation};
    use cie_semantic::facts::SymbolFact;
    use cie_semantic::lang::LangId;

    fn snap(rev: RevId, symbols: Vec<SymbolFact>) -> RevSnapshot {
        RevSnapshot {
            state_model: None,
            concepts_tier: None,
            usecase_view: Vec::new(),
            interaction_gaps: Vec::new(),
            rev,
            files: vec![],
            stack: None,
            symbols,
            summaries: vec![],
            findings: vec![],
            contracts: vec![],
            concepts: Default::default(),
            coverage: Default::default(),
            evidence: cie_core::evidence::Evidence::new(rev, "test", 0),
            warnings: vec![],
            ingest_wall_ns: 0,
            incremental: false,
            summary_cache: vec![],
            summary_reuse: None,
        }
    }

    fn sym(id: u32, qual: &str) -> SymbolFact {
        SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(id),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: qual.rsplit("::").next().unwrap().into(),
            qual_name: qual.into(),
            loc: SourceLocation::span_bytes(FileId(0), 0, 10),
            lang: LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        }
    }

    fn imp(module: &str, name: &str) -> ImportFact {
        ImportFact {
            module_symbol: SymbolId(0),
            module: module.into(),
            names: vec![name.into()],
            alias: None,
            line: 1,
        }
    }

    #[test]
    fn removal_is_a_cited_concern_with_affected_clients() {
        let s1 = snap(1, vec![sym(1, "auth::authorize")]);
        let s2 = snap(2, vec![]);
        let mut maps = BTreeMap::new();
        maps.insert(
            1,
            BTreeMap::from([(
                "web/api.py".to_string(),
                vec![imp("auth", "auth::authorize")],
            )]),
        );
        let t = contract_timeline("auth::authorize", &[s1, s2], &maps);
        assert_eq!(t.revisions.len(), 2);
        assert!(t.revisions[0].present);
        assert!(!t.revisions[1].present);
        assert_eq!(t.breaking_concerns.len(), 1);
        let c = &t.breaking_concerns[0];
        assert_eq!(c.kind, BreakingKind::Removed);
        assert!(c.detail.contains("r1"));
        assert_eq!(c.affected_clients, vec!["web/api.py".to_string()]);
        assert!(t.known_clients.contains("web/api.py"));
        assert!(t.unknown_consumer_revs.is_empty());
    }

    #[test]
    fn rename_reads_as_removal_of_old_contract() {
        let s1 = snap(1, vec![sym(1, "auth::authorize")]);
        let s2 = snap(2, vec![sym(2, "auth::authorize_async")]);
        let t = contract_timeline("auth::authorize", &[s1, s2], &BTreeMap::new());
        assert!(t
            .breaking_concerns
            .iter()
            .any(|c| c.kind == BreakingKind::Removed));
    }

    #[test]
    fn unknown_consumers_stay_a_visible_gap() {
        let s1 = snap(1, vec![sym(1, "auth::authorize")]);
        let s2 = snap(2, vec![sym(1, "auth::authorize")]);
        let t = contract_timeline("auth::authorize", &[s1, s2], &BTreeMap::new());
        assert_eq!(t.unknown_consumer_revs, vec![1, 2]);
        assert!(t.known_clients.is_empty());
        assert!(t.breaking_concerns.is_empty()); // no change → no concern
    }

    #[test]
    fn reinstatement_is_announced_with_history() {
        let s1 = snap(1, vec![sym(1, "auth::authorize")]);
        let s2 = snap(2, vec![]);
        let s3 = snap(3, vec![sym(1, "auth::authorize")]);
        let t = contract_timeline("auth::authorize", &[s1, s2, s3], &BTreeMap::new());
        assert!(t
            .breaking_concerns
            .iter()
            .any(|c| c.kind == BreakingKind::Reinstated && c.rev == 3));
    }
}
