//! cie-storage — revisioned, file-backed persistence implementing the CIE
//! key namespaces (PRD §57) with a JSON layout that is transparent and
//! diff-friendly. The PRD's L1 (sled/redb) backend is a drop-in future
//! replacement behind this same API.
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

use cie_core::concept::ConceptDag;
use cie_core::contract::Contract;
use cie_core::drift::DriftReport;
use cie_core::evidence::{Coverage, Evidence};
use cie_core::facts::SymbolFact;
use cie_core::findings::Finding;
use cie_core::incremental::FileIndex;
use cie_core::summary::FunctionSummary;
use cie_core::types::{Ns, RevId};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

/// One revision's full analysis snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevSnapshot {
    pub rev: RevId,
    pub files: Vec<String>,
    pub symbols: Vec<SymbolFact>,
    pub summaries: Vec<FunctionSummary>,
    pub findings: Vec<Finding>,
    pub contracts: Vec<Contract>,
    pub concepts: ConceptDag,
    pub coverage: Coverage,
    pub evidence: Evidence,
    pub warnings: Vec<String>,
    pub ingest_wall_ns: u64,
    pub incremental: bool,
    /// TASK-030: per-file (blake3(file), blake3(dep set)) keys valid for
    /// THIS snapshot's summaries — the next pass reuses on both-match.
    #[serde(default)]
    pub summary_cache: Vec<SummaryCacheEntry>,
    /// TASK-030: reuse counters recorded for this revision.
    #[serde(default)]
    pub summary_reuse: Option<ReuseStats>,
}

/// One file's summary-reuse cache key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryCacheEntry {
    pub path: String,
    pub file_hash: [u8; 32],
    pub deps_hash: [u8; 32],
    /// TASK-056: per-symbol (qual_name -> local-facts blake3) for the
    /// symbol-level dirty worklist.
    #[serde(default)]
    pub symbol_keys: std::collections::BTreeMap<String, [u8; 32]>,
}

/// Summary reuse counters (TASK-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReuseStats {
    pub reused_files: usize,
    pub recomputed_files: usize,
    #[serde(default)]
    pub reused_symbols: usize,
    #[serde(default)]
    pub recomputed_symbols: usize,
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
}

