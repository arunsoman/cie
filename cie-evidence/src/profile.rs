//! Imported profile hotspot attribution (CIE-US US-047 close).
//!
//! Profiles are imported, never fabricated: every sample traces to a
//! declared symbol name and a measured weight, with an explicit
//! uncertainty derived from what the importer actually dropped. A sample
//! whose stack frame does not resolve to a known symbol stays
//! unattributed rather than guessed onto the nearest known frame.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a profiled sample's time went. Kept open-ended rather than a
/// closed relation kind (US-082 will distinguish these explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SampleCategory {
    Cpu,
    Wait,
    DataWork,
    ExternalCall,
    /// Collected but the profiler could not classify it — stays visible,
    /// never folded into Cpu by default.
    Unclassified,
}

/// One imported sample: an optional resolved symbol, its category, and
/// its weight in the profiler's own units (samples, nanoseconds, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSample {
    pub symbol: Option<String>,
    pub category: SampleCategory,
    pub weight: u64,
}

/// An imported profile: what was collected and what was dropped, so
/// "attributed" never silently means "everything that was measured".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub source: String,
    pub samples: Vec<ProfileSample>,
    /// Samples the importer saw but could not parse or attribute.
    pub dropped_samples: u64,
    /// Measured wall time for the profiled window, if the importer
    /// captured one (optional: never fabricated when absent).
    pub duration_ns: Option<u64>,
    /// Failed/errored executions observed in the same window — kept in
    /// the population, never dropped to make the profile look cleaner.
    pub error_count: u64,
}

impl Profile {
    pub fn total_weight(&self) -> u64 {
        self.samples.iter().map(|s| s.weight).sum()
    }
}

/// One symbol's measured share with an honest uncertainty half-width.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hotspot {
    pub symbol: String,
    pub category: SampleCategory,
    /// Share of *attributed* weight (resolved samples only).
    pub share: f64,
    /// Half-width derived from the dropped-sample ratio — not a
    /// statistical confidence interval, and labelled as such by callers.
    pub share_uncertainty: f64,
}

/// Full attribution result: hotspots plus what stayed unknown.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HotspotAttribution {
    pub hotspots: Vec<Hotspot>,
    /// Share of attributed weight whose stack frame had no resolved symbol.
    pub unattributed_share: f64,
    /// Share of the *whole* collection window that the importer dropped.
    pub withheld_share: f64,
}

/// Attribute hotspots from an imported profile (US-047). An empty profile
/// (no samples collected) returns an empty, zeroed report — never a
/// fabricated hotspot.
pub fn attribute_hotspots(profile: &Profile) -> HotspotAttribution {
    let total = profile.total_weight();
    if total == 0 {
        return HotspotAttribution::default();
    }
    let mut by_symbol: BTreeMap<(String, SampleCategory), u64> = BTreeMap::new();
    let mut unattributed = 0u64;
    for s in &profile.samples {
        match &s.symbol {
            Some(sym) => *by_symbol.entry((sym.clone(), s.category)).or_insert(0) += s.weight,
            None => unattributed += s.weight,
        }
    }
    let grand = total + profile.dropped_samples;
    let withheld_share = profile.dropped_samples as f64 / grand.max(1) as f64;
    let mut hotspots: Vec<Hotspot> = by_symbol
        .into_iter()
        .map(|((symbol, category), w)| Hotspot {
            symbol,
            category,
            share: w as f64 / total as f64,
            // the dropped-sample ratio is the stated half-width: more
            // drops, less confidence every share deserves.
            share_uncertainty: withheld_share,
        })
        .collect();
    hotspots.sort_by(|a, b| b.share.partial_cmp(&a.share).unwrap().then(a.symbol.cmp(&b.symbol)));
    HotspotAttribution {
        hotspots,
        unattributed_share: unattributed as f64 / total as f64,
        withheld_share,
    }
}

/// Where a profiled window's time went, by category, plus what was never
/// observed at all (US-082: CPU, waits, data work, external calls and
/// missing observations distinguished — never collapsed into one "slow").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryBreakdown {
    /// Category → share of the *whole* collection window (collected + dropped).
    pub shares: BTreeMap<SampleCategory, f64>,
    /// Share of the window the importer never collected at all.
    pub missing_share: f64,
}

/// Break an imported profile down by category (US-082). Dropped samples
/// count toward `missing_share`, never toward any category — a dropped
/// sample has no known category, so guessing one would be a fabrication.
pub fn category_breakdown(profile: &Profile) -> CategoryBreakdown {
    let total = profile.total_weight();
    let grand = total + profile.dropped_samples;
    if grand == 0 {
        return CategoryBreakdown::default();
    }
    let mut shares: BTreeMap<SampleCategory, f64> = BTreeMap::new();
    for s in &profile.samples {
        *shares.entry(s.category).or_insert(0.0) += s.weight as f64 / grand as f64;
    }
    CategoryBreakdown {
        shares,
        missing_share: profile.dropped_samples as f64 / grand as f64,
    }
}

