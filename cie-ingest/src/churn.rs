//! Churn + co-change factors from history facts (CIE-US US-086 / US-060).
//!
//! Honest-scoring discipline (story obligations):
//! * every factor and weight is exposed — scores are labelled heuristics,
//!   never calibrated defect probabilities;
//! * exclusion effects are computable (bulk edits, bot/author filters) and
//!   reported per-exclusion, not hidden;
//! * rank stability under re-weighting is computed, not asserted;
//! * history-based scores state their lower-bound nature (history read up
//!   to `max_commits` is a sample of the true history).

use std::collections::BTreeMap;

use crate::history::{FileTouch, HistoryFacts};

/// Exclusion policy for churn hotspots (US-086: effect of each exclusion
/// must be inspectable, so this is data, not a hidden filter).
#[derive(Debug, Clone)]
pub struct ChurnExclusions {
    /// Commits touching more than this many paths are treated as bulk
    /// edits (repo reorganisations, formatting sweeps) and excluded from
    /// per-path churn. `0` disables the exclusion.
    pub bulk_edit_min_paths: usize,
    /// Author names to exclude verbatim (bots, generated-code accounts).
    pub authors: Vec<String>,
    /// Subjects containing any of these substrings are excluded (e.g.
    /// "format", "cargo fmt").
    pub subject_substrings: Vec<String>,
}

impl Default for ChurnExclusions {
    fn default() -> Self {
        ChurnExclusions {
            bulk_edit_min_paths: 50,
            authors: Vec::new(),
            subject_substrings: Vec::new(),
        }
    }
}

/// Per-path churn factor with every contributing number exposed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChurnFactor {
    pub path: String,
    pub commit_count: u64,
    pub added_lines: u64,
    pub deleted_lines: u64,
    /// `added + deleted` over counted commits (lower bound: history sample).
    pub total_churn: u64,
    /// Authors that touched the path (knowledge concentration signal).
    pub authors: Vec<String>,
}

/// One exclusion's measurable effect on the ranking (US-086 obligation).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExclusionEffect {
    pub rule: String,
    pub commits_excluded: u64,
    /// Paths whose rank changed when the rule applied.
    pub paths_reranked: usize,
}

/// Full churn report: factors + exclusion ledger + rank stability.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChurnReport {
    /// Ranked by `total_churn` desc, then path asc (deterministic).
    pub factors: Vec<ChurnFactor>,
    pub exclusions: Vec<ExclusionEffect>,
    /// Rank stability: for the top-`k` under the base ranking, how often
    /// each path stays in the top-`k` across weight perturbations
    /// (churn vs commit_count weight sweeps). 1.0 = immovable.
    pub top_k: usize,
    /// path → fraction of perturbed runs where the path stays top-k.
    pub stability: BTreeMap<String, f64>,
    /// Honest lower-bound note (history sample vs full history).
    pub lower_bound_note: String,
}

