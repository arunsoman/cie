//! Differential-style relation maintenance (TASK-057, FR-16/FR-20, §7
//! McSherry et al. Differential Dataflow CIDR 2013 row).
//!
//! Relations are maintained as MULTISETS of `(fact, rev, diff)` updates:
//! additions carry `+1`, removals retract `-1` — deletions propagate
//! through the derived dataflow (the transitive closure), and retraction
//! completeness is asserted (a stale fact is a defect, FR-8, global rule
//! 2). Frontier epochs are `RevId`; `as_of(rev)` time-travel reads replay
//! the update log within the retained frontier — compaction never
//! consumes the frontier needed by `--as-of-rev` (FR-21/FR-23, global
//! rule 1).
//!
//! Deviation (documented, single-point): the trace semantics are
//! implemented in-house rather than through the `differential-dataflow`
//! crate — its timely worker-scope model does not compose with CIE's
//! synchronous revisioned engine, and the §7 sketch is about the
//! SEMANTICS (multiset updates, retraction, frontier, as-of), which this
//! module provides deterministically. Iterative operators (closure,
//! fixpoints) run under iteration caps + `budget_ns`; exhaustion marks
//! the affected relation `Unknown` — never a stale partial answer labeled
//! complete (global rule 3). No speculative caching: the full rebuild per
//! frontier is the measured baseline (FR-16 discipline — optimization is
//! measurement-driven).

use cie_core::types::RevId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// One multiset update: fact, revision epoch, +1/-1 multiplicity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Update<F> {
    pub fact: F,
    pub rev: RevId,
    pub diff: i64,
}

/// Relation state under global rule 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationState {
    Complete,
    /// Iteration cap or budget exhausted — answers are Unknown, never
    /// stale-partial-labeled-complete.
    Unknown,
}

/// A revisioned relation trace with iterative transitive-closure
/// maintenance (sketch: `RelationTrace::transitive_closure()`).
#[derive(Debug, Clone)]
pub struct RelationTrace {
    /// The input edge multiset log (append-only).
    updates: Vec<Update<(u32, u32)>>,
    /// Frontier: the last `advance_to` revision.
    frontier: RevId,
    /// Derived closure at the frontier (source → reachable set).
    closure: BTreeMap<u32, BTreeSet<u32>>,
    /// Compaction watermark: updates strictly below this rev are compact
    /// (as-of reads at or above the watermark stay exact — the frontier
    /// is never consumed, global rule 1).
    compacted_below: RevId,
    pub state: RelationState,
    /// Iteration cap for the closure fixpoint.
    pub max_iterations: usize,
    /// Per-advance wall budget in ns (global rule 1: pure Ns arithmetic).
    pub budget_ns: i64,
}

impl Default for RelationTrace {
    fn default() -> Self {
        RelationTrace {
            updates: vec![],
            frontier: 0,
            closure: BTreeMap::new(),
            compacted_below: 0,
            state: RelationState::Complete,
            max_iterations: 1_024,
            budget_ns: 500_000_000,
        }
    }
}

impl RelationTrace {
    /// The transitive-closure relation (sketch constructor).
    pub fn transitive_closure() -> Self {
        Self::default()
    }

    /// Record an input addition at `rev` (+1 delta).
    pub fn add(&mut self, rev: RevId, fact: (u32, u32)) {
        self.updates.push(Update { fact, rev, diff: 1 });
    }

    /// Record an input removal at `rev` (−1 retraction — deletions
    /// retract deltas through the dataflow, FR-8).
    pub fn remove(&mut self, rev: RevId, fact: (u32, u32)) {
        self.updates.push(Update { fact, rev, diff: -1 });
    }

    /// TASK-056 integration: the dirty worklist output drives `feed` —
    /// dirty symbol pairs arrive as revisioned input deltas.
    pub fn feed(&mut self, rev: RevId, dirty_edges: &[(u32, u32)]) {
        for e in dirty_edges {
            self.add(rev, *e);
        }
    }

    /// Advance the frontier to `rev`: apply all pending updates ≤ rev and
    /// re-derive the closure (fixpoint under iteration cap + budget).
    pub fn advance_to(&mut self, rev: RevId) {
        let start = Instant::now();
        self.frontier = rev;
        let edges = self.edges_at(self.frontier);
        let (closure, _iterations, converged) = closure_of(&edges, self.max_iterations);
        self.closure = closure;
        if !converged || (start.elapsed().as_nanos() as i64) >= self.budget_ns {
            self.state = RelationState::Unknown; // rule 3: honest exhaustion
        } else {
            self.state = RelationState::Complete;
        }
    }

    /// Current-frontier membership (closure containment).
    pub fn contains(&self, fact: (u32, u32)) -> bool {
        let (a, b) = fact;
        if a == b {
            return self.edges_at(self.frontier).iter().any(|(x, _)| *x == a);
        }
        self.closure.get(&a).map(|s| s.contains(&b)).unwrap_or(false)
    }

    /// Time-travel read: closure as of `rev` (replay within the retained
    /// frontier — compaction never consumes it, global rule 1).
    pub fn as_of(&self, rev: RevId) -> BTreeMap<u32, BTreeSet<u32>> {
        let edges = self.edges_at(rev);
        closure_of(&edges, self.max_iterations).0
    }


