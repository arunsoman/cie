//! Repository persistence implementation behind a typed API. SQL, physical
//! records, cache layout, transactions and promotion are storage-owned.
use crate::StoragePolicy;
use cie_adapters::{assemble_facts, ExtractedProject};
use cie_core::types::Hash;
use cie_semantic::facts::LangFacts;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
const FORMAT: &str = "stream-facts-v5";

#[derive(Serialize, Deserialize)]
struct CachedFacts {
    facts: LangFacts,
    lines: Vec<u32>,
}

pub struct FileSubmission<'a> {
    pub path: &'a str,
    pub hash: Hash,
    pub source: &'a str,
    pub facts: LangFacts,
    pub tree: Option<&'a tree_sitter::Tree>,
}
#[derive(Debug, Clone, Copy)]
pub struct CachedFile {
    pub partial: bool,
}

pub struct RepositoryWriter {
    conn: Connection,
    project_root: PathBuf,
    _lock: ProjectLock,
    building: PathBuf,
    final_path: PathBuf,
    dirty: bool,
    writable: bool,
    policy: StoragePolicy,
    _cleanup: ScratchCleanup,
}
fn err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

impl RepositoryWriter {
    pub fn open(root: &Path, policy: StoragePolicy) -> io::Result<Self> {
        policy.validate()?;
        let storage_root = root.join(".cie");
        std::fs::create_dir_all(&storage_root)?;
        let lock = ProjectLock::acquire(&storage_root)?;
        // Initialize compatibility metadata only while holding the writer lease.
        let _legacy = cie_store::Store::open(root)?;
        let final_path = cie_polyglot::schema::default_db_path(root);
        let building = final_path.with_extension("building");
        // The project lock makes this scratch name single-writer; remove
        // crash leftovers including WAL before starting a fresh build.
        let cleanup = ScratchCleanup(building.clone());
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", building.display()));
        }
        // Keep no-op runs read-only: do not copy a potentially huge DB.
        let old =
            Connection::open_with_flags(&final_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()
                .filter(|old| {
                    old.query_row(
                        "SELECT value FROM ingest_meta WHERE key='format'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                    .as_deref()
                        == Some(FORMAT)
                });
        let (conn, writable) = match old {
            Some(conn) => (conn, false),
            None => (Self::open_writer(&building)?, true),
        };
        if writable {
            policy.configure(&conn)?;
        }
        Ok(Self {
            conn,
            project_root: root.to_path_buf(),
            _lock: lock,
            building: building.clone(),
            final_path,
            dirty: false,
            writable,
            policy,
            _cleanup: cleanup,
        })
    }
    pub fn location_hint(&self) -> &Path {
        &self.final_path
    }

    pub fn lookup_file(&self, path: &str, hash: &Hash) -> io::Result<Option<CachedFile>> {
        let cached:Option<(Vec<u8>,Option<String>)>=self.conn.query_row(
            "SELECT s.hash,f.status FROM semantic_files s LEFT JOIN file_status f ON f.path=s.path WHERE s.path=?1",[path],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
        Ok(cached
            .filter(|(old, _)| old.as_slice() == hash.as_slice())
            .map(|(_, status)| CachedFile {
                partial: status.as_deref() == Some("partial-parse"),
            }))
    }

    pub fn put_file(&mut self, file: FileSubmission<'_>) -> io::Result<()> {
        if file.path.is_empty()
            || Path::new(file.path).is_absolute()
            || Path::new(file.path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || file.path.contains('\\')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file identity must be a normalized relative path",
            ));
        }
        if file.facts.path != file.path
            || cie_core::types::content_hash(file.source.as_bytes()) != file.hash
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source identity does not match the submitted facts/hash",
            ));
        }

        self.invalidate_derived()?;
        self.remove_file(file.path)?;
        let rel = file.path;
        let hash = file.hash;
        let src = file.source;
        let tree = file.tree;
        let file_facts = file.facts;
        let bytes = src.as_bytes();
        self.conn.execute("INSERT OR REPLACE INTO captured_sources(path,content_hash,size_bytes,zstd_bytes) VALUES(?1,?2,?3,?4)",params![rel,blake3::hash(bytes).to_hex().to_string(),bytes.len(),zstd::bulk::compress(bytes,self.policy.compression_level).map_err(err)?]).map_err(err)?;
        // The existing polyglot grammar set owns AST/IR derivation. Other
        // adapters still persist their complete semantic facts below.
        if let (Some(lang), Some(tree)) = (cie_polyglot::schema::Lang::from_path(&rel), tree) {
            let fid = cie_polyglot::schema::ingest_tree(
                &self.conn,
                1,
                0,
                rel,
                lang,
                src,
                tree,
                &mut Default::default(),
            )
            .map_err(err)?;
            self.conn.execute("INSERT OR REPLACE INTO file_source_cache(file_id,zstd_bytes,revision) SELECT ?1,?2,revision FROM files WHERE id=?1",
                    params![fid,zstd::bulk::compress(bytes,self.policy.compression_level).map_err(err)?]).map_err(err)?;
            cie_polyglot::ir::lower_tree(
                &self.conn,
                fid,
                lang,
                &bytes,
                tree,
                &BTreeMap::new(),
                &mut Default::default(),
            )
            .map_err(err)?;
            cie_polyglot::frameworks::detect_file_tree(&self.conn, fid, lang, src, tree)
                .map_err(err)?;
            cie_polyglot::statemachines::detect_file_tree(
                &self.conn,
                fid,
                lang,
                &bytes,
                tree,
                &mut Default::default(),
            )
            .map_err(err)?;
        }
        let mut lines = vec![0];
        lines.extend(
            bytes
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| (i + 1) as u32),
        );
        let payload = serde_json::to_vec(&CachedFacts {
            facts: file_facts.clone(),
            lines: lines.clone(),
        })
        .map_err(err)?;
        self.conn
            .execute(
                "INSERT INTO semantic_files(path,hash,payload) VALUES (?1,?2,?3)",
                params![rel, hash.to_vec(), payload],
            )
            .map_err(err)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO file_status VALUES (?1,?2)",
                params![
                    rel,
                    if tree.is_some_and(|t| t.root_node().has_error()) {
                        "partial-parse"
                    } else {
                        "done"
                    }
                ],
            )
            .map_err(err)?;
        Ok(())
    }

    pub fn reconcile_files(&mut self, seen: &BTreeSet<String>) -> io::Result<()> {
        let old_paths: Vec<String> = {
            let mut q = self
                .conn
                .prepare("SELECT path FROM semantic_files")
                .map_err(err)?;
            let rows = q
                .query_map([], |r| r.get(0))
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            rows
        };
        for path in old_paths {
            if !seen.contains(&path) {
                self.invalidate_derived()?;
                self.remove_file(&path)?;
            }
        }
        Ok(())
    }
    fn open_writer(path: &Path) -> io::Result<Connection> {
        let conn = cie_polyglot::schema::open_db(path).map_err(err)?;
        cie_polyglot::ensure_all_schemas(&conn).map_err(err)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ingest_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS idx_retrieval_file_path ON files(path);
            CREATE TABLE IF NOT EXISTS semantic_files(path TEXT PRIMARY KEY,hash BLOB NOT NULL,payload BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS captured_sources(path TEXT PRIMARY KEY,content_hash TEXT NOT NULL,size_bytes INTEGER NOT NULL,zstd_bytes BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS file_status(path TEXT PRIMARY KEY,status TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS stage_status(name TEXT PRIMARY KEY,status TEXT NOT NULL,detail TEXT NOT NULL);
            PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;").map_err(err)?;
        Ok(conn)
    }

    fn ensure_writer(&mut self) -> io::Result<()> {
        if !self.writable {
            self.conn
                .backup(rusqlite::DatabaseName::Main, &self.building, None)
                .map_err(err)?;
            self.conn = Self::open_writer(&self.building)?;
            self.policy.configure(&self.conn)?;
            self.writable = true;
        }
        Ok(())
    }

    /// Only a changed revision needs a global fact graph. No-op ingestion
    /// hashes source and reads compact cache metadata, without deserialization.
    pub fn materialize(&self) -> io::Result<ExtractedProject> {
        let mut facts = Vec::new();
        let mut q = self
            .conn
            .prepare("SELECT payload FROM semantic_files ORDER BY path")
            .map_err(err)?;
        let rows = q.query_map([], |r| r.get::<_, Vec<u8>>(0)).map_err(err)?;
        for row in rows {
            let payload = row.map_err(err)?;
            let cached: CachedFacts = serde_json::from_slice(&payload).map_err(err)?;
            facts.push((cached.facts, cached.lines));
        }
        Ok(assemble_facts(facts))
    }

    pub fn has_revision(&self, rev: u64, skipped_inputs: &[String]) -> bool {
        let revision = self
            .conn
            .query_row(
                "SELECT value FROM ingest_meta WHERE key='revision'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok();
        let skipped = self
            .conn
            .query_row(
                "SELECT value FROM ingest_meta WHERE key='skipped'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok();
        revision.as_deref() == Some(rev.to_string().as_str())
            && skipped == serde_json::to_string(skipped_inputs).ok()
    }

    pub fn record_skip(&mut self, path: &str, reason: &str) -> io::Result<()> {
        let prior = self
            .conn
            .query_row(
                "SELECT status FROM file_status WHERE path=?1",
                [path],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if prior.as_deref() == Some(reason) {
            return Ok(());
        }
        self.ensure_writer()?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO file_status VALUES (?1,?2)",
                params![path, reason],
            )
            .map_err(err)?;
        Ok(())
    }
    fn invalidate_derived(&mut self) -> io::Result<()> {
        if self.dirty {
            return Ok(());
        }
        self.ensure_writer()?;
        // Rebuild only project derivations. Keep unchanged AST/IR/local facts.
        self.conn.execute_batch("DELETE FROM store_meta WHERE key='tree_fingerprint'; DELETE FROM workflow_edges; DELETE FROM workflow_nodes; DELETE FROM workflows;
            DELETE FROM c4_relationships; DELETE FROM concept_node_symbols; DELETE FROM concept_edges; DELETE FROM concept_nodes;
            DELETE FROM state_machines WHERE tier='T3'; DELETE FROM cross_language_bindings;
            DELETE FROM duplicate_clusters; DELETE FROM duplicates; DELETE FROM duplicate_gaps;
            DELETE FROM runtime_evidence; DELETE FROM runtime_unresolved; DELETE FROM runtime_summary; DELETE FROM runtime_drift;
            UPDATE calls SET callee_symbol_id=NULL, resolution='UNRESOLVED';").map_err(err)?;
        self.dirty = true;
        Ok(())
    }
    fn remove_file(&self, path: &str) -> io::Result<()> {
        self.conn
            .execute("DELETE FROM captured_sources WHERE path=?1", [path])
            .map_err(err)?;
        self.conn
            .execute_batch(
                "CREATE TEMP TABLE IF NOT EXISTS removed_symbols(id INTEGER PRIMARY KEY);
            DELETE FROM removed_symbols;",
            )
            .map_err(err)?;
        self.conn.execute("INSERT INTO removed_symbols SELECT s.id FROM symbols s JOIN files f ON f.id=s.file_id WHERE f.path=?1", [path]).map_err(err)?;
        self.conn.execute("DELETE FROM state_machines WHERE source_symbol_id IN removed_symbols OR evidence_node_id IN (SELECT a.id FROM ast_nodes a JOIN files f ON f.id=a.file_id WHERE f.path=?1)",[path]).map_err(err)?;
        self.conn
            .execute(
                "DELETE FROM framework_roles WHERE file_id IN (SELECT id FROM files WHERE path=?1)",
                [path],
            )
            .map_err(err)?;
        self.conn
            .execute(
                "DELETE FROM client_paths WHERE file_id IN (SELECT id FROM files WHERE path=?1)",
                [path],
            )
            .map_err(err)?;
        self.conn.execute_batch("DELETE FROM ir_edges WHERE from_block_id IN (SELECT id FROM ir_blocks WHERE symbol_id IN removed_symbols) OR to_block_id IN (SELECT id FROM ir_blocks WHERE symbol_id IN removed_symbols);
            DELETE FROM ir_instructions WHERE block_id IN (SELECT id FROM ir_blocks WHERE symbol_id IN removed_symbols);
            DELETE FROM calls WHERE caller_symbol_id IN removed_symbols;
            DELETE FROM ir_blocks WHERE symbol_id IN removed_symbols;
            DELETE FROM ir_meta WHERE symbol_id IN removed_symbols;
            DELETE FROM symbols WHERE id IN removed_symbols;").map_err(err)?;
        self.conn
            .execute(
                "DELETE FROM ast_nodes WHERE file_id IN (SELECT id FROM files WHERE path=?1)",
                [path],
            )
            .map_err(err)?;
        self.conn.execute("DELETE FROM file_source_cache WHERE file_id IN (SELECT id FROM files WHERE path=?1)", [path]).map_err(err)?;
        self.conn
            .execute("DELETE FROM files WHERE path=?1", [path])
            .map_err(err)?;
        self.conn
            .execute("DELETE FROM semantic_files WHERE path=?1", [path])
            .map_err(err)?;
        Ok(())
    }

    fn record_stage<T>(&self, name: &str, result: Result<T, String>) -> io::Result<()> {
        let (status, detail) = match result {
            Ok(_) => ("done", String::new()),
            Err(e) => ("failed", e),
        };
        self.conn
            .execute(
                "INSERT OR REPLACE INTO stage_status VALUES (?1,?2,?3)",
                params![name, status, detail],
            )
            .map_err(err)?;
        Ok(())
    }

    pub fn commit<T: Serialize>(
        mut self,
        rev: u64,
        extracted: &ExtractedProject,
        receipt: &T,
        skipped_inputs: &[String],
    ) -> io::Result<PublishedRevision> {
        self.commit_with_progress(rev, extracted, receipt, skipped_inputs, &|_, _, _| {})
    }

    pub fn commit_with_progress<T: Serialize>(
        mut self,
        rev: u64,
        extracted: &ExtractedProject,
        receipt: &T,
        skipped_inputs: &[String],
        progress: &dyn Fn(&str, &str, &str),
    ) -> io::Result<PublishedRevision> {
        let root = self.project_root.clone();
        progress("storage", "running", "Resolving polyglot call targets…");
        self.invalidate_derived()?;
        // Resolve polyglot calls from the authoritative adapter graph using
        // path+span identity, never first-name-wins across unrelated scopes.
        let mut ids = BTreeMap::new();
        {
            let mut q = self.conn.prepare("SELECT s.id,f.path,s.start_byte,s.end_byte FROM symbols s JOIN files f ON f.id=s.file_id").map_err(err)?;
            let rows = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, u32>(3)?,
                    ))
                })
                .map_err(err)?;
            for row in rows {
                let (id, path, start, end) = row.map_err(err)?;
                ids.insert((path, start, end), id);
            }
        }
        let mut call_heartbeat: usize = 0;
        for facts in &extracted.per_file {
            for call in &facts.calls {
                call_heartbeat += 1;
                if call_heartbeat % 1000 == 0 {
                    progress("storage", "running", &format!("Resolving polyglot call targets… ({} processed)", call_heartbeat));
                }
                let caller = extracted.store.get(call.caller);
                let callee = match call.callee {
                    cie_semantic::facts::Callee::Resolved(id) => extracted.store.get(id),
                    _ => None,
                };
                if let Some(caller) = caller {
                    let ckey = (
                        extracted.files.path(caller.loc.file).to_string(),
                        caller.loc.start,
                        caller.loc.end,
                    );
                    if let Some(cid) = ids.get(&ckey) {
                        let target = callee
                            .and_then(|s| {
                                ids.get(&(
                                    extracted.files.path(s.loc.file).to_string(),
                                    s.loc.start,
                                    s.loc.end,
                                ))
                            })
                            .copied();
                        self.conn.execute("UPDATE calls SET callee_symbol_id=?1,resolution=?2 WHERE caller_symbol_id=?3 AND callee_text=?4 AND call_node_id IN (SELECT id FROM ast_nodes WHERE start_byte=?5 AND end_byte=?6)", params![target, if target.is_some() {"RESOLVED"} else {"UNRESOLVED"},cid,call.callee_text,call.span.0,call.span.1]).map_err(err)?;
                    }
                }
            }
        }
        self.conn
            .execute("UPDATE files SET revision=?1", [rev])
            .map_err(err)?;
        self.conn
            .execute("UPDATE file_source_cache SET revision=?1", [rev])
            .map_err(err)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO ingest_meta VALUES ('format',?1)",
                [FORMAT],
            )
            .map_err(err)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO ingest_meta VALUES ('revision',?1)",
                [rev.to_string()],
            )
            .map_err(err)?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO ingest_meta VALUES ('skipped',?1)",
                [serde_json::to_string(skipped_inputs).map_err(err)?],
            )
            .map_err(err)?;
        self.conn
            .execute_batch("COMMIT; PRAGMA foreign_keys=ON;")
            .map_err(err)?;
        let invalid: bool = self
            .conn
            .prepare("PRAGMA foreign_key_check")
            .map_err(err)?
            .query([])
            .map_err(err)?
            .next()
            .map_err(err)?
            .is_some();
        if invalid {
            return Err(err("stream store contains dangling references"));
        }
        // TEMPORARY diagnostic timing — isolating which storage sub-stage is
        // responsible for the real-repo 240s timeout observed on
        // shahy-restructure (2366 files). Progress heartbeats now keep the
        // runner from killing a slow-but-healthy stage; this timing stays
        // useful for identifying genuine slow substages.
        macro_rules! stage {
            ($name:literal, $expr:expr) => {{
                progress("storage", "running", concat!("Building view: ", $name, "…"));
                let __r = $expr;
                __r
            }};
        }
        macro_rules! timed {
            ($name:literal, $expr:expr) => {{
                let __t0 = std::time::Instant::now();
                let __r = $expr;
                eprintln!("CIE_STAGE_TIMING {{\"stage\":{:?},\"ms\":{}}}", $name, __t0.elapsed().as_millis());
                __r
            }};
        }
        self.record_stage("uast", timed!("uast", stage!("uast", cie_polyglot::uast::normalize(&self.conn))))?;
        self.record_stage(
            "framework_registry",
            timed!("framework_registry", stage!("framework_registry", cie_polyglot::frameworks::install_registry(&self.conn))),
        )?;
        self.record_stage(
            "frameworks",
            timed!("frameworks", stage!("frameworks", cie_polyglot::frameworks::link_clients(&self.conn))),
        )?;
        self.record_stage(
            "c4",
            timed!("c4", stage!("c4", cie_polyglot::c4::detect_c4(&self.conn, &root, rev as i64))),
        )?;
        self.record_stage(
            "state_machines",
            timed!("state_machines", stage!("state_machines", cie_polyglot::statemachines::detect_t3(&self.conn, &mut Default::default()))),
        )?;
        self.record_stage("workflows", timed!("workflows", stage!("workflows", cie_polyglot::workflows::detect(&self.conn))))?;
        self.record_stage("duplicates", timed!("duplicates", stage!("duplicates", cie_polyglot::duplicates::detect_with_progress(&self.conn,
            &|detail| progress("storage", "running", &format!("Building view: duplicates: {detail}"))
        ))))?;
        self.record_stage(
            "duplicate_clusters",
            timed!("duplicate_clusters", stage!("duplicate_clusters", cie_polyglot::duplicates::cluster_canonical(&self.conn))),
        )?;
        let bundle = match timed!("viewspec", stage!("viewspec", cie_polyglot::viewspec::produce_bundle(&self.conn))) {
            Ok(bundle) => bundle,
            Err(e) => {
                self.record_stage::<()>("viewspec", Err(e))?;
                serde_json::Value::Null
            }
        };
        let artifacts = crate::artifacts::RevisionArtifacts::open(&root)?;
        let mut q = self
            .conn
            .prepare("SELECT name,status,detail FROM stage_status ORDER BY name")
            .map_err(err)?;
        let stages: Vec<(String, String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        drop(q);
        artifacts.put(rev, crate::artifacts::Artifact::StageStatus, &stages)?;
        artifacts.put(rev, crate::artifacts::Artifact::StreamReceipt, receipt)?;
        artifacts.put(rev, crate::artifacts::Artifact::Polyglot, &bundle)?;
        progress("storage", "running", "Publishing the revision…");
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(err)?;
        // Close before promotion. Drop also cleans failed builds.
        self.conn.close().map_err(|(_, e)| err(e))?;
        let revision_dir = root.join(".cie/revs").join(rev.to_string());
        std::fs::create_dir_all(&revision_dir)?;
        let archived = revision_dir.join("repository.sqlite.tmp");
        std::fs::copy(&self.building, &archived)?;
        std::fs::rename(&archived, revision_dir.join("repository.sqlite"))?;
        std::fs::rename(&self.building, &self.final_path)?;
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.final_path.display()));
        }
        Ok(PublishedRevision {
            revision: rev,
            _lock: self._lock,
        })
    }
}

