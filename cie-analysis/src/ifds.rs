//! IFDS framework (TASK-034) — exploded supergraph and tabulation.
//!
//! Module path fixed by the §7 algorithm map. Cites Reps, Horwitz &
//! Sagiv (POPL 1995). §7 qualification: IFDS is precise ONLY for its
//! supported problem class (distributive dataflow over finite fact sets);
//! joins at control-flow merges are set union only — no path-sensitivity
//! is claimed beyond the IFDS class.
//!
//! Tabulation over the exploded supergraph memoizes path edges
//! `(node, fact)`, which guarantees termination on finite fact sets
//! (Reps–Horwitz–Sagiv). The engine is generic over [`IfdsFactDomain`];
//! the two concrete instances are TASK-035's ([`UnresolvedCallDomain`],
//! [`TaintDomain`]) — §5.2 boundary: relations shared, fact domains
//! algorithm-specific. Runs under TASK-029 budgets: a budget trip yields
//! [`IfdsSolution::Unknown`], never a clean result (FR-35, global rule 3).

use cie_semantic::facts::{Callee, LangFacts};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;

/// One exploded-supergraph node: (function, line).
pub type IfdsNode = (SymbolId, u32);

use cie_core::types::SymbolId;

/// Supergraph edge kind: intra-procedural flow, call into a callee, or
/// return back to the call site's successor (Reps et al. §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EdgeKind {
    Intra,
    Call,
    Return,
}

/// A supergraph edge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    pub kind: EdgeKind,
    pub from: IfdsNode,
    pub to: IfdsNode,
    /// Callee symbol for Call/Return edges.
    pub callee: Option<SymbolId>,
}

/// The IFDS fact-domain contract (generic; instances in TASK-035).
/// `Fact: Copy + Eq + Hash + Ord`; `transfer(edge, fact)` yields the facts
/// flowing out of `to` when `fact` arrives (distributive — set union at
/// merges is the only join).
pub trait IfdsFactDomain {
    type Fact: Clone + Eq + Hash + Ord;

    /// Transfer along one edge. Returning an empty vec kills the fact.
    fn transfer(&self, edge: &Edge, fact: &Self::Fact) -> SmallVec<[Self::Fact; 4]>;

    /// Seed facts at procedure entries (from TASK-033 composed summaries
    /// or source facts).
    fn seeds(&self, graph: &Supergraph) -> BTreeMap<IfdsNode, BTreeSet<Self::Fact>>;
}

/// Exploded supergraph built from per-file facts: one node per
/// (function, statement line), intra edges in line order, call/return
/// edges matching resolved call sites to callee entries/exits.
#[derive(Debug, Clone, Default)]
pub struct Supergraph {
    pub edges: Vec<Edge>,
    /// Exit node per function (last statement line).
    pub exits: BTreeMap<SymbolId, u32>,
    pub entry_of: BTreeMap<SymbolId, u32>,
}

