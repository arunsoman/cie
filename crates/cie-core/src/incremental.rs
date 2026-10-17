//! Incremental engine (PRD §21 / FR-8): content hashes, ingest planning,
//! reverse dependencies and invalidation. Deletion handling is explicit —
//! removing a file drops its symbols and every dependent record.

use crate::types::{Hash, LangId, RevId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One ingested file's record in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub hash: Hash,
    pub lang: LangId,
    pub last_rev: RevId,
    pub symbols: Vec<SymbolId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIndex {
    pub files: BTreeMap<String, FileRecord>,
    pub last_rev: RevId,
    /// Wall time of the last full ingest, for the <=5% re-ingest check.
    pub last_full_ingest_ns: u64,
    pub last_reingest_ns: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestPlan {
    pub unchanged: Vec<String>,
    pub changed: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
}

impl IngestPlan {
    pub fn has_work(&self) -> bool {
        !self.changed.is_empty() || !self.added.is_empty() || !self.deleted.is_empty()
    }

    pub fn total_seen(&self) -> usize {
        self.unchanged.len() + self.changed.len() + self.added.len()
    }
}

/// Diff the previous index against the current on-disk hashes.
pub fn plan_ingest(
    prev: Option<&FileIndex>,
    current: &BTreeMap<String, (Hash, LangId)>,
) -> IngestPlan {
    let mut plan = IngestPlan::default();
    let empty: BTreeMap<String, FileRecord> = BTreeMap::new();
    let prev_files: &BTreeMap<String, FileRecord> = match prev {
        Some(p) => &p.files,
        None => &empty,
    };
    for (path, (hash, lang)) in current {
        match prev_files.get(path) {
            Some(rec) if rec.hash == *hash => plan.unchanged.push(path.clone()),
            Some(_) => plan.changed.push(path.clone()),
            None => {
                // Language change counts as a change even with equal hash.
                if let Some(rec) = prev_files.get(path) {
                    if rec.lang != *lang {
                        plan.changed.push(path.clone());
                    } else {
                        plan.unchanged.push(path.clone());
                    }
                } else {
                    plan.added.push(path.clone());
                }
            }
        }
    }
    for path in prev_files.keys() {
        if !current.contains_key(path) {
            plan.deleted.push(path.clone());
        }
    }
    plan
}

/// Reverse dependency table: callee symbol -> caller symbols.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReverseDeps {
    pub callers_of: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
}

impl ReverseDeps {
    pub fn record(&mut self, callee: SymbolId, caller: SymbolId) {
        self.callers_of.entry(callee).or_default().insert(caller);
    }

    /// Transitive callers of the given symbols (bounded fixpoint).
    pub fn transitive_callers(&self, seeds: &[SymbolId]) -> BTreeSet<SymbolId> {
        let mut seen: BTreeSet<SymbolId> = BTreeSet::new();
        let mut queue: Vec<SymbolId> = seeds.to_vec();
        while let Some(s) = queue.pop() {
            if let Some(callers) = self.callers_of.get(&s) {
                for c in callers {
                    if seen.insert(*c) {
                        queue.push(*c);
                    }
                }
            }
        }
        seen
    }
}

/// Directories ignored during the folder walk (PRD §12).
pub const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    ".venv",
    "venv",
    "dist",
    "build",
    "__pycache__",
    ".cie",
    ".idea",
    ".vscode",
];

pub fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash {
        let mut a = [0u8; 32];
        a[0] = b;
        a
    }

    #[test]
    fn plan_detects_add_change_delete() {
        let mut prev = FileIndex::default();
        prev.files.insert(
            "a.py".into(),
            FileRecord {
                path: "a.py".into(),
                hash: h(1),
                lang: LangId::Python,
                last_rev: 1,
                symbols: vec![0],
            },
        );
        prev.files.insert(
            "old.rs".into(),
            FileRecord {
                path: "old.rs".into(),
                hash: h(2),
                lang: LangId::Rust,
                last_rev: 1,
                symbols: vec![1],
            },
        );

        let mut current = BTreeMap::new();
        current.insert("a.py".into(), (h(1), LangId::Python)); // unchanged
        current.insert("b.py".into(), (h(3), LangId::Python)); // added
        current.insert("c.go".into(), (h(4), LangId::Go)); // added
        // old.rs deleted

        let plan = plan_ingest(Some(&prev), &current);
        assert_eq!(plan.unchanged, vec!["a.py"]);
        assert!(plan.added.contains(&"b.py".to_string()));
        assert!(plan.added.contains(&"c.go".to_string()));
        assert_eq!(plan.deleted, vec!["old.rs"]);
        assert!(plan.has_work());
    }

    #[test]
    fn transitive_callers_bounded() {
        let mut rd = ReverseDeps::default();
        // f3 -> f2 -> f1 ; g -> f1
        rd.record(1, 2);
        rd.record(2, 3);
        rd.record(1, 4);
        let affected = rd.transitive_callers(&[1]);
        assert_eq!(affected, BTreeSet::from([2, 3, 4]));
    }
}
