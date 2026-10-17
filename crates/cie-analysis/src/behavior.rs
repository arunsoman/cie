//! Behavior extraction (PRD §22 / FR-11): bottom-up interprocedural
//! summaries over the resolved call graph, with effect propagation to a
//! fixpoint. Budget-aware: a deadline cut marks remaining summaries
//! `complete = false` — never silently "no defect".

use cie_core::facts::{BuiltinKind, Callee, LangFacts, RhsKind, SymbolStore};
use serde::{Deserialize, Serialize};
use cie_core::summary::{builtin_effects, EffectKind, FunctionSummary};
use cie_core::types::{Ns, RevId, SymbolId};
use std::collections::{BTreeMap, BTreeSet};

pub struct BehaviorResult {
    pub summaries: BTreeMap<SymbolId, FunctionSummary>,
    /// callee -> callers (reverse dependencies, PRD FR-8).
    pub reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// TASK-030 reuse counters for this pass.
    pub reuse: SummaryReuseStats,
}

/// TASK-030 reuse counters: `reused_files` had summaries carried over
/// verbatim (unchanged file AND unchanged dependency set); `recomputed_files`
/// were rebuilt this pass (changed file ⇒ recompute plus transitive
/// reverse-deps via the fixpoint).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryReuseStats {
    pub reused_files: usize,
    pub recomputed_files: usize,
    /// TASK-056: symbol-level counters (seeded vs invalidated worklist).
    pub reused_symbols: usize,
    pub recomputed_symbols: usize,
}

/// TASK-030 reuse decision inputs, assembled by the engine from the prior
/// snapshot. A file is reusable iff its content hash AND its dep-set hash
/// (`blake3` over the sorted content hashes of the files its resolved
/// callees live in) match the prior analysis.
#[derive(Debug, Clone, Default)]
pub struct ReusePlan {
    /// path -> (file hash, dep-set hash) valid at the prior analysis.
    pub prior_keys: BTreeMap<String, ([u8; 32], [u8; 32])>,
    /// Prior summaries keyed by (path, qual_name) — stable identity across
    /// revisions where raw symbol ids are not.
    pub prior_summaries: BTreeMap<(String, String), FunctionSummary>,
    /// Current per-file content hashes.
    pub current_hashes: BTreeMap<String, [u8; 32]>,
    /// Current dep-set: file -> sorted paths of files its callees live in.
    pub current_deps: BTreeMap<String, BTreeSet<String>>,
    /// TASK-056: symbols allowed to seed from prior summaries — the clean
    /// set under the symbol-level dirty worklist (changed symbols and their
    /// transitive callers are excluded).
    pub reusable_symbols: BTreeSet<(String, String)>,
}

impl ReusePlan {
    /// blake3 over the sorted dep file hashes (the "dep set" half of the
    /// (blake3(file), blake3(dep set)) cache key).
    pub fn dep_hash(&self, path: &str) -> [u8; 32] {
        let mut buf: Vec<u8> = vec![];
        if let Some(deps) = self.current_deps.get(path) {
            for d in deps {
                buf.extend_from_slice(self.current_hashes.get(d).unwrap_or(&[0; 32]));
            }
        }
        cie_core::types::content_hash(&buf)
    }

    /// True when `path` may reuse its prior summaries this pass.
    pub fn is_reusable(&self, path: &str) -> bool {
        let Some(file_hash) = self.current_hashes.get(path) else {
            return false;
        };
        let deps_hash = self.dep_hash(path);
        self.prior_keys.get(path) == Some(&(*file_hash, deps_hash))
    }
}

pub fn is_auth_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("auth")
        || n.contains("permission")
        || n.contains("is_allowed")
        || n.contains("has_access")
        || n.contains("can_")
        || n.contains("check_access")
        || n.contains("check_right")
        || n.contains("verify_")
        || n.contains("grant_")
        || n.contains("deny_")
}

pub fn is_money_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("charge")
        || n.contains("pay")
        || n.contains("refund")
        || n.contains("transfer")
        || n.contains("withdraw")
        || n.contains("debit")
        || n.contains("spend")
        || n.contains("invoice")
        || n.contains("billing")
}

pub fn is_destructive_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("delete")
        || n.starts_with("remove")
        || n.starts_with("drop_")
        || n.starts_with("purge")
        || n.contains("_delete")
}

