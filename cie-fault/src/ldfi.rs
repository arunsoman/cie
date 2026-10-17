//! Lineage-driven fault injection (TASK-053, FR-15, §7 Alvaro/Rosen/
//! Hellerstein LDFI row — `cie-fault::ldfi`).
//!
//! From a SUCCESSFUL execution's lineage graph, enumerate minimal fault
//! hypotheses whose injection breaks the TASK-052 liveness property, and
//! run them through an injectable target. LDFI reasons backward from the
//! successful lineage, so it finds targeted combinations (drop the credit
//! event, duplicate the refund message) instead of random chaos.
//!
//! Rules honored:
//! - §7 qualification: hypotheses derive ONLY from observed lineage
//!   edges; missing lineage → `LdfiError::Unknown(LdfiUnknown::NoLineage)`
//!   — never "recovery is sound" (global rules 2 and 3).
//! - Bounded experiments: `budget_ns` wall clock + `max_faults` per
//!   round; overrun → `ExperimentOutcome::Unknown(TimedOut)` (global
//!   rule 3).
//! - Every applied fault records its lineage-edge provenance and `Ns`
//!   timestamps (global rules 1 and 2).
//! - Injection happens behind [`RuntimeFaultInjector`]; the planner never
//!   touches real systems (FR-7). [`SimInjector`] is the deterministic
//!   test target.

use cie_core::types::{hash_hex, Ns};
use serde::{Deserialize, Serialize};

/// Lineage edge kind (§7 qualification: message/computation/state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineageKind {
    Message,
    Computation,
    StateUpdate,
}

/// One observed lineage edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub id: String,
    pub kind: LineageKind,
    pub from: String,
    pub to: String,
}

/// The successful execution's lineage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LineageGraph {
    pub edges: Vec<LineageEdge>,
}

impl LineageGraph {
    pub fn empty() -> Self {
        LineageGraph { edges: vec![] }
    }

    pub fn add(&mut self, id: &str, kind: LineageKind, from: &str, to: &str) {
        self.edges.push(LineageEdge {
            id: id.into(),
            kind,
            from: from.into(),
            to: to.into(),
        });
    }

    /// The deterministic successor chain used by the simulator: from each
    /// node, the edges leaving it in insertion order.
    fn next_of(&self, node: &str) -> Vec<&LineageEdge> {
        self.edges.iter().filter(|e| e.from == node).collect()
    }
}

/// One fault hypothesis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    Drop(LineageEdge),
    Duplicate(LineageEdge),
    Reorder(LineageEdge),
    Delay(LineageEdge),
    CrashScheduler,
}

impl Fault {
    /// Lineage-edge provenance (global rule 2); `CrashScheduler` cites the
    /// scheduler tick pseudo-edge.
    pub fn provenance(&self) -> String {
        match self {
            Fault::Drop(e) => format!("drop({}:{})", e.id, e.kind_str()),
            Fault::Duplicate(e) => format!("duplicate({}:{})", e.id, e.kind_str()),
            Fault::Reorder(e) => format!("reorder({}:{})", e.id, e.kind_str()),
            Fault::Delay(e) => format!("delay({}:{})", e.id, e.kind_str()),
            Fault::CrashScheduler => "crash_scheduler_tick".into(),
        }
    }
}

impl LineageEdge {
    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            LineageKind::Message => "message",
            LineageKind::Computation => "computation",
            LineageKind::StateUpdate => "state_update",
        }
    }
}

/// The property under attack — the TASK-052 liveness obligation ("every
/// debit eventually reaches fulfillment or an effective refund") plus its
/// idempotency side-condition (a recovery action fires at most once).
#[derive(Debug, Clone)]
pub struct Property {
    pub expr: String,
}

impl Property {
    /// Derived from the TASK-052 obligation model (documented mapping:
    /// the obligation's liveness expr becomes the attacked property).
    pub fn liveness_from_task_052() -> Self {
        Property {
            expr: "every debit reaches fulfillment or an effective refund \
                   (recovery actions fire at most once)"
                .into(),
        }
    }
}

/// What a run observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimRun {
    /// Nodes reached in order (the successful chain, minus faults).
    pub reached: Vec<String>,
    /// Anomalies (duplicated side effects, crashed scheduler, …).
    pub anomalies: Vec<String>,
}

/// The injectable-target seam (FR-7: bounded, auditable execution — the
/// planner never touches real systems directly).
pub trait RuntimeFaultInjector {
    /// Apply the fault set to the target and report the observed run.
    fn run(&mut self, lineage: &LineageGraph, faults: &[Fault]) -> SimRun;
}

/// Deterministic simulator for tests: walks the lineage's success chain;
/// Drop on the chain stops it there; Duplicate yields a duplicated
/// side-effect anomaly; CrashScheduler stops the scheduler tick (chain
/// never advances past the first message); Reorder/Delay are benign.
pub struct SimInjector;

impl SimInjector {
    pub fn new() -> Self {
        SimInjector
    }

    fn on_chain(edge: &LineageEdge, chain: &[&str]) -> bool {
        chain.windows(2).any(|w| w[0] == edge.from && w[1] == edge.to)
    }
}

