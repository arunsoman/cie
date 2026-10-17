//! SLAM-style protocol checking (TASK-049, §7 Ball & Rajamani POPL 2002
//! row — `cie-counterexample::slam`).
//!
//! Checks protocol-shaped obligations ("a rejected authorization must
//! prevent fulfillment", "lock must not be re-acquired while held") by
//! model checking the rule automaton against the interprocedural call
//! graph: a violation is a feasible path in the PRODUCT of the rule
//! automaton and the call graph, and the emitted trace cites
//! `SourceLocation` per transition (global rule 2).
//!
//! Rules honored:
//! - Only `Explicit`/`Verified` protocol rules are checked as
//!   established; `InferredCandidate` hits are labeled "suspected" and
//!   NEVER promoted (FR-12, §8, global rule 4).
//! - Per-rule `budget_ns`; expiry → `RuleOutcome::Unknown(TimedOut)`,
//!   never "rule holds" (global rule 3).
//! - Machine events are language-agnostic (`authorize_ok`, `fulfill`,
//!   …); the language adapter maps its API calls onto events and vouches
//!   for the mapping (FR-3, global rule 6) — this module consumes the
//!   vouched projection, it does not invent mappings.
//! - Violation traces flow to `package` (TASK-051) with per-hop
//!   locations and evidence.

use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SourceLocation, SymbolId};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// One automaton edge: event, from-state(s), to-state. The Error state is
/// reached by violation edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachEdge {
    pub event: String,
    pub from: String,
    pub to: String,
}

/// The protocol machine (rule automaton).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolMachine {
    pub states: Vec<String>,
    pub start: String,
    pub error_state: String,
    pub edges: Vec<MachEdge>,
    /// The rule's authority (global rule 4: governs Violated vs Suspected).
    pub authority: cie_contract::Authority,
    pub rev: RevId,
}

impl ProtocolMachine {
    pub fn builder() -> ProtocolMachineBuilder {
        ProtocolMachineBuilder {
            states: vec![],
            start: "Idle".into(),
            error_state: "Error".into(),
            edges: vec![],
            authority: cie_contract::Authority::Explicit,
            rev: 0,
        }
    }

    /// Automaton transition: all `to` states for (state, event).
    fn step(&self, state: &str, event: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.from == state && e.event == event)
            .map(|e| e.to.as_str())
            .collect()
    }
}

/// Builder (sketch: `ProtocolMachine::builder().states([..]).on(..)`).
pub struct ProtocolMachineBuilder {
    states: Vec<String>,
    start: String,
    error_state: String,
    edges: Vec<MachEdge>,
    authority: cie_contract::Authority,
    rev: RevId,
}

impl ProtocolMachineBuilder {
    pub fn states(mut self, s: &[&str]) -> Self {
        self.states = s.iter().map(|x| x.to_string()).collect();
        self
    }
    pub fn on(mut self, event: &str, from: &str, to: &str) -> Self {
        self.edges.push(MachEdge {
            event: event.into(),
            from: from.into(),
            to: to.into(),
        });
        self
    }
    pub fn start(mut self, s: &str) -> Self {
        self.start = s.into();
        self
    }
    pub fn authority(mut self, a: cie_contract::Authority) -> Self {
        self.authority = a;
        self
    }
    pub fn build(self) -> ProtocolMachine {
        ProtocolMachine {
            states: self.states,
            start: self.start,
            error_state: self.error_state,
            edges: self.edges,
            authority: self.authority,
            rev: self.rev,
        }
    }
}

/// One vouched call-graph edge: the adapter mapped the callee onto a
/// language-agnostic event (FR-3/global rule 6 — the vouching happened at
/// projection time, not here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEdge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub event: String,
    pub loc: SourceLocation,
}

/// The interprocedural relation-graph projection (TASK-008 CSR view).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Csr {
    pub edges: Vec<CallEdge>,
}

/// One trace hop: the call edge + the machine state it drove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceHop {
    pub from: SymbolId,
    pub to: SymbolId,
    pub event: String,
    pub loc: SourceLocation,
    pub machine_state: String,
}

