//! Coverage rollups over the registry — honest "matched so far" semantics.
//! Totals are counts of dispositions, never a percentage claimed as
//! completeness: a Partial is visibly partial, a Planned is visibly absent.

use crate::catalog::{Story, SECTIONS, STORIES};
use crate::registry::{Disposition, Entry, REGISTRY};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StoryCoverage {
    pub id: &'static str,
    pub title: &'static str,
    pub section: &'static str,
    pub disposition: Disposition,
    pub capabilities: &'static [&'static str],
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionRollup {
    pub section: &'static str,
    pub total: usize,
    pub delivered: usize,
    pub partial: usize,
    pub planned: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub sections: Vec<SectionRollup>,
    pub delivered: usize,
    pub partial: usize,
    pub planned: usize,
    pub total: usize,
}

impl CoverageReport {
    /// Build the full report. STORIES and REGISTRY are same-length and
    /// order-aligned (enforced by registry tests).
    pub fn build() -> Self {
        assert_eq!(STORIES.len(), REGISTRY.len());
        let mut sections: Vec<SectionRollup> = SECTIONS
            .iter()
            .map(|s| SectionRollup {
                section: s,
                total: 0,
                delivered: 0,
                partial: 0,
                planned: 0,
            })
            .collect();
        let index: std::collections::BTreeMap<&'static str, usize> =
            sections.iter().enumerate().map(|(i, s)| (s.section, i)).collect();
        let (mut d, mut p, mut pl) = (0usize, 0usize, 0usize);
        for story in STORIES {
            let e = crate::registry::entry_for(story.id).expect("registry covers every story");
            let si = *index.get(story.section).expect("section known");
            sections[si].total += 1;
            match e.disposition {
                Disposition::Delivered => {
                    sections[si].delivered += 1;
                    d += 1;
                }
                Disposition::Partial => {
                    sections[si].partial += 1;
                    p += 1;
                }
                Disposition::Planned => {
                    sections[si].planned += 1;
                    pl += 1;
                }
            }
        }
        CoverageReport {
            sections,
            delivered: d,
            partial: p,
            planned: pl,
            total: STORIES.len(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("coverage serializes")
    }

    /// Markdown rendering with the honesty preamble.
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# User-story coverage (engine capabilities)\n\n");
        out.push_str(
            "Dispositions count *engine-level* delivery. `partial` results stay \
             visibly partial; `planned` items are absent, not hidden. Percentages \
             are deliberately not shown — they would imply a completeness the \
             catalogue refuses to claim.\n\n",
        );
        out.push_str("| Section | Delivered | Partial | Planned | Total |\n|---|---|---|---|---|\n");
        for s in &self.sections {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                s.section, s.delivered, s.partial, s.planned, s.total
            ));
        }
        out.push_str(&format!(
            "\n**Totals** — delivered: {}, partial: {}, planned: {}, total: {}\n",
            self.delivered, self.partial, self.planned, self.total
        ));
        out
    }
}

/// Per-story coverage rows (for `cie stories list`).
pub fn all_story_coverage() -> Vec<StoryCoverage> {
    STORIES
        .iter()
        .map(|s: &Story| {
            let e: &Entry = crate::registry::entry_for(s.id).expect("registry covers every story");
            StoryCoverage {
                id: s.id,
                title: s.title,
                section: s.section,
                disposition: e.disposition,
                capabilities: e.capabilities,
                note: e.note,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_add_up() {
        let r = CoverageReport::build();
        assert_eq!(r.total, 147);
        assert_eq!(r.delivered + r.partial + r.planned, 147);
        let section_sum: usize = r.sections.iter().map(|s| s.total).sum();
        assert_eq!(section_sum, 147);
        for s in &r.sections {
            assert_eq!(s.delivered + s.partial + s.planned, s.total);
        }
    }

    #[test]
    fn json_roundtrip_has_totals() {
        let r = CoverageReport::build();
        let j = r.to_json();
        assert_eq!(j["total"], 147);
        assert_eq!(j["sections"].as_array().map(|a| a.len()), Some(20));
    }

    #[test]
    fn markdown_names_no_percentage() {
        let md = CoverageReport::build().render_markdown();
        assert!(!md.contains('%'), "coverage must not fake completeness");
        assert!(md.contains("visibly partial"));
    }
}