/// Compute per-path churn factors with the exclusion ledger.
pub fn churn_factors(history: &HistoryFacts, excl: &ChurnExclusions) -> ChurnReport {
    // commit id → (paths touched, author, subject)
    let mut commit_paths: BTreeMap<&str, (usize, &str, &str)> = BTreeMap::new();
    for c in &history.commits {
        commit_paths
            .entry(c.id.as_str())
            .or_insert((0, c.author.as_str(), c.subject.as_str()));
    }
    // numstat rows grouped by commit: count distinct paths per commit
    let mut paths_per_commit: BTreeMap<&str, Vec<&FileTouch>> = BTreeMap::new();
    for t in &history.touches {
        commit_paths.entry(t.commit.as_str()).or_insert((
            0,
            history
                .commits
                .iter()
                .find(|c| c.id == t.commit)
                .map(|c| c.author.as_str())
                .unwrap_or(""),
            history
                .commits
                .iter()
                .find(|c| c.id == t.commit)
                .map(|c| c.subject.as_str())
                .unwrap_or(""),
        ));
        paths_per_commit.entry(t.commit.as_str()).or_default().push(t);
    }
    for (id, (n, _, _)) in commit_paths.iter_mut() {
        *n = paths_per_commit.get(id).map(|v| v.len()).unwrap_or(0);
    }

    // Exclusion ledger: which commits are dropped by which rule.
    let mut excluded: BTreeMap<&str, &'static str> = BTreeMap::new();
    let mut effects: Vec<ExclusionEffect> = Vec::new();
    let mut n_bulk = 0u64;
    for (id, (n, _, _)) in &commit_paths {
        if excl.bulk_edit_min_paths > 0 && *n > excl.bulk_edit_min_paths {
            excluded.insert(id, "bulk_edit_min_paths");
            n_bulk += 1;
        }
    }
    effects.push(ExclusionEffect {
        rule: format!("bulk_edit_min_paths>{}", excl.bulk_edit_min_paths),
        commits_excluded: n_bulk,
        paths_reranked: 0,
    });
    let mut n_author = 0u64;
    for (id, (_, author, _)) in &commit_paths {
        if !excluded.contains_key(id) && excl.authors.iter().any(|a| a == author) {
            excluded.insert(id, "author");
            n_author += 1;
        }
    }
    effects.push(ExclusionEffect {
        rule: format!("authors={:?}", excl.authors),
        commits_excluded: n_author,
        paths_reranked: 0,
    });
    let mut n_subject = 0u64;
    for (id, (_, _, subject)) in &commit_paths {
        if !excluded.contains_key(id)
            && excl
                .subject_substrings
                .iter()
                .any(|s| subject.to_lowercase().contains(&s.to_lowercase()))
        {
            excluded.insert(id, "subject_substrings");
            n_subject += 1;
        }
    }
    effects.push(ExclusionEffect {
        rule: format!("subject_substrings={:?}", excl.subject_substrings),
        commits_excluded: n_subject,
        paths_reranked: 0,
    });

    // Aggregate per-path factors with exclusions applied.
    let mut agg: BTreeMap<String, ChurnFactor> = BTreeMap::new();
    for t in &history.touches {
        if excluded.contains_key(t.commit.as_str()) {
            continue;
        }
        let author = commit_paths
            .get(t.commit.as_str())
            .map(|(_, a, _)| a.to_string())
            .unwrap_or_default();
        let f = agg.entry(t.path.clone()).or_insert_with(|| ChurnFactor {
            path: t.path.clone(),
            commit_count: 0,
            added_lines: 0,
            deleted_lines: 0,
            total_churn: 0,
            authors: Vec::new(),
        });
        f.commit_count += 1;
        if let (Some(a), Some(d)) = (t.added, t.deleted) {
            f.added_lines += a;
            f.deleted_lines += d;
        }
        if !f.authors.contains(&author) {
            f.authors.push(author);
        }
    }
    for f in agg.values_mut() {
        f.total_churn = f.added_lines + f.deleted_lines;
        f.authors.sort();
    }
    let base_rank: Vec<String> = {
        let mut v: Vec<&ChurnFactor> = agg.values().collect();
        v.sort_by(|a, b| b.total_churn.cmp(&a.total_churn).then(a.path.cmp(&b.path)));
        v.into_iter().map(|f| f.path.clone()).collect()
    };

    // paths_reranked per exclusion: compare top-10 with/without each rule.
    for e in effects.iter_mut() {
        let mut agg_no_rule = BTreeMap::new();
        for t in &history.touches {
            let rule = excluded.get(t.commit.as_str());
            if rule.is_some() && rule != Some(&rule_name(e)) {
                continue; // still excluded by another rule
            }
            let f = agg_no_rule
                .entry(t.path.clone())
                .or_insert_with(|| ChurnFactor {
                    path: t.path.clone(),
                    commit_count: 0,
                    added_lines: 0,
                    deleted_lines: 0,
                    total_churn: 0,
                    authors: Vec::new(),
                });
            f.commit_count += 1;
            if let (Some(a), Some(d)) = (t.added, t.deleted) {
                f.added_lines += a;
                f.deleted_lines += d;
            }
        }
        for f in agg_no_rule.values_mut() {
            f.total_churn = f.added_lines + f.deleted_lines;
        }
        let mut v: Vec<&ChurnFactor> = agg_no_rule.values().collect();
        v.sort_by(|a, b| b.total_churn.cmp(&a.total_churn).then(a.path.cmp(&b.path)));
        let top10: Vec<String> = v.iter().take(10).map(|f| f.path.clone()).collect();
        e.paths_reranked = top10
            .iter()
            .zip(base_rank.iter().take(10))
            .filter(|(a, b)| a != b)
            .count();
    }

    // Rank stability: sweep churn-vs-commit weight, top-k membership.
    let k = base_rank.len().min(10).max(1);
    let mut stability: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for w in [0.0f64, 0.25, 0.5, 0.75, 1.0] {
        // score = w * churn_norm + (1-w) * commit_norm
        let max_churn = agg.values().map(|f| f.total_churn).max().unwrap_or(1).max(1) as f64;
        let max_commits = agg.values().map(|f| f.commit_count).max().unwrap_or(1).max(1) as f64;
        let mut v: Vec<(&String, f64)> = agg
            .values()
            .map(|f| {
                (
                    &f.path,
                    w * (f.total_churn as f64 / max_churn)
                        + (1.0 - w) * (f.commit_count as f64 / max_commits),
                )
            })
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(b.0)));
        for (rank, (path, _)) in v.iter().enumerate() {
            stability.entry((*path).clone()).or_default().push(rank < k);
        }
    }
    let stability: BTreeMap<String, f64> = stability
        .into_iter()
        .map(|(p, v)| {
            let top = v.iter().filter(|b| **b).count();
            (p, top as f64 / v.len() as f64)
        })
        .collect();

    ChurnReport {
        factors: {
            let mut v: Vec<ChurnFactor> = agg.into_values().collect();
            v.sort_by(|a, b| b.total_churn.cmp(&a.total_churn).then(a.path.cmp(&b.path)));
            v
        },
        exclusions: effects,
        top_k: k,
        stability,
        lower_bound_note: "heuristic, lower bound: history read up to the ingest sample; \
                           scores are not calibrated defect probabilities"
            .into(),
    }
}

