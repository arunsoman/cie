//! Exact-comparison diff generation (CIE-US US-114 engine close).
//!
//! "Complete candidate files and exact comparisons with the baseline":
//! revision comparison ships (`report --as-of-rev`); this module adds
//! the renderers — unified and side-by-side — computed by a real
//! LCS-based diff over the stored file contents, with the story's
//! disclosure obligations: truncation is stated (never silently cut),
//! binary files are disclosed as binary (never rendered as mojibake),
//! and full-code view is the stored content itself (the engine returns
//! it unmodified).

use serde::{Deserialize, Serialize};

/// One diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// 1-based start line in the old file.
    pub old_start: usize,
    /// 1-based start line in the new file.
    pub new_start: usize,
    pub lines: Vec<HunkLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkLine {
    pub tag: Tag,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Context,
    Add,
    Del,
}

impl Tag {
    pub fn marker(&self) -> char {
        match self {
            Tag::Context => ' ',
            Tag::Add => '+',
            Tag::Del => '-',
        }
    }
}

/// The disclosure envelope around a rendered diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub binary: bool,
    pub truncated: bool,
    pub hunks: Vec<Hunk>,
    /// Full old/new content line counts (so "complete candidate file"
    /// can be verified, not asserted).
    pub old_lines: usize,
    pub new_lines: usize,
}

/// LCS-based diff over lines (standard dynamic programming; files are
/// bounded by the budget discipline so the O(n·m) table is acceptable).
fn lcs_matrix(a: &[String], b: &[String]) -> Vec<Vec<u32>> {
    let mut t = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            t[i][j] = if a[i] == b[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    t
}

/// Full edit script (context-free) old→new, deterministic.
pub fn edit_script(old: &str, new: &str) -> Vec<(Tag, String)> {
    let a: Vec<String> = old.lines().map(String::from).collect();
    let b: Vec<String> = new.lines().map(String::from).collect();
    let t = lcs_matrix(&a, &b);
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out.push((Tag::Context, a[i].clone()));
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            out.push((Tag::Del, a[i].clone()));
            i += 1;
        } else {
            out.push((Tag::Add, b[j].clone()));
            j += 1;
        }
    }
    while i < a.len() {
        out.push((Tag::Del, a[i].clone()));
        i += 1;
    }
    while j < b.len() {
        out.push((Tag::Add, b[j].clone()));
        j += 1;
    }
    out
}

/// Render a unified diff with `ctx` context lines; change groups merge
/// when within `2*ctx+1` script indices. Truncation beyond `max_hunks`
/// is disclosed, never silent.
pub fn unified_diff(path: &str, old: &str, new: &str, ctx: usize, max_hunks: usize) -> FileDiff {
    let binary = old.contains('\0') || new.contains('\0');
    let old_total = old.lines().count();
    let new_total = new.lines().count();
    if binary {
        return FileDiff {
            path: path.into(),
            binary: true,
            truncated: false,
            hunks: vec![],
            old_lines: old_total,
            new_lines: new_total,
        };
    }
    let script = edit_script(old, new);
    // entries with 1-based positions + anchors (anchors let a hunk that
    // BEGINS with adds still report correct old/new starts)
    struct Entry {
        tag: Tag,
        text: String,
        old_pos: usize, // 0 = not on the old side
        new_pos: usize, // 0 = not on the new side
        old_anchor: usize,
        new_anchor: usize,
    }
    let mut entries: Vec<Entry> = Vec::with_capacity(script.len());
    let (mut next_old, mut next_new) = (1usize, 1usize);
    let (mut prev_old, mut prev_new) = (1usize, 1usize);
    for (tag, text) in &script {
        let (op, np) = match tag {
            Tag::Context => {
                let e = (next_old, next_new);
                next_old += 1;
                next_new += 1;
                e
            }
            Tag::Del => {
                let e = (next_old, 0usize);
                next_old += 1;
                e
            }
            Tag::Add => {
                let e = (0usize, next_new);
                next_new += 1;
                e
            }
        };
        entries.push(Entry {
            tag: *tag,
            text: text.clone(),
            old_pos: op,
            new_pos: np,
            old_anchor: if op > 0 { op } else { prev_old },
            new_anchor: if np > 0 { np } else { prev_new },
        });
        if op > 0 {
            prev_old = op;
        }
        if np > 0 {
            prev_new = np;
        }
    }

    let changed: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.tag != Tag::Context)
        .map(|(i, _)| i)
        .collect();
    let mut hunks: Vec<Hunk> = Vec::new();
    if !changed.is_empty() {
        // group change indices into merged context windows
        let mut groups: Vec<(usize, usize)> = Vec::new();
        for &ci in &changed {
            let lo = ci.saturating_sub(ctx);
            let hi = (ci + ctx).min(entries.len() - 1);
            match groups.last_mut() {
                Some(last) if lo <= last.1 + 1 => last.1 = hi,
                _ => groups.push((lo, hi)),
            }
        }
        for (lo, hi) in groups {
            let lines: Vec<HunkLine> = entries[lo..=hi]
                .iter()
                .map(|e| HunkLine { tag: e.tag, text: e.text.clone() })
                .collect();
            hunks.push(Hunk {
                old_start: entries[lo].old_anchor,
                new_start: entries[lo].new_anchor,
                lines,
            });
        }
    }
    let truncated = hunks.len() > max_hunks;
    hunks.truncate(max_hunks);
    FileDiff {
        path: path.into(),
        binary: false,
        truncated,
        hunks,
        old_lines: old_total,
        new_lines: new_total,
    }
}

