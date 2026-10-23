//! cie-store — revisioned, file-backed persistence implementing the CIE
//! key namespaces (PRD §57) with a JSON layout that is transparent and
//! diff-friendly. The PRD's L1 (sled/redb) backend is a drop-in future
//! replacement behind this same API (TASK-024).
//!
//! Scope (TASK-001 layering): this crate depends on `cie-core` ONLY. Typed
//! persistence for domain payloads ships as extension traits in the domain
//! crates (`cie-incremental::SnapshotStoreExt`, `cie-llm::LabelStoreExt`,
//! `cie-claims::ClaimStoreExt`, `cie-drift::DriftStoreExt`,
//! `cie-concepts::ConceptStoreExt`) so no crate cycle can form.
//!
//! Layout under `<project>/.cie/`:
//! ```text
//! meta.json                      -> rev:{rev_id}:meta
//! files.json                     -> file:{path}:hash + file:{path}:lang
//! labels.json                    -> label:{record_id}
//! revs/{rev}/snapshot.json       -> sym:{id}, sum:{rev}:{sym}, findings,
//!                                   concept_node:{id}, evidence:{record}
//! revs/{rev}/drift.json          -> drift:{rev}:{doc}
//! org.json                       -> merged_concept:{id}
//! ```

pub mod archive;
pub mod hnsw;
pub mod kv;
pub mod snapshot;
pub mod tables;
use cie_core::types::RevId;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub mod records;
pub use records::{Conclusion, DerivedRecord, RecordId, RecordKind, RecordStoreExt, TimeoutMarker, TimedRecord};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Meta {
    last_rev: RevId,
    schema_version: u32,
}

/// TASK-028 deletion ledger: the §15.1 `file:{path}:tombstone` namespace.
/// A tombstone records "this path was deleted at some revision" so the
/// snapshot builder can purge stale facts explicitly instead of relying on
/// incidental rebuilds (deletion correctness = acc #7).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Tombstones {
    paths: std::collections::BTreeSet<String>,
}

