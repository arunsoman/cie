//! BRIDGE (backlog item 5, un-deferred): the polyglot store becomes the
//! DEFAULT source of ingestion content.
//!
//! What this module does, exactly and honestly:
//! * the polyglot store at the shared path convention
//!   (`cie_polyglot::schema::default_db_path` — the SAME file the `cie
//!   polyglot` command promotes and the web reader opens) is consulted
//!   BEFORE any parse input is chosen;
//! * every walked file whose row in the store carries the SAME blake3
//!   `content_hash` as the walk feeds its zstd-cached source
//!   (`file_source_cache`) to the adapters — the store, not the disk, is
//!   where the parse input comes from;
//! * every miss — no store for this path, no row (Kotlin/SQL/Shell/Lua are
//!   outside polyglot's 10-language set), or hash drift (the store predates
//!   the current file) — falls back to the disk read, so behavior degrades
//!   to today's ingest, never breaks it;
//! * the disk copy is STILL re-read and re-hashed exactly as before
//!   (TASK race discipline): the walk-time hash must match the re-read
//!   hash or the file is skipped this run. Because blake3 is collision-
//!   resistant, store-verified bytes and disk bytes that both hash to the
//!   walk hash are the SAME bytes — so the adapter inputs are identical
//!   either way and the artifacts are byte-identical BY CONSTRUCTION.
//!   `tests/bridge_parity.rs` pins that gate.
//!
//! What this is NOT (yet): a row-level facts conversion. The store's
//! schema does not carry `imports`/`assignments`/returns/exceptions detail
//! or the four missing languages, so rebuilding `LangFacts` from rows
//! alone cannot pass the parity gate (worklog item 5's precondition).
//! The source seam this module lands is the plumbing INCREMENTAL-ON
//! (content_hash reuse) and any future row-level facts build on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cie_core::types::{content_hash, Hash};
use cie_semantic::lang::LangId;

/// Where the ingestion pass takes its per-file parse input.
///
/// * `Polyglot` (the DEFAULT): consult the polyglot store first, hash-
///   verified, disk fallback per file (see module docs).
/// * `Own`: today's disk-only read — the `--facts-from own` escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactsSource {
    Polyglot,
    Own,
}

impl Default for FactsSource {
    fn default() -> Self {
        FactsSource::Polyglot
    }
}

/// Receipt for one ingest pass — how the parse inputs were sourced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceReceipt {
    /// The store actually consulted (None: `Own` mode, or no store exists
    /// at the convention path for this project).
    pub store_path: Option<PathBuf>,
    /// Sources whose bytes came from the store's `file_source_cache`,
    /// each hash-verified twice (store row == walk hash, decoded bytes ==
    /// walk hash).
    pub from_store: usize,
    /// Sources read from disk (store misses of every kind).
    pub from_disk: usize,
    /// Files dropped by the unchanged race check (re-read hash != walk
    /// hash) — counted in both modes for symmetry.
    pub skipped_race: usize,
}

impl SourceReceipt {
    /// The persistent disclosure that rides in `RevSnapshot.warnings` —
    /// only when a store was genuinely consulted, so `Own` mode and
    /// store-less runs keep today's snapshot bytes.
    pub fn disclosure(&self) -> Option<String> {
        let db = self.store_path.as_ref()?;
        let total = self.from_store + self.from_disk;
        Some(format!(
            "facts-from polyglot: {} of {} sources hash-verified from the store ({}) — {} from disk; artifacts byte-identical to own-parse (tests/bridge_parity.rs)",
            self.from_store, total, db.display(), self.from_disk
        ))
    }
}

/// Resolve the parse inputs for the walked file set.
///
/// Returns the same `(path, lang, source)` vec `extract_project` consumed
/// yesterday, plus the receipt. The order (walk order — path-sorted), the
/// race check, and the invalid-UTF-8 skip semantics are exactly the
/// inline loop this module replaced; only the BYTES' provenance changes.
pub fn resolve_current_files(
    project: &Path,
    current: &BTreeMap<String, (Hash, LangId)>,
    source: FactsSource,
) -> (Vec<(String, LangId, String)>, SourceReceipt) {
    let mut receipt = SourceReceipt::default();

    // Consult the store once per run, strictly read-only. Any failure to
    // open/read is a silent all-disk fallback — the store is an
    // optimization of provenance here, never a dependency.
    let mut db_rows: BTreeMap<String, cie_polyglot::schema::SourceBlob> = BTreeMap::new();
    if source == FactsSource::Polyglot {
        let db = cie_polyglot::schema::default_db_path(project);
        if db.exists() {
            match cie_polyglot::schema::read_source_cache(&db) {
                Ok(rows) => {
                    receipt.store_path = Some(db);
                    db_rows = rows;
                }
                Err(_) => {
                    // unreadable store (locked, corrupt, foreign) — disk
                    // fallback, no disclosure (nothing was consumed)
                }
            }
        }
    }

    let mut files: Vec<(String, LangId, String)> = Vec::with_capacity(current.len());
    for (rel, (hash, lang)) in current {
        // The race check is UNCHANGED from the inline loop it replaces:
        // re-read the disk copy and verify it still hashes to what the
        // walk planned on. A file edited between walk and read sits out
        // this revision.
        let disk_src = match std::fs::read_to_string(project.join(rel)) {
            Ok(s) => s,
            Err(_) => String::new(),
        };
        if content_hash(disk_src.as_bytes()) != *hash {
            receipt.skipped_race += 1;
            continue;
        }

        // BRIDGE: when the store holds bytes that hash to the walk hash,
        // feed the DECODED STORE bytes to the adapters. Belt and braces:
        // the decoded string is re-hashed before it is trusted, so a
        // corrupt row can never reach the adapters as "verified".
        let mut pushed = false;
        if !db_rows.is_empty() {
            if let Some(blob) = db_rows.get(rel) {
                if blob.content_hash.as_slice() == hash.as_slice() {
                    if let Ok(decoded) =
                        zstd::bulk::decompress(&blob.zstd_bytes, blob.size_bytes)
                    {
                        if let Ok(src) = String::from_utf8(decoded) {
                            if content_hash(src.as_bytes()) == *hash {
                                files.push((rel.clone(), *lang, src));
                                receipt.from_store += 1;
                                pushed = true;
                            }
                        }
                    }
                }
            }
        }
        if !pushed {
            files.push((rel.clone(), *lang, disk_src));
            receipt.from_disk += 1;
        }
    }
    (files, receipt)
}
