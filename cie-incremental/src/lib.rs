//! Incremental engine (PRD §21 / FR-8): content hashes, ingest planning,
//! reverse dependencies and invalidation. Deletion handling is explicit —
//! removing a file drops its symbols and every dependent record.

use cie_core::types::{Hash, RevId, SymbolId};
use cie_semantic::LangId;
use serde::{Deserialize, Serialize};
pub mod differential;
pub mod engine;
pub mod proptests;
pub mod snapshot;

pub use differential::{RelationState, RelationTrace};
pub use engine::{AnalysisStatus, CancellationToken, Checkpoint, Engine, EngineBudget, RunOutcome, WorkItem};
pub use proptests::{clean_recompute, extract_segments, Edit, IncrWorld, RepoSnapshot, SymInfo};
pub use snapshot::{RevSnapshot, SnapshotStoreExt};

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

/// TASK-056: symbol-level dirty worklist. Instead of invalidating whole
/// files, invalidate exactly the transitive reverse-dependency set of the
/// changed symbols: a symbol is dirty if it is new, or its local-facts hash
/// changed, or it (transitively) calls a dirty symbol. Everything else can
/// reuse its stored summary. Deleted symbols are handled by the tombstone
/// purge (TASK-028) — their former callers re-resolve and surface as
/// changed/unresolved on the next pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolWorklist {
    pub dirty: BTreeSet<SymbolId>,
    pub changed: BTreeSet<SymbolId>,
}

/// Compute the worklist. `prior_local` maps a stable key
/// (`"path::qual_name"`) to the blake3 hash of that symbol's local facts at
/// the prior analysis; `current_local` is the same map for the current pass;
/// `id_of` resolves a stable key to its current symbol id; `callers_of` is
/// the current reverse-dependency graph (callee -> direct callers).
pub fn symbol_dirty_worklist(
    prior_local: &BTreeMap<String, Hash>,
    current_local: &BTreeMap<String, Hash>,
    id_of: &BTreeMap<String, SymbolId>,
    callers_of: &BTreeMap<SymbolId, BTreeSet<SymbolId>>,
) -> SymbolWorklist {
    let mut seeds: BTreeSet<SymbolId> = BTreeSet::new();
    let mut changed: BTreeSet<SymbolId> = BTreeSet::new();
    for (key, cur_hash) in current_local {
        let id = match id_of.get(key) {
            Some(id) => *id,
            None => continue,
        };
        match prior_local.get(key) {
            None => {
                seeds.insert(id); // added symbol
                changed.insert(id);
            }
            Some(prev) if prev != cur_hash => {
                seeds.insert(id); // changed local facts
                changed.insert(id);
            }
            _ => {}
        }
    }
    // Expand by transitive reverse dependencies (bounded fixpoint).
    let mut dirty = seeds.clone();
    let mut queue: Vec<SymbolId> = seeds.iter().copied().collect();
    while let Some(s) = queue.pop() {
        if let Some(callers) = callers_of.get(&s) {
            for c in callers {
                if dirty.insert(*c) {
                    queue.push(*c);
                }
            }
        }
    }
    SymbolWorklist { dirty, changed }
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
                symbols: vec![SymbolId(0)],
            },
        );
        prev.files.insert(
            "old.rs".into(),
            FileRecord {
                path: "old.rs".into(),
                hash: h(2),
                lang: LangId::Rust,
                last_rev: 1,
                symbols: vec![SymbolId(1)],
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
        rd.record(SymbolId(1), SymbolId(2));
        rd.record(SymbolId(2), SymbolId(3));
        rd.record(SymbolId(1), SymbolId(4));
        let affected = rd.transitive_callers(&[SymbolId(1)]);
        assert_eq!(
            affected,
            BTreeSet::from([SymbolId(2), SymbolId(3), SymbolId(4)])
        );
    }

    #[test]
    fn task_056_worklist_invalidates_transitive_callers_only() {
        // prior: a::f, a::g, b::k, c::m all with hash h(1)
        let mut prior = BTreeMap::new();
        prior.insert("a.py::f".to_string(), h(1));
        prior.insert("a.py::g".to_string(), h(1));
        prior.insert("b.py::k".to_string(), h(1));
        prior.insert("c.py::m".to_string(), h(1));

        // current: a::f changed; a::g unchanged; b::k unchanged but calls f;
        // c::m unchanged, calls nothing dirty.
        let mut current = prior.clone();
        current.insert("a.py::f".to_string(), h(9));

        let mut id_of = BTreeMap::new();
        id_of.insert("a.py::f".to_string(), SymbolId(0));
        id_of.insert("a.py::g".to_string(), SymbolId(1));
        id_of.insert("b.py::k".to_string(), SymbolId(2));
        id_of.insert("c.py::m".to_string(), SymbolId(3));

        let mut callers_of: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();
        callers_of
            .entry(SymbolId(0))
            .or_default()
            .insert(SymbolId(2)); // k -> f

        let wl = symbol_dirty_worklist(&prior, &current, &id_of, &callers_of);
        assert_eq!(wl.changed, BTreeSet::from([SymbolId(0)]), "only f changed");
        assert_eq!(
            wl.dirty,
            BTreeSet::from([SymbolId(0), SymbolId(2)]),
            "f plus its transitive caller k — exactly, no more"
        );

        // New symbols are seeds too.
        current.insert("d.py::n".to_string(), h(3));
        id_of.insert("d.py::n".to_string(), SymbolId(4));
        let wl2 = symbol_dirty_worklist(&prior, &current, &id_of, &callers_of);
        assert!(wl2.changed.contains(&SymbolId(4)), "added symbol is a seed");
    }
}
