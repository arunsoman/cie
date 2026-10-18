//! Live investigation steering (CIE-US US-066 engine close).
//!
//! "Set a goal, constrain scope and redirect work in progress." The
//! engine ships budgets + cancellation checkpoints (TASK-029); this
//! module adds the *steering session* on top:
//! * pause — cancel with a resumable checkpoint (the engine already
//!   resumes to the same fixpoint);
//! * stop — cancel with a STATED REASON (every stop has one);
//! * steer/exclude — remaining work items filtered by path prefix; the
//!   exclusion is recorded as a scope limitation that stays visible in
//!   the final board (the conclusion's shrunken scope is disclosed);
//! * the board — ongoing / completed / blocked, always separable.
//!
//! No engine signature changes: the session wraps the existing token +
//! checkpoint primitives.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::engine::{WorkItem};

/// One steering session.
#[derive(Default)]
pub struct SteerSession {
    paused: AtomicBool,
    stopped: AtomicBool,
    reason: OnceLock<String>,
    excluded: Mutex<Vec<String>>,
}

impl SteerSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pause: work stops at the next checkpoint and is resumable.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Stop with a stated reason (US-066: every stop has one).
    pub fn stop(&self, reason: &str) {
        self.reason.get_or_init(|| reason.to_string());
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn stop_reason(&self) -> Option<String> {
        self.reason.get().cloned()
    }

    /// Exclude an area (path prefix) from the REMAINING work; the
    /// exclusion becomes a recorded limitation.
    pub fn exclude(&self, prefix: &str) {
        self.excluded.lock().unwrap().push(prefix.to_string());
    }

    pub fn limitations(&self) -> Vec<String> {
        self.excluded
            .lock()
            .unwrap()
            .iter()
            .map(|p| format!("scope excluded: {p}* — conclusion limited accordingly"))
            .collect()
    }

    /// Steer: filter remaining items by the exclusions (the redirect).
    /// Done items are never touched; only pending work is reshaped.
    pub fn steer_remaining(&self, pending: &[WorkItem]) -> Vec<WorkItem> {
        let excl = self.excluded.lock().unwrap();
        pending
            .iter()
            .filter(|i| !excl.iter().any(|p| i.payload.starts_with(p.as_str())))
            .cloned()
            .collect()
    }

    /// The live board (US-066: ongoing, completed, blocked separated).
    pub fn board(&self, pending: &[WorkItem], done: &[WorkItem], budget_exhausted: bool) -> Board {
        let blocked: Vec<String> = if budget_exhausted {
            vec!["budget exhausted — further work blocked until re-armed".into()]
        } else if self.stopped() {
            vec![format!(
                "stopped: {}",
                self.stop_reason().unwrap_or_else(|| "no reason recorded".into())
            )]
        } else {
            Vec::new()
        };
        Board {
            ongoing: pending.iter().map(|i| i.payload.clone()).collect(),
            completed: done.iter().map(|i| i.payload.clone()).collect(),
            blocked,
            limitations: self.limitations(),
        }
    }
}

/// The visible board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub ongoing: Vec<String>,
    pub completed: Vec<String>,
    /// Blocked work with its stated reason.
    pub blocked: Vec<String>,
    /// Scope limitations (exclusions), permanently visible.
    pub limitations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::WorkItem;

    fn items(paths: &[&str]) -> Vec<WorkItem> {
        paths
            .iter()
            .map(|p| WorkItem {
                sort_key: p.to_string(),
                payload: p.to_string(),
            })
            .collect()
    }

    #[test]
    fn pause_is_resumable_and_stop_carries_reason() {
        let s = SteerSession::new();
        s.pause();
        assert!(s.paused());
        s.resume();
        assert!(!s.paused());
        assert!(!s.stopped());
        assert!(s.stop_reason().is_none());
        s.stop("user redirected to a different question");
        assert!(s.stopped());
        assert_eq!(
            s.stop_reason().as_deref(),
            Some("user redirected to a different question")
        );
    }

    #[test]
    fn exclusion_filters_remaining_and_stays_visible() {
        let s = SteerSession::new();
        let all = items(&["gen/schema.py", "src/pay.py", "src/refund.py"]);
        s.exclude("gen/");
        let remaining = s.steer_remaining(&all);
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|i| !i.payload.starts_with("gen/")));
        let b = s.board(&remaining, &items(&["src/pay.py"]), false);
        assert!(b.limitations.iter().any(|l| l.contains("gen/*")));
        assert_eq!(b.completed, vec!["src/pay.py".to_string()]);
        assert!(b.blocked.is_empty());
    }

    #[test]
    fn board_separates_ongoing_completed_blocked() {
        let s = SteerSession::new();
        let b = s.board(
            &items(&["src/a.py", "src/b.py"]),
            &items(&["src/c.py"]),
            true,
        );
        assert_eq!(b.ongoing, vec!["src/a.py".to_string(), "src/b.py".to_string()]);
        assert_eq!(b.completed, vec!["src/c.py".to_string()]);
        assert!(b.blocked[0].contains("budget exhausted"));
        // stopped board names the reason
        s.stop("out of scope");
        let b = s.board(&[], &[], false);
        assert!(b.blocked[0].contains("out of scope"));
    }
}