    /// Edge multiset at a revision (add + retract deltas applied in rev
    /// order; ties in insertion order — deterministic).
    fn edges_at(&self, rev: RevId) -> BTreeSet<(u32, u32)> {
        let mut multiset: BTreeMap<(u32, u32), i64> = BTreeMap::new();
        for u in &self.updates {
            if u.rev <= rev {
                *multiset.entry(u.fact).or_insert(0) += u.diff;
            }
        }
        multiset
            .into_iter()
            .filter(|(_, d)| *d > 0)
            .map(|(f, _)| f)
            .collect()
    }

    /// Retraction completeness audit (FR-8): every retracted fact is gone
    /// from the current closure — a surviving stale fact is a defect.
    pub fn retraction_complete(&self) -> bool {
        let multiset = {
            let mut m: BTreeMap<(u32, u32), i64> = BTreeMap::new();
            for u in &self.updates {
                if u.rev <= self.frontier {
                    *m.entry(u.fact).or_insert(0) += u.diff;
                }
            }
            m
        };
        for (fact, diff) in &multiset {
            if *diff <= 0 && self.contains(*fact) {
                // a fully-retracted INPUT fact may still appear in the
                // closure via OTHER paths — that is correct dataflow, not
                // staleness. Staleness is: a retracted edge that is the
                // ONLY witness surviving. Check: closure path exists that
                // does not rely solely on this edge.
                if !self.has_alternative_path(fact.0, fact.1, &multiset) {
                    return false;
                }
            }
        }
        true
    }

    /// Does a path a→b exist in the current edge set other than the
    /// direct retracted edge itself?
    fn has_alternative_path(
        &self,
        a: u32,
        b: u32,
        multiset: &BTreeMap<(u32, u32), i64>,
    ) -> bool {
        let live: BTreeSet<(u32, u32)> = multiset
            .iter()
            .filter(|(_, d)| **d > 0)
            .map(|(f, _)| *f)
            .collect();
        // BFS from a avoiding the direct edge a→b as the LAST hop
        let mut queue = std::collections::VecDeque::from([(a, 0usize)]);
        let mut seen: BTreeSet<u32> = BTreeSet::from([a]);
        while let Some((n, depth)) = queue.pop_front() {
            if depth > 64 {
                break; // bounded audit
            }
            for (x, y) in &live {
                if *x == n && !(*x == a && *y == b && depth == 0) {
                    if *y == b {
                        return true;
                    }
                    if seen.insert(*y) {
                        queue.push_back((*y, depth + 1));
                    }
                }
            }
        }
        false
    }

    /// Compaction: consolidate the net effect of updates strictly below
    /// the watermark into single snapshot updates AT the watermark. The
    /// frontier state and every as-of read AT OR ABOVE the watermark stay
    /// EXACT (global rule 1 — never consume the frontier that
    /// `--as-of-rev` needs); reads below the watermark are no longer
    /// available, which is what compaction means.
    pub fn compact_below(&mut self, watermark: RevId) {
        assert!(
            watermark <= self.frontier,
            "compaction watermark must not exceed the frontier (FR-23)"
        );
        let mut net: BTreeMap<(u32, u32), i64> = BTreeMap::new();
        for u in &self.updates {
            if u.rev < watermark {
                *net.entry(u.fact).or_insert(0) += u.diff;
            }
        }
        let mut consolidated: Vec<Update<(u32, u32)>> = self
            .updates
            .iter()
            .filter(|u| u.rev >= watermark)
            .cloned()
            .collect();
        for (fact, diff) in net {
            if diff > 0 {
                consolidated.push(Update { fact, rev: watermark, diff });
            } else if diff < 0 {
                // negative net: emit paired retract updates so the
                // multiset stays balanced
                for _ in 0..(-diff) {
                    consolidated.push(Update { fact, rev: watermark, diff: -1 });
                }
            }
        }
        consolidated.sort_by(|a, b| (a.rev, a.fact).cmp(&(b.rev, b.fact)));
        self.updates = consolidated;
        self.compacted_below = watermark;
    }

    pub fn frontier(&self) -> RevId {
        self.frontier
    }

    pub fn len_updates(&self) -> usize {
        self.updates.len()
    }
}

/// Iterative closure fixpoint (worklist) — returns (closure, iterations
/// used, converged). Bounded by the caller's cap (global rule 3).
fn closure_of(
    edges: &BTreeSet<(u32, u32)>,
    max_iterations: usize,
) -> (BTreeMap<u32, BTreeSet<u32>>, usize, bool) {
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (a, b) in edges {
        adj.entry(*a).or_default().insert(*b);
    }
    // iterate: closure(x) = adj(x) ∪ ⋃ closure(y) for y ∈ adj(x)
    let mut closure: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut iterations = 0;
    loop {
        iterations += 1;
        let mut changed = false;
        for (a, succs) in adj.iter() {
            let mut extend: Vec<u32> = vec![];
            for y in succs {
                extend.push(*y);
                if let Some(cy) = closure.get(y) {
                    extend.extend(cy.iter().copied());
                }
            }
            let entry = closure.entry(*a).or_default();
            let before = entry.len();
            for y in extend {
                entry.insert(y);
            }
            changed |= entry.len() != before;
        }
        if !changed {
            return (closure, iterations, true);
        }
        if iterations >= max_iterations {
            break; // cap hit BEFORE the fixpoint — not converged
        }
    }
    (closure, iterations, false)
}