pub fn is_entry_name(name: &str) -> bool {
    matches!(name, "main" | "index" | "handler" | "app" | "serve" | "run")
        || name.starts_with("handle_")
}

pub fn compute_summaries(
    store: &SymbolStore,
    per_file: &[LangFacts],
    rev: RevId,
    deadline_ns: Option<Ns>,
) -> BehaviorResult {
    compute_summaries_with_reuse(store, per_file, rev, deadline_ns, None)
}

/// TASK-030: summary computation with per-file reuse. A file whose content
/// hash and dep-set hash both match the prior analysis reuses its stored
/// `FunctionSummary`s verbatim (only rev/sym re-stamped); every other file
/// is recomputed and the fixpoint re-propagates into its transitive
/// reverse-deps (absorb is monotone, so reused seeds converge to the same
/// fixpoint as a clean recompute — pinned by the reuse test).
pub fn compute_summaries_with_reuse(
    store: &SymbolStore,
    per_file: &[LangFacts],
    rev: RevId,
    deadline_ns: Option<Ns>,
    reuse: Option<&ReusePlan>,
) -> BehaviorResult {
    let now = cie_core::types::now_ns();
    let mut summaries: BTreeMap<SymbolId, FunctionSummary> = BTreeMap::new();
    let mut reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();
    let mut stats = SummaryReuseStats::default();
    let mut seeded: BTreeSet<SymbolId> = BTreeSet::new();

    // Decide reuse once per file.
    let reusable_paths: BTreeMap<String, bool> = match reuse {
        Some(p) => p
            .current_hashes
            .keys()
            .map(|k| (k.clone(), p.is_reusable(k)))
            .collect(),
        None => BTreeMap::new(),
    };
    // Seed per-function facts. Symbol facts live in the global store;
    // per_file carries the behavior facts keyed by global symbol id.
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let sid = sym.id;
        let prior = reuse.and_then(|p| {
            // TASK-056: seeding is per-symbol via the dirty worklist.
            let key = (sym.loc.path.clone(), sym.qual_name.clone());
            if p.reusable_symbols.contains(&key) {
                p.prior_summaries.get(&key)
            } else {
                None
            }
        });
        let mut s = match prior {
            Some(prev) => {
                let mut s = prev.clone();
                s.sym = sid;
                s.rev = rev;
                s
            }
            None => FunctionSummary::empty(sid, rev, now),
        };
            let fresh = prior.is_none();
        if fresh {
            stats.recomputed_symbols += 1;
        } else {
            stats.reused_symbols += 1;
            seeded.insert(sid);
        }
        if fresh {
        for facts in per_file {
            for call in facts.calls.iter().filter(|c| c.caller == sid) {
                match &call.callee {
                    Callee::Builtin(b) => {
                        for e in builtin_effects(*b) {
                            s.effects.insert(*e);
                        }
                        match b {
                            BuiltinKind::DbQuery => {
                                s.reads.insert("db".into());
                            }
                            BuiltinKind::DbExecute => {
                                s.writes.insert("db".into());
                            }
                            BuiltinKind::FileRead => {
                                s.reads.insert("fs".into());
                            }
                            BuiltinKind::FileWrite => {
                                s.writes.insert("fs".into());
                            }
                            BuiltinKind::Env => {
                                s.reads.insert("env".into());
                            }
                            _ => {}
                        }
                    }
                    Callee::Resolved(id) => {
                        s.callees.push(*id);
                    }
                    Callee::Unresolved(reason) => {
                        s.unresolved_callees
                            .push(format!("{} ({})", call.callee_text, reason));
                    }
                }
            }

            for r in facts.returns.iter().filter(|r| r.symbol == sid) {
                if let Some(c) = &r.constant {
                    s.returns_constants.insert(c.clone());
                }
            }
            let returns: Vec<_> = facts.returns.iter().filter(|r| r.symbol == sid).collect();
            if !returns.is_empty() {
                s.all_returns_constant = returns.iter().all(|r| r.constant.is_some());
            }

            for x in facts.exceptions.iter().filter(|x| x.symbol == sid) {
                if !x.in_handler {
                    // raised exception = failure path
                    s.exceptions.push(x.kind_text.clone());
                    s.has_failure_path = true;
                }
            }

            if facts.loops.iter().any(|l| l.symbol == sid) {
                s.has_loops = true;
            }

            for a in facts.assignments.iter().filter(|a| a.symbol == sid) {
                if a.rhs_kind == RhsKind::Input {
                    s.tags.insert("reads_input".into());
                }
            }

            // decorators like @app.route make it an entry point
            for c in facts.calls.iter().filter(|c| c.caller == sid) {
                let t = c.callee_text.to_ascii_lowercase();
                if t.contains("route")
                    || t.contains("app.get")
                    || t.contains("app.post")
                    || t.contains("app.put")
                    || t.contains("app.delete")
                    || t.contains("command")
                    || t.contains("task")
                    || t.contains("endpoint")
                {
                    s.tags.insert("entrypoint".into());
                }
            }

            // path conditions from branch conditions (bounded)
            for i in facts.ifs.iter().filter(|i| i.symbol == sid).take(16) {
                s.path_conditions.push(i.cond.clone());
            }
        }
        }

        // returning false/none explicitly is also a failure path
        if s.returns_constants.contains("false") || s.returns_constants.contains("none") {
            s.has_failure_path = true;
        }

        // Tags
        if sym.is_test() {
            s.tags.insert("test".into());
        }
        if is_auth_name(&sym.name) {
            s.tags.insert("auth".into());
        }
        if is_money_name(&sym.name) {
            s.tags.insert("money".into());
        }
        if is_destructive_name(&sym.name) {
            s.tags.insert("destructive".into());
        }
        if is_entry_name(&sym.name) {
            s.tags.insert("entrypoint".into());
        }
        if s.effects.contains(&EffectKind::DbRead) {
            s.tags.insert("query".into());
        }
        if s.effects.contains(&EffectKind::DbWrite) {
            s.tags.insert("mutation".into());
        }

        summaries.insert(sid, s);
    }

    // File-level stats: a file is reused iff every function-like symbol in
    // it was seeded; interface-only files follow the (file,deps) key check.
    if reuse.is_some() {
        let mut path_total: BTreeMap<&str, usize> = BTreeMap::new();
        let mut path_seeded: BTreeMap<&str, usize> = BTreeMap::new();
        for sym in &store.symbols {
            if !sym.is_function_like() {
                continue;
            }
            *path_total.entry(sym.loc.path.as_str()).or_default() += 1;
            if seeded.contains(&sym.id) {
                *path_seeded.entry(sym.loc.path.as_str()).or_default() += 1;
            }
        }
        for path in reuse.map(|p| p.current_hashes.keys()).unwrap_or_default() {
            let all_seeded = path_seeded.get(path.as_str()) == path_total.get(path.as_str());
            if all_seeded {
                stats.reused_files += 1;
            } else {
                stats.recomputed_files += 1;
            }
        }
    }

    // Reverse dependencies from the final callee lists (covers reused files).
    for (caller, s) in &summaries {
        for c in &s.callees {
            reverse_deps.entry(*c).or_default().insert(*caller);
        }
    }

    // Fixpoint effect propagation over resolved edges (bounded).
    let mut changed = true;
    let mut iters = 0u32;
    while changed && iters < 64 {
        changed = false;
        iters += 1;
        if let Some(dl) = deadline_ns {
            if cie_core::types::now_ns() > dl {
                for s in summaries.values_mut() {
                    s.complete = false;
                }
                break;
            }
        }
        // Propagate into callers: for each edge callee -> caller.
        let edges: Vec<(SymbolId, SymbolId)> = summaries
            .iter()
            .flat_map(|(caller, s)| s.callees.iter().map(|callee| (*callee, *caller)))
            .collect();
        for (callee, caller) in edges {
            let callee_summary = match summaries.get(&callee) {
                Some(cs) => cs.clone(),
                None => continue,
            };
            if let Some(cs) = summaries.get_mut(&caller) {
                let before = (cs.effects.len(), cs.reads.len(), cs.writes.len(), cs.has_failure_path);
                cs.absorb_callee(&callee_summary);
                if (cs.effects.len(), cs.reads.len(), cs.writes.len(), cs.has_failure_path) != before {
                    changed = true;
                }
            }
        }
    }

    BehaviorResult {
        summaries,
        reverse_deps,
        reuse: stats,
    }
}
