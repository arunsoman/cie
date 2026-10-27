//! Coalesced, persisted on-demand builds for `AvailabilityPolicy::BuildIfMissing`
//! artifacts the pinned snapshot does not already contain (RFC-0026 §9).
//!
//! Concurrent misses for the same `(revision, kind)` must not each redo the
//! expensive work (the thundering-herd / cache-stampede problem; cf.
//! Nishtala et al., "Scaling Memcache at Facebook", NSDI 2013, §3.2.1's
//! leases). The first miss starts exactly one background build; every other
//! caller waits on it instead of starting its own. Once it succeeds, the
//! result is persisted to the revision's artifact store so later callers —
//! in this process or a new one — never rebuild it at all.
//!
//! This module never depends on the crate that actually knows how to build
//! an artifact (e.g. cie-ingest's drift checker): that would invert the
//! workspace's dependency order (cie-ingest already depends on
//! cie-storage). Instead the caller injects a [`DeferredBuilder`]
//! implementation built in a crate that can see both sides.

use super::contract::ArtifactKind;
use crate::artifacts::{Artifact, RevisionArtifacts};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Supplies the actual computation for a deferred artifact kind. Runs on a
/// background thread decoupled from whichever request triggered it — the
/// triggering request may cancel or time out while this keeps running, so
/// every other caller waiting on the same build still gets a result.
pub trait DeferredBuilder: Send + Sync {
    fn build(
        &self,
        root: &Path,
        rev: u64,
        kind: ArtifactKind,
        budget: Duration,
    ) -> std::result::Result<Value, String>;
}

/// Background builds get a budget independent of any caller's own deadline
/// — liberalized relative to a live retrieval call's `timeout_ms`/MCP
/// budget, because the point of coalescing is that the build outlives the
/// request that happened to trigger it.
pub const BACKGROUND_BUILD_BUDGET: Duration = Duration::from_secs(30 * 60);

/// Which persisted artifact file a kind's on-demand result lands in. Only
/// kinds listed here are eligible for coalesced/persisted building; every
/// other kind keeps today's behavior (stored-only, errors if missing).
fn artifact_file_for(kind: ArtifactKind) -> Option<Artifact> {
    match kind {
        ArtifactKind::Drift => Some(Artifact::Drift),
        ArtifactKind::Overlap => Some(Artifact::Overlap),
        _ => None,
    }
}

enum Slot {
    InProgress,
    Done(Value),
    Failed(String),
}

struct BuildSlot {
    state: Mutex<Slot>,
    ready: Condvar,
}

#[derive(Default)]
pub(crate) struct BuildRegistry {
    slots: Mutex<HashMap<(u64, &'static str), Arc<BuildSlot>>>,
}

#[derive(Debug)]
pub(crate) enum BuildOutcome {
    Ready(Value),
    /// Still running; the caller's own budget ran out first. Not an error —
    /// the build keeps going in the background and a retry will likely find
    /// it finished.
    Pending,
    Failed(String),
    /// This kind has no registered persisted-artifact slot or no builder was
    /// injected for this host — callers fall back to today's plain error.
    Unsupported,
}

impl BuildRegistry {
    /// Read a prior on-demand build's persisted result, without touching the
    /// in-process registry. Covers the case where the build completed in a
    /// different process (or an earlier session in this one) after this
    /// reader's pinned snapshot was taken.
    pub(crate) fn read_persisted(root: &Path, rev: u64, kind: ArtifactKind) -> Option<Value> {
        let file = artifact_file_for(kind)?;
        RevisionArtifacts::open(root)
            .ok()?
            .get::<Value>(rev, file)
            .ok()?
    }

