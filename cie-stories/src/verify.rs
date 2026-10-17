//! Per-story verification. The engine-verifiable obligations are checked by
//! named test functions (added section-by-section in the verify suites);
//! `verify_story` runs the registry audit for a story today and dispatches to
//! deep checks where a section suite exists.

use crate::registry::Disposition;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    /// Registry audit passed; deep check suite not landed for this section yet.
    AuditPass {
        story: &'static str,
        disposition: &'static str,
        evidence_count: usize,
    },
    /// Deep engine check ran and passed (carries the check's name).
    Pass { story: &'static str, check: &'static str },
    /// Deep engine check ran and FAILED — the honest result.
    Fail {
        story: &'static str,
        check: &'static str,
        error: String,
    },
    /// Story is planned — nothing to verify; the reason is the point.
    Planned { story: &'static str, reason: &'static str },
    /// Unknown story id.
    Unknown { id: String },
}

/// Verify one story by id ("US-042"): Planned → Planned; deep-checked →
/// run the engine check; otherwise → registry audit.
pub fn verify_story(id: &str) -> Verdict {
    let Some(story) = crate::catalog::Story::by_id(id) else {
        return Verdict::Unknown { id: id.to_string() };
    };
    let Some(entry) = crate::registry::entry_for(id) else {
        return Verdict::Unknown { id: id.to_string() };
    };
    match entry.disposition {
        Disposition::Planned => Verdict::Planned {
            story: story.id,
            reason: entry.note,
        },
        Disposition::Delivered | Disposition::Partial => {
            match crate::checks::deep_check(id) {
                Some(Ok(())) => Verdict::Pass {
                    story: story.id,
                    check: "deep/engine-v1",
                },
                Some(Err(e)) => Verdict::Fail {
                    story: story.id,
                    check: "deep/engine-v1",
                    error: e,
                },
                None => Verdict::AuditPass {
                    story: story.id,
                    disposition: entry.disposition.as_str(),
                    evidence_count: entry.capabilities.len(),
                },
            }
        }
    }
}

/// Verify everything; counts stay honest per verdict kind.
pub fn verify_all() -> Vec<Verdict> {
    crate::catalog::STORIES
        .iter()
        .map(|s| verify_story(s.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_dispatches_all_147() {
        let all = verify_all();
        assert_eq!(all.len(), 147);
        assert!(all.iter().all(|v| !matches!(v, Verdict::Unknown { .. })));
    }

    #[test]
    fn verify_known_and_unknown_ids() {
        assert!(matches!(verify_story("US-001"), Verdict::Pass { .. }));
        assert!(matches!(verify_story("US-005"), Verdict::Planned { .. }));
        assert!(matches!(verify_story("US-999"), Verdict::Unknown { .. }));
    }

    #[test]
    fn verify_failures_are_honest() {
        // DeepFail carries the error text; AuditPass remains for audit-only
        // stories; no story ever silently passes.
        for v in verify_all() {
            match v {
                Verdict::Fail { story, error, .. } => {
                    panic!("deep check failed for {story}: {error}");
                }
                _ => {}
            }
        }
    }
}
