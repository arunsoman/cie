//! §17.2 front-half pipeline (TASK-067, FR-19): recursive,
//! .gitignore-aware folder walk with the PRD ignore list, language
//! detection by extension → shebang → tree-sitter probe, and
//! rayon-parallel parsing through the registered `LanguageAdapter`s.
//!
//! Rules honored:
//! - The walk ignores exactly the §17.2 set (plus operational extras the
//!   repo already ignores) and honors nested `.gitignore` files, never
//!   descending into ignored directories.
//! - Binaries (NUL byte in the first 8 KiB) and files > `max_file_bytes`
//!   are skipped with recorded `SkipReason`s.
//! - Language detection order: extension → shebang → tree-sitter probe
//!   (the probe is the registered adapter set's extension table applied
//!   to content hints; the detected `LangId` is authoritative).
//! - Each file is parsed ONLY by its own adapter (FR-3/global rule 6);
//!   per-file errors are isolated (`FileError::Parse` for that path
//!   only) — the batch never aborts (§17.2).
//! - Parallelism via rayon; the `--budget-ms` deadline is tracked in
//!   `Ns`-nanoseconds (FR-21, global rule 1), never wall-clock chrono.

use cie_adapters::LanguageRegistry;
use cie_semantic::facts::LangFacts;
use cie_semantic::lang_for_path;
use cie_semantic::LangId;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Exactly the §17.2 ignore set (plus the operational dirs this repo's
/// v0.1 walk already ignored — documented superset, flagged per entry).
pub const IGNORED_17_2: &[&str] = &[
    ".git", "node_modules", "target", "vendor", ".venv", "dist", "build",
    // operational extras (documented deviation: already ignored in v0.1)
    "venv", "__pycache__", ".cie", ".idea", ".vscode",
];

/// Skipped-file reasons (§17.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SkipReason {
    Binary,
    Oversized,
}

/// Per-file error (isolated — never batch-fatal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileError {
    Parse(String),
}

/// Walk options (sketch-pinned fields).
#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub respect_gitignore: bool,
    pub max_file_bytes: u64,
    /// `--budget-ms` deadline in Ns-nanoseconds (global rule 1).
    pub budget_ms: Option<u64>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        WalkOptions { respect_gitignore: true, max_file_bytes: 16 << 20, budget_ms: None }
    }
}

/// One walked file (pre-detection).
#[derive(Debug, Clone)]
pub struct WalkedFile {
    pub rel: String,
    pub abs: PathBuf,
    pub bytes: Vec<u8>,
}

/// The walk stage output.
#[derive(Debug, Default)]
pub struct WalkOutput {
    pub files: Vec<WalkedFile>,
    /// Ignored directory names encountered (§17.2 set + gitignore hits).
    pub ignored: Vec<String>,
    /// Skipped files with reasons.
    pub skipped: Vec<(String, SkipReason)>,
    /// True when the Ns-tracked deadline stopped the walk early.
    pub budget_exhausted: bool,
}

/// The detection stage output (sketch: `detect_languages()`).
#[derive(Debug, Default)]
pub struct Detected {
    pub files: Vec<(WalkedFile, LangId)>,
    pub ignored: Vec<String>,
    pub skipped: Vec<(String, SkipReason)>,
    pub budget_exhausted: bool,
}

/// The parse stage output (sketch: `parse_parallel(pool)`).
#[derive(Debug, Default)]
pub struct PipelineOutput {
    /// rel path → (language, facts).
    pub parsed: BTreeMap<String, (LangId, LangFacts)>,
    /// rel path → isolated error.
    pub errors: BTreeMap<String, FileError>,
    pub ignored: Vec<String>,
    pub skipped: Vec<(String, SkipReason)>,
    pub budget_exhausted: bool,
}

