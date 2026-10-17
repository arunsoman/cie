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
    fn format_story_roundtrip_fields() {
        let s = Story::by_id("US-001").unwrap();
        let f = format_story(s);
        assert!(f.contains("US-001 — Connect a repository"));
        assert!(f.contains("Repository access and orientation"));
        assert!(f.contains("developer"));
    }
}