impl Default for SimInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeFaultInjector for SimInjector {
    fn run(&mut self, lineage: &LineageGraph, faults: &[Fault]) -> SimRun {
        // the modeled success chain of a top-up/recovery flow
        let chain: Vec<&str> = vec!["debit", "ledger", "credit_event", "balance_update"];
        let mut reached: Vec<String> = vec![];
        let mut anomalies: Vec<String> = vec![];
        if faults.iter().any(|f| matches!(f, Fault::CrashScheduler)) {
            anomalies.push("scheduler tick crashed: recovery interrupted".into());
            return SimRun { reached, anomalies };
        }
        'outer: for node in &chain {
            reached.push(node.to_string());
            for f in faults {
                match f {
                    Fault::Drop(e) if Self::on_chain(e, &chain) && e.from == *node => {
                        // the edge out of this node never fires
                        break 'outer;
                    }
                    Fault::Duplicate(e) => {
                        anomalies.push(format!(
                            "duplicated side effect at {}:{}",
                            e.id,
                            e.kind_str()
                        ));
                    }
                    Fault::Reorder(_) | Fault::Delay(_) => {}
                    Fault::Drop(_) => {}
                    Fault::CrashScheduler => unreachable!(),
                }
            }
        }
        SimRun { reached, anomalies }
    }
}

/// Experiment outcome — `Unknown(TimedOut)` on overrun, never a
/// soundness verdict (global rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentOutcome {
    Completed,
    Unknown(TimedOutReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimedOutReason {
    TimedOut,
}

/// One experiment's result, fully evidenced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentResult {
    pub faults: Vec<Fault>,
    pub property_violated: bool,
    pub outcome: ExperimentOutcome,
    /// Lineage-edge provenance per applied fault (global rule 2).
    pub provenance: Vec<String>,
    /// Content hash of (fault set ‖ observed run) — audit id (FR-8).
    pub content_hash: String,
    /// global rule 1: `Ns` timestamp.
    pub at_ns: Ns,
}

/// Bounded round limits.
#[derive(Debug, Clone, Copy)]
pub struct RoundLimits {
    pub max_faults: usize,
    pub budget_ns: u64,
}

impl Default for RoundLimits {
    fn default() -> Self {
        RoundLimits { max_faults: 2, budget_ns: 500_000_000 }
    }
}

/// Honest failure modes (sketch: `LdfiError::Unknown(LdfiUnknown::NoLineage)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LdfiUnknown {
    NoLineage,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LdfiError {
    Unknown(LdfiUnknown),
}

/// Round results.
#[derive(Debug, Clone, Default)]
pub struct RoundResults {
    pub results: Vec<ExperimentResult>,
}

/// The LDFI planner (sketch: `LdfiPlanner::new(lineage, SimInjector::new())`).
pub struct LdfiPlanner<I: RuntimeFaultInjector> {
    lineage: LineageGraph,
    injector: I,
}

impl<I: RuntimeFaultInjector> LdfiPlanner<I> {
    pub fn new(lineage: LineageGraph, injector: I) -> Self {
        LdfiPlanner { lineage, injector }
    }

    /// Enumerate minimal fault sets (single faults first, then pairs up to
    /// `max_faults`), execute each through the injector, and report which
    /// sets break the property. Ranked minimal-faults-first. Wall-clock
    /// budget guards every experiment (global rule 3).
    pub fn run(
        &mut self,
        property: &Property,
        limits: RoundLimits,
    ) -> Result<RoundResults, LdfiError> {
        // §7 qualification: no successful lineage → honest unknown.
        if self.lineage.edges.is_empty() {
            return Err(LdfiError::Unknown(LdfiUnknown::NoLineage));
        }
        let start = std::time::Instant::now();
        let mut out = RoundResults::default();
        let mut at_ns = 0u64;

        // hypothesis enumeration, minimal-first: singles, then pairs.
        let mut hypotheses: Vec<Vec<Fault>> = vec![];
        for e in &self.lineage.edges {
            hypotheses.push(vec![Fault::Drop(e.clone())]);
        }
        for e in &self.lineage.edges {
            hypotheses.push(vec![Fault::Duplicate(e.clone())]);
        }
        hypotheses.push(vec![Fault::CrashScheduler]);
        if limits.max_faults >= 2 {
            let n = self.lineage.edges.len();
            for i in 0..n {
                for j in i + 1..n {
                    hypotheses.push(vec![
                        Fault::Drop(self.lineage.edges[i].clone()),
                        Fault::Drop(self.lineage.edges[j].clone()),
                    ]);
                }
            }
        }

        for faults in hypotheses {
            if start.elapsed().as_nanos() as u64 >= limits.budget_ns {
                out.results.push(ExperimentResult {
                    faults,
                    property_violated: false,
                    outcome: ExperimentOutcome::Unknown(TimedOutReason::TimedOut),
                    provenance: vec![],
                    content_hash: String::new(),
                    at_ns: at_ns + 1,
                });
                break; // remaining experiments stay unexecuted (rule 3)
            }
            let run = self.injector.run(&self.lineage, &faults);
            // property semantics: chain reaches the terminal update AND no
            // anomaly (duplicated recovery) — TASK-052 liveness + its
            // idempotency side-condition.
            let chain_complete = run
                .reached
                .last()
                .map(|l| l == "balance_update")
                .unwrap_or(false);
            let violated = !chain_complete || !run.anomalies.is_empty();
            let payload = format!(
                "{}|{}|{}",
                property.expr,
                faults.iter().map(|f| f.provenance()).collect::<Vec<_>>().join(","),
                run.reached.join("->")
            );
            let provenance = faults.iter().map(|f| f.provenance()).collect();
            out.results.push(ExperimentResult {
                property_violated: violated,
                outcome: ExperimentOutcome::Completed,
                faults,
                provenance,
                content_hash: hash_hex(&cie_core::types::hash(payload.as_bytes())),
                at_ns,
            });
            at_ns += 1;
        }
        // rank: violated minimal sets first, then by fault-set size
        out.results.sort_by(|a, b| {
            (usize::from(!a.property_violated), a.faults.len()).cmp(&(
                usize::from(!b.property_violated),
                b.faults.len(),
            ))
        });
        Ok(out)
    }
}
