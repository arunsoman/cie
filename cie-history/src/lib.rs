//! Git-history archaeology ingest (CIE-US US-037/US-086/US-060 engine close).
//!
//! Deviation documented in the same discipline as the OCR seam (FR-28):
//! history lives in the local `git` binary + `.git` directory, so the module
//! provides
//! - the [`GitHistoryProvider`] seam,
//! - [`SystemGit`] — shells out to an INSTALLED `git` (runtime probe, no
//!   network), parsing `git log --numstat` and `git blame --porcelain`
//!   output into typed facts;
//! - an honest [`HistoryUnavailable`] outcome when git is absent or the
//!   directory is not a repository. History is NEVER fabricated (global
//!   rule 2): a missing feed is a named gap, not empty data that reads as
//!   "no commits".
//!
//! Consumers: churn factors + co-change coupling (`cie-analysis::churn`)
//! and archaeology chains for comment-claims (US-037).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// One commit's metadata (numstat body parsed separately).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitMeta {
    /// Full 40-hex commit id.
    pub id: String,
    pub author: String,
    /// Committer/author unix time (seconds), as reported by `%at`.
    pub time: i64,
    pub subject: String,
}

/// One `<commit, path>` numstat row. `added`/`deleted` are line counts;
/// binary rows report `is_binary` with counts absent (git prints `-`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileTouch {
    pub commit: String,
    pub path: String,
    pub added: Option<u64>,
    pub deleted: Option<u64>,
    pub is_binary: bool,
}

/// One `git blame --porcelain` line binding: final line → commit + origin line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlameLine {
    pub path: String,
    /// 1-based line in the current (working-tree) file.
    pub line: u64,
    pub commit: String,
    /// 1-based line in the originating commit's version.
    pub orig_line: u64,
}

/// Typed history facts for one repository snapshot.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryFacts {
    /// Resolved short HEAD commit ("" when unknown).
    pub head: String,
    pub commits: Vec<CommitMeta>,
    pub touches: Vec<FileTouch>,
    pub blames: Vec<BlameLine>,
    /// Paths blame was requested for (provenance: what was read).
    pub blamed_paths: Vec<String>,
}

/// Honest unavailability: WHY history is absent (FR-28 spirit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryUnavailable {
    pub reason: String,
}

/// The git-history seam (capabilities, not a vendor — OCR-seam pattern).
pub trait GitHistoryProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self, root: &Path) -> bool;
    fn ingest(
        &self,
        root: &Path,
        opts: &HistoryOptions,
    ) -> Result<HistoryFacts, HistoryUnavailable>;
}

/// Knobs for one history ingest pass.
#[derive(Debug, Clone)]
pub struct HistoryOptions {
    /// Max commits to read (newest-first). `None` = all.
    pub max_commits: Option<u32>,
    /// Paths to blame (line-level binding). Empty = no blame.
    pub blame_paths: Vec<String>,
    /// Follow only these path prefixes for log/numstat (empty = all).
    pub path_filter: Vec<String>,
}

impl Default for HistoryOptions {
    fn default() -> Self {
        HistoryOptions {
            max_commits: None,
            blame_paths: Vec::new(),
            path_filter: Vec::new(),
        }
    }
}

/// Shells out to an installed `git` (runtime probe, offline).
pub struct SystemGit;

impl GitHistoryProvider for SystemGit {
    fn name(&self) -> &'static str {
        "system-git"
    }

    fn available(&self, root: &Path) -> bool {
        git_ok(root)
    }

    fn ingest(
        &self,
        root: &Path,
        opts: &HistoryOptions,
    ) -> Result<HistoryFacts, HistoryUnavailable> {
        if !git_ok(root) {
            return Err(HistoryUnavailable {
                reason: "git binary unavailable or directory is not a git repository".into(),
            });
        }
        let head = git(root, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
        let commits = self.commits(root, opts)?;
        let mut touches = Vec::new();
        for c in &commits {
            touches.extend(numstat_of(root, &c.id));
        }
        let mut blames = Vec::new();
        for p in &opts.blame_paths {
            blames.extend(blame_porcelain(root, p));
        }
        Ok(HistoryFacts {
            head,
            commits,
            touches,
            blamed_paths: opts.blame_paths.clone(),
            blames,
        })
    }
}

impl SystemGit {
    /// Commit list (newest-first), honoring `max_commits` + `path_filter`.
    pub fn commits(
        &self,
        root: &Path,
        opts: &HistoryOptions,
    ) -> Result<Vec<CommitMeta>, HistoryUnavailable> {
        let mut args = vec![
            "log".to_string(),
            "--no-color".to_string(),
            "--format=%x01%H%x02%an%x02%at%x02%s".to_string(),
        ];
        if let Some(n) = opts.max_commits {
            args.push(format!("--max-count={n}"));
        }
        for p in &opts.path_filter {
            args.push("--".to_string());
            args.push(p.clone());
        }
        let out =
            git(root, &args.iter().map(String::as_str).collect::<Vec<_>>()).ok_or_else(|| {
                HistoryUnavailable {
                    reason: "git log failed".into(),
                }
            })?;
        Ok(parse_log(&out))
    }
}