fn rule_name(e: &ExclusionEffect) -> &'static str {
    if e.rule.starts_with("bulk_edit") {
        "bulk_edit_min_paths"
    } else if e.rule.starts_with("authors") {
        "author"
    } else {
        "subject_substrings"
    }
}

/// Co-change coupling edge (US-060): two paths changed in the same commit
/// at least `min_shared` times — a candidate coupling / boundary input.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeEdge {
    pub a: String,
    pub b: String,
    pub shared_commits: u64,
}

/// Co-change matrix from history touches. Deterministic: edges sorted by
/// (shared desc, a, b); within a commit paths pair in sorted order.
pub fn co_change(history: &HistoryFacts, min_shared: u64) -> Vec<CoChangeEdge> {
    let mut by_commit: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in &history.touches {
        by_commit.entry(t.commit.as_str()).or_default().push(&t.path);
    }
    let mut pairs: BTreeMap<(String, String), u64> = BTreeMap::new();
    for paths in by_commit.values() {
        let mut ps = paths.clone();
        ps.sort();
        ps.dedup();
        for i in 0..ps.len() {
            for j in (i + 1)..ps.len() {
                *pairs
                    .entry((ps[i].to_string(), ps[j].to_string()))
                    .or_insert(0) += 1;
            }
        }
    }
    let mut edges: Vec<CoChangeEdge> = pairs
        .into_iter()
        .filter(|(_, n)| *n >= min_shared)
        .map(|((a, b), n)| CoChangeEdge {
            a,
            b,
            shared_commits: n,
        })
        .collect();
    edges.sort_by(|x, y| {
        y.shared_commits
            .cmp(&x.shared_commits)
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });
    edges
}

