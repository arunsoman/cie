//! Static concurrency views (CIE-US US-040 / US-079 / US-080 engine close).
//!
//! Built over the facts the deep adapters already extract:
//! * lock acquire/release sites — `BuiltinKind::Lock` callsites, lock name
//!   taken from the first string-literal-ish argument text;
//! * wait-for graph — "function A holds lock L_a and (transitively) calls
//!   a function that acquires L_b ≠ L_a" is a suspected ordering risk;
//!   observed cycles do not exist statically, so every cycle edge is
//!   labelled `suspected` (US-080 separates observed from suspected);
//! * interleaving windows — pairs of accesses (assignments) to the same
//!   variable across different functions where at least one side is
//!   reachable from a spawn/async context, labelled hypothesis-only
//!   (US-040: never "a race happened");
//! * witness proposals — per suspected race, a static witness test
//!   proposal (US-079: a proposal, never a certificate; passing bounded
//!   tests is never race-free proof).
//!
//! Honesty: every output carries its label; no lock/queue/scheduler data
//! is invented (US-040 explicitly forbids decorating the picture with
//! runtime scheduler settings the engine did not read).

use std::collections::{BTreeMap, BTreeSet};

use cie_core::types::SymbolId;
use cie_semantic::facts::{AsyncKind, CallSite, SymbolStore, TypedFacts};
use serde::{Deserialize, Serialize};

/// One lock acquire/release site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockSite {
    pub symbol: SymbolId,
    /// Lock identity text (argument text; empty when anonymous).
    pub lock_name: String,
    /// acquire | release — release is inferred from the paired lock
    /// builtin naming conventions the adapters classify (`unlock`,
    /// `release`); otherwise the site is an acquire.
    pub kind: LockOpKind,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LockOpKind {
    Acquire,
    Release,
}

impl LockOpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LockOpKind::Acquire => "acquire",
            LockOpKind::Release => "release",
        }
    }
}

/// A→B ordering risk: A's function holds `lock_a` while (transitively)
/// reaching a function that acquires `lock_b`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderingRisk {
    pub holder: SymbolId,
    pub waiter: SymbolId,
    pub lock_a: String,
    pub lock_b: String,
    /// Call path from holder to waiter (symbol ids, holder first).
    pub call_path: Vec<SymbolId>,
    /// Static analysis has no observed cycles — always `suspected`.
    pub label: SuspectLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuspectLabel {
    /// Static suspicion only (the only label this module emits).
    Suspected,
}

impl SuspectLabel {
    pub fn as_str(&self) -> &'static str {
        "suspected"
    }
}

/// One interleaving window (US-040): both statements in the window cited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterleavingWindow {
    pub var: String,
    pub writer: SymbolId,
    pub writer_line: u32,
    pub reader: SymbolId,
    pub reader_line: u32,
    /// How the async reachability was established (spawn site ids).
    pub via_async: Vec<SymbolId>,
    /// Hypothesis-only: interleaving possible, never "a race happened".
    pub label: SuspectLabel,
}

/// A static witness-test proposal (US-079: proposal, NOT certificate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessProposal {
    pub window: InterleavingWindow,
    /// What the proposal would exercise (textual plan, no fabricated code).
    pub plan: String,
    /// Stated exploration scope: the async sites involved.
    pub scope: Vec<String>,
    /// Permanent honesty banner for this proposal.
    pub caveat: String,
}

pub const WITNESS_CAVEAT: &str = "static proposal only: passing bounded tests is never \
a race-free certificate";

/// The full static concurrency view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConcurrencyView {
    pub lock_sites: Vec<LockSite>,
    /// Deterministic order: (holder, waiter, lock_a, lock_b).
    pub ordering_risks: Vec<OrderingRisk>,
    pub interleaving_windows: Vec<InterleavingWindow>,
    pub witness_proposals: Vec<WitnessProposal>,
    /// Call graph used (symbol → callees), for provenance/inspection.
    pub callees_of: BTreeMap<SymbolId, Vec<SymbolId>>,
}