/// Side-by-side render: aligned (old, new) row pairs; `None` = blank.
pub fn side_by_side(diff: &FileDiff) -> Vec<(Option<String>, Option<String>)> {
    let mut rows = Vec::new();
    for h in &diff.hunks {
        let mut dels: Vec<String> = Vec::new();
        let mut adds: Vec<String> = Vec::new();
        for l in &h.lines {
            match l.tag {
                Tag::Context => {
                    // flush pending del/add block side by side
                    flush_block(&mut dels, &mut adds, &mut rows);
                    rows.push((Some(l.text.clone()), Some(l.text.clone())));
                }
                Tag::Del => dels.push(l.text.clone()),
                Tag::Add => adds.push(l.text.clone()),
            }
        }
        flush_block(&mut dels, &mut adds, &mut rows);
    }
    rows
}

fn flush_block(dels: &mut Vec<String>, adds: &mut Vec<String>, rows: &mut Vec<(Option<String>, Option<String>)>) {
    let n = dels.len().max(adds.len());
    for k in 0..n {
        rows.push((
            dels.get(k).cloned(),
            adds.get(k).cloned(),
        ));
    }
    dels.clear();
    adds.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "alpha\nbravo\ncarol\ndelta\necho\nfoxtrot\n";
    const NEW: &str = "alpha\nbravo\ncharlie\ndelta\necho\nfoxtrot!\n";

    #[test]
    fn unified_diff_marks_changes_with_hunks_and_counts() {
        let d = unified_diff("src/x.py", OLD, NEW, 1, 10);
        assert!(!d.binary);
        assert_eq!(d.old_lines, 6);
        assert_eq!(d.new_lines, 6);
        assert!(!d.truncated);
        let all: Vec<&HunkLine> = d.hunks.iter().flat_map(|h| &h.lines).collect();
        assert!(all.iter().any(|l| l.tag == Tag::Del && l.text == "carol"));
        assert!(all.iter().any(|l| l.tag == Tag::Add && l.text == "charlie"));
        assert!(all.iter().any(|l| l.tag == Tag::Del && l.text == "foxtrot"));
        assert!(all.iter().any(|l| l.tag == Tag::Add && l.text == "foxtrot!"));
        // context lines present (bravo/alpha near changes)
        assert!(all.iter().any(|l| l.tag == Tag::Context && l.text == "bravo"));
    }

    #[test]
    fn identical_files_yield_no_hunks() {
        let d = unified_diff("src/x.py", OLD, OLD, 1, 10);
        assert!(d.hunks.is_empty());
        assert!(!d.truncated);
    }

    #[test]
    fn truncation_is_disclosed_not_silent() {
        // ctx=0 keeps the two change sites as separate hunks; max 1 → cut
        let d = unified_diff("src/x.py", OLD, NEW, 0, 1);
        assert!(d.truncated);
        assert_eq!(d.hunks.len(), 1);
        // and with room, both hunks render untruncated
        let d = unified_diff("src/x.py", OLD, NEW, 0, 10);
        assert!(!d.truncated);
        assert_eq!(d.hunks.len(), 2);
    }

    #[test]
    fn binary_files_are_disclosed_never_rendered() {
        let old = "text\0bin".to_string();
        let d = unified_diff("img.png", &old, "other\0bin", 1, 10);
        assert!(d.binary);
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn side_by_side_aligns_del_add_blocks() {
        let d = unified_diff("src/x.py", OLD, NEW, 0, 10);
        let rows = side_by_side(&d);
        // "carol" pairs with "charlie" (del/add block)
        assert!(rows.contains(&(Some("carol".into()), Some("charlie".into()))));
        // "foxtrot" pairs with "foxtrot!"
        assert!(rows.contains(&(Some("foxtrot".into()), Some("foxtrot!".into()))));
    }
}
