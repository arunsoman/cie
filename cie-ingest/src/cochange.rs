//! Co-change coupling report (CIE-US US-087 engine close).
//!
//! Story obligation: "historical co-change relationships that cross directory
//! boundaries … supporting commits and sample sizes are inspectable, and bulk
//! edits and renames are accounted for. Co-change is distinguished from
//! runtime or static dependency and always shown with its support count."
//!
//! Built on the [`crate::history::SystemGit`] seam (offline, local `git`):
//! * **Renames accounted for**: `git log --name-status` is parsed once; every
//!   rename row (R old→new) is resolved so both eras of a file pair under its
//!   FINAL canonical name (transitively across rename chains), and rename
//!   events are counted. A pure-rename commit therefore creates no pairs.
//! * **Bulk edits accounted for**: commits touching more than
//!   [`CoChangeOptions::bulk_edit_min_paths`] paths are excluded from pairing
//!   (repository reorganisations / formatting sweeps) and counted — the
//!   exclusion effect is inspectable, never hidden.
//! * **Sample sizes inspectable**: the report carries total history size and
//!   the counted subset.
//! * **Honesty note**: the report is labelled as historical coupling — it is
//!   neither a static dependency nor a runtime one.

use std::collections::BTreeMap;
use std::path::Path;

use crate::history::{git, git_ok, HistoryUnavailable};

/// Knobs for one co-change report.
#[derive(Debug, Clone)]
pub struct CoChangeOptions {
    /// Minimum shared commits for an edge to be reported (≥1).
    pub min_shared: u64,
    /// Commits touching more paths than this are bulk edits — excluded from
    /// pairing and counted in the report. `0` disables the exclusion.
    pub bulk_edit_min_paths: usize,
    /// Bound the history read (newest-first). `None` = all commits.
    pub max_commits: Option<u32>,
}

impl Default for CoChangeOptions {
    fn default() -> Self {
        Self {
            min_shared: 1,
            bulk_edit_min_paths: 20,
            max_commits: None,
        }
    }
}

/// One co-change edge: two paths changed together `support` times.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeEdge {
    /// Canonical path A.
    pub a: String,
    /// Canonical path B.
    pub b: String,
    /// Number of (non-bulk) commits touching both paths — the support count.
    pub support: u64,
    /// True when the two paths live under different top-level directories.
    pub cross_directory: bool,
    /// The supporting commit ids (inspectable, sorted oldest-last order as
    /// git reported them — newest-first).
    pub supporting_commits: Vec<String>,
}

/// The full report (everything the story promises is inspectable here).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeReport {
    /// Total commits read from history (the sample).
    pub sample_commits: usize,
    /// Commits actually used for pairing (sample minus bulk edits).
    pub counted_commits: usize,
    /// Commits excluded as bulk edits (the exclusion effect, inspectable).
    pub excluded_bulk: usize,
    /// Rename rows resolved to canonical paths.
    pub rename_events: usize,
    /// Edges sorted by (support desc, a, b).
    pub edges: Vec<CoChangeEdge>,
    /// Honesty label — co-change is historical coupling, nothing more.
    pub note: &'static str,
}

/// One row of `git log --name-status`.
#[derive(Debug, Clone)]
pub struct NameStatusRow {
    pub status: char,
    /// Path for A/M/D; OLD path for R/C rows.
    pub path: String,
    /// NEW path for R/C rows.
    pub path_to: Option<String>,
}

/// Parse `git log --name-status` output framed with the same `%x01/%x02`
/// convention as [`crate::history::parse_log`].
pub fn parse_name_status(out: &str) -> BTreeMap<String, Vec<NameStatusRow>> {
    let mut per_commit: BTreeMap<String, Vec<NameStatusRow>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix('\u{1}') {
            // commit header: \u{1}<hash>\u{2}author\u{2}time\u{2}subject
            if let Some(id) = rest.split('\u{2}').next() {
                if !id.is_empty() {
                    current = Some(id.to_string());
                }
            }
        } else {
            // status row: "M\tpath", "R100\told\tnew", "A\tpath", "D\tpath"
            let Some(id) = &current else { continue };
            let mut parts = line.split('\t').filter(|s| !s.is_empty());
            if let Some(st) = parts.next() {
                let status = st.chars().next().unwrap_or('?');
                let path = parts.next().unwrap_or("").to_string();
                let path_to = parts.next().map(|s| s.to_string());
                if !path.is_empty() {
                    per_commit
                        .entry(id.clone())
                        .or_default()
                        .push(NameStatusRow { status, path, path_to });
                }
            }
        }
    }
    per_commit
}

/// Resolve rename rows so every historical touch of a file pairs under its
/// final canonical name. Deterministic: chains applied until fixpoint.
fn canonical_map(rows: &[NameStatusRow]) -> (BTreeMap<String, String>, usize) {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut events = 0usize;
    for r in rows {
        if (r.status == 'R' || r.status == 'C') && r.path_to.is_some() && !r.path.is_empty() {
            map.insert(r.path.clone(), r.path_to.clone().unwrap());
            events += 1;
        }
    }
    // transitively close chains (a→b, b→c ⇒ a→c); collect-then-apply so
    // immutable and mutable borrows never overlap
    loop {
        let updates: Vec<(String, String)> = map
            .iter()
            .filter_map(|(k, v)| {
                let next = map.get(v)?;
                (next != v).then(|| (k.clone(), next.clone()))
            })
            .collect();
        if updates.is_empty() {
            break;
        }
        for (k, v) in updates {
            map.insert(k, v);
        }
    }
    (map, events)
}

fn top_dir(p: &str) -> &str {
    match p.split_once('/') {
        Some((d, _)) => d,
        None => "",
    }
}

