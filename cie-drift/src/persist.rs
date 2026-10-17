//! Drift report persistence (TASK-076) over the generic store, including
//! the acc #24 re-run cache keyed (code hash, docs hash).

use cie_claims::model::DriftReport;
use cie_store::Store;

/// Extension trait: drift persistence over the generic store.
pub trait DriftStoreExt {
    /// Cached full drift report: identical code + identical docs ⇒ the
    /// re-run reuses the persisted report (≤ 5% of first-run wall time).
    fn load_drift_cache_entry(&self, key: &str) -> std::io::Result<Option<DriftReport>>;
    fn save_drift_cache_entry(&self, key: &str, report: &DriftReport) -> std::io::Result<()>;
    fn save_drift(&self, report: &DriftReport) -> std::io::Result<()>;
    fn load_drift(&self, rev: cie_core::types::RevId) -> std::io::Result<Option<DriftReport>>;
}

impl DriftStoreExt for Store {
    // ---- TASK-076 acc #24: drift re-run cache keyed (code hash, docs
    // hash). One file per key (`drift_cache/{key}.json`) so a cache hit
    // parses a single report, not the whole cache.

    fn load_drift_cache_entry(&self, key: &str) -> std::io::Result<Option<DriftReport>> {
        // Keys are lowercase hex — safe as file names.
        self.read_json(&self.root().join("drift_cache").join(format!("{key}.json")))
    }

    fn save_drift_cache_entry(&self, key: &str, report: &DriftReport) -> std::io::Result<()> {
        let path = self.root().join("drift_cache").join(format!("{key}.json"));
        self.write_json(&path, report)
    }

    fn save_drift(&self, report: &DriftReport) -> std::io::Result<()> {
        self.write_json(
            &self
                .root()
                .join("revs")
                .join(report.rev.to_string())
                .join("drift.json"),
            report,
        )
    }

    fn load_drift(&self, rev: cie_core::types::RevId) -> std::io::Result<Option<DriftReport>> {
        self.read_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("drift.json"),
        )
    }
}
