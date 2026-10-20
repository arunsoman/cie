//! RFC-0020: filtered static state model, pinned to its analysis revision.
use cie_core::types::RevId;
use cie_contract::obligations::StateModel;
use cie_store::Store;

/// Extension trait: per-revision stack artifact persistence over the
/// generic store. Lives in cie-ingest (not cie-stack) because it is an
/// ingest concern — cie-stack owns the EXTRACTION, this owns the
/// PERSISTENCE, and the crate cycle stays broken (cie-stack depends on
/// nothing engine-side).
pub trait StateModelStoreExt {
    /// Persist the report for `rev` (`revs/<N>/state-model.json`), whole.
    fn save_state_model(&self, rev: RevId, report: &StateModel) -> std::io::Result<()>;
    /// Load the report persisted for `rev`, when this revision has one
    /// (`None` = pre-RFC revision or a bare folder — compute-if-absent).
    fn load_state_model(&self, rev: RevId) -> std::io::Result<Option<StateModel>>;
}

impl StateModelStoreExt for Store {
    fn save_state_model(&self, rev: RevId, report: &StateModel) -> std::io::Result<()> {
        self.write_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("state-model.json"),
            report,
        )
    }

    fn load_state_model(&self, rev: RevId) -> std::io::Result<Option<StateModel>> {
        self.read_json(
            &self
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("state-model.json"),
        )
    }
}