/// Directory-crossing co-change coupling (US-087): bulk edits and
/// author/subject exclusions are applied (same ledger as [`churn_factors`]),
/// pure same-commit rename pairs are dropped under a labelled heuristic
/// (never presented as a calibrated detection), and every edge carries its
/// supporting commit ids so the sample is inspectable. This is a distinct
/// type from [`CoChangeEdge`]/[`cie_relations::RelKind`]: history coupling
/// is never claimed as a static or runtime dependency.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeCoupling {
    pub a: String,
    pub b: String,
    pub shared_commits: u64,
    /// Up to 10 supporting commit ids, oldest-filtered-first by git log order.
    pub supporting_commits: Vec<String>,
    /// True when `a` and `b` sit under different top-level path segments.
    pub crosses_directory_boundary: bool,
}

/// Full report for [`co_change_coupling`]: edges plus the exclusion ledger
/// that produced them, so "accounted for" is inspectable, not asserted.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeCouplingReport {
    /// Sorted by (shared desc, a, b) — deterministic.
    pub edges: Vec<CoChangeCoupling>,
    pub exclusions: Vec<ExclusionEffect>,
    /// Same-commit (path removed, path added) pairs with equal line counts,
    /// dropped as a likely rename — a heuristic, not a verified rename.
    pub rename_heuristic_excluded: u64,
    pub note: &'static str,
}

