//! Requirement attribution ledger (CIE-US US-113 engine close).
//!
//! "Which requirements changed which components, files and symbols": the
//! changed-file/symbol accounting ships for re-ingests; this module adds
//! the attribution ledger — requirement ids mapped onto the change set,
//! each entry typed Added / Modified / Deleted / Renamed / Reused /
//! AffectedButUnchanged, searchable as a list (the graph view is the
//! product layer).
//!
//! Honesty: `Renamed` and `Reused` require the caller's identity mapping
//! (the engine does not guess renames); `AffectedButUnchanged` comes
//! from the reverse-dep computation and is labelled as such.

use std::collections::BTreeMap;

use cie_core::types::SymbolId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    Added,
    Modified,
    Deleted,
    Renamed,
    Reused,
    AffectedButUnchanged,
}

impl ChangeClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeClass::Added => "added",
            ChangeClass::Modified => "modified",
            ChangeClass::Deleted => "deleted",
            ChangeClass::Renamed => "renamed",
            ChangeClass::Reused => "reused",
            ChangeClass::AffectedButUnchanged => "affected_but_unchanged",
        }
    }
}

/// One attribution entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    pub rev: u64,
    pub path: String,
    pub symbol: SymbolId,
    pub symbol_name: String,
    pub class: ChangeClass,
}

/// The searchable ledger (entries + requirement index together).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionLedger {
    pub entries: Vec<Attribution>,
    pub by_requirement: BTreeMap<String, Vec<usize>>,
}

impl AttributionLedger {
    /// Search by requirement id.
    pub fn for_requirement(&self, req: &str) -> Vec<&Attribution> {
        self.by_requirement
            .get(req)
            .map(|idx| idx.iter().map(|i| &self.entries[*i]).collect())
            .unwrap_or_default()
    }

    /// Distinct classes present for a requirement (the story's
    /// "distinguishable" obligation, as data).
    pub fn classes_of(&self, req: &str) -> Vec<ChangeClass> {
        let mut v: Vec<ChangeClass> = self
            .for_requirement(req)
            .into_iter()
            .map(|a| a.class)
            .collect();
        v.sort_by_key(|c| c.as_str());
        v.dedup();
        v
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Build the ledger (US-113). Inputs are the caller's requirement map
/// (requirement → paths) plus the per-path change sets of the revision.
pub fn attribute(
    requirement_paths: &BTreeMap<String, Vec<String>>,
    rev: u64,
    changes: &BTreeMap<String, Vec<(SymbolId, String, ChangeClass)>>,
) -> AttributionLedger {
    let mut ledger = AttributionLedger::default();
    let mut by_requirement: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (req, paths) in requirement_paths {
        for p in paths {
            if let Some(syms) = changes.get(p) {
                for (id, name, class) in syms {
                    by_requirement.entry(req.clone()).or_default().push(ledger.entries.len());
                    ledger.entries.push(Attribution {
                        rev,
                        path: p.clone(),
                        symbol: *id,
                        symbol_name: name.clone(),
                        class: *class,
                    });
                }
            }
        }
    }
    AttributionLedger {
        entries: ledger.entries,
        by_requirement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<(SymbolId, String, ChangeClass)>>) {
        let reqs = BTreeMap::from([
            ("REQ-7".to_string(), vec!["src/pay.py".to_string(), "src/refund.py".to_string()]),
            ("REQ-9".to_string(), vec!["src/health.py".to_string()]),
        ]);
        let changes = BTreeMap::from([
            (
                "src/pay.py".to_string(),
                vec![
                    (SymbolId(1), "charge".to_string(), ChangeClass::Modified),
                    (SymbolId(2), "charge_v2".to_string(), ChangeClass::Added),
                    (SymbolId(3), "old_charge".to_string(), ChangeClass::Deleted),
                ],
            ),
            (
                "src/refund.py".to_string(),
                vec![(SymbolId(4), "refund".to_string(), ChangeClass::AffectedButUnchanged)],
            ),
        ]);
        (reqs, changes)
    }

    #[test]
    fn attribution_is_searchable_and_classes_distinguishable() {
        let (reqs, changes) = fixture();
        let idx = attribute(&reqs, 41, &changes);
        let l = &idx;
        assert_eq!(l.len(), 4);
        let r7 = l.for_requirement("REQ-7");
        assert_eq!(r7.len(), 4);
        let classes = l.classes_of("REQ-7");
        assert!(classes.contains(&ChangeClass::Modified));
        assert!(classes.contains(&ChangeClass::Added));
        assert!(classes.contains(&ChangeClass::Deleted));
        assert!(classes.contains(&ChangeClass::AffectedButUnchanged));
        assert_eq!(l.for_requirement("REQ-7")[0].rev, 41);
        // REQ-9 maps to a path with no changes → empty, not fabricated
        assert!(l.for_requirement("REQ-9").is_empty());
        assert!(l.classes_of("REQ-9").is_empty());
    }

    #[test]
    fn rename_and_reuse_require_caller_identity_mapping() {
        let (reqs, mut changes) = fixture();
        changes.insert(
            "src/pay.py".to_string(),
            vec![
                (SymbolId(1), "charge".into(), ChangeClass::Renamed),
                (SymbolId(2), "charge_v2".into(), ChangeClass::Reused),
            ],
        );
        let idx = attribute(&reqs, 42, &changes);
        let classes = idx.classes_of("REQ-7");
        assert!(classes.contains(&ChangeClass::Renamed));
        assert!(classes.contains(&ChangeClass::Reused));
    }
}