    /// Join an in-flight build for `(rev, kind)`, or start one. Waits up to
    /// `caller_budget` for a result; the build itself runs under
    /// [`BACKGROUND_BUILD_BUDGET`] regardless of how long this particular
    /// caller is willing to wait.
    pub(crate) fn get_or_build(
        registry: &Arc<BuildRegistry>,
        root: PathBuf,
        rev: u64,
        kind: ArtifactKind,
        builder: Arc<dyn DeferredBuilder>,
        caller_budget: Duration,
    ) -> BuildOutcome {
        let Some(file) = artifact_file_for(kind) else {
            return BuildOutcome::Unsupported;
        };
        let key = (rev, kind.key());
        let slot = {
            let mut slots = registry.slots.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = slots.get(&key) {
                existing.clone()
            } else {
                let fresh = Arc::new(BuildSlot {
                    state: Mutex::new(Slot::InProgress),
                    ready: Condvar::new(),
                });
                slots.insert(key, fresh.clone());
                let for_thread = fresh.clone();
                let registry_for_thread = registry.clone();
                std::thread::spawn(move || {
                    let outcome = builder
                        .build(&root, rev, kind, BACKGROUND_BUILD_BUDGET)
                        .and_then(|value| {
                            RevisionArtifacts::open(&root)
                                .map_err(|e| e.to_string())?
                                .put(rev, file, &value)
                                .map_err(|e| e.to_string())?;
                            Ok(value)
                        });
                    {
                        let mut state =
                            for_thread.state.lock().unwrap_or_else(|e| e.into_inner());
                        *state = match outcome {
                            Ok(value) => Slot::Done(value),
                            Err(e) => Slot::Failed(e),
                        };
                    }
                    for_thread.ready.notify_all();
                    // Evict once settled: a persisted success is now found
                    // via read_persisted() by anyone, and a failure must not
                    // wedge future callers out of retrying.
                    if let Ok(mut slots) =
                        registry_for_thread.slots.lock().map_err(|e| e.into_inner())
                    {
                        slots.remove(&key);
                    }
                });
                fresh
            }
        };
        let state = slot.state.lock().unwrap_or_else(|e| e.into_inner());
        let (state, _timed_out) = slot
            .ready
            .wait_timeout_while(state, caller_budget, |s| matches!(s, Slot::InProgress))
            .unwrap_or_else(|e| e.into_inner());
        match &*state {
            Slot::InProgress => BuildOutcome::Pending,
            Slot::Done(value) => BuildOutcome::Ready(value.clone()),
            Slot::Failed(e) => BuildOutcome::Failed(e.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingBuilder {
        calls: Arc<AtomicUsize>,
        work: Duration,
        fail: bool,
    }
    impl DeferredBuilder for CountingBuilder {
        fn build(
            &self,
            _root: &Path,
            _rev: u64,
            _kind: ArtifactKind,
            _budget: Duration,
        ) -> std::result::Result<Value, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.work);
            if self.fail {
                Err("synthetic failure".into())
            } else {
                Ok(serde_json::json!({"built": true}))
            }
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cie-deferred-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn concurrent_misses_coalesce_into_one_build_and_persist_once() {
        let root = test_dir("coalesce");
        let calls = Arc::new(AtomicUsize::new(0));
        let builder: Arc<dyn DeferredBuilder> = Arc::new(CountingBuilder {
            calls: calls.clone(),
            work: Duration::from_millis(150),
            fail: false,
        });
        let registry = Arc::new(BuildRegistry::default());
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let registry = registry.clone();
                let builder = builder.clone();
                let root = root.clone();
                std::thread::spawn(move || {
                    BuildRegistry::get_or_build(
                        &registry,
                        root,
                        1,
                        ArtifactKind::Drift,
                        builder,
                        Duration::from_secs(5),
                    )
                })
            })
            .collect();
        let mut ready = 0;
        for h in handles {
            match h.join().unwrap() {
                BuildOutcome::Ready(v) => {
                    assert_eq!(v, serde_json::json!({"built": true}));
                    ready += 1;
                }
                other => panic!("expected Ready, got a different outcome: {other:?}"),
            }
        }
        assert_eq!(ready, 5, "every waiter should get the one build's result");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "five concurrent misses must coalesce into exactly one build"
        );
        assert_eq!(
            BuildRegistry::read_persisted(&root, 1, ArtifactKind::Drift),
            Some(serde_json::json!({"built": true})),
            "a successful on-demand build must be persisted, not just cached in memory"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_caller_with_a_short_budget_sees_pending_while_the_build_keeps_running() {
        let root = test_dir("pending");
        let builder: Arc<dyn DeferredBuilder> = Arc::new(CountingBuilder {
            calls: Arc::new(AtomicUsize::new(0)),
            work: Duration::from_millis(300),
            fail: false,
        });
        let registry = Arc::new(BuildRegistry::default());
        let outcome = BuildRegistry::get_or_build(
            &registry,
            root.clone(),
            2,
            ArtifactKind::Drift,
            builder,
            Duration::from_millis(10),
        );
        assert!(
            matches!(outcome, BuildOutcome::Pending),
            "a caller whose budget is shorter than the build must not block past it"
        );
        // The build keeps going in the background; give it time to finish
        // and persist so a later caller gets the fast path.
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            BuildRegistry::read_persisted(&root, 2, ArtifactKind::Drift),
            Some(serde_json::json!({"built": true}))
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_failed_build_is_not_persisted_and_does_not_wedge_future_retries() {
        let root = test_dir("failure");
        let calls = Arc::new(AtomicUsize::new(0));
        let failing: Arc<dyn DeferredBuilder> = Arc::new(CountingBuilder {
            calls: calls.clone(),
            work: Duration::from_millis(20),
            fail: true,
        });
        let registry = Arc::new(BuildRegistry::default());
        let first = BuildRegistry::get_or_build(
            &registry,
            root.clone(),
            3,
            ArtifactKind::Drift,
            failing,
            Duration::from_secs(5),
        );
        assert!(matches!(first, BuildOutcome::Failed(_)));
        assert!(BuildRegistry::read_persisted(&root, 3, ArtifactKind::Drift).is_none());
        // A later retry (e.g. after a transient error) must get a fresh
        // attempt, not a permanently wedged failure.
        let succeeding: Arc<dyn DeferredBuilder> = Arc::new(CountingBuilder {
            calls: calls.clone(),
            work: Duration::from_millis(20),
            fail: false,
        });
        let second = BuildRegistry::get_or_build(
            &registry,
            root.clone(),
            3,
            ArtifactKind::Drift,
            succeeding,
            Duration::from_secs(5),
        );
        assert!(matches!(second, BuildOutcome::Ready(_)));
        std::fs::remove_dir_all(&root).ok();
    }
}