/// Label cache (PRD §63): content-addressed by blake3(prompt + model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelCache {
    pub entries: std::collections::BTreeMap<String, CachedLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedLabel {
    pub label: String,
    pub model_id: String,
    pub source: String,
    pub created_ns: Ns,
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
            fs::write(
                root.join("meta.json"),
                serde_json::to_vec_pretty(&meta)?,
            )?;
        }
        Ok(Store { root })
    }

    fn read_json<T: serde::de::DeserializeOwned>(&self, path: &Path) -> std::io::Result<Option<T>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> std::io::Result<()> {
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

    pub fn load_index(&self) -> std::io::Result<Option<FileIndex>> {
        self.read_json(&self.root.join("files.json"))
    }

    pub fn save_index(&self, index: &FileIndex) -> std::io::Result<()> {
        self.write_json(&self.root.join("files.json"), index)
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

    /// Clear the tombstone when the path reappears (re-added/changed).
    pub fn clear_tombstone(&self, path: &str) -> std::io::Result<()> {
        let mut t = self.load_tombstones()?;
        t.paths.remove(path);
        self.write_json(&self.root.join("tombstones.json"), &t)
    }

    pub fn tombstones(&self) -> std::io::Result<std::collections::BTreeSet<String>> {
        Ok(self.load_tombstones()?.paths)
    }

    /// Snapshot-builder step: drop symbols of tombstoned files, then every
    /// summary/finding referencing them, then orphaned call edges. Returns
    /// the number of purged symbols. Idempotent.
    pub fn purge_tombstoned(&self, snap: &mut RevSnapshot) -> std::io::Result<usize> {
        let tomb = self.load_tombstones()?.paths;
        if tomb.is_empty() {
            return Ok(0);
        }
        let before = snap.symbols.len();
        let dead: std::collections::BTreeSet<cie_core::types::SymbolId> = snap
            .symbols
            .iter()
            .filter(|s| tomb.contains(&s.loc.path))
            .map(|s| s.id)
            .collect();
        snap.symbols.retain(|s| !tomb.contains(&s.loc.path));
        let mut purged = before - snap.symbols.len();

        // Summaries: drop dead summaries, prune orphaned call edges.
        for sum in &mut snap.summaries {
            let keep = !dead.contains(&sum.sym);
            if !keep {
                purged += 1;
            }
            sum.callees.retain(|c| !dead.contains(c));
        }
        snap.summaries.retain(|s| !dead.contains(&s.sym));

        // Findings anchored on dead symbols are stale by construction.
        snap.findings.retain(|f| !dead.contains(&f.symbol));

        // Concept nodes must not reference purged symbols.
        for node in &mut snap.concepts.nodes {
            node.symbols.retain(|s| !dead.contains(s));
            node.behaviors.retain(|b| !dead.contains(&b.summary));
        }

        // Files list drops tombstoned paths.
        snap.files.retain(|f| !tomb.contains(f));
        Ok(purged)
    }

    pub fn save_snapshot(&self, snap: &RevSnapshot) -> std::io::Result<()> {
        self.write_json(
            &self.root.join("revs").join(snap.rev.to_string()).join("snapshot.json"),
            snap,
        )
    }

    pub fn load_snapshot(&self, rev: RevId) -> std::io::Result<Option<RevSnapshot>> {
        self.read_json(&self.root.join("revs").join(rev.to_string()).join("snapshot.json"))
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

    // ---- TASK-076 acc #24: drift re-run cache keyed (code hash, docs hash).
    // One file per key (`drift_cache/{key}.json`) so a cache hit parses a
    // single report, not the whole cache.

    /// Cached full drift report: identical code + identical docs ⇒ the
    /// re-run reuses the persisted report (≤ 5% of first-run wall time).
    pub fn load_drift_cache_entry(
        &self,
        key: &str,
    ) -> std::io::Result<Option<DriftReport>> {
        // Keys are lowercase hex — safe as file names.
        self.read_json(&self.root.join("drift_cache").join(format!("{key}.json")))
    }

    pub fn save_drift_cache_entry(&self, key: &str, report: &DriftReport) -> std::io::Result<()> {
        let path = self.root.join("drift_cache").join(format!("{key}.json"));
        self.write_json(&path, report)
    }

    pub fn save_drift(&self, report: &DriftReport) -> std::io::Result<()> {
        self.write_json(
            &self
                .root
                .join("revs")
                .join(report.rev.to_string())
                .join("drift.json"),
            report,
        )
    }

    pub fn load_drift(&self, rev: RevId) -> std::io::Result<Option<DriftReport>> {
        self.read_json(&self.root.join("revs").join(rev.to_string()).join("drift.json"))
    }

    pub fn load_labels(&self) -> std::io::Result<LabelCache> {
        Ok(self
            .read_json::<LabelCache>(&self.root.join("labels.json"))?
            .unwrap_or_default())
    }

    pub fn save_labels(&self, cache: &LabelCache) -> std::io::Result<()> {
        self.write_json(&self.root.join("labels.json"), cache)
    }

    pub fn save_org(&self, org: &OrgGraph) -> std::io::Result<()> {
        self.write_json(&self.root.join("org.json"), org)
    }

    pub fn load_org(&self) -> std::io::Result<Option<OrgGraph>> {
        self.read_json(&self.root.join("org.json"))
    }

    // ---- TASK-078: the six §15.1 `concept_node:{id}:*` keys ----
    // Per node: `:node` (identity/coverage core), `:behaviors`, `:rules`,
    // `:parent`, `:children`, `:rollup` (the rollup aggregate view). The
    // DAG-level unattached lists ride alongside as `unattached.json`.
    // Layout: revs/{rev}/concepts/{id}/{node,behaviors,rules,parent,
    // children,rollup}.json — JSON behind the same Store API; the L1 KV
    // backend maps each file to its namespace key.

    /// Persist a concept DAG split across the six per-node keys.
    pub fn save_concept_six(&self, rev: RevId, dag: &ConceptDag) -> std::io::Result<()> {
        let base = self.root.join("revs").join(rev.to_string()).join("concepts");
        for node in &dag.nodes {
            let dir = base.join(node.id.to_string());
            let six = ConceptNodeSix {
                node: ConceptNodeCore {
                    id: node.id,
                    label: node.label.clone(),
                    kind: node.kind.clone(),
                    primary_edge: node.primary_edge.clone(),
                    symbols: node.symbols.clone(),
                    intent: node.intent.clone(),
                    coverage_ratio: node.coverage_ratio,
                    coverage_complete: node.coverage_complete,
                    confidence: node.confidence,
                    provenance: node.provenance.clone(),
                    finding_ids: node.finding_ids.clone(),
                    created_ns: node.created_ns,
                    compute_ns: node.compute_ns,
                },
                behaviors: node.behaviors.clone(),
                rules: node.rules.clone(),
                parent: node.parent,
                children: node.children.clone(),
                rollup: RollupView {
                    behaviors: node.behaviors.iter().map(|b| b.summary).collect(),
                    rules: node.rules.iter().map(|r| r.contract).collect(),
                    symbols: node.symbols.clone(),
                    coverage_ratio: node.coverage_ratio,
                    coverage_complete: node.coverage_complete,
                    confidence: node.confidence,
                },
            };
            // `:children` carries (child, edge reason) pairs so the
            // multi-parent DAG roundtrips with its traceable reasons (FR-42).
            let children_with_reasons: Vec<(u32, String)> = node
                .children
                .iter()
                .map(|&c| {
                    let reason = if node.parent == Some(c) {
                        // self-reference cannot happen (TASK-080 rejects);
                        // fall back to the node's own primary edge.
                        node.primary_edge
                    } else if let Some(child) = dag.get(c) {
                        if child.parent == Some(node.id) {
                            child.primary_edge
                        } else {
                            child
                                .extra_edges
                                .iter()
                                .find(|(p, _)| *p == node.id)
                                .map(|(_, r)| *r)
                                .unwrap_or(node.primary_edge)
                        }
                    } else {
                        node.primary_edge
                    };
                    (c, reason.as_str().to_string())
                })
                .collect();
            self.write_json(&dir.join("node.json"), &six.node)?;
            self.write_json(&dir.join("behaviors.json"), &six.behaviors)?;
            self.write_json(&dir.join("rules.json"), &six.rules)?;
            self.write_json(&dir.join("parent.json"), &six.parent)?;
            self.write_json(&dir.join("children.json"), &children_with_reasons)?;
            self.write_json(&dir.join("rollup.json"), &six.rollup)?;
        }
        self.write_json(
            &base.join("unattached.json"),
            &(dag.unattached_behaviors.clone(), dag.unattached_rules.clone()),
        )?;
        Ok(())
    }

    /// Rebuild a concept DAG from the six-key layout (roundtrip partner of
    /// [`Store::save_concept_six`]).
    pub fn load_concept_six(&self, rev: RevId) -> std::io::Result<Option<ConceptDag>> {
        let base = self.root.join("revs").join(rev.to_string()).join("concepts");
        if !base.exists() {
            return Ok(None);
        }
        let mut dag = ConceptDag::default();
        let mut ids: Vec<u32> = vec![];
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(id) = name.parse::<u32>() {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        for id in ids {
            let dir = base.join(id.to_string());
            let node: ConceptNodeCore = self
                .read_json(&dir.join("node.json"))?
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "node.json"))?;
            let behaviors: Vec<cie_core::concept::BehaviorRef> = self
                .read_json(&dir.join("behaviors.json"))?
                .unwrap_or_default();
            let rules: Vec<cie_core::concept::RuleRef> =
                self.read_json(&dir.join("rules.json"))?.unwrap_or_default();
            let parent: Option<u32> =
                self.read_json(&dir.join("parent.json"))?.unwrap_or(None);
            dag.nodes.push(cie_core::concept::ConceptNode {
                id: node.id,
                label: node.label,
                kind: node.kind,
                parent,
                children: vec![],
                extra_edges: vec![],
                primary_edge: node.primary_edge,
                symbols: node.symbols,
                behaviors,
                rules,
                finding_ids: node.finding_ids,
                intent: node.intent,
                coverage_ratio: node.coverage_ratio,
                coverage_complete: node.coverage_complete,
                confidence: node.confidence,
                provenance: node.provenance,
                created_ns: node.created_ns,
                compute_ns: node.compute_ns,
            });
        }
        // Reassemble the DAG: `:children` pairs carry the edge reason; an
        // entry (C, reason) on parent P is C's primary link when C.parent
        // == P, otherwise an extra edge with its reason (FR-42).
        for pid in 0..dag.nodes.len() as u32 {
            let pairs: Vec<(u32, String)> = self
                .read_json(&base.join(pid.to_string()).join("children.json"))?
                .unwrap_or_default();
            for (cid, reason_str) in pairs {
                let reason = cie_core::concept::EdgeReason::parse(&reason_str);
                let is_primary = dag.get(cid).map(|c| c.parent == Some(pid)).unwrap_or(false);
                if let Some(p) = dag.get_mut(pid) {
                    if !p.children.contains(&cid) {
                        p.children.push(cid);
                    }
                }
                if !is_primary {
                    if let Some(c) = dag.get_mut(cid) {
                        if c.parent != Some(pid)
                            && !c.extra_edges.iter().any(|(p, _)| *p == pid)
                        {
                            c.extra_edges.push((pid, reason));
                        }
                    }
                }
            }
        }
        if let Some((behaviors, rules)) =
            self.read_json::<(Vec<cie_core::concept::BehaviorRef>, Vec<cie_core::concept::RuleRef>)>(
                &base.join("unattached.json"),
            )?
        {
            dag.unattached_behaviors = behaviors;
            dag.unattached_rules = rules;
        }
        Ok(Some(dag))
    }
}

/// TASK-078 `:node` key payload — the node identity/coverage core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptNodeCore {
    pub id: u32,
    pub label: String,
    pub kind: cie_core::concept::ConceptKind,
    pub primary_edge: cie_core::concept::EdgeReason,
    pub symbols: Vec<cie_core::types::SymbolId>,
    pub intent: Vec<String>,
    pub coverage_ratio: f32,
    pub coverage_complete: bool,
    pub confidence: f32,
    pub provenance: String,
    pub finding_ids: Vec<String>,
    pub created_ns: Ns,
    pub compute_ns: i64,
}