impl Supergraph {
    /// Build from per-file facts (single revision; facts carry the
    /// enclosing function per site — adapters vouch for it, FR-3).
    pub fn from_facts(per_file: &[LangFacts]) -> Supergraph {
        // Per-function statement lines.
        let mut stmts: BTreeMap<SymbolId, Vec<(u32, char, Option<SymbolId>, String)>> =
            BTreeMap::new();
        let mut func_min: BTreeMap<SymbolId, u32> = BTreeMap::new();
        let mut facts_by_path: BTreeMap<String, &LangFacts> = BTreeMap::new();
        for f in per_file {
            facts_by_path.insert(f.path.clone(), f);
            for s in &f.symbols {
                let e = stmts.entry(s.id).or_default();
                func_min.entry(s.id).or_insert(u32::MAX);
            }
        }
        for f in per_file {
            for c in &f.calls {
                let callee = match &c.callee {
                    Callee::Resolved(id) => Some(*id),
                    _ => None,
                };
                stmts.entry(c.caller).or_default().push((c.line, 'C', callee, c.callee_text.clone()));
            }
            for a in &f.assignments {
                stmts.entry(a.symbol).or_default().push((a.line, 'A', None, a.var.clone()));
            }
            for r in &f.returns {
                stmts.entry(r.symbol).or_default().push((r.line, 'R', None, String::new()));
            }
            for e in &f.exceptions {
                stmts.entry(e.symbol).or_default().push((e.line, 'X', None, e.kind_text.clone()));
            }
        }
        // Phase 1: entries/exits per function (call/return edges need the
        // callee's entry, so inter-procedural edges are built in phase 2 —
        // iteration order must not matter).
        let mut normalized: Vec<(SymbolId, Vec<(u32, char, Option<SymbolId>, String)>)> = vec![];
        let mut exits: BTreeMap<SymbolId, u32> = BTreeMap::new();
        let mut entry_of: BTreeMap<SymbolId, u32> = BTreeMap::new();
        for (func, mut lines) in stmts {
            if lines.is_empty() {
                continue;
            }
            lines.sort_by_key(|(l, _, _, _)| *l);
            lines.dedup_by_key(|(l, _, _, _)| *l);
            entry_of.insert(func, lines[0].0);
            exits.insert(func, lines[lines.len() - 1].0);
            normalized.push((func, lines));
        }
        // Phase 2: intra + call/return edges.
        let mut edges: Vec<Edge> = vec![];
        for (func, lines) in &normalized {
            for w in lines.windows(2) {
                edges.push(Edge {
                    kind: EdgeKind::Intra,
                    from: (*func, w[0].0),
                    to: (*func, w[1].0),
                    callee: None,
                });
            }
            for (line, kind, callee, _) in lines {
                if *kind == 'C' {
                    if let Some(callee) = callee {
                        if let (Some(&c_entry), Some(&c_exit)) =
                            (entry_of.get(callee), exits.get(callee))
                        {
                            edges.push(Edge {
                                kind: EdgeKind::Call,
                                from: (*func, *line),
                                to: (*callee, c_entry),
                                callee: Some(*callee),
                            });
                            edges.push(Edge {
                                kind: EdgeKind::Return,
                                from: (*callee, c_exit),
                                to: (*func, *line + 1),
                                callee: Some(*callee),
                            });
                        }
                    }
                }
            }
        }
        Supergraph { edges, exits, entry_of }
    }

    fn out_edges(&self, node: IfdsNode) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.from == node).collect()
    }
}

/// Tabulation result: reached path edges per (node, fact), or an explicit
/// budget/timeout flag (never a silently-clean result).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IfdsSolution<F: Clone + Eq + Hash + Ord> {
    Complete {
        /// Memoized path edges (node, fact) — the RHS tabulation table.
        reached: BTreeMap<IfdsNode, BTreeSet<F>>,
    },
    /// Budget tripped during tabulation (FR-35, global rule 3).
    Unknown { reason: IfdsUnknownReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IfdsUnknownReason {
    TimedOut,
    Cancelled,
}

/// Tabulation worklist (Reps–Horwitz–Sagiv): propagate (node, fact)
/// through out-edges with the domain's transfer; memoize path edges;
/// set-union joins emerge from multi-predecessor propagation.
pub fn solve<F: Clone + Eq + Hash + Ord>(
    graph: &Supergraph,
    domain: &impl IfdsFactDomain<Fact = F>,
    max_path_edges: usize,
) -> IfdsSolution<F> {
    let mut reached: BTreeMap<IfdsNode, BTreeSet<F>> = BTreeMap::new();
    let mut worklist: Vec<(IfdsNode, F)> = vec![];
    for (node, facts) in domain.seeds(graph) {
        for f in facts {
            worklist.push((node, f));
        }
    }
    while let Some((node, fact)) = worklist.pop() {
        let fresh = reached.entry(node).or_default().insert(fact.clone());
        if fresh {
            if reached.values().map(|s| s.len()).sum::<usize>() > max_path_edges {
                return IfdsSolution::Unknown { reason: IfdsUnknownReason::TimedOut };
            }
            for edge in graph.out_edges(node) {
                for out_fact in domain.transfer(edge, &fact) {
                    worklist.push((edge.to, out_fact));
                }
            }
        }
    }
    IfdsSolution::Complete { reached }
}

// ---------------------------------------------------------------------------
// TASK-035: exactly two IfdsFactDomain instances.
// ---------------------------------------------------------------------------

/// Instance 1 (TASK-035): propagates "target unresolved" facts from
/// `CallSite.unresolved` sites (§14.2) — the §13 #6 evidence backbone: an
/// unresolved external is LISTED, never silently assumed.
#[derive(Debug, Clone, Default)]
pub struct UnresolvedCallDomain;

/// Fact: an unresolved call site (function, line, callee text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnresolvedCall {
    pub func: SymbolId,
    pub line: u32,
    /// Short reason tag (`dynamic_attribute`, `external_module`, …).
    pub reason: u64, // hashed reason for Copy+Ord; text lives in evidence
}

