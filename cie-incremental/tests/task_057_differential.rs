//! TASK-057 validation: differential closure tracks adds AND removals.
//!
//! Sketch deviations (documented, single-point):
//! - `RelationTrace::transitive_closure()` — same name/shape; the trace
//!   semantics are the in-house differential implementation (module docs:
//!   multiset updates, frontier, retraction, as-of).
//! - `SYM_A/B/C` → `u32` symbol ids (the relation layer's raw u32s).

use cie_incremental::{RelationState, RelationTrace};

const SYM_A: u32 = 1;
const SYM_B: u32 = 2;
const SYM_C: u32 = 3;
const SYM_D: u32 = 4;

/// The sketch's `task_057_differential_closure_tracks_adds_and_removals`.
#[test]
fn task_057_differential_closure_tracks_adds_and_removals() {
    // edges a->b, b->c; iterative closure must derive a->c
    let mut trace = RelationTrace::transitive_closure();
    trace.add(41, (SYM_A, SYM_B));
    trace.add(41, (SYM_B, SYM_C));
    trace.advance_to(41);
    assert!(trace.contains((SYM_A, SYM_C)), "closure derives a->c at rev 41");
    assert!(trace.contains((SYM_A, SYM_B)));
    assert!(trace.contains((SYM_B, SYM_C)));
    assert_eq!(trace.state, RelationState::Complete);

    // rev 42 removes b->c (its file was deleted) — the retraction
    // propagates through the dataflow: a->c retracts too
    trace.remove(42, (SYM_B, SYM_C));
    trace.advance_to(42);
    assert!(!trace.contains((SYM_B, SYM_C)), "retracted edge gone");
    assert!(
        !trace.contains((SYM_A, SYM_C)),
        "a->c retracts through the dataflow (deletion completeness)"
    );
    assert!(trace.contains((SYM_A, SYM_B)), "unrelated facts survive");
    assert!(trace.retraction_complete(), "no stale fact survived (FR-8)");

    // re-adding at rev 43 restores the closure (delete-then-readd)
    trace.add(43, (SYM_B, SYM_C));
    trace.advance_to(43);
    assert!(trace.contains((SYM_A, SYM_C)));
}

/// as-of reads are exact within the retained frontier (time travel).
#[test]
fn task_057_as_of_time_travel() {
    let mut trace = RelationTrace::transitive_closure();
    trace.add(41, (SYM_A, SYM_B));
    trace.add(41, (SYM_B, SYM_C));
    trace.advance_to(41);
    trace.remove(42, (SYM_B, SYM_C));
    trace.advance_to(42);

    // as-of 41: the closure still has a->c
    let past = trace.as_of(41);
    assert!(past.get(&SYM_A).map(|s| s.contains(&SYM_C)).unwrap_or(false));
    // as-of 42: retracted
    let now = trace.as_of(42);
    assert!(!now.get(&SYM_A).map(|s| s.contains(&SYM_C)).unwrap_or(false));

    // compaction below the frontier keeps as-of reads at the frontier exact
    trace.compact_below(42);
    trace.advance_to(42);
    assert!(trace.as_of(42).get(&SYM_A).map(|s| !s.contains(&SYM_C)).unwrap_or(true));
    assert!(trace.contains((SYM_A, SYM_B)));
        // compaction consolidated the pre-watermark log (net effect kept,
    // raw update count bounded): frontier state stays exact
    assert!(
        trace.len_updates() <= 3,
        "compaction consolidated the pre-watermark updates"
    );
}

/// TASK-056 integration: the dirty worklist output drives `feed()`, and
/// outputs equal clean recompute (the TASK-058 equivalence gate).
#[test]
fn task_057_feed_matches_clean_recompute() {
    let mut inc = RelationTrace::transitive_closure();
    // incremental path: feed dirty edges per revision
    inc.feed(41, &[(SYM_A, SYM_B), (SYM_B, SYM_C), (SYM_C, SYM_D)]);
    inc.advance_to(41);
    inc.feed(42, &[(SYM_B, SYM_D)]); // new edge from the dirty pass
    inc.advance_to(42);

    // clean recompute: same updates replayed from scratch
    let mut clean = RelationTrace::transitive_closure();
    clean.add(41, (SYM_A, SYM_B));
    clean.add(41, (SYM_B, SYM_C));
    clean.add(41, (SYM_C, SYM_D));
    clean.add(42, (SYM_B, SYM_D));
    clean.advance_to(42);

    for a in [SYM_A, SYM_B, SYM_C] {
        for b in [SYM_B, SYM_C, SYM_D] {
            assert_eq!(
                inc.contains((a, b)),
                clean.contains((a, b)),
                "incremental != clean for ({a},{b})"
            );
        }
    }
    assert!(inc.contains((SYM_A, SYM_D)), "4-hop closure derived");
}

/// Iterative operators run under iteration caps: exhaustion marks the
/// relation Unknown, never stale-complete (global rule 3).
#[test]
fn task_057_iteration_cap_is_honest() {
    // a long chain forces many closure iterations
    let mut trace = RelationTrace::transitive_closure();
    trace.max_iterations = 2; // deliberately too small
    for i in 0..12u32 {
        trace.add(41, (i, i + 1));
    }
    trace.advance_to(41);
    assert_eq!(trace.state, RelationState::Unknown, "cap exhausted → Unknown");
    // the closure is PARTIAL under exhaustion — the honest reading is
    // that answers are Unknown, not trusted
    let partial_reach = trace.contains((SYM_A, SYM_D));
    let _ = partial_reach; // value not relied upon while state == Unknown

    // with a sane cap the same graph completes
    let mut ok = RelationTrace::transitive_closure();
    for i in 0..12u32 {
        ok.add(41, (i, i + 1));
    }
    ok.advance_to(41);
    assert_eq!(ok.state, RelationState::Complete);
    assert!(ok.contains((0, 12)));
}

/// Diamond graph: multiple derivation paths survive a single-edge
/// retraction (alternative-path dataflow, not staleness).
#[test]
fn task_057_diamond_retraction_keeps_alternative_paths() {
    let mut trace = RelationTrace::transitive_closure();
    trace.add(41, (SYM_A, SYM_B));
    trace.add(41, (SYM_A, SYM_C));
    trace.add(41, (SYM_B, SYM_D));
    trace.add(41, (SYM_C, SYM_D));
    trace.advance_to(41);
    assert!(trace.contains((SYM_A, SYM_D)));

    // remove a->b: a->d survives via a->c->d — correct dataflow
    trace.remove(42, (SYM_A, SYM_B));
    trace.advance_to(42);
    assert!(trace.contains((SYM_A, SYM_D)));
    assert!(!trace.contains((SYM_A, SYM_B)));
    assert!(trace.retraction_complete());
}
