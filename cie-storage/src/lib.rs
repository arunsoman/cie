//! In-process storage module. Ingestion supplies typed domain records; this
//! module owns physical layout, SQL, compression, transactions and query plans.
//! It is linked into the CIE binary: there is no daemon, HTTP or IPC boundary.
pub mod artifacts;
pub mod org_store;
pub mod relations_store;
pub mod repository;
pub mod stack_store;
pub mod state_model_store;
pub mod vector_store;
pub use repository::{
    CachedFile, FileSubmission, RepositoryReader, RepositoryWriter, StoredSymbol, SymbolQuery,
};

use std::{io, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadProfile {
    LowMemory,
    Interactive,
    Bulk,
}

/// Explicit, deterministic policies, not an unmeasured claim of automatic
/// optimality. Callers name the workload; only storage interprets its layout.
#[derive(Debug, Clone)]
pub struct StoragePolicy {
    pub profile: WorkloadProfile,
    pub cache_kib: u32,
    pub compression_level: i32,
    pub max_page_size: usize,
    pub busy_timeout: Duration,
}
impl Default for StoragePolicy {
    fn default() -> Self {
        Self::for_workload(WorkloadProfile::Interactive)
    }
}
impl StoragePolicy {
    pub fn for_workload(profile: WorkloadProfile) -> Self {
        let (cache_kib, compression_level) = match profile {
            WorkloadProfile::LowMemory => (8 * 1024, 1),
            WorkloadProfile::Interactive => (32 * 1024, 3),
            WorkloadProfile::Bulk => (64 * 1024, 1),
        };
        Self {
            profile,
            cache_kib,
            compression_level,
            max_page_size: 500,
            busy_timeout: Duration::from_secs(5),
        }
    }
    pub(crate) fn validate(&self) -> io::Result<()> {
        if !(1024..=1024 * 1024).contains(&self.cache_kib)
            || !(1..=19).contains(&self.compression_level)
            || !(1..=10_000).contains(&self.max_page_size)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid storage policy",
            ));
        }
        Ok(())
    }
    pub(crate) fn configure(&self, conn: &rusqlite::Connection) -> io::Result<()> {
        self.validate()?;
        let err = |e: rusqlite::Error| io::Error::other(e.to_string());
        conn.busy_timeout(self.busy_timeout).map_err(err)?;
        conn.pragma_update(None, "cache_size", -(self.cache_kib as i64))
            .map_err(err)?;
        // Large temporary sort materializations spill to disk on low-memory systems.
        conn.pragma_update(
            None,
            "temp_store",
            if self.profile == WorkloadProfile::LowMemory {
                "FILE"
            } else {
                "MEMORY"
            },
        )
        .map_err(err)?;
        Ok(())
    }
}