/// TASK-078 `:rollup` key payload — the rollup aggregate view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollupView {
    pub behaviors: Vec<cie_core::types::SymbolId>,
    pub rules: Vec<cie_core::types::ContractId>,
    pub symbols: Vec<cie_core::types::SymbolId>,
    pub coverage_ratio: f32,
    pub coverage_complete: bool,
    pub confidence: f32,
}

/// The six-key bundle as written (kept for documentation/tests).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptNodeSix {
    pub node: ConceptNodeCore,
    pub behaviors: Vec<cie_core::concept::BehaviorRef>,
    pub rules: Vec<cie_core::concept::RuleRef>,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub rollup: RollupView,
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
    fn rev_chain_and_snapshots() {
        let root = tmpdir("chain");
        let store = Store::open(&root).unwrap();
        assert_eq!(store.last_rev(), 0);
        let r1 = store.bump_rev().unwrap();
        assert_eq!(r1, 1);

        let snap = RevSnapshot {
            rev: r1,
            files: vec!["a.py".into()],
            ..Default::default()
        };
        store.save_snapshot(&snap).unwrap();
        let loaded = store.load_snapshot(r1).unwrap().unwrap();
        assert_eq!(loaded.files, vec!["a.py".to_string()]);
        assert_eq!(store.list_revs().unwrap(), vec![1]);

        // Time travel: rev 2 exists only after a second bump.
        let r2 = store.bump_rev().unwrap();
        assert_eq!(r2, 2);
        assert!(store.load_snapshot(r2).unwrap().is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn labels_roundtrip() {
        let root = tmpdir("labels");
        let store = Store::open(&root).unwrap();
        let mut labels = LabelCache::default();
        labels.entries.insert(
            "abc".into(),
            CachedLabel {
                label: "Authorize user".into(),
                model_id: "offline-v1".into(),
                source: "offline".into(),
                created_ns: 42,
            },
        );
        store.save_labels(&labels).unwrap();
        let loaded = store.load_labels().unwrap();
        assert_eq!(loaded.entries["abc"].label, "Authorize user");
        let _ = fs::remove_dir_all(&root);
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
