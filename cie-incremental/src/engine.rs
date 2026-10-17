//! Analysis execution engine with budgets, cancellation and checkpoints
//! (TASK-029, FR-7): bounded, resumable, deterministic.
//!
//! - Two-dimensional budgets: wall time (`NsDelta`, ns) plus a node
//!   budget; tripping either yields `AnalysisStatus::TimedOut` with
//!   partial coverage — never a clean pass (FR-35, §13 #26, global rule 3).
//! - Cancellation tokens are honored BETWEEN work items (never mid-item),
//!   leaving a valid checkpoint behind.
//! - Checkpoints serialize worklist + visited state as JSON and resume
//!   byte-compatibly across process restarts without recomputing
//!   completed nodes.
//! - Scheduling is deterministic given identical input (sorted worklists)
//!   so a resumed run and an uninterrupted run agree byte-for-byte.

use cie_core::protocol::{AnalysisInput, AnalysisOutput, CieError};
use cie_core::types::{Ns, NsDelta};
use serde::{Deserialize, Serialize};

/// Termination status of one engine run (§8 status rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisStatus {
    /// Fixpoint reached within budget — the only clean verdict.
    Complete,
    /// Wall-time or node budget tripped — coverage is partial; results are
    /// NEVER marked clean on timeout (global rule 3, §13 #26).
    TimedOut,
    /// Token cancelled between work items — resumable from checkpoint.
    Cancelled,
}

/// Two-dimensional budget (FR-7): wall time AND work items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineBudget {
    /// Wall-time budget in ns (`NsDelta = i64`); `i64::MAX` = unbounded.
    pub budget_ns: NsDelta,
    /// Maximum work items processed this run.
    pub node_budget: u64,
    /// Run start (ns since epoch) — pure `Ns` arithmetic, no chrono (FR-21).
    pub started_ns: Ns,
}

impl EngineBudget {
    pub fn new(budget_ns: NsDelta, node_budget: u64, started_ns: Ns) -> Self {
        EngineBudget { budget_ns, node_budget, started_ns }
    }

    fn wall_exceeded(&self, now: Ns) -> bool {
        self.budget_ns != i64::MAX && (now as i128 - self.started_ns as i128) >= self.budget_ns as i128
    }
}

/// A deterministic work item: `(sort_key, payload)` — sorted worklists make
/// scheduling reproducible (same input ⇒ same order ⇒ resumed runs agree).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub sort_key: String,
    pub payload: String,
}

/// Serialized worklist + visited state (resumes byte-compatibly).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub pending: Vec<WorkItem>,
    pub visited: Vec<String>,
    pub conclusions: Vec<String>,
}

impl Checkpoint {
    /// JSON bytes (persisted into `cie-store` by callers — TASK-024).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(bytes: &str) -> Result<Checkpoint, CieError> {
        serde_json::from_str(bytes)
            .map_err(|e| CieError::Storage(format!("checkpoint parse: {e}")))
    }
}

/// Cancellation token: honored between work items (never mid-item).
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        CancellationToken(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What one work item contributes (deterministic derive step).
pub trait WorkFn: Fn(&WorkItem) -> Option<String> {}
impl<T: Fn(&WorkItem) -> Option<String>> WorkFn for T {}

/// FR-7 engine: bounded, cancellable, resumable, deterministic.
pub struct Engine;

/// One run's outcome: status + output + resumable checkpoint.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub status: AnalysisStatus,
    pub output: AnalysisOutput,
    pub checkpoint: Checkpoint,
    pub processed: u64,
}

impl Engine {
    /// Run to fixpoint within `budget`; honors `token` between items.
    pub fn run(
        input: &AnalysisInput,
        items: Vec<WorkItem>,
        budget: EngineBudget,
        token: &CancellationToken,
        now: Ns,
        step: impl WorkFn,
    ) -> RunOutcome {
        let mut cp = Checkpoint {
            pending: items,
            visited: vec![],
            conclusions: vec![],
        };
        Self::resume(&Checkpoint { pending: cp.pending.clone(), ..Default::default() }, input, budget, token, now, step, &mut cp)
    }

