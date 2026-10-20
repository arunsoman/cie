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
    #[serde(default)]
    pub state_model: Option<cie_contract::obligations::StateModelView>,
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
    /// RFC-0019 §6.1: the stack lens' reading of this revision (routes,
    /// flows, apps, docker, jvm family facts), persisted with the snapshot
    /// so the two lenses finally share one artifact. Absent (None) on
    /// snapshots from before this RFC — old artifacts never gain claims
    /// they were never derived from.
    #[serde(default)]
    pub stack: Option<cie_stack::types::StackReport>,
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
    ///
    /// Issue #10: after dropping, everything is RENUMBERED so the stored
    /// invariant "SymbolId == index into symbols, FileId == index into
    /// files" survives the shrink. Without this, a reload rebuilt a
    /// SymbolStore whose `get(id)` (a raw Vec index) silently resolved to
    /// the wrong survivor or to nothing.
    fn purge_tombstoned(&self, snap: &mut RevSnapshot) -> std::io::Result<usize> {
        let tomb = self.tombstones()?;
        if tomb.is_empty() {
            return Ok(0);
        }
        let before = snap.symbols.len();
        // snap.files is the file table: FileId -> path (TASK-006 spans
        // carry no path; resolve through the snapshot's own list).
        let path_of = |f: cie_core::types::FileId| -> &str {
            snap.files.get(f.0 as usize).map(|s| s.as_str()).unwrap_or("")
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

        // Findings anchored on dead symbols are stale by construction.
        snap.findings.retain(|f| !dead.contains(&f.symbol));

        // Concept nodes must not reference purged symbols.
        for node in &mut snap.concepts.nodes {
            node.symbols.retain(|s| !dead.contains(s));
            node.behaviors.retain(|b| !dead.contains(&b.summary));
        }

        // ---- Issue #10: renumber ids so id == index again ----

        // 1. Files: keep the surviving paths; every surviving symbol's
        //    loc.file still holds its OLD index into the longer list —
        //    rewrite it to the path's new position.
        let file_remap: std::collections::BTreeMap<u32, cie_core::types::FileId> = snap
            .files
            .iter()
            .enumerate()
            .filter(|(_, p)| !tomb.contains(*p))
            .enumerate()
            .map(|(new, (old, _))| (old as u32, cie_core::types::FileId(new as u32)))
            .collect();
        snap.files.retain(|f| !tomb.contains(f));
        for sym in &mut snap.symbols {
            if let Some(newf) = file_remap.get(&sym.loc.file.0) {
                sym.loc.file = *newf;
            }
        }

        // 2. Symbols: survivors compact to positions 0..n; their ids move
        //    with them (parents ride along — purge is per file, so a
        //    survivor's parent is a survivor too; drop the pointer if it
        //    somehow was not).
        let sym_remap: std::collections::BTreeMap<u32, cie_core::types::SymbolId> = snap
            .symbols
            .iter()
            .enumerate()
            .map(|(new, s)| (s.id.0, cie_core::types::SymbolId(new as u32)))
            .collect();
        for (new, sym) in snap.symbols.iter_mut().enumerate() {
            sym.id = cie_core::types::SymbolId(new as u32);
            sym.parent = sym.parent.and_then(|p| sym_remap.get(&p.0).copied());
        }

        // 3. Every remaining SymbolId reference — summaries + call edges,
        //    findings, contracts, concept nodes and behaviors — moves to
        //    the new numbering. The pruning above already removed dead
        //    references, so every remap hit is expected; filter_map is the
        //    defensive backstop, not the happy path.
        let remap = |id: cie_core::types::SymbolId| -> Option<cie_core::types::SymbolId> {
            sym_remap.get(&id.0).copied()
        };
        for sum in &mut snap.summaries {
            if let Some(n) = remap(sum.sym) {
                sum.sym = n;
            }
            sum.callees = sum.callees.iter().filter_map(|c| remap(*c)).collect();
        }
        for f in &mut snap.findings {
            if let Some(n) = remap(f.symbol) {
                f.symbol = n;
            }
        }
        for c in &mut snap.contracts {
            c.symbols = c.symbols.iter().filter_map(|s| remap(*s)).collect();
            c.verified_by = c.verified_by.iter().filter_map(|s| remap(*s)).collect();
        }
        if let Some(model) = &mut snap.state_model {
            for t in &mut model.transitions {
                t.evidence = t.evidence.iter().filter_map(|p| remap(p.symbol).map(|id| {let mut p=p.clone();p.symbol=id;p})).collect();
                t.provenance = t.evidence.iter().map(|p|format!("symbol {}: {}",p.symbol.0,p.effect)).collect::<Vec<_>>().join("; ");
            }
            model.transitions.retain(|t|!t.evidence.is_empty());
            if model.transitions.is_empty() {snap.state_model=None;}
        }
        for node in &mut snap.concepts.nodes {
            node.symbols = node.symbols.iter().filter_map(|s| remap(*s)).collect();
            for b in &mut node.behaviors {
                if let Some(n) = remap(b.summary) {
                    b.summary = n;
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

#[cfg(test)]
mod tests {
    //! Issue #10 regression: purge must RENUMBER ids, not just shrink the
    //! vecs — a reload rebuilds SymbolStore on the persisted vec, where
    //! `get(id)` is a raw index lookup.

    use super::*;
    use cie_concepts::{BehaviorRef, ConceptKind, ConceptNode};
    use cie_contract::findings::Severity;
    use cie_contract::{Authority, Contract, ContractSource, Finding, FindingKind, ObligationKind};
    use cie_core::types::{FileId, SourceLocation, SymbolId};
    use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
    use cie_semantic::lang::LangId;
    use cie_summary::FunctionSummary;

    fn fact(id: u32, name: &str, file: u32, parent: Option<u32>) -> SymbolFact {
        SymbolFact {
            id: SymbolId(id),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qual_name: format!("m.{name}"),
            loc: SourceLocation {
                file: FileId(file),
                start: 0,
                end: 10,
            },
            lang: LangId::Python,
            parent: parent.map(SymbolId),
            rev: 1,
            doc: None,
        }
    }

    fn finding(symbol: SymbolId) -> Finding {
        Finding {
                introduced: None,
            id: format!("F-1-{}", symbol.0),
            kind: FindingKind::HardcodedSecret,
            severity: Severity::High,
            symbol,
            symbol_name: "s".into(),
            message: "m".into(),
            evidence_lines: vec![],
            remediation: String::new(),
            authority: Authority::InferredCandidate,
            rule: "sec/SEC-001".into(),
            tags: vec![],
        }
    }

    #[test]
    fn issue_10_purge_renumbers_symbols_files_and_references() {
        let dir = std::env::temp_dir().join(format!("cie-purge-remap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap();
        store.mark_tombstone("b.py").unwrap();

        // ids intentionally sparse: 0,1 die; 6,7 survive (id != index already
        // in the raw export — extract_project rewrites, this test stresses
        // the purge path on its own).
        let mut snap = RevSnapshot {
            rev: 2,
            files: vec!["a.py".into(), "b.py".into(), "c.py".into()],
            symbols: vec![
                fact(0, "a_one", 0, None),
                fact(1, "b_one", 1, None), // dies with b.py
                fact(6, "c_one", 2, None),
                fact(7, "c_two", 2, Some(6)),
            ],
            summaries: vec![
                FunctionSummary {
                    sym: SymbolId(6),
                    callees: vec![SymbolId(7), SymbolId(1)],
                    ..Default::default()
                },
                FunctionSummary {
                    sym: SymbolId(1), // dies
                    ..Default::default()
                },
            ],
            findings: vec![finding(SymbolId(7)), finding(SymbolId(1))],
            contracts: vec![Contract {
                verified_by: vec![],
                introduced: None,
                id: 0,
                obligation: ObligationKind::MustAuthorize,
                expr: "e".into(),
                authority: Authority::Explicit,
                symbols: vec![SymbolId(7), SymbolId(1)],
                source: ContractSource::Docstring,
            }],
            concepts: ConceptDag {
                nodes: vec![{
                    let mut n = ConceptNode::new(0, "L", ConceptKind::Module, 0);
                    n.symbols = vec![SymbolId(6), SymbolId(1)];
                    n.behaviors = vec![BehaviorRef {
                        summary: SymbolId(7),
                        kind: "does".into(),
                        confidence: 1.0,
                    }];
                    n
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        snap.contracts[0].verified_by=vec![SymbolId(7),SymbolId(1)];
        let purged = store.purge_tombstoned(&mut snap).unwrap();
        assert_eq!(snap.contracts[0].verified_by,vec![SymbolId(2)]);
        assert_eq!(purged, 2, "one symbol + one dead summary counted");

        // Files shrank, order preserved.
        assert_eq!(snap.files, vec!["a.py".to_string(), "c.py".to_string()]);

        // id == index again, survivors in order.
        for (i, s) in snap.symbols.iter().enumerate() {
            assert_eq!(s.id.0 as usize, i, "survivor ids must be renumbered");
        }
        let names: Vec<&str> = snap.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a_one", "c_one", "c_two"]);

        // loc.file remapped onto the retained path list: c.py moved 2 -> 1.
        assert_eq!(snap.symbols[1].loc.file, FileId(1));
        assert_eq!(snap.symbols[2].loc.file, FileId(1));
        // Parent pointer remapped with the same table (6 -> 1).
        assert_eq!(snap.symbols[2].parent, Some(SymbolId(1)));

        // THE issue #10 assertion: a rebuilt SymbolStore resolves ids to the
        // RIGHT records. Before the fix, get(1) returned c_one (b_one's old
        // slot) and get(2)/get(3) missed or hit leftovers.
        let st = SymbolStore::build(snap.symbols.clone());
        assert_eq!(st.get(SymbolId(0)).unwrap().name, "a_one");
        assert_eq!(st.get(SymbolId(1)).unwrap().name, "c_one");
        assert_eq!(st.get(SymbolId(2)).unwrap().name, "c_two");
        assert!(st.get(SymbolId(3)).is_none());

        // Every reference table moved to the new numbering.
        assert_eq!(snap.summaries.len(), 1);
        assert_eq!(snap.summaries[0].sym, SymbolId(1), "old 6 -> new 1");
        assert_eq!(snap.summaries[0].callees, vec![SymbolId(2)], "old 7 -> 2, dead callee dropped");
        assert_eq!(snap.findings.len(), 1);
        assert_eq!(snap.findings[0].symbol, SymbolId(2));
        assert_eq!(snap.contracts[0].symbols, vec![SymbolId(2)]);
        assert_eq!(snap.concepts.nodes[0].symbols, vec![SymbolId(1)]);
        assert_eq!(snap.concepts.nodes[0].behaviors[0].summary, SymbolId(2));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