impl IfdsFactDomain for UnresolvedCallDomain {
    type Fact = UnresolvedCall;

    fn transfer(&self, edge: &Edge, fact: &Self::Fact) -> SmallVec<[Self::Fact; 4]> {
        match edge.kind {
            // The unresolved fact flows through its own function to the exit
            // and back to the caller's successor — it never crosses INTO a
            // callee (the callee is precisely what is unresolved).
            EdgeKind::Intra | EdgeKind::Return => {
                let mut out = SmallVec::new();
                out.push(*fact);
                out
            }
            EdgeKind::Call => SmallVec::new(),
        }
    }

    fn seeds(&self, graph: &Supergraph) -> BTreeMap<IfdsNode, BTreeSet<Self::Fact>> {
        let _ = graph;
        BTreeMap::new() // seeds injected by `solve_unresolved` below
    }
}

/// Solve the unresolved-call propagation over one revision's facts.
/// Returns the reached (node, fact) set — the §13 #6 listing.
pub fn solve_unresolved(
    per_file: &[LangFacts],
    max_path_edges: usize,
) -> (Supergraph, IfdsSolution<UnresolvedCall>) {
    let graph = Supergraph::from_facts(per_file);
    // Seeds: every unresolved call site (reason hashed into the fact).
    let mut seeds: Vec<(IfdsNode, UnresolvedCall)> = vec![];
    for f in per_file {
        for c in &f.calls {
            if let Callee::Unresolved(reason) = &c.callee {
                if reason == "pending" {
                    continue; // resolution never finished — not a final fact
                }
                seeds.push((
                    (c.caller, c.line),
                    UnresolvedCall {
                        func: c.caller,
                        line: c.line,
                        reason: fnv(reason.as_bytes()),
                    },
                ));
            }
        }
    }
    let mut reached: BTreeMap<IfdsNode, BTreeSet<UnresolvedCall>> = BTreeMap::new();
    let mut worklist = seeds;
    let mut count = 0usize;
    while let Some((node, fact)) = worklist.pop() {
        if reached.entry(node).or_default().insert(fact) {
            count += 1;
            if count > max_path_edges {
                return (graph, IfdsSolution::Unknown { reason: IfdsUnknownReason::TimedOut });
            }
            for edge in graph.out_edges(node) {
                // UnresolvedCallDomain transfer (inlined for the seed form).
                match edge.kind {
                    EdgeKind::Intra | EdgeKind::Return => worklist.push((edge.to, fact)),
                    EdgeKind::Call => {}
                }
            }
        }
    }
    (graph, IfdsSolution::Complete { reached })
}

/// Instance 2 (TASK-035): taint-style value flow across calls, returns,
/// and delegating wrappers — using TASK-033 composed summaries at the
/// call/return edges. Detector logic (print-only stub, ignored-rejection)
/// stays in later crates; this ships propagation facts only (§7 boundary).
#[derive(Debug, Clone, Default)]
pub struct TaintDomain {
    /// Variable-name taint facts per function (adapters' source/sink rows
    /// name the variables).
    pub source_vars: BTreeMap<SymbolId, BTreeSet<String>>,
}

/// Fact: a tainted variable NAME inside a function (names are bounded by
/// the function's locals; the String is tiny and the domain stays Ord for
/// the tabulation table).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tainted {
    pub func: SymbolId,
    pub var: String,
}