/// Conditions two profiles must meet before "faster"/"slower" is allowed
/// to be stated (US-083). A failed declaration rejects the comparison
/// with named reasons rather than comparing anyway.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EquivalenceDeclaration {
    pub same_workload: bool,
    pub same_build: bool,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComparisonVerdict {
    Faster { improvement: f64 },
    Slower { regression: f64 },
    /// No verdict can be stated — `reasons` says why.
    Inconclusive,
}

/// The comparison result: a verdict with its reasons, so "faster" never
/// travels without the conditions and limitations behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileComparison {
    pub verdict: ComparisonVerdict,
    pub reasons: Vec<String>,
    /// True when the population itself was rejected (equivalence failed
    /// or one side has no comparable measurement) — distinct from an
    /// accepted population that simply shows no difference.
    pub rejected: bool,
}

/// Compare two imported profiles' measured duration under a declared
/// equivalence (US-083). Failed runs are never excluded from either
/// side's population before comparing — the caller passes the profile
/// exactly as collected, error_count and all.
pub fn compare_profiles(
    baseline: &Profile,
    candidate: &Profile,
    equivalence: &EquivalenceDeclaration,
) -> ProfileComparison {
    if !equivalence.same_workload || !equivalence.same_build {
        return ProfileComparison {
            verdict: ComparisonVerdict::Inconclusive,
            reasons: vec![format!("declared equivalence failed: {}", equivalence.notes)],
            rejected: true,
        };
    }
    let (Some(b_ns), Some(c_ns)) = (baseline.duration_ns, candidate.duration_ns) else {
        return ProfileComparison {
            verdict: ComparisonVerdict::Inconclusive,
            reasons: vec!["at least one profile has no measured duration — not fabricated".into()],
            rejected: true,
        };
    };
    if b_ns == 0 {
        return ProfileComparison {
            verdict: ComparisonVerdict::Inconclusive,
            reasons: vec!["baseline duration is zero — cannot compute a ratio".into()],
            rejected: true,
        };
    }
    let delta = (b_ns as f64 - c_ns as f64) / b_ns as f64;
    let verdict = if delta > 0.0 {
        ComparisonVerdict::Faster { improvement: delta }
    } else if delta < 0.0 {
        ComparisonVerdict::Slower { regression: -delta }
    } else {
        ComparisonVerdict::Inconclusive
    };
    let mut reasons = vec![format!("baseline {b_ns}ns vs candidate {c_ns}ns under declared-equivalent conditions")];
    if delta == 0.0 {
        reasons.push("no measured difference".into());
    }
    ProfileComparison { verdict, reasons, rejected: false }
}

/// Before/after regression verdict (US-084): a candidate that looks
/// faster by dropping work or hiding errors can never be presented as an
/// improvement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegressionReport {
    pub comparison: ProfileComparison,
    pub baseline_errors: u64,
    pub candidate_errors: u64,
    pub baseline_dropped: u64,
    pub candidate_dropped: u64,
    /// True only when the comparison says Faster *and* neither errors nor
    /// dropped samples increased on the candidate side.
    pub improvement_confirmed: bool,
}