/// Keeps publication serialized until the caller has published its compatible
/// snapshot/head. Dropping it releases the repository writer lease.
pub struct PublishedRevision {
    pub revision: u64,
    _lock: ProjectLock,
}

struct ScratchCleanup(PathBuf);
impl Drop for ScratchCleanup {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

/// Serialize writers for a project; the OS releases the lock after a crash.
pub struct ProjectLock(std::fs::File);
impl ProjectLock {
    /// Bounded reader lease; a writer cannot stall retrieval indefinitely.
    pub fn acquire_timeout(root: &Path, timeout: std::time::Duration) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("ingest.lock"))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self(file)),
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Storage publication lease deadline exceeded",
                    ))
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
        }
    }
    pub fn acquire(root: &Path) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("ingest.lock"))?;
        file.lock()?;
        Ok(Self(file))
    }
}
impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Dependency usage consumes persisted semantic imports, not a fresh parse.
pub fn cached_imports(root: &Path) -> io::Result<BTreeMap<String, Vec<String>>> {
    let db = Connection::open_with_flags(
        cie_polyglot::schema::default_db_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(err)?;
    let mut q = db
        .prepare("SELECT path,payload FROM semantic_files ORDER BY path")
        .map_err(err)?;
    let rows = q
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(err)?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (path, payload) = row.map_err(err)?;
        let cached: CachedFacts = serde_json::from_slice(&payload).map_err(err)?;
        out.insert(
            path,
            cached.facts.imports.into_iter().map(|i| i.module).collect(),
        );
    }
    Ok(out)
}

