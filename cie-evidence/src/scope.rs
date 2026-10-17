//! Release scope freeze + stakeholder lenses (CIE-US US-127 / US-128
//! engine close).
//!
//! US-127 — milestone-frozen scope: items in scope at freeze are
//! trusted; items appearing AFTER the freeze are marked for assessment
//! (never silently admitted); committed-vs-stretch counts are exposed;
//! the scope replays from its own recorded history (append-only events).
//!
//! US-128 — one evidence set through role lenses: every lens sorts by
//! what blocks its concern first, keeping pass / attention / blocked /
//! not-determined distinct, with the overall tone stated plainly.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::readiness::{ReadinessCheck, ReadinessSection, ReadinessStatus};

// ------------------------------------------------------------ US-127

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeEventKind {
    Freeze,
    Commit,
    Stretch,
    LateAddition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeEvent {
    pub kind: ScopeEventKind,
    pub item: String,
    pub at_ns: u64,
    /// For late additions: the assessment verdict, once assessed.
    pub assessment: Option<String>,
}

/// The frozen scope manifest with its replayable history.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeFreeze {
    pub milestone: String,
    pub frozen_ns: Option<u64>,
    pub committed: BTreeSet<String>,
    pub stretch: BTreeSet<String>,
    /// Items that appeared after the freeze — marked, never admitted.
    pub late_additions: BTreeSet<String>,
    pub history: Vec<ScopeEvent>,
}

impl ScopeFreeze {
    pub fn new(milestone: &str) -> Self {
        ScopeFreeze {
            milestone: milestone.to_string(),
            ..Default::default()
        }
    }

    /// Freeze: everything already committed/stretch is trusted; later
    /// arrivals go through late-addition assessment.
    pub fn freeze(&mut self, at_ns: u64) {
        self.frozen_ns = Some(at_ns);
        self.history.push(ScopeEvent {
            kind: ScopeEventKind::Freeze,
            item: self.milestone.clone(),
            at_ns,
            assessment: None,
        });
    }

    pub fn commit(&mut self, item: &str, at_ns: u64) {
        let frozen = self.frozen_ns.is_some() && at_ns > self.frozen_ns.unwrap_or(0);
        if frozen {
            self.late_additions.insert(item.to_string());
            self.history.push(ScopeEvent {
                kind: ScopeEventKind::LateAddition,
                item: item.to_string(),
                at_ns,
                assessment: None,
            });
        } else {
            self.committed.insert(item.to_string());
            self.history.push(ScopeEvent {
                kind: ScopeEventKind::Commit,
                item: item.to_string(),
                at_ns,
                assessment: None,
            });
        }
    }

    pub fn mark_stretch(&mut self, item: &str, at_ns: u64) {
        self.stretch.insert(item.to_string());
        self.history.push(ScopeEvent {
            kind: ScopeEventKind::Stretch,
            item: item.to_string(),
            at_ns,
            assessment: None,
        });
    }

    /// Assess a late addition: admit to committed (with the recorded
    /// assessment) or keep out. The drift warning counts stay honest.
    pub fn assess_late(&mut self, item: &str, verdict: &str, admit: bool, at_ns: u64) -> bool {
        if !self.late_additions.remove(item) {
            return false;
        }
        if admit {
            self.committed.insert(item.to_string());
        }
        self.history.push(ScopeEvent {
            kind: ScopeEventKind::LateAddition,
            item: item.to_string(),
            at_ns,
            assessment: Some(format!(
                "{verdict} — {}",
                if admit { "admitted (scope drift)" } else { "kept out" }
            )),
        });
        true
    }

    /// Committed vs stretch counts + the drift warning data.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.committed.len(), self.stretch.len(), self.late_additions.len())
    }

    /// Replay the history into a fresh manifest (the "replayed from its
    /// own recorded history" obligation — replay is total, not partial).
    pub fn replay(&self) -> ScopeFreeze {
        let mut out = ScopeFreeze::new(&self.milestone);
        for e in &self.history {
            out.history.push(e.clone());
            match e.kind {
                ScopeEventKind::Freeze => out.frozen_ns = Some(e.at_ns),
                ScopeEventKind::Commit => {
                    out.committed.insert(e.item.clone());
                }
                ScopeEventKind::Stretch => {
                    out.stretch.insert(e.item.clone());
                }
                ScopeEventKind::LateAddition => match &e.assessment {
                    None => {
                        out.late_additions.insert(e.item.clone());
                    }
                    Some(a) if a.contains("admitted") => {
                        out.late_additions.remove(&e.item);
                        out.committed.insert(e.item.clone());
                    }
                    Some(_) => {
                        out.late_additions.remove(&e.item);
                    }
                },
            }
        }
        out
    }
}

// ------------------------------------------------------------ US-128

/// Stakeholder lens (role → which sections block its concern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lens {
    Security,
    Quality,
    Operations,
    Product,
}

