//! Deletion-correctness proof world (TASK-058, FR-8, §13 item 7).
//!
//! A deterministic model of the incremental pipeline over a miniature
//! repo: files contain `fn NAME` segments; symbol identity is
//! content-addressed (blake3 of `path::name`) so delete-then-readd of
//! identical content is identity-stable; a symbol's facts hash covers its
//! own segment AND its sorted callee names, so deleting a callee
//! invalidates the caller's record too (the §13 item 7 propagation).
//!
//! The world maintains the §15.1 namespaces touched by the phase
//! (`sym:`, `record:`, `deps:`, `rev_deps:`, `sum:`) and exposes the two
//! binding invariants:
//! - [`IncrWorld::findings_stripped`] — evidence-stripped comparison
//!   (`created_ns`/`compute_ns` never compared, global rule 1);
//! - [`IncrWorld::no_orphan_records`] — no stale fact survives in ANY
//!   namespace (FR-8 acceptance, checked mechanically).

use crate::FileRecord;
use cie_core::types::{hash, Hash, SymbolId};
use cie_semantic::LangId;
use std::collections::{BTreeMap, BTreeSet};

/// One edit step (sketch: `Edit::DeleteFile(..)` et al).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    AddFile { path: String, content: String },
    EditFile { path: String, content: String },
    DeleteFile { path: String },
}

/// Miniature repo snapshot (the oracle input).
pub type RepoSnapshot = BTreeMap<String, String>;

/// Content-addressed symbol identity: blake3(`path::name`)[0..4].
pub fn symbol_id(path: &str, name: &str) -> SymbolId {
    let h = hash(format!("{path}::{name}").as_bytes());
    SymbolId(u32::from_le_bytes([h[0], h[1], h[2], h[3]]))
}

/// Extract the `fn NAME` symbol segments of a file: (name, body-text).
pub fn extract_segments(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, Vec<&str>)> = vec![];
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("fn ") {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                out.push((name, vec![line]));
                continue;
            }
        }
        if let Some(last) = out.last_mut() {
            last.1.push(line);
        }
    }
    out.into_iter()
        .map(|(n, lines)| (n, lines.join("\n")))
        .collect()
}