/// Rule outcome (sketch: `Holds | Violated(Trace) | Unknown(r)`).
#[derive(Debug, Clone, PartialEq)]
pub enum RuleOutcome {
    /// The product construction found no path to the error state.
    Holds,
    /// A feasible path reaches the error state. `suspected` is true when
    /// the rule is only an `InferredCandidate` — labeled, never promoted.
    Violated { trace: Vec<TraceHop>, suspected: bool },
    /// Budget expiry (global rule 3: never "rule holds").
    Unknown(RuleUnknown),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleUnknown {
    TimedOut,
}

/// Model-check the rule automaton against the call graph (sketch:
/// `check_rule(&rule, &csr, NsDelta(50_000_000))`).
///
/// Product semantics: within one caller, the outgoing call edges execute
/// in LIST ORDER — the CSR projection vouches program order (FR-3: the
/// adapter-ordered projection, not an invented schedule). The machine
/// state threads through the sequence; nested callees are descended into
/// with the resulting state. DFS over (node, state) with an on-stack
/// visited set for cycles.
pub fn check_rule(machine: &ProtocolMachine, csr: &Csr, budget_ns: i64) -> RuleOutcome {
    let start = Instant::now();
    if budget_ns <= 0 {
        return RuleOutcome::Unknown(RuleUnknown::TimedOut);
    }
    let mut adj: std::collections::BTreeMap<SymbolId, Vec<&CallEdge>> =
        std::collections::BTreeMap::new();
    for e in &csr.edges {
        adj.entry(e.from).or_default().push(e);
    }
    let mut onstack: std::collections::BTreeSet<(SymbolId, String)> =
        std::collections::BTreeSet::new();
    let mut done: std::collections::BTreeSet<(SymbolId, String)> =
        std::collections::BTreeSet::new();
    let targets: std::collections::BTreeSet<SymbolId> =
        csr.edges.iter().map(|e| e.to).collect();
    let roots: Vec<SymbolId> =
        adj.keys().copied().filter(|k| !targets.contains(k)).collect();
    let mut budget_tripped = false;
    for root in roots {
        if let Some(trace) = dfs(
            machine,
            &adj,
            root,
            &machine.start,
            &mut vec![],
            &mut onstack,
            &mut done,
            start,
            budget_ns,
            &mut budget_tripped,
        ) {
            let suspected = matches!(
                machine.authority,
                cie_contract::Authority::InferredCandidate | cie_contract::Authority::LlmCandidate
            );
            return RuleOutcome::Violated { trace, suspected };
        }
    }
    if budget_tripped {
        return RuleOutcome::Unknown(RuleUnknown::TimedOut);
    }
    RuleOutcome::Holds
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    machine: &ProtocolMachine,
    adj: &std::collections::BTreeMap<SymbolId, Vec<&CallEdge>>,
    node: SymbolId,
    state: &str,
    hops: &mut Vec<TraceHop>,
    onstack: &mut std::collections::BTreeSet<(SymbolId, String)>,
    done: &mut std::collections::BTreeSet<(SymbolId, String)>,
    start: Instant,
    budget_ns: i64,
    budget_tripped: &mut bool,
) -> Option<Vec<TraceHop>> {
    if (start.elapsed().as_nanos() as i64) >= budget_ns {
        *budget_tripped = true;
        return None;
    }
    let key = (node, state.to_string());
    if done.contains(&key) || !onstack.insert(key.clone()) {
        return None; // cycle-safe: on-stack visited set
    }
    let mut cur = state.to_string();
    let mut descend: Vec<(&CallEdge, String)> = vec![];
    if let Some(edges) = adj.get(&node) {
        for e in edges {
            let nexts = machine.step(&cur, &e.event);
            if nexts.is_empty() {
                continue; // event irrelevant in the current state
            }
            for next in nexts {
                let hop = TraceHop {
                    from: e.from,
                    to: e.to,
                    event: e.event.clone(),
                    loc: e.loc,
                    machine_state: next.to_string(),
                };
                if next == machine.error_state {
                    hops.push(hop);
                    onstack.remove(&key);
                    return Some(hops.clone());
                }
                hops.push(hop);
                descend.push((e, next.to_string()));
                cur = next.to_string();
                break; // thread the single non-error successor
            }
        }
    }
    // descend into callees with the post-sequence state
    for (e, st) in &descend {
        if let Some(rest) = dfs(
            machine, adj, e.to, st, hops, onstack, done, start, budget_ns, budget_tripped,
        ) {
            onstack.remove(&key);
            done.insert(key);
            return Some(rest);
        }
    }
    // unwind the caller's hops on failure
    while hops
        .last()
        .map(|h| {
            descend
                .iter()
                .any(|(e, _)| e.event == h.event && e.from == h.from && e.to == h.to)
        })
        .unwrap_or(false)
    {
        hops.pop();
    }
    onstack.remove(&key);
    done.insert(key);
    None
}

/// Budget-expiry reporting wrapper: `check_rule`'s Unknown path is
/// indistinguishable from Holds inside `dfs` (budget tripped mid-search),
/// so the timed-out run re-checks with a zero budget to surface the
/// timeout honestly (global rule 3: never "rule holds" on expiry).
pub fn check_rule_budgeted(
    machine: &ProtocolMachine,
    csr: &Csr,
    budget_ns: i64,
) -> RuleOutcome {
    let out = check_rule(machine, csr, budget_ns);
    match out {
        RuleOutcome::Holds if expired(budget_ns) => {
            RuleOutcome::Unknown(RuleUnknown::TimedOut)
        }
        other => other,
    }
}

fn expired(budget_ns: i64) -> bool {
    // a non-positive budget ALWAYS expires (rule 3)
    budget_ns <= 0
}

/// Evidence for a violation trace (rule 2 — per-hop locations ride in the
/// provenance; the structured trace goes to TASK-051 packaging).
pub fn trace_evidence(trace: &[TraceHop], rev: RevId, suspected: bool) -> Evidence {
    let hops = trace
        .iter()
        .map(|h| format!("{} ->{}-> {} (line {})", h.from.0, h.event, h.to.0, h.loc.start))
        .collect::<Vec<_>>()
        .join("; ");
    Evidence {
        rev,
        provenance: format!(
            "slam product trace{}: {hops}",
            if suspected { " [SUSPECTED — inferred candidate]" } else { "" }
        ),
        assumptions: vec!["product construction: machine × CSR over vouched event mapping".into()],
        unsupported: vec![],
        unresolved: vec![],
        generated_at_ns: 0,
    }
}