/// Build the co-change report for the repository at `root`.
pub fn co_change_report(
    root: &Path,
    opts: &CoChangeOptions,
) -> Result<CoChangeReport, HistoryUnavailable> {
    if !git_ok(root) {
        return Err(HistoryUnavailable {
            reason: "git binary unavailable or directory is not a git repository".into(),
        });
    }
    // One pass: name-status gives every commit's path set AND rename rows.
    let mut args = vec![
        "log",
        "--no-color",
        "--name-status",
        "--format=%x01%H%x02%an%x02%at%x02%s",
    ];
    let max_count = opts.max_commits.map(|n| format!("--max-count={n}"));
    if let Some(mc) = &max_count {
        args.push(mc.as_str());
    }
    let out = git(root, &args).ok_or_else(|| HistoryUnavailable {
        reason: "git log --name-status failed".into(),
    })?;
    let per_commit = parse_name_status(&out);

    // Global canonical rename map (final-name identity across all history).
    let all_rows: Vec<NameStatusRow> = per_commit.values().flatten().cloned().collect();
    let (canonical, rename_events) = canonical_map(&all_rows);

    let mut sample_commits = 0usize;
    let mut counted_commits = 0usize;
    let mut excluded_bulk = 0usize;
    // pair (a,b) → commit ids
    let mut pairs: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for (id, rows) in &per_commit {
        sample_commits += 1;
        // canonical paths touched by this commit (dedup, skip pure-rename dupes)
        let mut paths: Vec<String> = Vec::new();
        for r in rows {
            let p = if r.status == 'R' || r.status == 'C' {
                r.path_to.clone().unwrap_or_else(|| r.path.clone())
            } else {
                r.path.clone()
            };
            let p = canonical.get(&p).cloned().unwrap_or(p);
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        paths.sort();
        if opts.bulk_edit_min_paths > 0 && paths.len() > opts.bulk_edit_min_paths {
            excluded_bulk += 1;
            continue;
        }
        counted_commits += 1;
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                pairs
                    .entry((paths[i].clone(), paths[j].clone()))
                    .or_default()
                    .push(id.clone());
            }
        }
    }

    let mut edges: Vec<CoChangeEdge> = pairs
        .into_iter()
        .filter(|(_, commits)| commits.len() as u64 >= opts.min_shared)
        .map(|((a, b), commits)| CoChangeEdge {
            cross_directory: top_dir(&a) != top_dir(&b),
            a,
            b,
            support: commits.len() as u64,
            supporting_commits: commits,
        })
        .collect();
    edges.sort_by(|x, y| {
        y.support
            .cmp(&x.support)
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });

    Ok(CoChangeReport {
        sample_commits,
        counted_commits,
        excluded_bulk,
        rename_events,
        edges,
        note: "co-change is historical coupling — it is neither a static dependency nor a runtime one; support counts are from the sampled history only",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn name_status_parses_renames_and_deletes() {
        let out = "\u{1}abc\u{2}Ada\u{2}100\u{2}s\nR100\told.py\tnew.py\nM\tkept.py\n\n\u{1}def\u{2}Ada\u{2}200\u{2}t\nD\tgone.py\n";
        let m = parse_name_status(out);
        assert_eq!(m.len(), 2);
        let rows = &m["abc"];
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].status, 'R');
        assert_eq!(rows[0].path, "old.py");
        assert_eq!(rows[0].path_to.as_deref(), Some("new.py"));
        assert_eq!(m["def"][0].status, 'D');
    }

    #[test]
    fn canonical_resolves_chains_transitively() {
        let rows = vec![
            NameStatusRow { status: 'R', path: "a.py".into(), path_to: Some("b.py".into()) },
            NameStatusRow { status: 'R', path: "b.py".into(), path_to: Some("c.py".into()) },
        ];
        let (m, events) = canonical_map(&rows);
        assert_eq!(events, 2);
        assert_eq!(m["a.py"], "c.py");
        assert_eq!(m["b.py"], "c.py");
    }

    #[test]
    fn bulk_exclusion_and_cross_directory_flags() {
        // synthetic walk of the report logic via a real temp git repo
        let dir = std::env::temp_dir().join(format!("cie-cochange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            Command::new("git").args(args).current_dir(&dir).status().unwrap();
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();
        std::fs::write(dir.join("b.py"), "y = 1\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "pair"]);
        // bulk commit: 25 files at once
        for i in 0..25 {
            std::fs::write(dir.join(format!("gen{i:02}.txt")), "x").unwrap();
        }
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "bulk"]);
        // rename a.py → c.py, then touch c.py + b.py again
        run(&["mv", "a.py", "c.py"]);
        run(&["commit", "-q", "-m", "rename"]);
        std::fs::write(dir.join("c.py"), "x = 2\n").unwrap();
        std::fs::write(dir.join("b.py"), "y = 2\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "pair again"]);

        let rep = co_change_report(&dir, &CoChangeOptions::default()).unwrap();
        assert_eq!(rep.sample_commits, 4);
        assert_eq!(rep.excluded_bulk, 1);
        assert_eq!(rep.counted_commits, 3);
        assert_eq!(rep.rename_events, 1);
        // (b.py,c.py) pairs TWICE: rename resolved to canonical c.py
        let edge = rep
            .edges
            .iter()
            .find(|e| (e.a == "b.py" && e.b == "c.py") || (e.a == "c.py" && e.b == "b.py"))
            .expect("canonical co-change edge must exist");
        assert_eq!(edge.support, 2, "rename must be resolved (2 supporting commits)");
        assert_eq!(edge.supporting_commits.len(), 2);
        // generated txt files co-change with each other in the bulk commit — excluded
        assert!(rep.edges.iter().all(|e| !e.a.contains("gen")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