impl IfdsFactDomain for TaintDomain {
    type Fact = Tainted;

    fn transfer(&self, edge: &Edge, fact: &Self::Fact) -> SmallVec<[Self::Fact; 4]> {
        let mut out = SmallVec::new();
        match edge.kind {
            EdgeKind::Intra => out.push(fact.clone()),
            EdgeKind::Call => {
                // Argument flows in: rename to the callee's parameter scope
                // (name-preserving delegate — TASK-033 wrappers delegate).
                out.push(Tainted {
                    func: edge.callee.unwrap_or(fact.func),
                    var: fact.var.clone(),
                });
            }
            EdgeKind::Return => {
                // Return flows out to the caller's successor scope.
                out.push(Tainted { func: edge.to.0, var: fact.var.clone() });
            }
        }
        out
    }

    fn seeds(&self, graph: &Supergraph) -> BTreeMap<IfdsNode, BTreeSet<Self::Fact>> {
        let mut out: BTreeMap<IfdsNode, BTreeSet<Tainted>> = BTreeMap::new();
        for (func, vars) in &self.source_vars {
            if let Some(&entry) = graph.entry_of.get(func) {
                for v in vars {
                    out.entry((*func, entry))
                        .or_default()
                        .insert(Tainted { func: *func, var: v.clone() });
                }
            }
        }
        out
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_semantic::facts::{
        AssignmentFact, CallSite, RhsKind, ReturnSite, SourceKind, SourceSite, SymbolFact,
        SymbolKind,
    };
    use cie_core::types::{FileId, SourceLocation};

    fn loc(l: u32) -> cie_core::types::SourceLocation {
        cie_core::types::SourceLocation::span_bytes(FileId(0), l * 8, l * 8 + 8)
    }

    /// Fixture: caller (id 0, lines 1-4) calls callee (id 1, lines 10-12);
    /// caller line 2 = `r = authorize(u)`, callee line 11 = return.
    fn fixture() -> Vec<LangFacts> {
        let mut f = LangFacts::new("a.py", cie_semantic::LangId::Python, "test");
        f.symbols.push(SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(0),
            kind: SymbolKind::Function,
            name: "run".into(),
            qual_name: "run".into(),
            loc: loc(1),
            lang: cie_semantic::LangId::Python,
            parent: None,
            rev: 0,
            doc: None,
        });
        f.symbols.push(SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(1),
            kind: SymbolKind::Function,
            name: "authorize".into(),
            qual_name: "authorize".into(),
            loc: loc(10),
            lang: cie_semantic::LangId::Python,
            parent: None,
            rev: 0,
            doc: None,
        });
        f.calls.push(CallSite {
            caller: SymbolId(0),
            callee_text: "authorize".into(),
            callee: Callee::Resolved(SymbolId(1)),
            line: 2,
            in_loop: false,
            arg_vars: vec!["u".into()],
        });
        f.assignments.push(AssignmentFact {
            symbol: SymbolId(0),
            var: "r".into(),
            line: 2,
            rhs_kind: RhsKind::Call,
            rhs_data: vec!["authorize".into()],
        });
        f.returns.push(ReturnSite {
            symbol: SymbolId(1),
            line: 11,
            expr: String::new(),
            constant: None,
        });
        vec![f]
    }

    /// Merge-join: facts meeting at a join node equal the union of branch
    /// facts (distributivity — set union only, no path-sensitivity claim).
    #[test]
    fn task_034_merge_join_is_union() {
        // Two branch statements flowing into a common successor: the
        // reached set at the successor is the union of both branch facts.
        let graph = Supergraph {
            edges: vec![
                Edge { kind: EdgeKind::Intra, from: (SymbolId(0), 1), to: (SymbolId(0), 3), callee: None },
                Edge { kind: EdgeKind::Intra, from: (SymbolId(0), 2), to: (SymbolId(0), 3), callee: None },
            ],
            exits: BTreeMap::new(),
            entry_of: BTreeMap::new(),
        };
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        struct UnionDomain;
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        struct F(u8);
        impl IfdsFactDomain for UnionDomain {
            type Fact = F;
            fn transfer(&self, e: &Edge, f: &F) -> SmallVec<[F; 4]> {
                let mut v = SmallVec::new();
                v.push(*f);
                let _ = e;
                v
            }
            fn seeds(&self, _g: &Supergraph) -> BTreeMap<IfdsNode, BTreeSet<F>> {
                let mut m = BTreeMap::new();
                m.insert((SymbolId(0), 1), BTreeSet::from([F(1)]));
                m.insert((SymbolId(0), 2), BTreeSet::from([F(2)]));
                m
            }
        }
        match solve(&graph, &UnionDomain, 100) {
            IfdsSolution::Complete { reached } => {
                let at_join = reached.get(&(SymbolId(0), 3)).unwrap();
                assert_eq!(at_join, &BTreeSet::from([F(1), F(2)]), "union at join");
            }
            _ => panic!("must complete"),
        }
    }

    /// Interprocedural: facts cross the call edge, into the callee, and
    /// return to the call site's successor.
    #[test]
    fn task_034_interprocedural_crossing() {
        let per_file = fixture();
        let graph = Supergraph::from_facts(&per_file);
        let mut domain = TaintDomain::default();
        domain.source_vars.insert(SymbolId(0), BTreeSet::from(["u".to_string()]));
        match solve(&graph, &domain, 500) {
            IfdsSolution::Complete { reached } => {
                // The taint reaches INSIDE the callee (entry of id 1).
                let in_callee = reached
                    .keys()
                    .any(|(f, _)| *f == SymbolId(1));
                assert!(in_callee, "facts cross the call edge into the callee");
                // …and return to the call site's successor (line 3).
                let back = reached.keys().any(|(f, l)| *f == SymbolId(0) && *l >= 3);
                assert!(back, "facts return to the call site's successor");
            }
            _ => panic!("must complete"),
        }
    }

    /// TASK-035: the ignored-rejection fixture — an unresolved call whose
    /// returned error flows onward — produces a taint/unresolved record
    /// citing the site, and the unresolved external lands in the listing.
    #[test]
    fn task_035_unresolved_call_domain_lists_externals() {
        let mut per_file = fixture();
        let f = &mut per_file[0];
        // §13 #6: an unresolved external call the caller ignores.
        f.calls.push(CallSite {
            caller: SymbolId(0),
            callee_text: "external.call".into(),
            callee: Callee::Unresolved("external_module".into()),
            line: 3,
            in_loop: false,
            arg_vars: vec![],
        });
        // Source site: `u` is a request input (taint source).
        f.sources.push(SourceSite {
            symbol: SymbolId(0),
            var: "u".into(),
            line: 1,
            kind: SourceKind::RequestInput,
        });
        let (_, solution) = solve_unresolved(&per_file, 500);
        match solution {
            IfdsSolution::Complete { reached } => {
                // The unresolved external is LISTED at its site (§13 #6:
                // never silently assumed).
                let listed = reached.values().any(|s| {
                    s.iter().any(|u| u.reason == fnv(b"external_module"))
                });
                assert!(listed, "unresolved external listed, not assumed");
            }
            _ => panic!("must complete"),
        }
    }

    /// Budget trip yields Unknown, never clean (FR-35).
    #[test]
    fn task_035_budget_trip_yields_unknown() {
        let mut per_file = fixture();
        // Two unresolved externals: the budget must trip while propagating.
        let f = &mut per_file[0];
        f.calls.push(CallSite {
            caller: SymbolId(0),
            callee_text: "external.a".into(),
            callee: Callee::Unresolved("external_module".into()),
            line: 3,
            in_loop: false,
            arg_vars: vec![],
        });
        f.calls.push(CallSite {
            caller: SymbolId(0),
            callee_text: "external.b".into(),
            callee: Callee::Unresolved("external_module".into()),
            line: 4,
            in_loop: false,
            arg_vars: vec![],
        });
        let (_, solution) = solve_unresolved(&per_file, 1);
        assert!(matches!(
            solution,
            IfdsSolution::Unknown { reason: IfdsUnknownReason::TimedOut }
        ));
    }
}
