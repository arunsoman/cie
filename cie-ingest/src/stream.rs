//! Bounded file processing. Persistence, cache layout, and retrieval are
//! delegated to cie-storage through typed calls; no SQL or physical schemas.
use cie_adapters::{ExtractedProject, LanguageRegistry};
use cie_core::types::content_hash;
pub use cie_storage::repository::{cached_imports, ProjectLock};
use cie_storage::{FileSubmission, RepositoryWriter, StoragePolicy};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    io::{self, Read},
    path::{Path, PathBuf},
};
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

pub struct StreamSession {
    storage: RepositoryWriter,
    pub walked: crate::Walked,
    pub receipt: SourceReceipt,
    pub extracted: Option<ExtractedProject>,
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
        let storage = RepositoryWriter::open(root, StoragePolicy::default())?;
        let mut receipt = SourceReceipt::default();
        receipt.store_path = Some(storage.location_hint().to_path_buf());
        let mut this = Self {
            storage,
            walked: Default::default(),
            receipt,
            extracted: None,
        };
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
            if opts.facts_from == FactsSource::Polyglot {
                if let Some(cached) = this.storage.lookup_file(&rel, &hash)? {
                    if cached.partial {
                        this.receipt
                            .partial
                            .push(format!("{rel}: syntax errors; partial extraction"));
                    }
                    this.receipt.facts_reused += 1;
                    this.receipt.from_store += 1;
                    continue;
                }
            }
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
            this.storage.put_file(FileSubmission {
                path: &rel,
                hash,
                source: src,
                facts: file_facts,
                tree: tree.as_ref(),
            })?;
            this.receipt.from_disk += 1;
        }
        this.storage.reconcile_files(&seen)?;
        Ok(this)
    }
    fn skip(&mut self, path: &str, reason: &str) -> io::Result<()> {
        self.receipt.skipped.push(format!("{path}: {reason}"));
        self.storage.record_skip(path, reason)
    }
    pub fn materialize(&self) -> io::Result<ExtractedProject> {
        self.storage.materialize()
    }
    pub fn has_revision(&self, rev: u64) -> bool {
        self.storage.has_revision(rev, &self.receipt.skipped)
    }
    pub fn finish(
        self,
        _root: &Path,
        rev: u64,
        graph: &ExtractedProject,
    ) -> io::Result<cie_storage::repository::PublishedRevision> {
        self.storage
            .commit(rev, graph, &self.receipt, &self.receipt.skipped)
    }
}