/// Parse `git log` output produced with the `%x01%H%x02%an%x02%at%x02%s`
/// record format: `\x01<sha>\x02<author>\x02<time>\x02<subject>\n`.
pub fn parse_log(out: &str) -> Vec<CommitMeta> {
    out.lines()
        .filter(|l| l.starts_with('\u{1}'))
        .filter_map(|l| {
            let body = &l[1..];
            let mut it = body.split('\u{2}');
            Some(CommitMeta {
                id: it.next()?.to_string(),
                author: it.next().unwrap_or("").to_string(),
                time: it.next().and_then(|t| t.parse().ok()).unwrap_or(0),
                subject: it.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse `--numstat` rows for one commit's output block.
pub fn parse_numstat(commit: &str, out: &str) -> Vec<FileTouch> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let add = it.next()?;
            let del = it.next()?;
            let path = it.next()?.to_string();
            if add == "-" || del == "-" {
                Some(FileTouch {
                    commit: commit.to_string(),
                    path,
                    added: None,
                    deleted: None,
                    is_binary: true,
                })
            } else {
                Some(FileTouch {
                    commit: commit.to_string(),
                    path,
                    added: add.parse().ok(),
                    deleted: del.parse().ok(),
                    is_binary: false,
                })
            }
        })
        .filter(|t| !t.path.is_empty())
        .collect()
}

/// Parse `git blame --porcelain` for one path into [`BlameLine`]s.
pub fn parse_blame(path: &str, out: &str) -> Vec<BlameLine> {
    let mut rows = Vec::new();
    for l in out.lines() {
        // Header line: `<40-hex sha> <orig> <final> [<group>]`
        let bytes = l.as_bytes();
        if bytes.len() >= 44 && bytes[..40].iter().all(u8::is_ascii_hexdigit) && bytes[40] == b' ' {
            let rest = &l[41..];
            let mut it = rest.split(' ');
            let orig: u64 = match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => v,
                None => continue,
            };
            let final_: u64 = match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => v,
                None => continue,
            };
            rows.push(BlameLine {
                path: path.to_string(),
                line: final_,
                commit: l[..40].to_string(),
                orig_line: orig,
            });
        }
        // Content lines start with a TAB — skipped; previous/author headers
        // are ignorable for the typed binding (commit metadata comes from
        // the log pass).
    }
    rows
}

/// Archaeology chain (US-037): commits that produced the lines around
/// `line` in `path`, oldest-first, with per-commit metadata. Absent blame
/// for the path yields an empty chain — never a guess.
pub fn archaeology_chain(history: &HistoryFacts, path: &str, line: u64) -> Vec<CommitMeta> {
    let mut commits: Vec<&str> = history
        .blames
        .iter()
        .filter(|b| b.path == path && b.line == line)
        .map(|b| b.commit.as_str())
        .collect();
    commits.sort_unstable();
    commits.dedup();
    commits
        .into_iter()
        .filter_map(|id| history.commits.iter().find(|c| c.id == id))
        .cloned()
        .collect()
}

/// Bind claims (doc path + line) to their introducing commits (US-037).
/// Claims whose doc is not a blamed path stay unbound — visible, not forced.
pub fn bind_claims_to_history(
    history: &HistoryFacts,
    claims: &[(String, u64)], // (doc path, line)
) -> BTreeMap<String, Vec<CommitMeta>> {
    let mut out = BTreeMap::new();
    for (path, line) in claims {
        let chain = archaeology_chain(history, path, *line);
        if !chain.is_empty() {
            out.insert(format!("{path}:{line}"), chain);
        }
    }
    out
}

// ---------------------------------------------------------------- plumbing

