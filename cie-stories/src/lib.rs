//! cie-stories — the user-story capability layer (CIE-US).
//!
//! The product user-story catalogue (147 stories, US-001..US-147, dated
//! 2026-10-18) is kept as machine-readable data ([`catalog`]), mapped onto
//! the engine's real capabilities by an honest coverage registry
//! ([`registry`]), summarised by [`coverage`], and made verifiable by
//! per-story checks ([`verify`]).
//!
//! Honesty rules carried over from the PRD family:
//! * a disposition names evidence (crate :: module / CLI command / named
//!   test) — a story is never marked delivered because a name looks right;
//! * stories whose complementary view is a host/GUI concern are dispositioned
//!   [`registry::Disposition::HostLayer`] with the host artifact named, not
//!   silently claimed;
//! * partial coverage stays visible ("matched so far" semantics).

pub mod catalog;
pub mod checks;
pub mod coverage;
pub mod registry;
pub mod verify;

pub use catalog::{
    ACCEPTANCE_EXPECTATIONS, CATALOGUE_DATE, CATALOGUE_TITLE, FAMILIES, SECTIONS, STORIES,
    UNIVERSAL_EXPECTATIONS, Story,
};

pub use coverage::{CoverageReport, SectionRollup, StoryCoverage};
pub use registry::{Disposition, Entry, REGISTRY};
pub use verify::{Verdict, verify_story};

/// Deep checks available so far (batches land section by section).
pub const DEEP_CHECKS: &[&str] = &[
    // batch C — repo access + answers
    "US-001", "US-002", "US-003", "US-004", "US-006", "US-008", "US-011", "US-013", "US-014",
    "US-015", "US-016", "US-017",
    // batch D — visual exploration + catalogue
    "US-018", "US-019", "US-021", "US-025", "US-026", "US-027", "US-029", "US-030", "US-031",
    "US-032", "US-033", "US-034", "US-035", "US-036", "US-038", "US-040", "US-042", "US-044",
    "US-045", "US-049",
    // batch E — concepts/rules, architecture, investigations
    "US-050", "US-051", "US-053", "US-054", "US-055", "US-056", "US-057", "US-058", "US-060",
    "US-065", "US-067", "US-068", "US-072", "US-074",
];

/// Total number of stories in the catalogue (147).
pub const STORY_COUNT: usize = STORIES.len();

/// Format one story as the CLI `show` body.
pub fn format_story(s: &Story) -> String {
    format!(
        "{} — {}\nSection: {}\nAs: {}\n\n{}\n\nExpected experience: {}\n\nComplementary view: {}",
        s.id, s.title, s.section, s.actor, s.narrative, s.expected, s.view
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_count_matches_catalogue() {
        assert_eq!(STORY_COUNT, 147);
    }

    #[test]
    fn deep_check_ids_all_have_checks() {
        for id in DEEP_CHECKS {
            assert!(
                checks::deep_check(id).is_some(),
                "DEEP_CHECKS lists {id} but dispatch returns None"
            );
        }
    }

    /// Every deep check must pass against the real engine. One failure names
    /// the story and the honest error — no check is skipped silently.
    #[test]
    fn all_deep_checks_pass() {
        let mut failures = vec![];
        for id in DEEP_CHECKS {
            match checks::deep_check(id) {
                Some(Ok(())) => {}
                Some(Err(e)) => failures.push(format!("{id}: {e}")),
                None => failures.push(format!("{id}: dispatch returned None")),
            }
        }
        assert!(
            failures.is_empty(),
            "deep check failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn format_story_roundtrip_fields() {
        let s = Story::by_id("US-001").unwrap();
        let f = format_story(s);
        assert!(f.contains("US-001 — Connect a repository"));
        assert!(f.contains("Repository access and orientation"));
        assert!(f.contains("developer"));
    }
}
