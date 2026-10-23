//! Bounded, single-reader ingestion. A file buffer and its tree are borrowed
//! by the adapter, AST writer and IR lowerer, then released. Only compact
//! facts/line indexes survive the file barrier; cross-file resolution runs last.
use cie_adapters::{assemble_facts, ExtractedProject, LanguageRegistry};
use cie_core::types::content_hash;
use cie_semantic::facts::LangFacts;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const FORMAT: &str = "stream-facts-v3";
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FactsSource {
    #[default]
    Polyglot,
    Own,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceReceipt {
    pub store_path: Option<PathBuf>,
    pub from_store: usize,
    pub from_disk: usize,
    pub skipped_race: usize,
    pub files_read: usize,
    pub trees_parsed: usize,
    pub facts_reused: usize,
    pub skipped: Vec<String>,
    pub partial: Vec<String>,
}
impl SourceReceipt {
    pub fn disclosure(&self) -> Option<String> {
        Some(format!("ingest-polyglot: {} files read, {} trees parsed, {} cached fact sets reused; {} skipped files",
            self.files_read, self.trees_parsed, self.facts_reused, self.skipped.len()))
    }
}

#[derive(Serialize, Deserialize)]
struct CachedFacts {
    facts: LangFacts,
    lines: Vec<u32>,
}

pub struct StreamSession {
    conn: Connection,
    building: PathBuf,
    final_path: PathBuf,
    pub walked: crate::Walked,
    pub receipt: SourceReceipt,
    pub extracted: Option<ExtractedProject>,
    dirty: bool,
    writable: bool,
    _cleanup: ScratchCleanup,
}
fn err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

impl StreamSession {
    pub fn capture(
        root: &Path,
        registry: &LanguageRegistry,
        opts: &crate::IngestOptions,
    ) -> io::Result<Self> {
        let mut session = Self::capture_deferred(root, registry, opts)?;
        session.extracted = Some(session.materialize()?);
        Ok(session)
    }

    pub fn capture_deferred(
        root: &Path,
        registry: &LanguageRegistry,
        opts: &crate::IngestOptions,
    ) -> io::Result<Self> {
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
        let mut this = Self {
            conn,
            building: building.clone(),
            final_path,
            walked: Default::default(),
            receipt: Default::default(),
            extracted: None,
            dirty: false,
            writable,
            _cleanup: cleanup,
        };
        this.receipt.store_path = Some(this.final_path.clone());
        let mut paths = Vec::new();
        let mut dirs = vec![root.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                let name = entry.file_name();
                if ty.is_symlink() {
                    continue;
                }
                if ty.is_dir() {
                    if !crate::IGNORED.contains(&name.to_string_lossy().as_ref()) && name != ".next"
                    {
                        dirs.push(entry.path());
                    }
                } else if ty.is_file() {
                    paths.push(entry.path());
                }
            }
        }
        paths.sort();
        let mut seen = BTreeSet::new();
        for path in paths {
            let rel = path
                .strip_prefix(root)
                .map_err(err)?
                .to_string_lossy()
                .replace('\\', "/");
            let stack_input = cie_stack::is_stack_input(&rel);
            let adapter = registry.adapter_for(&rel);
            if !stack_input && adapter.is_none() {
                continue;
            }
            if !stack_input
                && opts
                    .langs
                    .as_ref()
                    .is_some_and(|langs| !langs.contains(&adapter.unwrap().id()))
            {
                continue;
            }
            // Capacity one: no queue of file bodies or syntax trees. take() also
            // bounds a file that grows after discovery. No raw memory pointers.
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            this.receipt.files_read += 1;
            if bytes.len() as u64 > MAX_FILE_BYTES || bytes.iter().take(8192).any(|b| *b == 0) {
                this.skip(&rel, "binary-or-oversized")?;
                continue;
            }
            let hash = content_hash(&bytes);
            if stack_input {
                this.walked.stack_inputs.insert(rel.clone(), hash);
                continue;
            }
            let adapter = adapter.unwrap();
            this.walked.files.insert(rel.clone(), (hash, adapter.id()));
            let Ok(src) = std::str::from_utf8(&bytes) else {
                this.skip(&rel, "invalid-utf8")?;
                continue;
            };
            seen.insert(rel.clone());
            let cached: Option<(Vec<u8>,Option<String>)> = this.conn.query_row(
                "SELECT s.hash,f.status FROM semantic_files s LEFT JOIN file_status f ON f.path=s.path WHERE s.path=?1",[&rel],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
            if opts.facts_from == FactsSource::Polyglot {
                if let Some((old_hash, status)) = cached {
                    if old_hash == hash {
                        if status.as_deref() == Some("partial-parse") {
                            this.receipt
                                .partial
                                .push(format!("{rel}: syntax errors; partial extraction"));
                        }
                        this.receipt.facts_reused += 1;
                        this.receipt.from_store += 1;
                        continue;
                    }
                }
            }
            this.invalidate_derived()?;
            this.remove_file(&rel)?;
            let mut parser = tree_sitter::Parser::new();
            let tree = if let Some(grammar) = adapter.grammar(&rel) {
                parser.set_language(&grammar).map_err(err)?;
                this.receipt.trees_parsed += 1;
                Some(
                    parser
                        .parse(&bytes, None)
                        .ok_or_else(|| err("parser aborted"))?,
                )
            } else {
                None
            };
            let mut file_facts = match &tree {
                Some(tree) => adapter.extract_tree(&rel, src, tree),
                None => adapter.extract(&rel, src), // SQL/Shell are text adapters.
            };
            if tree.as_ref().is_some_and(|t| t.root_node().has_error()) {
                this.receipt
                    .partial
                    .push(format!("{rel}: syntax errors; partial extraction"));
                if !file_facts
                    .unsupported
                    .iter()
                    .any(|note| note.contains("syntax errors"))
                {
                    file_facts
                        .unsupported
                        .push("syntax errors present — extraction is partial".into());
                }
            }
            // The existing polyglot grammar set owns AST/IR derivation. Other
            // adapters still persist their complete semantic facts below.
            if let (Some(lang), Some(tree)) =
                (cie_polyglot::schema::Lang::from_path(&rel), tree.as_ref())
            {
                let fid = cie_polyglot::schema::ingest_tree(
                    &this.conn,
                    1,
                    0,
                    &rel,
                    lang,
                    src,
                    tree,
                    &mut Default::default(),
                )
                .map_err(err)?;
                cie_polyglot::ir::cache_source_bytes(&this.conn, fid, src).map_err(err)?;
                cie_polyglot::ir::lower_tree(
                    &this.conn,
                    fid,
                    lang,
                    &bytes,
                    tree,
                    &BTreeMap::new(),
                    &mut Default::default(),
                )
                .map_err(err)?;
                cie_polyglot::frameworks::detect_file_tree(&this.conn, fid, lang, src, tree)
                    .map_err(err)?;
                cie_polyglot::statemachines::detect_file_tree(
                    &this.conn,
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
            this.conn
                .execute(
                    "INSERT INTO semantic_files(path,hash,payload) VALUES (?1,?2,?3)",
                    params![rel, hash.to_vec(), payload],
                )
                .map_err(err)?;
            this.conn
                .execute(
                    "INSERT OR REPLACE INTO file_status VALUES (?1,?2)",
                    params![
                        rel,
                        if tree.as_ref().is_some_and(|t| t.root_node().has_error()) {
                            "partial-parse"
                        } else {
                            "done"
                        }
                    ],
                )
                .map_err(err)?;
            this.receipt.from_disk += 1;
            // bytes, tree and parser drop here, before the next file is read.
        }
        let old_paths: Vec<String> = {
            let mut q = this
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
                this.invalidate_derived()?;
                this.remove_file(&path)?;
            }
        }
        Ok(this)
    }

    fn open_writer(path: &Path) -> io::Result<Connection> {
        let conn = cie_polyglot::schema::open_db(path).map_err(err)?;
        cie_polyglot::ensure_all_schemas(&conn).map_err(err)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ingest_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS semantic_files(path TEXT PRIMARY KEY,hash BLOB NOT NULL,payload BLOB NOT NULL);
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

    pub fn has_revision(&self, rev: u64) -> bool {
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
            && skipped == serde_json::to_string(&self.receipt.skipped).ok()
    }

    fn skip(&mut self, path: &str, reason: &str) -> io::Result<()> {
        self.receipt.skipped.push(format!("{path}: {reason}"));
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
        self.conn.execute_batch("DELETE FROM workflow_edges; DELETE FROM workflow_nodes; DELETE FROM workflows;
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

    pub fn finish(mut self, root: &Path, rev: u64, extracted: &ExtractedProject) -> io::Result<()> {
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
        for facts in &extracted.per_file {
            for call in &facts.calls {
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
                        self.conn.execute("UPDATE calls SET callee_symbol_id=?1,resolution=?2 WHERE caller_symbol_id=?3 AND callee_text=?4", params![target, if target.is_some() {"RESOLVED"} else {"UNRESOLVED"},cid,call.callee_text]).map_err(err)?;
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
                [serde_json::to_string(&self.receipt.skipped).map_err(err)?],
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
        self.record_stage("uast", cie_polyglot::uast::normalize(&self.conn))?;
        self.record_stage(
            "framework_registry",
            cie_polyglot::frameworks::install_registry(&self.conn),
        )?;
        self.record_stage(
            "frameworks",
            cie_polyglot::frameworks::link_clients(&self.conn),
        )?;
        self.record_stage(
            "c4",
            cie_polyglot::c4::detect_c4(&self.conn, root, rev as i64),
        )?;
        self.record_stage(
            "state_machines",
            cie_polyglot::statemachines::detect_t3(&self.conn, &mut Default::default()),
        )?;
        self.record_stage("workflows", cie_polyglot::workflows::detect(&self.conn))?;
        self.record_stage("duplicates", cie_polyglot::duplicates::detect(&self.conn))?;
        self.record_stage(
            "duplicate_clusters",
            cie_polyglot::duplicates::cluster_canonical(&self.conn),
        )?;
        let bundle = match cie_polyglot::viewspec::produce_bundle(&self.conn) {
            Ok(bundle) => bundle,
            Err(e) => {
                self.record_stage::<()>("viewspec", Err(e))?;
                serde_json::Value::Null
            }
        };
        let store = cie_store::Store::open(root)?;
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
        store.write_json(
            &store
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("stages.json"),
            &stages,
        )?;
        store.write_json(
            &store
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("stream.json"),
            &self.receipt,
        )?;
        store.write_json(
            &store
                .root()
                .join("revs")
                .join(rev.to_string())
                .join("polyglot.json"),
            &bundle,
        )?;
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(err)?;
        // Close before promotion. Drop also cleans failed builds.
        self.conn.close().map_err(|(_, e)| err(e))?;
        std::fs::rename(&self.building, &self.final_path)?;
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.final_path.display()));
        }
        Ok(())
    }
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
