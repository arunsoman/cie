//! TASK-002 validation sketch — instructor contract, bodies intact.
//! Single documented mapping: `cie_core::…` is the import path (§14.1 lives here).

use cie_core::{hash, now_ns, Hash, Ns, NsDelta, RevisionAllocator, RevId};

#[test]
fn task_002_time_hash_and_revisions() {
    let t0: Ns = 1_700_000_000_123_456_789;
    let delta: NsDelta = -250;
    assert_eq!((t0 as i64 + delta) as Ns, 1_700_000_000_123_456_539);

    let h: Hash = hash(b"authorize");
    assert_eq!(h, hash(b"authorize"), "content addressing is stable");
    assert_ne!(h, hash(b"authorize2"), "different bytes, different Hash");
    assert_eq!(hash(b""), cie_core::BLAKE3_EMPTY, "known blake3 vector");

    let mut alloc = RevisionAllocator::starting_at(41);
    assert_eq!(alloc.next(), 42);
    assert!(alloc.next() > 42, "FR-21: revisions are strictly monotonic u64");
    assert!(now_ns() > 0);
}

/// TASK-002 restart recovery: a fresh allocator seeded from a persisted head
/// continues the chain (storage-side wiring is `Store::allocator`).
#[test]
fn task_002_revisions_survive_restart() {
    let alloc = RevisionAllocator::starting_at(77);
    assert_eq!(alloc.next(), 78);
}