impl Lens {
    /// Section priority for the lens (blocking-first ordering).
    fn priority(&self) -> [ReadinessSection; 5] {
        match self {
            Lens::Security => [
                ReadinessSection::Security,
                ReadinessSection::Dependency,
                ReadinessSection::Operational,
                ReadinessSection::Functional,
                ReadinessSection::Performance,
            ],
            Lens::Quality => [
                ReadinessSection::Functional,
                ReadinessSection::Performance,
                ReadinessSection::Security,
                ReadinessSection::Dependency,
                ReadinessSection::Operational,
            ],
            Lens::Operations => [
                ReadinessSection::Operational,
                ReadinessSection::Performance,
                ReadinessSection::Dependency,
                ReadinessSection::Security,
                ReadinessSection::Functional,
            ],
            Lens::Product => [
                ReadinessSection::Functional,
                ReadinessSection::Security,
                ReadinessSection::Operational,
                ReadinessSection::Dependency,
                ReadinessSection::Performance,
            ],
        }
    }
}

/// One lens view: worst-first rows + the stated overall tone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensView {
    pub lens: &'static str,
    /// Rows sorted blocking-first for this lens's concern ordering;
    /// pass/attention/blocked/not_determined stay distinct.
    pub rows: Vec<ReadinessCheck>,
    /// ready | conditional | blocked | unknown — stated plainly.
    pub tone: &'static str,
}

fn rank(status: ReadinessStatus) -> u8 {
    match status {
        ReadinessStatus::Blocked => 0,
        ReadinessStatus::NotDetermined => 1,
        ReadinessStatus::Attention => 2,
        ReadinessStatus::Pass => 3,
    }
}

/// View the evidence set through one lens (US-128). Deterministic:
/// status rank first (worst first), then the lens's section priority,
/// then check name.
pub fn lens_view(checks: &[ReadinessCheck], lens: Lens) -> LensView {
    let prio = lens.priority();
    let section_rank = |s: ReadinessSection| -> usize {
        prio.iter().position(|p| *p == s).unwrap_or(prio.len())
    };
    let mut rows: Vec<ReadinessCheck> = checks.to_vec();
    rows.sort_by(|a, b| {
        rank(a.status)
            .cmp(&rank(b.status))
            .then(section_rank(a.section).cmp(&section_rank(b.section)))
            .then(a.check.cmp(&b.check))
    });
    let worst = checks
        .iter()
        .map(|c| c.status)
        .min_by_key(|s| rank(*s))
        .unwrap_or(ReadinessStatus::NotDetermined);
    let tone = match worst {
        ReadinessStatus::Blocked => "blocked",
        ReadinessStatus::NotDetermined => "unknown",
        ReadinessStatus::Attention => "conditional",
        ReadinessStatus::Pass => "ready",
    };
    LensView {
        lens: match lens {
            Lens::Security => "security",
            Lens::Quality => "quality",
            Lens::Operations => "operations",
            Lens::Product => "product",
        },
        rows,
        tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readiness::ReadinessCheck;

    fn check(section: ReadinessSection, check: &str, status: ReadinessStatus) -> ReadinessCheck {
        ReadinessCheck {
            section,
            check: check.into(),
            status,
            missing: None,
            waiver: None,
        }
    }

    #[test]
    fn late_additions_are_marked_and_assessed_never_silent() {
        let mut f = ScopeFreeze::new("m1");
        f.commit("feat-a", 10);
        f.mark_stretch("feat-b", 20);
        f.freeze(100);
        // post-freeze arrivals are marked for assessment
        f.commit("feat-late", 200);
        f.commit("feat-later", 300);
        let (c, s, l) = f.counts();
        assert_eq!((c, s, l), (1, 1, 2));
        assert!(f.late_additions.contains("feat-late"));
        // assessment admits one with a recorded drift note
        assert!(f.assess_late("feat-late", "small, reviewed", true, 400));
        assert!(f.assess_late("feat-later", "too big", false, 500));
        let (c, _, _) = f.counts();
        assert_eq!(c, 2);
        assert!(f.history.iter().any(|e| {
            e.assessment.as_deref().map(|a| a.contains("scope drift")).unwrap_or(false)
        }));
        // replay reconstructs the same state from history alone
        let r = f.replay();
        assert_eq!(r, f);
    }

    #[test]
    fn security_lens_sorts_blocked_security_first() {
        let checks = vec![
            check(ReadinessSection::Functional, "e2e-suite", ReadinessStatus::Blocked),
            check(ReadinessSection::Security, "sast-scan", ReadinessStatus::Blocked),
            check(ReadinessSection::Performance, "load-profile", ReadinessStatus::NotDetermined),
            check(ReadinessSection::Functional, "unit-suite", ReadinessStatus::Pass),
        ];
        let sec = lens_view(&checks, Lens::Security);
        assert_eq!(sec.tone, "blocked");
        assert_eq!(sec.rows[0].check, "sast-scan"); // security lens: security blocked first
        let qual = lens_view(&checks, Lens::Quality);
        assert_eq!(qual.rows[0].check, "e2e-suite"); // quality lens: functional first
        // four states stay distinct in the rows
        let statuses: BTreeSet<&'static str> = sec.rows.iter().map(|r| r.status.as_str()).collect();
        assert!(statuses.contains("pass") && statuses.contains("blocked"));
        let unknown_only = vec![check(ReadinessSection::Performance, "x", ReadinessStatus::NotDetermined)];
        assert_eq!(lens_view(&unknown_only, Lens::Product).tone, "unknown");
    }
}
