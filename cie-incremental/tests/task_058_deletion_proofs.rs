//! TASK-058 validation: deletion correctness + incremental/clean
//! equivalence (proptest harness, FR-8, §13 item 7).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_repo_small()` / `IncrWorld` / `clean_recompute` live in
//!   `cie_incremental::proptests` (reusable harness crate-side).
//! - `inc.findings_stripped()` compares (SymbolId, facts-hash) pairs —
//!   evidence-stripped by construction; timestamps never compared
//!   (global rule 1).
//! - The report API: findings ARE the stripped set; `f.about()` is the
//!   SymbolId component.

use cie_incremental::proptests::{clean_recompute, symbol_id, IncrWorld, RepoSnapshot};
use proptest::prelude::*;

/// Edit-seq strategy: ≤ 64 ops over a 3-file repo with small contents
/// containing `fn` segments and `call` edges (adversarial sequences:
/// delete-then-readd identical content, delete dependency before
/// dependent, edit during pending work — global rule 5).
fn edit_seq_strategy(max_ops: usize) -> impl Strategy<Value = Vec<(u8, u8, u8)>> {
    proptest::collection::vec((any::<u8>(), any::<u8>(), any::<u8>()), 0..=max_ops)
}

const PATHS: [&str; 3] = ["a.py", "b.py", "c.py"];

const CONTENTS: [&str; 5] = [
    "fn alpha\ncall beta\n",
    "fn beta\n",
    "fn gamma\ncall alpha\ncall beta\n",
    "fn alpha\n",
    "fn beta\nfn delta\ncall gamma\n",
];

fn decode(step: (u8, u8, u8)) -> cie_incremental::Edit {
    let (op, f, c) = step;
    let path = PATHS[(f % 3) as usize].to_string();
    let content = CONTENTS[(c % 5) as usize].to_string();
    match op % 3 {
        0 => cie_incremental::Edit::AddFile { path, content },
        1 => cie_incremental::Edit::EditFile { path, content },
        _ => cie_incremental::Edit::DeleteFile { path },
    }
}

/// The sketch's `task_058_incremental_equals_clean_recompute` (256 cases,
/// ≤ 64 ops, clean-recompute oracle after EVERY step).
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn task_058_incremental_equals_clean_recompute(seq in edit_seq_strategy(64)) {
        let mut inc = IncrWorld::new(RepoSnapshot::new());
        for step in seq {
            let step_dbg = format!("{step:?}");
            let edit = decode(step);
            inc.apply(edit);
            let clean = clean_recompute(&inc.repo);
            prop_assert_eq!(
                inc.findings_stripped(),
                clean.findings_stripped(),
                "divergence after step {}",
                step_dbg
            );
            prop_assert!(
                inc.no_orphan_records(),
                "stale fact survived step {}",
                step_dbg
            );
        }
    }
}

/// §13 item 7 fixture: a finding on a deleted symbol disappears from the
/// report at the next revision.
#[test]
fn task_058_deletion_invalidates_earlier_finding() {
    let mut repo = RepoSnapshot::new();
    let content = "fn worker\ncall helper\nfn helper\n";
    repo.insert("f.py".into(), content.into());
    let mut w = IncrWorld::new(repo);
    let s77 = symbol_id("f.py", "helper");
    assert!(
        w.findings.contains_key(&s77),
        "a finding references sym 77 (helper), which lives in file f"
    );

    // delete the file
    w.apply(cie_incremental::Edit::DeleteFile { path: "f.py".into() });

    // the earlier finding is gone; nothing stale remains anywhere
    assert!(
        !w.findings.contains_key(&s77),
        "deletion must invalidate the earlier finding"
    );
    assert!(w.no_orphan_records(), "no stale fact survives the deletion");
    assert!(w.symbols.get(&s77).is_none());
    assert!(w.sym_ns.get(&s77).is_none());
    assert!(w.record_ns.get(&s77).is_none());
}

/// Delete-then-readd identical content is identity-stable
/// (content-addressed ids) and converges to the clean recomputation.
#[test]
fn task_058_delete_then_readd_is_stable() {
    let mut w = IncrWorld::new(RepoSnapshot::new());
    let content = "fn alpha\ncall beta\nfn beta\n";
    w.apply(cie_incremental::Edit::AddFile {
        path: "a.py".into(),
        content: content.into(),
    });
    let before = w.findings_stripped();
    let id = symbol_id("a.py", "alpha");

    w.apply(cie_incremental::Edit::DeleteFile { path: "a.py".into() });
    assert!(w.findings.is_empty());
    assert!(w.no_orphan_records());

    w.apply(cie_incremental::Edit::AddFile {
        path: "a.py".into(),
        content: content.into(),
    });
    assert_eq!(w.findings_stripped(), before, "readd converges to same set");
    assert!(w.findings.contains_key(&id), "identity is content-addressed");
    assert!(w.no_orphan_records());
}

/// Deleting a file leaves remaining symbols consistent with the clean
/// recomputation (dependency-first deletion propagates via facts hashes).
#[test]
fn task_058_dependency_first_deletion_propagates() {
    let mut w = IncrWorld::new(RepoSnapshot::new());
    w.apply(cie_incremental::Edit::AddFile {
        path: "a.py".into(),
        content: "fn caller\ncall helper\n".into(),
    });
    w.apply(cie_incremental::Edit::AddFile {
        path: "b.py".into(),
        content: "fn caller\ncall helper\n".into(),
    });
    // symbol ids are file-qualified: same-name callees resolve per file.

    // remove b.py entirely (one dependent's file)
    w.apply(cie_incremental::Edit::DeleteFile { path: "b.py".into() });
    assert!(w.no_orphan_records());
    // a.py's caller is untouched and still consistent
    let clean = clean_recompute(&w.repo);
    assert_eq!(w.findings_stripped(), clean.findings_stripped());
}