/// Callee names referenced by a segment (`call NAME` lines).
fn callees_of(segment: &str) -> BTreeSet<String> {
    segment
        .lines()
        .filter_map(|l| l.trim().strip_prefix("call "))
        .map(|s| s.trim().to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymInfo {
    pub path: String,
    pub name: String,
    /// blake3 over (segment ‖ sorted callee names).
    pub facts_hash: Hash,
}

/// The incremental world under test.
#[derive(Debug, Clone, Default)]
pub struct IncrWorld {
    pub repo: RepoSnapshot,
    /// path → file record (hash + symbol ids).
    pub files: BTreeMap<String, FileRecord>,
    /// live symbols.
    pub symbols: BTreeMap<SymbolId, SymInfo>,
    /// §15.1 namespaces (miniature but structurally faithful).
    pub sym_ns: BTreeMap<SymbolId, String>,        // sym:{id} → path
    pub record_ns: BTreeMap<SymbolId, Hash>,       // record:{id} → facts hash
    pub deps_ns: BTreeMap<SymbolId, BTreeSet<SymbolId>>,   // deps:{id}
    pub rev_deps_ns: BTreeMap<SymbolId, BTreeSet<SymbolId>>, // rev_deps:{id}
    pub sum_ns: BTreeMap<String, Hash>,            // sum:{path} → file hash
    /// findings: symbol → the facts hash the finding was computed from.
    pub findings: BTreeMap<SymbolId, Hash>,
    /// Callers waiting for an unresolved callee to appear (deps are only
    /// recorded for OBSERVED lineage; the rest wait here — LDFI §7
    /// qualification applied to dependency recording).
    pub pending_callees: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    pub rev: u32,
}

impl IncrWorld {
    pub fn new(repo: RepoSnapshot) -> Self {
        let mut w = IncrWorld::default();
        let mut files: Vec<(String, String)> = repo.into_iter().collect();
        files.sort();
        for (path, content) in files {
            w.apply(Edit::AddFile { path, content });
        }
        w
    }

    /// Symbol id for a name (resolves through the file that owns it).
    fn ids_in_file(&self, path: &str) -> Vec<SymbolId> {
        self.files
            .get(path)
            .map(|r| r.symbols.clone())
            .unwrap_or_default()
    }

    /// Apply one edit at the next revision (TASK-055 diff → invalidate →
    /// recompute, the miniature pipeline).
    pub fn apply(&mut self, edit: Edit) {
        self.rev += 1;
        match edit {
            Edit::AddFile { path, content } | Edit::EditFile { path, content } => {
                self.purge_file(&path);
                self.ingest_file(&path, content);
            }
            Edit::DeleteFile { path } => {
                self.purge_file(&path);
            }
        }
    }

    /// Ingest one file: hash, symbols, records, deps both ways, findings.
    /// Two-pass: all of the file's symbols exist BEFORE dependency edges
    /// are resolved (intra-file calls are live immediately).
    fn ingest_file(&mut self, path: &str, content: String) {
        let fh = hash(content.as_bytes());
        let segments = extract_segments(&content);
        let mut ids = vec![];
        for (name, seg) in &segments {
            let id = symbol_id(path, name);
            let info = SymInfo {
                path: path.into(),
                name: name.clone(),
                facts_hash: hash(seg.as_bytes()),
            };
            let fh0 = info.facts_hash;
            self.symbols.insert(id, info);
            self.sym_ns.insert(id, path.into());
            self.record_ns.insert(id, fh0);
            self.findings.insert(id, fh0);
            ids.push(id);
        }
        for id in &ids {
            self.recompute_from_source(*id);
        }
        self.repo.insert(path.to_string(), content);
        self.files.insert(
            path.to_string(),
            FileRecord {
                path: path.into(),
                hash: fh,
                symbols: ids.clone(),
                lang: LangId::Python,
                last_rev: self.rev as u64,
            },
        );
        self.sum_ns.insert(path.to_string(), fh);
        // any caller that was WAITING on one of these symbols now resolves
        let waiting: Vec<SymbolId> = ids
            .iter()
            .filter_map(|c| self.pending_callees.get(c))
            .flatten()
            .copied()
            .collect();
        for caller in waiting {
            self.recompute_from_source(caller);
        }
    }

    /// Purge a file and every dependent record (deletion correctness —
    /// FR-8: recompute affected results without silently retaining stale
    /// facts; a deletion invalidates EARLIER findings).
    fn purge_file(&mut self, path: &str) {
        let Some(rec) = self.files.remove(path) else { return };
        self.repo.remove(path);
        self.sum_ns.remove(path);
        for id in rec.symbols {
            // dependents: callers whose deps reference this symbol
            let dependents: Vec<SymbolId> = self
                .rev_deps_ns
                .get(&id)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            for d in dependents {
                // the caller's record is recomputed from its own file's
                // CURRENT content (callee set changed → facts changed)
                self.recompute_from_source(d);
            }
            self.sym_ns.remove(&id);
            self.record_ns.remove(&id);
            self.deps_ns.remove(&id);
            self.rev_deps_ns.remove(&id);
            self.symbols.remove(&id);
            // the deleted symbol can no longer be a caller-in-waiting
            for set in self.pending_callees.values_mut() {
                set.remove(&id);
            }
            self.pending_callees.remove(&id);
            // §13 item 7: the finding on the deleted symbol disappears
            self.findings.remove(&id);
        }
    }

    /// Recompute one symbol's deps/record/findings from its file's
    /// CURRENT content: live callees become deps/rev_deps; unresolved
    /// ones wait in `pending_callees` (only observed lineage is recorded;
    /// the LDFI §7 qualification applied to dependency edges).
    fn recompute_from_source(&mut self, id: SymbolId) {
        let Some(info) = self.symbols.get(&id).cloned() else {
            self.record_ns.remove(&id);
            self.findings.remove(&id);
            self.deps_ns.remove(&id);
            return;
        };
        let Some(content) = self.repo.get(&info.path).cloned() else { return };
        let Some((_, seg)) = extract_segments(&content)
            .into_iter()
            .find(|(n, _)| *n == info.name)
        else {
            return;
        };
        let all_callees: BTreeSet<SymbolId> =
            callees_of(&seg).iter().map(|c| symbol_id(&info.path, c)).collect();
        let live: BTreeSet<SymbolId> =
            all_callees.iter().copied().filter(|c| self.symbols.contains_key(c)).collect();
        let waiting: BTreeSet<SymbolId> =
            all_callees.iter().copied().filter(|c| !self.symbols.contains_key(c)).collect();

        // facts hash covers the segment + LIVE callee ids (matches a full
        // rebuild by construction — the oracle uses the same ingestion).
        let mut facts = seg.clone().into_bytes();
        for c in &live {
            facts.extend_from_slice(&c.0.to_le_bytes());
        }
        let facts_hash = hash(&facts);

        // stale rev-deps: callees no longer live lose this caller
        let old: Vec<SymbolId> = self
            .deps_ns
            .get(&id)
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        for o in old {
            if !live.contains(&o) {
                if let Some(set) = self.rev_deps_ns.get_mut(&o) {
                    set.remove(&id);
                }
            }
        }
        self.symbols.get_mut(&id).unwrap().facts_hash = facts_hash;
        self.record_ns.insert(id, facts_hash);
        self.findings.insert(id, facts_hash);
        self.deps_ns.insert(id, live.clone());
        for c in live {
            self.rev_deps_ns.entry(c).or_default().insert(id);
        }
        if waiting.is_empty() {
            self.pending_callees.remove(&id);
        } else {
            self.pending_callees.insert(id, waiting);
        }
    }

    /// Evidence-stripped findings (global rule 1: timestamps never
    /// compared) — the structural comparison set.
    pub fn findings_stripped(&self) -> BTreeSet<(SymbolId, String)> {
        self.findings
            .iter()
            .map(|(id, h)| (*id, cie_core::types::hash_hex(h)))
            .collect()
    }

    /// FR-8 mechanical check: no stale fact survives in ANY namespace.
    pub fn no_orphan_records(&self) -> bool {
        // sym:{id} ↔ live symbols
        if self.sym_ns.len() != self.symbols.len()
            || self.sym_ns.keys().any(|k| !self.symbols.contains_key(k))
        {
            return false;
        }
        // record:{id} matches the live facts hash
        for (id, h) in &self.record_ns {
            match self.symbols.get(id) {
                Some(info) if info.facts_hash == *h => {}
                _ => return false,
            }
        }
        if self.record_ns.len() != self.symbols.len() {
            return false;
        }
        // deps:{id} refs are live symbols
        for (id, deps) in &self.deps_ns {
            if !self.symbols.contains_key(id) {
                return false;
            }
            if deps.iter().any(|d| !self.symbols.contains_key(d)) {
                return false;
            }
        }
        // rev_deps:{id} entries are live callers with matching deps
        for (id, callers) in &self.rev_deps_ns {
            if !self.symbols.contains_key(id) {
                return false;
            }
            for c in callers {
                match self.deps_ns.get(c) {
                    Some(set) if set.contains(id) => {}
                    _ => return false,
                }
            }
        }
        // sum:{path} and files agree with the repo
        for (p, h) in &self.sum_ns {
            match self.files.get(p) {
                Some(f) if &f.hash == h => {}
                _ => return false,
            }
        }
        for p in self.files.keys() {
            if !self.repo.contains_key(p) || !self.sum_ns.contains_key(p) {
                return false;
            }
        }
        // every finding references a live symbol with the live hash
        for (id, h) in &self.findings {
            match self.symbols.get(id) {
                Some(info) if info.facts_hash == *h => {}
                _ => return false,
            }
        }
        // pending callees: callers live, callees NOT live (they'd be deps)
        for (callee, callers) in &self.pending_callees {
            if self.symbols.contains_key(callee) || callers.is_empty() {
                return false;
            }
            if callers.iter().any(|c| !self.symbols.contains_key(c)) {
                return false;
            }
        }
        true
    }
}

/// The clean-recompute oracle: full rebuild from the snapshot.
pub fn clean_recompute(repo: &RepoSnapshot) -> IncrWorld {
    IncrWorld::new(repo.clone())
}