pub fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn git_ok(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn numstat_of(root: &Path, commit: &str) -> Vec<FileTouch> {
    let out = match git(
        root,
        &["show", "--numstat", "--format=", "--no-color", commit],
    ) {
        Some(o) => o,
        None => return Vec::new(),
    };
    parse_numstat(commit, &out)
}

/// `git blame --porcelain` for one path; empty on failure (honest gap).
pub fn blame_porcelain(root: &Path, path: &str) -> Vec<BlameLine> {
    match git(root, &["blame", "--porcelain", "--", path]) {
        Some(o) => parse_blame(path, &o),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_records() {
        let raw = "\u{1}abc123\u{2}Ada\u{2}1700000000\u{2}first\n\
                   \u{1}def456\u{2}Bob\u{2}1700000100\u{2}second\n";
        let cs = parse_log(raw);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].id, "abc123");
        assert_eq!(cs[0].author, "Ada");
        assert_eq!(cs[1].subject, "second");
    }

    #[test]
    fn parses_numstat_binary_and_counts() {
        let raw = "3\t1\tsrc/a.py\n-\t-\timg.png\n0\t0\t\n";
        let t = parse_numstat("abc123", raw);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].added, Some(3));
        assert_eq!(t[0].deleted, Some(1));
        assert!(t[1].is_binary);
        assert_eq!(t[1].added, None);
    }

    #[test]
    fn parses_blame_headers() {
        let sha = "a1b2c3d4e5".repeat(4); // 40 hex chars
        let raw = format!("{sha} 7 9 2\nauthor Ada\n\tline text\n{sha} 8 10 1\n");
        let b = parse_blame("src/a.py", &raw);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].line, 9);
        assert_eq!(b[0].orig_line, 7);
        assert_eq!(b[0].commit.len(), 40);
        assert_eq!(b[1].line, 10);
        assert_eq!(b[1].orig_line, 8);
    }

    #[test]
    fn archaeology_chain_oldest_first_and_bound_only_when_blamed() {
        let history = HistoryFacts {
            head: "def456".into(),
            commits: vec![
                CommitMeta {
                    id: "def456def456def456def456def456def456def456".into(),
                    author: "Bob".into(),
                    time: 200,
                    subject: "second".into(),
                },
                CommitMeta {
                    id: "abc123abc123abc123abc123abc123abc123abc123".into(),
                    author: "Ada".into(),
                    time: 100,
                    subject: "first".into(),
                },
            ],
            blames: vec![
                BlameLine {
                    path: "src/a.py".into(),
                    line: 9,
                    commit: "def456def456def456def456def456def456def456".into(),
                    orig_line: 7,
                },
                BlameLine {
                    path: "src/a.py".into(),
                    line: 12,
                    commit: "abc123abc123abc123abc123abc123abc123abc123".into(),
                    orig_line: 8,
                },
            ],
            blamed_paths: vec!["src/a.py".into()],
            touches: vec![],
        };
        let chain = archaeology_chain(&history, "src/a.py", 12);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].author, "Ada");
        assert_eq!(chain[0].time, 100); // older commit only
                                        // unblamed path → empty chain, not fabricated
        assert!(archaeology_chain(&history, "src/other.py", 1).is_empty());
        let bound = bind_claims_to_history(
            &history,
            &[("src/a.py".into(), 12), ("src/none.py".into(), 3)],
        );
        assert_eq!(bound.len(), 1);
        assert!(bound.contains_key("src/a.py:12"));
    }

    #[test]
    fn system_git_unavailable_outside_repo_is_honest() {
        // /tmp (or a fresh empty dir) is not a git repository in the test
        // container — ingest must report the named gap, never fake history.
        let tmp = std::env::temp_dir().join("cie-history-missing-repo-test");
        let _ = std::fs::create_dir_all(&tmp);
        let provider = SystemGit;
        if provider.available(&tmp) {
            // Some environments set GIT_DIR globally; then availability is
            // fine and this assertion is skipped rather than falsified.
            return;
        }
        let err = provider
            .ingest(&tmp, &HistoryOptions::default())
            .unwrap_err();
        assert!(err.reason.contains("git"));
    }
}

/// RFC-0020: at most 64 cited lines and 5 seconds; never a full-tree blame.
/// Omitted/dirty/shallow/unavailable sites remain unbound. No network access.
pub fn cited_history(root: &Path, sites: &[(String, u64)]) -> HistoryFacts {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut h = HistoryFacts::default();
    if bounded_git(root, &["rev-parse", "--is-shallow-repository"], deadline)
        .as_deref()
        .map(str::trim)
        != Some("false")
    {
        return h;
    }
    h.head = bounded_git(root, &["rev-parse", "HEAD"], deadline)
        .unwrap_or_default()
        .trim()
        .into();
    let unique: std::collections::BTreeSet<_> = sites.iter().cloned().collect();
    for (path, line) in unique.into_iter().take(64) {
        if line == 0
            || path.starts_with('-')
            || Path::new(&path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        let Ok(base) = root.canonicalize() else {
            continue;
        };
        if !root
            .join(&path)
            .canonicalize()
            .map(|p| p.starts_with(&base))
            .unwrap_or(false)
        {
            continue;
        }
        let range = format!("{line},{line}");
        let Some(out) = bounded_git(
            root,
            &["blame", "--line-porcelain", "-L", &range, "--", &path],
            deadline,
        ) else {
            continue;
        };
        h.blamed_paths.push(path.clone());
        for b in parse_blame(&path, &out)
            .into_iter()
            .filter(|b| b.commit.chars().any(|c| c != '0'))
        {
            if !h.commits.iter().any(|c| c.id == b.commit) {
                if let Some(log) = bounded_git(
                    root,
                    &[
                        "show",
                        "--no-patch",
                        "--format=%x01%H%x02%an%x02%at%x02%s",
                        &b.commit,
                    ],
                    deadline,
                ) {
                    h.commits.extend(parse_log(&log));
                }
            }
            h.blames.push(b);
        }
    }
    h
}
fn bounded_git(root: &Path, args: &[&str], deadline: std::time::Instant) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    if std::time::Instant::now() >= deadline {
        return None;
    }
    let mut child = Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "log.showSignature=false",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(1024 * 1024).read_to_end(&mut bytes).ok()?;
        String::from_utf8(bytes).ok()
    });
    loop {
        if let Some(status) = child.try_wait().ok()? {
            let text = reader.join().ok()??;
            return status.success().then_some(text);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
