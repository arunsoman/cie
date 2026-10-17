use crate::model::*;
use cie_core::types::{content_hash, hash_hex};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub const FRESHNESS_SCOPE: &str = "Freshness compares indexed supported source files and Git HEAD; it does not establish freshness of skipped configuration, external systems or runtime observations.";
pub const MAX_FILES: usize = 2000;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
#[derive(Debug)]
pub struct Capture {
    pub sources: BTreeMap<String, String>,
    pub fingerprint: String,
    pub git_head: Option<String>,
    pub skipped: Vec<String>,
}
pub fn allowed_path(path: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|_| {
        Error::new(
            400,
            "REPOSITORY_UNAVAILABLE",
            "Repository directory is unavailable",
        )
    })?;
    if !path.is_dir() || !roots.iter().any(|r| path.starts_with(r)) {
        return Err(Error::new(
            403,
            "OUTSIDE_SCOPE",
            "Repository is outside the server's explicitly allowed roots",
        ));
    }
    Ok(path)
}
fn excluded(name: &str) -> bool {
    cie_ingest::IGNORED.contains(&name)
        || matches!(name, ".cie-workspace" | ".cie-product" | ".cache")
}
pub fn capture(root: &Path) -> Result<Capture> {
    let mut paths = vec![];
    let mut skipped = vec![];
    let git = Command::new("git")
        .args(["-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    // Git's list honors nested ignore files; only use it when root itself is that repository.
    let git_root = git
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|p| fs::canonicalize(p.trim()).ok());
    if git_root.as_deref() == Some(root) {
        let output = Command::new("git")
            .args(["-c", "core.fsmonitor=false"])
            .arg("-C")
            .arg(root)
            .args([
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ])
            .output()?;
        if !output.status.success() {
            return Err(Error::new(
                400,
                "REPOSITORY_SCAN_FAILED",
                "Cannot enumerate the repository",
            ));
        }
        for raw in output.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
            let rel = std::str::from_utf8(raw).map_err(|_| {
                Error::new(400, "UNSUPPORTED_PATH", "Repository has non-UTF-8 paths")
            })?;
            if rel.split('/').any(excluded) {
                continue;
            }
            paths.push(root.join(rel));
        }
    } else {
        let mut stack = vec![root.to_path_buf()];
        let mut entries_seen = 0;
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                entries_seen += 1;
                if entries_seen > 20000 {
                    return Err(Error::new(
                        413,
                        "SCAN_LIMIT",
                        "Repository directory entry limit reached",
                    ));
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if excluded(&name) {
                    continue;
                }
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    skipped.push("A symlink was excluded".into());
                } else if kind.is_dir() {
                    stack.push(path);
                } else if kind.is_file() {
                    paths.push(path);
                }
            }
        }
        skipped.push(
            "Non-Git folder: operational exclusions apply; Git ignore semantics are unavailable"
                .into(),
        );
    }
    paths.sort();
    paths.dedup();
    let mut sources = BTreeMap::new();
    let mut total: usize = 0;
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::new(403, "OUTSIDE_SCOPE", "File outside repository"))?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                skipped.push(format!("Deleted tracked file: {relative}"));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if metadata.file_type().is_symlink() {
            skipped.push("A symlink was excluded".into());
            continue;
        }
        let actual = fs::canonicalize(&path)?;
        if !actual.starts_with(root) {
            skipped.push("An inaccessible boundary was excluded".into());
            continue;
        }
        if cie_semantic::lang_for_path(&relative) == cie_semantic::LangId::Unknown {
            skipped.push(format!("Unsupported file: {relative}"));
            continue;
        }
        let size = fs::metadata(&actual)?.len();
        if size > 1024 * 1024 {
            return Err(Error::new(
                413,
                "FILE_LIMIT",
                "An analyzable file exceeds the 1 MiB input limit",
            ));
        }
        if sources.len() >= MAX_FILES || total.saturating_add(size as usize) > MAX_BYTES {
            return Err(Error::new(
                413,
                "REPOSITORY_LIMIT",
                "Repository exceeds the local analysis file/byte limits",
            ));
        }
        let bytes = fs::read(actual)?;
        if bytes.contains(&0) {
            skipped.push(format!("Binary file: {relative}"));
            continue;
        }
        total += bytes.len();
        let text = String::from_utf8(bytes).map_err(|_| {
            Error::new(
                400,
                "UNSUPPORTED_ENCODING",
                "An analyzable file is not UTF-8",
            )
        })?;
        sources.insert(relative, text);
    }
    let git_head = Command::new("git")
        .args(["-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| (s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit()));
    let fingerprint = hash_hex(&content_hash(
        &serde_json::to_vec(&(&sources, &git_head))
            .map_err(|_| Error::new(500, "SERIALIZE", "Cannot fingerprint sources"))?,
    ));
    Ok(Capture {
        sources,
        fingerprint,
        git_head,
        skipped,
    })
}
pub fn stage(capture: &Capture, directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)?;
    for (path, source) in &capture.sources {
        let p = directory.join(path);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?
        }
        fs::write(p, source)?;
    }
    Ok(())
}
pub fn freshness(repo: &Repository, revision: &Revision) -> &'static str {
    match capture(Path::new(&repo.path)) {
        Ok(current) if current.fingerprint == revision.fingerprint => {
            if repo.revisions.last().is_some_and(|r| r.id == revision.id) {
                "current"
            } else {
                "historical"
            }
        }
        Ok(_) => "stale",
        Err(_) => "unavailable",
    }
}