/// Investigate a performance regression between a baseline and a
/// candidate run under a declared equivalence (US-084).
pub fn regression_report(
    baseline: &Profile,
    candidate: &Profile,
    equivalence: &EquivalenceDeclaration,
) -> RegressionReport {
    let comparison = compare_profiles(baseline, candidate, equivalence);
    let improvement_confirmed = matches!(comparison.verdict, ComparisonVerdict::Faster { .. })
        && candidate.error_count <= baseline.error_count
        && candidate.dropped_samples <= baseline.dropped_samples;
    RegressionReport {
        comparison,
        baseline_errors: baseline.error_count,
        candidate_errors: candidate.error_count,
        baseline_dropped: baseline.dropped_samples,
        candidate_dropped: candidate.dropped_samples,
        improvement_confirmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(sym: Option<&str>, cat: SampleCategory, w: u64) -> ProfileSample {
        ProfileSample { symbol: sym.map(str::to_string), category: cat, weight: w }
    }

    fn timed_profile(ns: u64) -> Profile {
        Profile { duration_ns: Some(ns), ..Default::default() }
    }

    fn equivalent() -> EquivalenceDeclaration {
        EquivalenceDeclaration { same_workload: true, same_build: true, notes: String::new() }
    }

    #[test]
    fn attributes_shares_by_symbol_and_category() {
        let p = Profile {
            source: "pprof".into(),
            samples: vec![
                sample(Some("hot_fn"), SampleCategory::Cpu, 70),
                sample(Some("cold_fn"), SampleCategory::Cpu, 30),
            ],
            dropped_samples: 0,
            duration_ns: None,
            error_count: 0,
        };
        let rep = attribute_hotspots(&p);
        assert_eq!(rep.hotspots[0].symbol, "hot_fn");
        assert!((rep.hotspots[0].share - 0.7).abs() < 1e-9);
        assert_eq!(rep.unattributed_share, 0.0);
    }

    #[test]
    fn unresolved_frames_stay_unattributed_not_guessed() {
        let p = Profile {
            source: "pprof".into(),
            samples: vec![sample(Some("hot_fn"), SampleCategory::Cpu, 50), sample(None, SampleCategory::Cpu, 50)],
            dropped_samples: 0,
            duration_ns: None,
            error_count: 0,
        };
        let rep = attribute_hotspots(&p);
        assert_eq!(rep.hotspots.len(), 1);
        assert!((rep.unattributed_share - 0.5).abs() < 1e-9);
    }

    #[test]
    fn dropped_samples_widen_uncertainty_not_hidden() {
        let p = Profile {
            source: "pprof".into(),
            samples: vec![sample(Some("hot_fn"), SampleCategory::Cpu, 90)],
            dropped_samples: 10,
            duration_ns: None,
            error_count: 0,
        };
        let rep = attribute_hotspots(&p);
        assert!((rep.withheld_share - 0.1).abs() < 1e-9);
        assert!((rep.hotspots[0].share_uncertainty - 0.1).abs() < 1e-9);
    }

    #[test]
    fn empty_profile_is_an_empty_report_not_a_fabricated_one() {
        let p = Profile::default();
        let rep = attribute_hotspots(&p);
        assert!(rep.hotspots.is_empty());
    }

    #[test]
    fn category_breakdown_distinguishes_cpu_wait_data_external_and_missing() {
        let p = Profile {
            source: "trace".into(),
            samples: vec![
                sample(Some("a"), SampleCategory::Cpu, 40),
                sample(Some("b"), SampleCategory::Wait, 20),
                sample(Some("c"), SampleCategory::DataWork, 20),
                sample(Some("d"), SampleCategory::ExternalCall, 10),
            ],
            dropped_samples: 10,
            duration_ns: None,
            error_count: 0,
        };
        let b = category_breakdown(&p);
        assert!((b.shares[&SampleCategory::Cpu] - 0.4).abs() < 1e-9);
        assert!((b.missing_share - 0.1).abs() < 1e-9);
        let sum: f64 = b.shares.values().sum::<f64>() + b.missing_share;
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dropped_samples_never_get_a_guessed_category() {
        let p = Profile {
            source: "trace".into(),
            samples: vec![sample(Some("a"), SampleCategory::Cpu, 50)],
            dropped_samples: 50,
            duration_ns: None,
            error_count: 0,
        };
        let b = category_breakdown(&p);
        assert_eq!(b.shares.len(), 1);
        assert!((b.missing_share - 0.5).abs() < 1e-9);
    }

    #[test]
    fn comparison_states_faster_with_reasons() {
        let eq = EquivalenceDeclaration { same_workload: true, same_build: true, notes: String::new() };
        let cmp = compare_profiles(&timed_profile(1000), &timed_profile(800), &eq);
        assert!(matches!(cmp.verdict, ComparisonVerdict::Faster { .. }));
        assert!(!cmp.rejected);
        assert!(!cmp.reasons.is_empty());
    }

    #[test]
    fn failed_equivalence_rejects_rather_than_comparing_anyway() {
        let eq = EquivalenceDeclaration { same_workload: false, same_build: true, notes: "different dataset size".into() };
        let cmp = compare_profiles(&timed_profile(1000), &timed_profile(800), &eq);
        assert!(cmp.rejected);
        assert!(matches!(cmp.verdict, ComparisonVerdict::Inconclusive));
        assert!(cmp.reasons[0].contains("different dataset size"));
    }

    #[test]
    fn missing_duration_on_either_side_is_inconclusive_not_fabricated() {
        let eq = EquivalenceDeclaration { same_workload: true, same_build: true, notes: String::new() };
        let cmp = compare_profiles(&Profile::default(), &timed_profile(800), &eq);
        assert!(cmp.rejected);
    }

    #[test]
    fn genuine_improvement_is_confirmed() {
        let base = timed_profile(1000);
        let cand = timed_profile(800);
        let rep = regression_report(&base, &cand, &equivalent());
        assert!(rep.improvement_confirmed);
    }

    #[test]
    fn faster_but_more_errors_is_not_confirmed_as_an_improvement() {
        let base = timed_profile(1000);
        let cand = Profile { duration_ns: Some(800), error_count: 5, ..Default::default() };
        let rep = regression_report(&base, &cand, &equivalent());
        assert!(matches!(rep.comparison.verdict, ComparisonVerdict::Faster { .. }));
        assert!(!rep.improvement_confirmed, "dropping into more errors is not an improvement");
    }

    #[test]
    fn faster_but_more_dropped_samples_is_not_confirmed() {
        let base = timed_profile(1000);
        let cand = Profile { duration_ns: Some(800), dropped_samples: 50, ..Default::default() };
        let rep = regression_report(&base, &cand, &equivalent());
        assert!(!rep.improvement_confirmed, "dropping work is not an improvement");
    }
}