    /// Resume from `checkpoint` without recomputing `visited` nodes; a
    /// resumed run reaches the SAME fixpoint as an uninterrupted run
    /// (deterministic sorted worklists).
    pub fn resume(
        checkpoint: &Checkpoint,
        _input: &AnalysisInput,
        budget: EngineBudget,
        token: &CancellationToken,
        now: Ns,
        step: impl WorkFn,
        out_cp: &mut Checkpoint,
    ) -> RunOutcome {
        let mut pending = checkpoint.pending.clone();
        let mut visited = checkpoint.visited.clone();
        let mut conclusions = checkpoint.conclusions.clone();
        let mut processed: u64 = 0;

        let status = loop {
            if token.is_cancelled() {
                break AnalysisStatus::Cancelled;
            }
            if budget.wall_exceeded(cie_core::types::now_ns().max(now)) && budget.wall_exceeded(cie_core::types::now_ns()) {
                break AnalysisStatus::TimedOut;
            }
            if processed >= budget.node_budget {
                break AnalysisStatus::TimedOut;
            }
            if pending.is_empty() {
                break AnalysisStatus::Complete;
            }
            // Deterministic order: sort by (sort_key, payload).
            pending.sort_by(|a, b| (&a.sort_key, &a.payload).cmp(&(&b.sort_key, &b.payload)));
            let item = pending.remove(0);
            if visited.contains(&item.sort_key) {
                continue; // completed nodes never recompute
            }
            if let Some(conclusion) = step(&item) {
                conclusions.push(conclusion);
            }
            visited.push(item.sort_key);
            processed += 1;
        };

        pending.sort_by(|a, b| (&a.sort_key, &a.payload).cmp(&(&b.sort_key, &b.payload)));
        *out_cp = Checkpoint {
            pending: pending.clone(),
            visited: visited.clone(),
            conclusions: conclusions.clone(),
        };

        // §9 output: evidence always present (FR-4); partial coverage on
        // timeout is visible, never marked clean (global rule 3).
        let mut output = AnalysisOutput::empty_with_evidence(_input.revision, now);
        output.conclusions = conclusions.clone();
        let complete = status == AnalysisStatus::Complete;
        output.coverage.complete = complete;
        output
            .evidence
            .assumptions
            .push(format!("engine status: {:?}", status));
        if status == AnalysisStatus::TimedOut {
            output
                .evidence
                .unsupported
                .push("budget tripped — coverage partial, results NOT clean".into());
        }

        RunOutcome {
            status,
            output,
            checkpoint: out_cp.clone(),
            processed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: u64) -> Vec<WorkItem> {
        (0..n)
            .map(|i| WorkItem {
                sort_key: format!("node{i:03}"),
                payload: format!("p{i}"),
            })
            .collect()
    }

    /// TASK-029 DoD: budget trip → TimedOut + partial coverage, never clean.
    #[test]
    fn task_029_budget_trip_yields_timed_out_with_partial_coverage() {
        let input = AnalysisInput::for_revision(1);
        let budget = EngineBudget::new(i64::MAX, 5, 0);
        let out = Engine::run(&input, items(50), budget, &CancellationToken::new(), 0, |_| None);
        assert_eq!(out.status, AnalysisStatus::TimedOut);
        assert_eq!(out.processed, 5);
        assert!(!out.output.coverage.complete, "results never clean on timeout");
        assert!(out
            .output
            .evidence
            .unsupported
            .iter()
            .any(|u| u.contains("NOT clean")));
    }

    /// TASK-029 DoD: resume reaches the same fixpoint as an uninterrupted run.
    #[test]
    fn task_029_resume_reaches_same_fixpoint() {
        let input = AnalysisInput::for_revision(1);
        let step = |it: &WorkItem| Some(format!("done:{}", it.sort_key));
        // Uninterrupted:
        let full = Engine::run(
            &input,
            items(20),
            EngineBudget::new(i64::MAX, 1000, 0),
            &CancellationToken::new(),
            0,
            step,
        );
        assert_eq!(full.status, AnalysisStatus::Complete);
        // Interrupted after 7, then resumed:
        let first = Engine::run(
            &input,
            items(20),
            EngineBudget::new(i64::MAX, 7, 0),
            &CancellationToken::new(),
            0,
            step,
        );
        assert_eq!(first.status, AnalysisStatus::TimedOut);
        // Serialize/deserialize (process restart).
        let bytes = first.checkpoint.to_json();
        let restored = Checkpoint::from_json(&bytes).unwrap();
        let mut out_cp = Checkpoint::default();
        let resumed = Engine::resume(
            &restored,
            &input,
            EngineBudget::new(i64::MAX, 1000, 0),
            &CancellationToken::new(),
            0,
            step,
            &mut out_cp,
        );
        assert_eq!(resumed.status, AnalysisStatus::Complete);
        assert_eq!(
            resumed.output.conclusions, full.output.conclusions,
            "resumed fixpoint == uninterrupted fixpoint"
        );
    }

    /// TASK-029 DoD: cancellation honored between items, checkpoint valid.
    #[test]
    fn task_029_cancellation_leaves_valid_checkpoint() {
        let input = AnalysisInput::for_revision(1);
        let token = CancellationToken::new();
        let budget = EngineBudget::new(i64::MAX, 1000, 0);
        let step = |it: &WorkItem| {
            if it.sort_key == "node003" {
                Some("cancelled-during-003".into())
            } else {
                None
            }
        };
        // Cancel AFTER the engine started: run in a thread would race; here
        // the step cancels mid-run so the NEXT loop iteration observes it
        // (between items, never mid-item).
        let token2 = token.clone();
        let cancel_during = move |it: &WorkItem| {
            let r = step(it);
            if it.sort_key == "node002" {
                token2.cancel();
            }
            r
        };
        let out = Engine::run(&input, items(10), budget, &token, 0, cancel_during);
        assert_eq!(out.status, AnalysisStatus::Cancelled);
        // Checkpoint resumable: pending + visited serialize cleanly.
        let restored = Checkpoint::from_json(&out.checkpoint.to_json()).unwrap();
        assert_eq!(restored, out.checkpoint);
        assert!(!restored.pending.is_empty(), "remaining work preserved");
    }

    /// TASK-029 DoD: same input + budget ⇒ same worklist order.
    #[test]
    fn task_029_deterministic_scheduling() {
        let input = AnalysisInput::for_revision(1);
        let shuffled = vec![
            WorkItem { sort_key: "c".into(), payload: "3".into() },
            WorkItem { sort_key: "a".into(), payload: "1".into() },
            WorkItem { sort_key: "b".into(), payload: "2".into() },
        ];
        let run1 = Engine::run(&input, shuffled.clone(), EngineBudget::new(i64::MAX, 100, 0), &CancellationToken::new(), 0, |_| None);
        let run2 = Engine::run(&input, shuffled, EngineBudget::new(i64::MAX, 100, 0), &CancellationToken::new(), 0, |_| None);
        assert_eq!(run1.checkpoint.visited, run2.checkpoint.visited);
        assert_eq!(run1.checkpoint.visited, vec!["a", "b", "c"], "sorted order");
    }
}
