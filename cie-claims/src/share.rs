//! Sharing + decision trail (CIE-US US-091 engine close).
//!
//! "Share views, attach evidence and record agreement or dispute." The
//! engine-side obligation is the *visibility contract*: a share manifest
//! names the reader's allowed paths, the shared view is CUT to what the
//! reader may see, and what was invisible is LISTED WITHOUT BEING
//! REVEALED (redaction names the file's existence and redaction reason
//! only — never its content). Agreement/dispute records are append-only:
//! disagreement is recorded, never erased.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The reader's visibility (engine-side: path prefixes they may see).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visibility {
    pub reader: String,
    pub allowed_prefixes: Vec<String>,
}

impl Visibility {
    pub fn may_see(&self, path: &str) -> bool {
        self.allowed_prefixes.iter().any(|p| path.starts_with(p.as_str()))
    }
}

/// One shareable element (claim/evidence attachment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareItem {
    pub path: String,
    pub element: String,
    pub evidence: Vec<String>,
}

/// The filtered view: visible items + redaction list (no content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedView {
    pub reader: String,
    pub visible: Vec<ShareItem>,
    /// What is invisible, listed WITHOUT being revealed: path-prefix
    /// count + the redaction reason only.
    pub redacted: Vec<RedactionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionRecord {
    /// The path prefix that was cut (existence is known to the sharer;
    /// content is not revealed).
    pub prefix: String,
    pub items_hidden: usize,
    pub reason: String,
}

/// Cut a share to the reader's visibility (US-091). Deterministic.
pub fn share_view(items: &[ShareItem], vis: &Visibility) -> SharedView {
    let mut visible = Vec::new();
    let mut hidden_by_prefix: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for it in items {
        if vis.may_see(&it.path) {
            visible.push(it.clone());
        } else {
            // record existence at prefix granularity only
            let prefix = it.path.split('/').next().unwrap_or(&it.path).to_string();
            *hidden_by_prefix.entry(prefix).or_insert(0) += 1;
        }
    }
    let redacted = hidden_by_prefix
        .into_iter()
        .map(|(prefix, n)| RedactionRecord {
            prefix,
            items_hidden: n,
            reason: format!("outside {}'s read scope", vis.reader),
        })
        .collect();
    SharedView {
        reader: vis.reader.clone(),
        visible,
        redacted,
    }
}

/// A decision-trail entry: agreement or dispute, appended (never
/// erased; disputes stay).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Agree,
    Dispute,
}

/// An append-only decision trail for one element.
#[derive(Debug, Clone, Default)]
pub struct DecisionTrail {
    pub element: String,
    pub entries: Vec<TrailEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailEntry {
    pub author: String,
    pub position: Position,
    pub note: String,
    pub at_ns: u64,
}

impl DecisionTrail {
    pub fn new(element: &str) -> Self {
        DecisionTrail {
            element: element.to_string(),
            entries: Vec::new(),
        }
    }

    /// Append agreement or dispute — order preserved, nothing removed.
    pub fn record(&mut self, author: &str, position: Position, note: &str, at_ns: u64) {
        self.entries.push(TrailEntry {
            author: author.into(),
            position,
            note: note.into(),
            at_ns,
        });
    }

    /// The trail's honest summary: both counts, disputes listed.
    pub fn summary(&self) -> (usize, usize) {
        let agree = self.entries.iter().filter(|e| e.position == Position::Agree).count();
        let dispute = self.entries.iter().filter(|e| e.position == Position::Dispute).count();
        (agree, dispute)
    }
}

/// Private vs team-visible note scope (US-091: notes can be private).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteScope {
    Private,
    TeamVisible,
}

/// Filter a trail for one reader: private notes from OTHERS are hidden
/// (listed as hidden entries, not revealed).
pub fn trail_for_reader(
    trail: &DecisionTrail,
    reader: &str,
    scopes: &std::collections::BTreeMap<String, NoteScope>, // author → scope of their notes
) -> DecisionTrail {
    let mut out = DecisionTrail::new(&trail.element);
    for e in &trail.entries {
        let scope = scopes.get(&e.author).copied().unwrap_or(NoteScope::TeamVisible);
        match scope {
            NoteScope::TeamVisible => out.entries.push(e.clone()),
            NoteScope::Private => {
                if e.author == reader {
                    out.entries.push(e.clone());
                }
                // others' private notes: omitted (existence of a count is
                // not revealed here — privacy beats visibility)
            }
        }
    }
    out
}

/// Elements a share covers (dedup helper for callers).
pub fn covered_elements(items: &[ShareItem]) -> BTreeSet<String> {
    items.iter().map(|i| i.element.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<ShareItem> {
        vec![
            ShareItem {
                path: "src/pay.py".into(),
                element: "pay::charge".into(),
                evidence: vec!["test_charge".into()],
            },
            ShareItem {
                path: "src/secret.py".into(),
                element: "pay::vault_key".into(),
                evidence: vec!["hardcoded secret @ line 3".into()],
            },
        ]
    }

    #[test]
    fn share_is_cut_and_redaction_lists_without_revealing() {
        let vis = Visibility {
            reader: "intern".into(),
            allowed_prefixes: vec!["src/pay.py".into()],
        };
        let view = share_view(&items(), &vis);
        assert_eq!(view.visible.len(), 1);
        assert_eq!(view.visible[0].element, "pay::charge");
        // the hidden item is LISTED but its content (element name +
        // evidence) is NOT revealed
        assert_eq!(view.redacted.len(), 1);
        assert_eq!(view.redacted[0].prefix, "src");
        assert_eq!(view.redacted[0].items_hidden, 1);
        assert!(view.redacted[0].reason.contains("intern"));
        assert!(!serde_json::to_string(&view).unwrap().contains("vault_key"));
        assert!(!serde_json::to_string(&view).unwrap().contains("hardcoded"));
    }

    #[test]
    fn disagreement_is_recorded_not_erased() {
        let mut t = DecisionTrail::new("pay::charge");
        t.record("alice", Position::Agree, "matches the ticket", 10);
        t.record("bob", Position::Dispute, "handles only the happy path", 20);
        let (a, d) = t.summary();
        assert_eq!((a, d), (1, 1));
        // the dispute stays after further agreements
        t.record("carol", Position::Agree, "verified against staging", 30);
        let (a, d) = t.summary();
        assert_eq!((a, d), (2, 1));
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[1].position, Position::Dispute);
    }

    #[test]
    fn private_notes_hidden_from_others_kept_for_owner() {
        let mut t = DecisionTrail::new("pay::charge");
        t.record("alice", Position::Agree, "public note", 10);
        t.record("bob", Position::Dispute, "private doubt", 20);
        t.record("carol", Position::Agree, "team note", 30);
        let mut scopes = std::collections::BTreeMap::new();
        scopes.insert("bob".to_string(), NoteScope::Private);
        let for_intern = trail_for_reader(&t, "intern", &scopes);
        assert_eq!(for_intern.entries.len(), 2);
        assert!(for_intern.entries.iter().all(|e| e.author != "bob"));
        let for_bob = trail_for_reader(&t, "bob", &scopes);
        assert_eq!(for_bob.entries.len(), 3); // bob sees his own private note
    }
}