/// Build the view from typed facts (single revision).
pub fn build_concurrency_view(facts: &TypedFacts, store: &SymbolStore) -> ConcurrencyView {
    let lock_sites = collect_lock_sites(facts);
    let callees_of = call_graph(facts);
    let async_roots: BTreeSet<SymbolId> = facts
        .async_facts
        .iter()
        .filter(|a| matches!(a.kind, AsyncKind::Spawn | AsyncKind::AsyncFn))
        .map(|a| a.symbol)
        .collect();
    let ordering_risks = ordering_risks(&lock_sites, &callees_of);
    let (windows, proposals) =
        interleaving_windows(facts, store, &async_roots, &callees_of);
    ConcurrencyView {
        lock_sites,
        ordering_risks,
        interleaving_windows: windows,
        witness_proposals: proposals,
        callees_of,
    }
}

fn collect_lock_sites(facts: &TypedFacts) -> Vec<LockSite> {
    let mut out = Vec::new();
    for c in &facts.calls {
        if c.callee != cie_semantic::facts::Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock)
        {
            continue;
        }
        let name = callee_text(c).to_lowercase();
        let kind = if name.contains("unlock") || name.contains("release") {
            LockOpKind::Release
        } else {
            LockOpKind::Acquire
        };
        out.push(LockSite {
            symbol: c.caller,
            lock_name: lock_identity(c),
            kind,
            line: c.line,
        });
    }
    out.sort_by(|a, b| {
        a.symbol
            .cmp(&b.symbol)
            .then(a.line.cmp(&b.line))
            .then(a.kind.cmp(&b.kind))
    });
    out
}

fn callee_text(c: &CallSite) -> &str {
    &c.callee_text
}

/// Lock identity: first argument variable text (e.g. `self._lock`,
/// `mu`). Empty when the call has no args — an anonymous lock.
fn lock_identity(c: &CallSite) -> String {
    c.arg_vars.first().cloned().unwrap_or_default()
}

fn call_graph(facts: &TypedFacts) -> BTreeMap<SymbolId, Vec<SymbolId>> {
    let mut g: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
    for c in &facts.calls {
        if let cie_semantic::facts::Callee::Resolved(target) = c.callee {
            g.entry(c.caller).or_default().push(target);
        }
    }
    for v in g.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    g
}

/// Wait-for ordering risks: for each function holding a lock (acquire
/// site) and each distinct function reachable in the call graph that
/// acquires a DIFFERENT lock, emit one suspected edge with the path.
fn ordering_risks(
    locks: &[LockSite],
    callees_of: &BTreeMap<SymbolId, Vec<SymbolId>>,
) -> Vec<OrderingRisk> {
    let acquires: Vec<&LockSite> = locks.iter().filter(|l| l.kind == LockOpKind::Acquire).collect();
    let mut out = Vec::new();
    let mut seen: BTreeSet<(SymbolId, SymbolId, String, String)> = BTreeSet::new();
    for h in &acquires {
        // BFS from the holder through the resolved call graph
        let mut parent: BTreeMap<SymbolId, SymbolId> = BTreeMap::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(h.symbol);
        let mut visited: BTreeSet<SymbolId> = BTreeSet::new();
        visited.insert(h.symbol);
        while let Some(cur) = queue.pop_front() {
            for next in callees_of.get(&cur).into_iter().flatten() {
                if visited.insert(*next) {
                    parent.insert(*next, cur);
                    queue.push_back(*next);
                }
            }
        }
        for w in &acquires {
            if w.symbol == h.symbol || w.lock_name == h.lock_name || w.lock_name.is_empty() {
                continue;
            }
            if !visited.contains(&w.symbol) {
                continue;
            }
            let key = (h.symbol, w.symbol, h.lock_name.clone(), w.lock_name.clone());
            if !seen.insert(key) {
                continue;
            }
            // reconstruct path holder → waiter
            let mut path = vec![w.symbol];
            let mut cur = w.symbol;
            while let Some(&p) = parent.get(&cur) {
                path.push(p);
                cur = p;
                if cur == h.symbol {
                    break;
                }
            }
            path.reverse();
            out.push(OrderingRisk {
                holder: h.symbol,
                waiter: w.symbol,
                lock_a: h.lock_name.clone(),
                lock_b: w.lock_name.clone(),
                call_path: path,
                label: SuspectLabel::Suspected,
            });
        }
    }
    out.sort_by(|a, b| {
        a.holder
            .cmp(&b.holder)
            .then(a.waiter.cmp(&b.waiter))
            .then(a.lock_a.cmp(&b.lock_a))
            .then(a.lock_b.cmp(&b.lock_b))
    });
    out
}