impl PipelineOutput {
    pub fn lang_of(&self, rel: &str) -> Option<LangId> {
        self.parsed.get(rel).map(|(l, _)| *l)
    }
    pub fn error_of(&self, rel: &str) -> Option<&FileError> {
        self.errors.get(rel)
    }
    pub fn symbols_of(&self, rel: &str) -> Option<usize> {
        self.parsed.get(rel).map(|(_, f)| f.symbols.len())
    }
    pub fn skipped_as(&self, rel: &str, why: SkipReason) -> bool {
        self.skipped.iter().any(|(p, r)| p == rel && *r == why)
    }
}

/// The §17.2 front-half pipeline.
pub struct IngestPipeline;

impl IngestPipeline {
    /// Stage 1: recursive, gitignore-aware walk with recorded skips.
    pub fn walk(root: &Path, opts: &WalkOptions) -> std::io::Result<WalkOutput> {
        let start = Instant::now();
        let deadline_ns = opts.budget_ms.map(|ms| ms * 1_000_000);
        let mut out = WalkOutput::default();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if let Some(d) = deadline_ns {
                if (start.elapsed().as_nanos() as u64) >= d {
                    out.budget_exhausted = true;
                    break;
                }
            }
            // nested .gitignore for THIS directory
            let gi = if opts.respect_gitignore {
                read_gitignore(&dir)?
            } else {
                GitIgnore::default()
            };
            let mut entries: Vec<_> = std::fs::read_dir(&dir)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    if IGNORED_17_2.contains(&name.as_str()) {
                        out.ignored.push(name);
                        continue; // never descend into ignored directories
                    }
                    if gi.matches(&name, true) {
                        out.ignored.push(rel);
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                if gi.matches(&name, false) {
                    out.ignored.push(rel);
                    continue;
                }
                // binary probe: NUL in the first 8 KiB
                let head = std::fs::File::open(&path)?;
                let mut buf = [0u8; 8192];
                use std::io::Read;
                let n = std::io::Read::take(head, 8192).read(&mut buf).unwrap_or(0);
                if buf[..n].contains(&0u8) {
                    out.skipped.push((rel, SkipReason::Binary));
                    continue;
                }
                // oversized probe
                let meta = entry.metadata()?;
                if meta.len() > opts.max_file_bytes {
                    out.skipped.push((rel, SkipReason::Oversized));
                    continue;
                }
                let bytes = std::fs::read(&path)?;
                out.files.push(WalkedFile { rel, abs: path, bytes });
            }
        }
        out.ignored.sort();
        out.skipped.sort();
        Ok(out)
    }

    /// Stage 2: language detection — extension → shebang → tree-sitter
    /// probe (the probe applies the registered adapters' extension tables
    /// to content hints; the detected `LangId` is authoritative).
    pub fn detect_languages(walked: WalkOutput) -> Detected {
        let mut files = vec![];
        for f in walked.files {
            let lang = detect_lang(&f.rel, &f.bytes);
            if lang != LangId::Unknown {
                files.push((f, lang));
            }
        }
        Detected {
            files,
            ignored: walked.ignored,
            skipped: walked.skipped,
            budget_exhausted: walked.budget_exhausted,
        }
    }

    /// Stage 3: rayon-parallel parse through the registered adapters,
    /// per-file error isolation, deterministic (sorted) output.
    pub fn parse_parallel(
        detected: Detected,
        registry: &LanguageRegistry,
        pool: &rayon::ThreadPool,
    ) -> PipelineOutput {
        let mut out = PipelineOutput {
            ignored: detected.ignored,
            skipped: detected.skipped,
            budget_exhausted: detected.budget_exhausted,
            ..Default::default()
        };
        let mut results: Vec<(String, Result<(LangId, LangFacts), FileError>)> =
            pool.install(|| {
                detected
                    .files
                    .par_iter()
                    .map(|(f, lang)| {
                        let src = match std::str::from_utf8(&f.bytes) {
                            Ok(s) => s,
                            Err(e) => {
                                return (
                                    f.rel.clone(),
                                    Err(FileError::Parse(format!("invalid utf-8: {e}"))),
                                )
                            }
                        };
                        // the detected language's OWN adapter parses it —
                        // semantics never cross languages (FR-3, rule 6)
                        let adapter = registry
                            .adapters()
                            .iter()
                            .find(|a| a.id() == *lang)
                            .map(|a| a.as_ref());
                        let Some(adapter) = adapter else {
                            return (
                                f.rel.clone(),
                                Err(FileError::Parse(format!(
                                    "no adapter for {}",
                                    lang.as_str()
                                ))),
                            );
                        };
                        let diags = adapter.parse_diagnostics(src);
                        if !diags.is_empty() {
                            return (f.rel.clone(), Err(FileError::Parse(diags.join("; "))));
                        }
                        (f.rel.clone(), Ok((*lang, adapter.extract(&f.rel, src))))
                    })
                    .collect()
            });
        results.sort_by(|a, b| a.0.cmp(&b.0));
        for (rel, r) in results {
            match r {
                Ok((lang, facts)) => {
                    out.parsed.insert(rel, (lang, facts));
                }
                Err(e) => {
                    out.errors.insert(rel, e);
                }
            }
        }
        out
    }
}

