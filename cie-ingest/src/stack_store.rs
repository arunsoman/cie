//! RFC-0019 §6.1 — StackStore persistence: the stack lens' report becomes
//! the FOURTH sibling of the Claim/Drift/Label store extensions, persisted
//! per revision as `<repo>/.cie/revs/<N>/stack.json`.
//!
//! One parse per revision, one artifact family: after this lands, "what did
//! CIE see for rev N?" has one answer (the revision folder), not two. The
//! trait shape mirrors `ClaimStoreExt` (cie-claims/src/persist.rs) so a
//! maintainer reads one and already understands the other.

use cie_core::types::RevId;
use cie_stack::types::StackReport;
use cie_store::Store;

/// Extension trait: per-revision stack artifact persistence over the
/// generic store. Lives in cie-ingest (not cie-stack) because it is an
/// ingest concern — cie-stack owns the EXTRACTION, this owns the
/// PERSISTENCE, and the crate cycle stays broken (cie-stack depends on
/// nothing engine-side).
pub trait StackStoreExt {
    /// Persist the report for `rev` (`revs/<N>/stack.json`), whole.
    fn save_stack(&self, rev: RevId, report: &StackReport) -> std::io::Result<()>;
    /// Load the report persisted for `rev`, when this revision has one
    /// (`None` = pre-RFC revision or a bare folder — compute-if-absent).
    fn load_stack(&self, rev: RevId) -> std::io::Result<Option<StackReport>>;
}

impl StackStoreExt for Store {
    fn save_stack(&self, rev: RevId, report: &StackReport) -> std::io::Result<()> {
        self.write_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("stack.json"),
            report,
        )
    }

    fn load_stack(&self, rev: RevId) -> std::io::Result<Option<StackReport>> {
        self.read_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("stack.json"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_artifact_roundtrips_per_rev() {
        let dir = std::env::temp_dir().join(format!("cie-stack-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = cie_store::Store::open(&dir).unwrap();
        let rev: cie_core::types::RevId = 7;

        // absent before save — a pre-RFC revision answers None, never lies
        assert!(store.load_stack(rev).unwrap().is_none());

        let report = StackReport::unavailable("test fixture");
        store.save_stack(rev, &report).unwrap();
        let loaded = store.load_stack(rev).unwrap().expect("saved report loads");
        assert_eq!(loaded.notes.len(), 1);
        assert!(loaded.notes[0].contains("unavailable"));

        // the artifact sits at the documented path
        assert!(dir
            .join(".cie")
            .join("revs")
            .join("7")
            .join("stack.json")
            .exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
