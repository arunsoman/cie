//! One revision's full analysis snapshot and its persistence (TASK-001
//! layering: typed snapshot persistence lives in the incremental engine as
//! an extension over the generic `cie-store::Store`).

use cie_concepts::ConceptDag;
use cie_contract::findings::Finding;
use cie_contract::Contract;
use cie_core::evidence::{Coverage, Evidence};
use cie_core::types::RevId;
use cie_semantic::facts::{LangFacts, SymbolFact};
use cie_store::Store;
use cie_summary::{FunctionSummary, ReuseStats, SummaryCacheEntry};
use serde::{Deserialize, Serialize};

/// Bump when extraction/analysis semantics change so unchanged files cannot
/// silently reuse an older evidence model.
pub const EXTRACTION_SCHEMA_VERSION: u32 = 1;

/// One revision's full analysis snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevSnapshot {
    pub rev: RevId,
    #[serde(default)]
    pub extraction_schema_version: u32,
    pub files: Vec<String>,
    /// Extraction-time line indexes; never rebuilt from a changed live file.
    #[serde(default)]
    pub file_table: cie_core::types::FileTable,
    pub symbols: Vec<SymbolFact>,
    /// Parser observations from the same extraction pass. Missing on legacy
    /// snapshots; never manufacture them from names or summaries.
    #[serde(default)]
    pub facts: Vec<LangFacts>,
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
        // snap.files is the file table: FileId -> path (TASK-006 spans
        // carry no path; resolve through the snapshot's own list).
        let path_of = |f: cie_core::types::FileId| -> &str {
            snap.files
                .get(f.0 as usize)
                .map(|s| s.as_str())
                .unwrap_or("")
        };
        let dead: std::collections::BTreeSet<cie_core::types::SymbolId> = snap
            .symbols
            .iter()
            .filter(|s| tomb.contains(path_of(s.loc.file)))
            .map(|s| s.id)
            .collect();
        snap.symbols.retain(|s| !tomb.contains(path_of(s.loc.file)));
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

        let before_contracts = snap.contracts.len();
        snap.contracts.retain_mut(|contract| {
            let had_symbols = !contract.symbols.is_empty();
            contract.symbols.retain(|id| !dead.contains(id));
            !had_symbols || !contract.symbols.is_empty()
        });
        purged += before_contracts - snap.contracts.len();
        let live_contracts: std::collections::BTreeSet<_> =
            snap.contracts.iter().map(|c| c.id).collect();
        snap.concepts
            .unattached_behaviors
            .retain(|b| !dead.contains(&b.summary));
        snap.concepts
            .unattached_rules
            .retain(|r| live_contracts.contains(&r.contract));
        snap.summary_cache
            .retain(|entry| !tomb.contains(&entry.path));

        // Findings anchored on dead symbols are stale by construction.
        snap.findings.retain(|f| !dead.contains(&f.symbol));

        // Concept nodes must not reference purged symbols.
        for node in &mut snap.concepts.nodes {
            node.symbols.retain(|s| !dead.contains(s));
            node.behaviors.retain(|b| !dead.contains(&b.summary));
            node.rules.retain(|r| live_contracts.contains(&r.contract));
            node.finding_ids
                .retain(|id| snap.findings.iter().any(|f| &f.id == id));
        }

        // Files list drops tombstoned paths.
        let mut next_file = 0u32;
        let file_map: std::collections::BTreeMap<_, _> = snap
            .files
            .iter()
            .enumerate()
            .filter_map(|(old, path)| {
                if tomb.contains(path) {
                    None
                } else {
                    let entry = (old as u32, next_file);
                    next_file += 1;
                    Some(entry)
                }
            })
            .collect();
        snap.files.retain(|f| !tomb.contains(f));
        let old_table = std::mem::take(&mut snap.file_table);
        for path in &snap.files {
            snap.file_table.paths.push(path.clone());
            let lines = old_table
                .paths
                .iter()
                .position(|p| p == path)
                .and_then(|id| old_table.line_starts.get(id))
                .cloned()
                .unwrap_or_default();
            snap.file_table.line_starts.push(lines);
        }
        for symbol in &mut snap.symbols {
            if let Some(file) = file_map.get(&symbol.loc.file.0) {
                symbol.loc.file = cie_core::types::FileId(*file);
            }
        }
        snap.facts.retain(|facts| !tomb.contains(&facts.path));
        for facts in &mut snap.facts {
            for call in &mut facts.calls {
                if let cie_semantic::facts::Callee::Resolved(id) = call.callee {
                    if dead.contains(&id) {
                        call.callee = cie_semantic::facts::Callee::Unresolved(
                            "Referenced symbol was removed by tombstone".into(),
                        );
                    }
                }
            }
            for branch in &mut facts.ifs {
                if let Some(file) = file_map.get(&branch.loc.file.0) {
                    branch.loc.file = cie_core::types::FileId(*file);
                }
            }
            for node in &mut facts.loops {
                if let Some(file) = file_map.get(&node.loc.file.0) {
                    node.loc.file = cie_core::types::FileId(*file);
                }
            }
        }
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