fn top_level_dir(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

/// Co-change coupling with exclusions applied (US-087). `min_shared`
/// filters on the post-exclusion support count.
pub fn co_change_coupling(
    history: &HistoryFacts,
    min_shared: u64,
    excl: &ChurnExclusions,
) -> CoChangeCouplingReport {
    let mut commit_meta: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for c in &history.commits {
        commit_meta
            .entry(c.id.as_str())
            .or_insert((c.author.as_str(), c.subject.as_str()));
    }
    let mut paths_per_commit: BTreeMap<&str, Vec<&FileTouch>> = BTreeMap::new();
    for t in &history.touches {
        paths_per_commit.entry(t.commit.as_str()).or_default().push(t);
    }

    let mut excluded: BTreeMap<&str, &'static str> = BTreeMap::new();
    let mut n_bulk = 0u64;
    for (id, touches) in &paths_per_commit {
        let n_paths = touches.len();
        if excl.bulk_edit_min_paths > 0 && n_paths > excl.bulk_edit_min_paths {
            excluded.insert(id, "bulk_edit_min_paths");
            n_bulk += 1;
        }
    }
    let mut n_author = 0u64;
    for (id, (author, _)) in &commit_meta {
        if !excluded.contains_key(id) && excl.authors.iter().any(|a| a == author) {
            excluded.insert(id, "author");
            n_author += 1;
        }
    }
    let mut n_subject = 0u64;
    for (id, (_, subject)) in &commit_meta {
        if !excluded.contains_key(id)
            && excl
                .subject_substrings
                .iter()
                .any(|s| subject.to_lowercase().contains(&s.to_lowercase()))
        {
            excluded.insert(id, "subject_substrings");
            n_subject += 1;
        }
    }
    let exclusions = vec![
        ExclusionEffect {
            rule: format!("bulk_edit_min_paths>{}", excl.bulk_edit_min_paths),
            commits_excluded: n_bulk,
            paths_reranked: 0,
        },
        ExclusionEffect {
            rule: format!("authors={:?}", excl.authors),
            commits_excluded: n_author,
            paths_reranked: 0,
        },
        ExclusionEffect {
            rule: format!("subject_substrings={:?}", excl.subject_substrings),
            commits_excluded: n_subject,
            paths_reranked: 0,
        },
    ];

    let mut pairs: BTreeMap<(String, String), (u64, Vec<String>)> = BTreeMap::new();
    let mut rename_heuristic_excluded = 0u64;
    for (commit, touches) in &paths_per_commit {
        if excluded.contains_key(commit) {
            continue;
        }
        // Rename heuristic: a wholesale delete paired with a same-size add
        // in the same commit is dropped from the pair set entirely —
        // labelled, not claimed as a verified rename.
        let mut paths: Vec<&str> = touches.iter().map(|t| t.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        let is_rename_pair = |a: &str, b: &str| -> bool {
            let ta = touches.iter().find(|t| t.path == a);
            let tb = touches.iter().find(|t| t.path == b);
            matches!(
                (ta, tb),
                (Some(x), Some(y))
                    if x.deleted.unwrap_or(0) > 0
                        && x.added.unwrap_or(0) == 0
                        && y.added.unwrap_or(0) > 0
                        && y.deleted.unwrap_or(0) == 0
                        && x.deleted == y.added
            )
        };
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                let (a, b) = (paths[i], paths[j]);
                if is_rename_pair(a, b) || is_rename_pair(b, a) {
                    rename_heuristic_excluded += 1;
                    continue;
                }
                let entry = pairs
                    .entry((a.to_string(), b.to_string()))
                    .or_insert_with(|| (0, Vec::new()));
                entry.0 += 1;
                if entry.1.len() < 10 {
                    entry.1.push(commit.to_string());
                }
            }
        }
    }

    let mut edges: Vec<CoChangeCoupling> = pairs
        .into_iter()
        .filter(|(_, (n, _))| *n >= min_shared)
        .map(|((a, b), (n, commits))| CoChangeCoupling {
            crosses_directory_boundary: top_level_dir(&a) != top_level_dir(&b),
            a,
            b,
            shared_commits: n,
            supporting_commits: commits,
        })
        .collect();
    edges.sort_by(|x, y| {
        y.shared_commits
            .cmp(&x.shared_commits)
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });

    CoChangeCouplingReport {
        edges,
        exclusions,
        rename_heuristic_excluded,
        note: "history coupling only: a shared-commit signal, never a static \
               or runtime dependency claim; rename exclusion is a same-commit \
               size-matched heuristic, not a verified rename",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{CommitMeta, FileTouch};

    fn history() -> HistoryFacts {
        let cid = |n: usize| format!("{n:040}");
        HistoryFacts {
            head: cid(3),
            commits: vec![
                CommitMeta { id: cid(1), author: "Ada".into(), time: 100, subject: "feat: a".into() },
                CommitMeta { id: cid(2), author: "bot".into(), time: 200, subject: "chore: deps".into() },
                CommitMeta { id: cid(3), author: "Ada".into(), time: 300, subject: "fix: b".into() },
            ],
            touches: vec![
                FileTouch { commit: cid(1), path: "src/a.py".into(), added: Some(100), deleted: Some(0), is_binary: false },
                FileTouch { commit: cid(1), path: "src/b.py".into(), added: Some(10), deleted: Some(2), is_binary: false },
                FileTouch { commit: cid(2), path: "src/a.py".into(), added: Some(5000), deleted: Some(0), is_binary: false },
                FileTouch { commit: cid(3), path: "src/b.py".into(), added: Some(5), deleted: Some(1), is_binary: false },
            ],
            blames: vec![],
            blamed_paths: vec![],
        }
    }

    #[test]
    fn churn_excludes_and_exposes_factors() {
        let mut excl = ChurnExclusions::default();
        excl.authors = vec!["bot".into()];
        let rep = churn_factors(&history(), &excl);
        // bot commit excluded → a.py keeps only commit 1 (100 added)
        let a = rep.factors.iter().find(|f| f.path == "src/a.py").unwrap();
        assert_eq!(a.commit_count, 1);
        assert_eq!(a.added_lines, 100);
        assert!(a.authors.contains(&"Ada".to_string()));
        // exclusion ledger names the author rule with 1 commit excluded
        assert!(rep
            .exclusions
            .iter()
            .any(|e| e.rule.starts_with("authors") && e.commits_excluded == 1));
        assert!(rep.lower_bound_note.contains("not calibrated"));
    }

    #[test]
    fn churn_bulk_edit_exclusion_effect_reported() {
        let mut excl = ChurnExclusions::default();
        excl.bulk_edit_min_paths = 2; // commit 2 touches 1 path; commit 1 touches 2
        let rep = churn_factors(&history(), &excl);
        let bulk = rep
            .exclusions
            .iter()
            .find(|e| e.rule.starts_with("bulk_edit"))
            .unwrap();
        // commit 1 touches exactly 2 paths → not > threshold → 0 excluded
        assert_eq!(bulk.commits_excluded, 0);
    }

    #[test]
    fn co_change_pairs_shared_commits_deterministically() {
        let mut h = history();
        // make commit 3 also touch a.py → (a,b) pair twice
        h.touches.push(FileTouch {
            commit: h.commits[2].id.clone(),
            path: "src/a.py".into(),
            added: Some(1),
            deleted: Some(1),
            is_binary: false,
        });
        let edges = co_change(&h, 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].a, "src/a.py");
        assert_eq!(edges[0].b, "src/b.py");
        assert_eq!(edges[0].shared_commits, 2);
        assert!(co_change(&h, 3).is_empty());
    }

    #[test]
    fn stability_is_computed_not_asserted() {
        let rep = churn_factors(&history(), &ChurnExclusions::default());
        assert!(!rep.stability.is_empty());
        for (path, s) in &rep.stability {
            assert!(*s >= 0.0 && *s <= 1.0, "stability out of range: {path}");
        }
    }

    #[test]
    fn coupling_crosses_boundary_and_names_support() {
        let rep = co_change_coupling(&history(), 1, &ChurnExclusions::default());
        let edge = rep.edges.iter().find(|e| e.a == "src/a.py" && e.b == "src/b.py").unwrap();
        assert_eq!(edge.shared_commits, 1);
        assert_eq!(edge.supporting_commits.len(), 1);
        assert!(!edge.crosses_directory_boundary, "both under src/");
    }

    #[test]
    fn coupling_detects_directory_crossing() {
        let mut h = history();
        let cid4 = format!("{:040}", 4);
        h.commits.push(CommitMeta { id: cid4.clone(), author: "Ada".into(), time: 400, subject: "move".into() });
        h.touches.push(FileTouch { commit: cid4.clone(), path: "src/a.py".into(), added: Some(1), deleted: Some(0), is_binary: false });
        h.touches.push(FileTouch { commit: cid4, path: "docs/a.md".into(), added: Some(1), deleted: Some(0), is_binary: false });
        let rep = co_change_coupling(&h, 1, &ChurnExclusions::default());
        let edge = rep.edges.iter().find(|e| e.b == "docs/a.md" || e.a == "docs/a.md").unwrap();
        assert!(edge.crosses_directory_boundary);
    }

    #[test]
    fn coupling_excludes_bulk_edit_commits_and_counts_them() {
        let mut excl = ChurnExclusions::default();
        excl.bulk_edit_min_paths = 1; // commit 1 touches 2 paths -> excluded
        let rep = co_change_coupling(&history(), 1, &excl);
        assert!(rep.edges.is_empty(), "the only pair came from the bulk-edit commit");
        let bulk = rep.exclusions.iter().find(|e| e.rule.starts_with("bulk_edit")).unwrap();
        assert_eq!(bulk.commits_excluded, 1);
    }

    #[test]
    fn coupling_drops_same_commit_rename_pair_as_heuristic() {
        let mut h = history();
        let cid4 = format!("{:040}", 4);
        h.commits.push(CommitMeta { id: cid4.clone(), author: "Ada".into(), time: 400, subject: "rename".into() });
        h.touches.push(FileTouch { commit: cid4.clone(), path: "src/old.py".into(), added: Some(0), deleted: Some(40), is_binary: false });
        h.touches.push(FileTouch { commit: cid4, path: "src/new.py".into(), added: Some(40), deleted: Some(0), is_binary: false });
        let rep = co_change_coupling(&h, 1, &ChurnExclusions::default());
        assert!(!rep.edges.iter().any(|e| (e.a == "src/old.py" && e.b == "src/new.py")));
        assert_eq!(rep.rename_heuristic_excluded, 1);
    }
}