/// Interleaving windows: same variable written in one function and read/
/// written in another, where either function is spawn-reachable.
fn interleaving_windows(
    facts: &TypedFacts,
    store: &SymbolStore,
    async_roots: &BTreeSet<SymbolId>,
    callees_of: &BTreeMap<SymbolId, Vec<SymbolId>>,
) -> (Vec<InterleavingWindow>, Vec<WitnessProposal>) {
    // spawn-reachable set: reverse BFS from async roots over callers.
    // (Facts give callees_of; approximate "runs under spawn" by marking
    // callees reachable FROM an async root.)
    let mut spawn_reachable: BTreeSet<SymbolId> = BTreeSet::new();
    let mut queue: std::collections::VecDeque<SymbolId> = async_roots.iter().copied().collect();
    spawn_reachable.extend(async_roots.iter());
    while let Some(cur) = queue.pop_front() {
        for next in callees_of.get(&cur).into_iter().flatten() {
            if spawn_reachable.insert(*next) {
                queue.push_back(*next);
            }
        }
    }

    // var → (symbol, line, is_write) sites; writes = assignments
    let mut sites: Vec<(String, SymbolId, u32, bool)> = Vec::new();
    for a in &facts.assignments {
        sites.push((a.var.clone(), a.symbol, a.line, true));
    }
    // reads approximated: call args referencing vars (conservative)
    for c in &facts.calls {
        for v in &c.arg_vars {
            sites.push((v.clone(), c.caller, c.line, false));
        }
    }
    sites.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));

    let mut windows: Vec<InterleavingWindow> = Vec::new();
    let mut proposals: Vec<WitnessProposal> = Vec::new();
    let mut seen: BTreeSet<(String, SymbolId, SymbolId)> = BTreeSet::new();
    for i in 0..sites.len() {
        for j in (i + 1)..sites.len() {
            let (vi, si, li, wi) = &sites[i];
            let (vj, sj, lj, wj) = &sites[j];
            if vi != vj || si == sj {
                continue;
            }
            // at least one write, both sides spawn-reachable
            if !(*wi || *wj) {
                continue;
            }
            if !(spawn_reachable.contains(si) && spawn_reachable.contains(sj)) {
                continue;
            }
            let key = (vi.clone(), *si, *sj);
            if !seen.insert(key) {
                continue;
            }
            // writer first for determinism: the writing site (smaller
            // symbol id when both write), the other side as reader
            let (writer, writer_line, reader, reader_line) = match (*wi, *wj) {
                (true, false) => (*si, *li, *sj, *lj),
                (false, true) => (*sj, *lj, *si, *li),
                _ => {
                    if si <= sj {
                        (*si, *li, *sj, *lj)
                    } else {
                        (*sj, *lj, *si, *li)
                    }
                }
            };
            let via: Vec<SymbolId> = async_roots.iter().copied().collect();
            let window = InterleavingWindow {
                var: vi.clone(),
                writer,
                writer_line,
                reader,
                reader_line,
                via_async: via,
                label: SuspectLabel::Suspected,
            };
            let name_of = |id: SymbolId| {
                store
                    .get(id)
                    .map(|s| s.qual_name.clone())
                    .unwrap_or_else(|| format!("symbol#{id}"))
            };
            let scope: Vec<String> = window
                .via_async
                .iter()
                .map(|id| name_of(*id))
                .collect();
            let plan = format!(
                "propose a witness test exercising {} (write @ line {}) and \
                 {} (access @ line {}) under interleaved scheduling of {}",
                name_of(window.writer),
                window.writer_line,
                name_of(window.reader),
                window.reader_line,
                if scope.is_empty() { "spawn contexts".to_string() } else { scope.join(", ") }
            );
            windows.push(window.clone());
            proposals.push(WitnessProposal {
                window,
                plan,
                scope,
                caveat: WITNESS_CAVEAT.to_string(),
            });
        }
    }
    windows.sort_by(|a, b| {
        a.var.cmp(&b.var)
            .then(a.writer.cmp(&b.writer))
            .then(a.reader.cmp(&b.reader))
    });
    proposals.sort_by(|a, b| {
        a.window
            .var
            .cmp(&b.window.var)
            .then(a.window.writer.cmp(&b.window.writer))
            .then(a.window.reader.cmp(&b.window.reader))
    });
    (windows, proposals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::{FileId, SymbolId};
    use cie_semantic::facts::{
        AssignmentFact, AsyncSite, Callee, RhsKind, SymbolFact, SymbolKind,
    };
    use cie_semantic::lang::LangId;
    use cie_core::types::SourceLocation;

    fn sym(store: &mut Vec<SymbolFact>, name: &str, qual: &str) -> SymbolId {
        let id = SymbolId(store.len() as u32 + 1);
        store.push(SymbolFact {
            annotations: Vec::new(),
            id,
            kind: SymbolKind::Function,
            name: name.into(),
            qual_name: qual.into(),
            loc: SourceLocation::span_bytes(FileId(0), 0, 10),
            lang: LangId::Unknown,
            parent: None,
            rev: 1,
            doc: None,
        });
        id
    }

    #[test]
    fn lock_sites_acquired_and_released_by_name() {
        let facts = TypedFacts {
            calls: vec![
                CallSite {
                    caller: SymbolId(1),
                    callee_text: "acquire".into(),
                    callee: Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock),
                    line: 10,
                    in_loop: false,
                    arg_vars: vec!["self._lock".into()],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                CallSite {
                    caller: SymbolId(1),
                    callee_text: "release".into(),
                    callee: Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock),
                    line: 20,
                    in_loop: false,
                    arg_vars: vec!["self._lock".into()],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
            ],
            ..Default::default()
        };
        let store = SymbolStore::build(Vec::new());
        let view = build_concurrency_view(&facts, &store);
        assert_eq!(view.lock_sites.len(), 2);
        assert_eq!(view.lock_sites[0].kind, LockOpKind::Acquire);
        assert_eq!(view.lock_sites[0].lock_name, "self._lock");
        assert_eq!(view.lock_sites[1].kind, LockOpKind::Release);
    }

    #[test]
    fn wait_for_cycle_detected_as_suspected_ordering_risk() {
        // f1 acquires mu1, calls f2 which acquires mu2; f2 (via another
        // path) also reaches f1's acquire → both directions appear.
        let mut syms = Vec::new();
        let f1 = sym(&mut syms, "f1", "mod::f1");
        let f2 = sym(&mut syms, "f2", "mod::f2");
        let facts = TypedFacts {
            symbols: syms,
            calls: vec![
                CallSite {
                    caller: f1,
                    callee_text: "acquire".into(),
                    callee: Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock),
                    line: 2,
                    in_loop: false,
                    arg_vars: vec!["mu1".into()],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                CallSite {
                    caller: f1,
                    callee_text: "f2".into(),
                    callee: Callee::Resolved(f2),
                    line: 3,
                    in_loop: false,
                    arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                CallSite {
                    caller: f2,
                    callee_text: "acquire".into(),
                    callee: Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock),
                    line: 8,
                    in_loop: false,
                    arg_vars: vec!["mu2".into()],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                CallSite {
                    caller: f2,
                    callee_text: "acquire".into(),
                    callee: Callee::Builtin(cie_semantic::facts::BuiltinKind::Lock),
                    line: 9,
                    in_loop: false,
                    arg_vars: vec!["mu1".into()],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                // f2 calls back into f1 → the reverse direction of the
                // suspected cycle is also emitted
                CallSite {
                    caller: f2,
                    callee_text: "f1".into(),
                    callee: Callee::Resolved(f1),
                    line: 10,
                    in_loop: false,
                    arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
            ],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let view = build_concurrency_view(&facts, &store);
        // f1 holds mu1 reaches mu2 (in f2); f2 holds mu2 reaches mu1
        assert_eq!(view.ordering_risks.len(), 2);
        for r in &view.ordering_risks {
            assert_eq!(r.label.as_str(), "suspected");
            assert!(!r.call_path.is_empty());
        }
        let pair: Vec<(SymbolId, SymbolId)> =
            view.ordering_risks.iter().map(|r| (r.holder, r.waiter)).collect();
        assert!(pair.contains(&(f1, f2)));
        assert!(pair.contains(&(f2, f1)));
    }

    #[test]
    fn interleaving_windows_cite_both_statements_and_propose_witnesses() {
        let mut syms = Vec::new();
        let spawner = sym(&mut syms, "run", "mod::run");
        let worker_a = sym(&mut syms, "worker_a", "mod::worker_a");
        let worker_b = sym(&mut syms, "worker_b", "mod::worker_b");
        let facts = TypedFacts {
            symbols: syms,
            calls: vec![
                CallSite {
                    caller: spawner,
                    callee_text: "worker_a".into(),
                    callee: Callee::Resolved(worker_a),
                    line: 2,
                    in_loop: false,
                    arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
                CallSite {
                    caller: spawner,
                    callee_text: "worker_b".into(),
                    callee: Callee::Resolved(worker_b),
                    line: 3,
                    in_loop: false,
                    arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 0),
            },
            ],
            assignments: vec![
                AssignmentFact {
                    symbol: worker_a,
                    var: "cache".into(),
                    line: 12,
                    rhs_kind: RhsKind::Vars,
                    rhs_data: vec![],
                },
                AssignmentFact {
                    symbol: worker_b,
                    var: "cache".into(),
                    line: 30,
                    rhs_kind: RhsKind::Vars,
                    rhs_data: vec![],
                },
            ],
            async_facts: vec![AsyncSite {
                symbol: spawner,
                kind: AsyncKind::Spawn,
                line: 1,
            }],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let view = build_concurrency_view(&facts, &store);
        assert_eq!(view.interleaving_windows.len(), 1);
        let w = &view.interleaving_windows[0];
        assert_eq!(w.var, "cache");
        assert_eq!(w.writer_line, 12);
        assert_eq!(w.reader_line, 30);
        assert_eq!(w.label.as_str(), "suspected");
        assert_eq!(view.witness_proposals.len(), 1);
        let p = &view.witness_proposals[0];
        assert!(p.plan.contains("worker_a") && p.plan.contains("worker_b"));
        assert!(p.caveat.contains("never"));
        assert!(!p.scope.is_empty());
    }

    #[test]
    fn no_async_context_means_no_interleaving_claim() {
        let mut syms = Vec::new();
        let a = sym(&mut syms, "a", "mod::a");
        let b = sym(&mut syms, "b", "mod::b");
        let facts = TypedFacts {
            symbols: syms,
            assignments: vec![
                AssignmentFact {
                    symbol: a,
                    var: "x".into(),
                    line: 1,
                    rhs_kind: RhsKind::Vars,
                    rhs_data: vec![],
                },
                AssignmentFact {
                    symbol: b,
                    var: "x".into(),
                    line: 2,
                    rhs_kind: RhsKind::Vars,
                    rhs_data: vec![],
                },
            ],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let view = build_concurrency_view(&facts, &store);
        assert!(view.interleaving_windows.is_empty());
        assert!(view.witness_proposals.is_empty());
    }
}