/// Cross-repo merged concept (FR-46 / §55).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedConcept {
    pub merged_id: u32,
    pub label: String,
    /// (repo path, concept node id in that repo's latest snapshot)
    pub sources: Vec<(String, u32)>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrgGraph {
    pub repos: Vec<String>,
    pub merged_concepts: Vec<MergedConcept>,
    /// repo A -> repo B -> list of (symbol_a, symbol_b, relation)
    pub api_edges: Vec<ApiEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEdge {
    pub from_repo: String,
    pub to_repo: String,
    pub from_symbol: String,
    pub to_symbol: String,
    pub kind: String,
    /// Association evidence, not a proven import/export or runtime call.
    #[serde(default)]
    pub match_basis: Option<String>,
    #[serde(default)]
    pub claim_class: Option<String>,
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Open (creating if needed) the store rooted at `<project>/.cie`.
    pub fn open(project_dir: &Path) -> std::io::Result<Store> {
        let root = project_dir.join(".cie");
        fs::create_dir_all(root.join("revs"))?;
        if !root.join("meta.json").exists() {
            let meta = Meta {
                last_rev: 0,
                schema_version: 1,
            };
            fs::write(root.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        }
        Ok(Store { root })
    }

    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &Path,
    ) -> std::io::Result<Option<T>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn last_rev(&self) -> RevId {
        self.read_json::<Meta>(&self.root.join("meta.json"))
            .ok()
            .flatten()
            .map(|m| m.last_rev)
            .unwrap_or(0)
    }

    /// Allocate the next revision id and persist it as current.
    pub fn bump_rev(&self) -> std::io::Result<RevId> {
        let next = self.last_rev() + 1;
        self.commit_rev(next)?;
        Ok(next)
    }

    /// RevisionAllocator seeded from the persisted meta head — the TASK-002
    /// restart-recovery contract: after a process restart the revision chain
    /// continues strictly monotonically from `rev:{rev_id}:meta` (§15.1).
    pub fn allocator(&self) -> cie_core::types::RevisionAllocator {
        cie_core::types::RevisionAllocator::starting_at(self.last_rev())
    }

    /// Persist `rev` as the new head (only ever moves forward).
    pub fn commit_rev(&self, rev: RevId) -> std::io::Result<()> {
        let current = self.last_rev();
        if rev <= current {
            return Ok(()); // restart replay: head already advanced
        }
        let meta = Meta {
            last_rev: rev,
            schema_version: 1,
        };
        self.write_json(&self.root.join("meta.json"), &meta)?;
        Ok(())
    }

    // ---- TASK-028 tombstones (§15.1 `file:{path}:tombstone`) ----

    fn load_tombstones(&self) -> std::io::Result<Tombstones> {
        Ok(self
            .read_json::<Tombstones>(&self.root.join("tombstones.json"))?
            .unwrap_or_default())
    }

    /// Record a deletion so the next snapshot build purges stale facts.
    pub fn mark_tombstone(&self, path: &str) -> std::io::Result<()> {
        let mut t = self.load_tombstones()?;
        t.paths.insert(path.to_string());
        self.write_json(&self.root.join("tombstones.json"), &t)
    }

    /// Batch form of `mark_tombstone`: loads and writes the tombstone file
    /// ONCE per run instead of once per deleted file. The per-record rewrite
    /// is O(n²) in bytes — every record rewrites the whole growing file
    /// (DS-EVAL §6.4, bench A3 measured 0.80 ms/record and climbing) — so a
    /// mass-delete run used to pay file-size × deleted-count. No-op (no file
    /// write at all) when the batch changes nothing.
    pub fn mark_tombstones<I, S>(&self, paths: I) -> std::io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut t = self.load_tombstones()?;
        let before = t.paths.len();
        t.paths.extend(paths.into_iter().map(Into::into));
        if t.paths.len() == before {
            return Ok(());
        }
        self.write_json(&self.root.join("tombstones.json"), &t)
    }

    /// Batch form of `clear_tombstone` (paths reappeared — re-added/changed):
    /// same once-per-run load/write discipline. No-op when nothing clears,
    /// so a run with no deletions never even creates `tombstones.json`.
    pub fn clear_tombstones<I, S>(&self, paths: I) -> std::io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut t = self.load_tombstones()?;
        let before = t.paths.len();
        for p in paths {
            t.paths.remove(&p.into());
        }
        if t.paths.len() == before {
            return Ok(());
        }
        self.write_json(&self.root.join("tombstones.json"), &t)
    }

    /// Clear the tombstone when the path reappears (re-added/changed).
    pub fn clear_tombstone(&self, path: &str) -> std::io::Result<()> {
        let mut t = self.load_tombstones()?;
        t.paths.remove(path);
        self.write_json(&self.root.join("tombstones.json"), &t)
    }

    pub fn tombstones(&self) -> std::io::Result<std::collections::BTreeSet<String>> {
        Ok(self.load_tombstones()?.paths)
    }

    pub fn list_revs(&self) -> std::io::Result<Vec<RevId>> {
        let mut revs = vec![];
        for entry in fs::read_dir(self.root.join("revs"))? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(rev) = name.parse::<RevId>() {
                    revs.push(rev);
                }
            }
        }
        revs.sort_unstable();
        Ok(revs)
    }

    pub fn save_org(&self, org: &OrgGraph) -> std::io::Result<()> {
        self.write_json(&self.root.join("org.json"), org)
    }

    pub fn load_org(&self) -> std::io::Result<Option<OrgGraph>> {
        self.read_json(&self.root.join("org.json"))
    }

    /// The store root (`<project>/.cie`) — extension traits build their
    /// namespace paths from here.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory holding per-revision subdirectories.
    pub fn revs_dir(&self) -> PathBuf {
        self.root.join("revs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cie-store-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn tombstone_ledger_roundtrip() {
        let dir = tmpdir("tomb_rt");
        let store = Store::open(&dir).unwrap();
        store.mark_tombstone("gone.py").unwrap();
        assert!(store.tombstones().unwrap().contains("gone.py"));
        store.clear_tombstone("gone.py").unwrap();
        assert!(store.tombstones().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_002_revision_allocator_restart_recovery() {
        // TASK-002: last RevId recoverable after restart via the meta head.
        let root = tmpdir("revalloc");
        let store = Store::open(&root).unwrap();
        let a1 = store.allocator();
        assert_eq!(a1.next(), 1);
        assert_eq!(a1.next(), 2);
        store.commit_rev(a1.peek()).unwrap();

        // "Restart": new Store + new allocator seeded from meta.json.
        let store2 = Store::open(&root).unwrap();
        let a2 = store2.allocator();
        assert_eq!(a2.next(), 3, "chain continues strictly monotonically");
        store2.commit_rev(a2.peek()).unwrap();
        assert_eq!(store2.last_rev(), 3);
        let _ = fs::remove_dir_all(&root);
    }
}
