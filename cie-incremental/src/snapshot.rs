//! One revision's full analysis snapshot and its persistence (TASK-001
//! layering: typed snapshot persistence lives in the incremental engine as
//! an extension over the generic `cie-store::Store`).

use cie_concepts::ConceptDag;
use cie_contract::findings::Finding;
use cie_contract::Contract;
use cie_core::evidence::{Coverage, Evidence};
use cie_core::types::RevId;
use cie_semantic::facts::SymbolFact;
use cie_store::Store;
use cie_summary::{FunctionSummary, ReuseStats, SummaryCacheEntry};
use serde::{Deserialize, Serialize};

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

/// Extension trait: snapshot + file-index persistence over the generic
/// store (TASK-001 layering — no crate cycle).
pub trait SnapshotStoreExt {
    fn purge_tombstoned(&self, snap: &mut RevSnapshot) -> std::io::Result<usize>;
    fn save_snapshot(&self, snap: &RevSnapshot) -> std::io::Result<()>;
    fn load_snapshot(&self, rev: RevId) -> std::io::Result<Option<RevSnapshot>>;
    fn load_index(&self) -> std::io::Result<Option<crate::FileIndex>>;
    fn save_index(&self, index: &crate::FileIndex) -> std::io::Result<()>;
}

impl SnapshotStoreExt for Store {
    /// Snapshot-builder step: drop symbols of tombstoned files, then every
    /// summary/finding referencing them, then orphaned call edges. Returns
    /// the number of purged symbols. Idempotent.
    fn purge_tombstoned(&self, snap: &mut RevSnapshot) -> std::io::Result<usize> {
        let tomb = self.tombstones()?;
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

    fn save_snapshot(&self, snap: &RevSnapshot) -> std::io::Result<()> {
        self.write_json(
            &self
                .root()
                .join("revs")
                .join(snap.rev.to_string())
                .join("snapshot.json"),
            snap,
        )
    }

    fn load_snapshot(&self, rev: RevId) -> std::io::Result<Option<RevSnapshot>> {
        self.read_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("snapshot.json"),
        )
    }

    /// Persist/load the per-revision file index (`files.json`).
    fn load_index(&self) -> std::io::Result<Option<crate::FileIndex>> {
        self.read_json(&self.root().join("files.json"))
    }

    fn save_index(&self, index: &crate::FileIndex) -> std::io::Result<()> {
        self.write_json(&self.root().join("files.json"), index)
    }
}