/// A bounded, typed exact-match query. Physical indexes and SQL stay private.
#[derive(Debug, Clone, Default)]
pub struct SymbolQuery {
    pub name: Option<String>,
    pub file: Option<String>,
    pub kind: Option<String>,
    pub limit: usize,
    pub offset: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSymbol {
    pub id: i64,
    pub revision: u64,
    pub path: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub start_byte: u32,
    pub end_byte: u32,
}
/// Read-only access to the last completely promoted store.
pub struct RepositoryReader {
    conn: Connection,
    policy: StoragePolicy,
}
impl RepositoryReader {
    pub fn open(root: &Path, policy: StoragePolicy) -> io::Result<Self> {
        policy.validate()?;
        let conn = Connection::open_with_flags(
            cie_polyglot::schema::default_db_path(root),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(err)?;
        policy.configure(&conn)?;
        Ok(Self { conn, policy })
    }
    pub fn file_facts(&self, path: &str) -> io::Result<Option<LangFacts>> {
        let payload: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT payload FROM semantic_files WHERE path=?1",
                [path],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        payload
            .map(|bytes| {
                serde_json::from_slice::<CachedFacts>(&bytes)
                    .map(|v| v.facts)
                    .map_err(err)
            })
            .transpose()
    }
    pub fn symbols(&self, query: &SymbolQuery) -> io::Result<Vec<StoredSymbol>> {
        use rusqlite::types::Value;
        let mut predicates = Vec::new();
        let mut values = Vec::new();
        for (column, value) in [
            ("s.name", &query.name),
            ("f.path", &query.file),
            ("s.kind", &query.kind),
        ] {
            if let Some(value) = value {
                predicates.push(format!("{column}=?"));
                values.push(Value::Text(value.clone()));
            }
        }
        let filter = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };
        let limit = if query.limit == 0 { 50 } else { query.limit }.min(self.policy.max_page_size);
        values.push(Value::Integer(limit as i64));
        values.push(Value::Integer(i64::try_from(query.offset).map_err(err)?));
        let sql=format!("SELECT s.id,f.revision,f.path,s.name,s.qual_name,s.kind,s.start_byte,s.end_byte FROM symbols s JOIN files f ON f.id=s.file_id{filter} ORDER BY f.path,s.start_byte,s.id LIMIT ? OFFSET ?");
        let mut stmt = self.conn.prepare(&sql).map_err(err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values), |r| {
                Ok(StoredSymbol {
                    id: r.get(0)?,
                    revision: r.get(1)?,
                    path: r.get(2)?,
                    name: r.get(3)?,
                    qualified_name: r.get(4)?,
                    kind: r.get(5)?,
                    start_byte: r.get(6)?,
                    end_byte: r.get(7)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }
}