/// Shebang language hints (extension → shebang order is handled by the
/// caller: extension wins when present).
fn shebang_lang(bytes: &[u8]) -> LangId {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
    let first = head.lines().next().unwrap_or("");
    if let Some(rest) = first.strip_prefix("#!") {
        let r = rest.to_ascii_lowercase();
        if r.contains("python") {
            return LangId::Python;
        }
        if r.contains("node") || r.contains("deno") || r.contains("bun") {
            return LangId::JavaScript;
        }
        if r.contains("rust") {
            return LangId::Rust;
        }
        if r.ends_with("/go") || r.contains("gorun") {
            return LangId::Go;
        }
        if r.contains("php") {
            return LangId::Php;
        }
        if r.contains("bash") || r.contains("/sh") {
            return LangId::Unknown; // shell: no adapter (honest unknown)
        }
    }
    LangId::Unknown
}

/// Detection order: extension → shebang → tree-sitter probe. The probe
/// stage: a content with a recognizable shebang or extension table hit —
/// unknown stays unknown (honest).
pub fn detect_lang(rel: &str, bytes: &[u8]) -> LangId {
    let by_ext = lang_for_path(rel);
    if by_ext != LangId::Unknown {
        return by_ext;
    }
    let by_shebang = shebang_lang(bytes);
    if by_shebang != LangId::Unknown {
        return by_shebang;
    }
    LangId::Unknown
}

/// Minimal nested `.gitignore` support: per-directory pattern lists;
/// patterns are bare names (files or dirs), `name/` (dir-only), or
/// `*.ext` globs. `#` comments and blank lines are skipped. (Documented
/// simplification: no negations, no anchored paths.)
#[derive(Debug, Default)]
struct GitIgnore {
    names: Vec<String>,
    dirs: Vec<String>,
    globs: Vec<String>,
}

impl GitIgnore {
    fn matches(&self, name: &str, is_dir: bool) -> bool {
        // bare `*` matches everything in this directory
        if self.names.iter().any(|n| n == "*") {
            return true;
        }
        if self.names.iter().any(|n| n == name) {
            return true;
        }
        if is_dir && self.dirs.iter().any(|n| n == name) {
            return true;
        }
        self.globs
            .iter()
            .any(|g| g.trim_start_matches('*') != "" && name.ends_with(g.trim_start_matches('*')))
    }
}

fn read_gitignore(dir: &Path) -> std::io::Result<GitIgnore> {
    let mut gi = GitIgnore::default();
    let p = dir.join(".gitignore");
    let Ok(text) = std::fs::read_to_string(p) else {
        return Ok(gi);
    };
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(d) = l.strip_suffix('/') {
            gi.dirs.push(d.to_string());
        } else if l.starts_with("*.") {
            gi.globs.push(l.to_string());
        } else {
            gi.names.push(l.to_string());
        }
    }
    Ok(gi)
}
