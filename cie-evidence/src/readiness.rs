//! Readiness, external evidence, capacity scenarios and calibration
//! (CIE-US US-119 / US-120 / US-100 / US-102 engine close).
//!
//! * [`ExternalEvidence`] (US-119) — typed import for independent check
//!   results: expected-result source, assertion strength and any
//!   unauthorized weakening are recorded, not averaged away; generated
//!   expectations count only after a human confirmation flag.
//! * [`ReadinessRollup`] (US-120) — one picture over security/dependency/
//!   performance/operational/functional checks; missing tools render as
//!   `not_determined` with the named tool; waivers are disclosed and
//!   never relabelled as passes.
//! * [`CapacityScenario`] (US-100) — what-if comparisons over budget
//!   knobs inside the *supported prediction domain* (linear queueing
//!   bound); unsupported scenarios refuse numbers and say why.
//! * [`Calibration`] (US-102) — observed histograms bound to a revision +
//!   environment; model states separate observed/modelled/assumed, and
//!   freshness is the capture timestamp.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ------------------------------------------------------ US-119 external

/// Source of a check's expected result (the independence question).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedResultSource {
    /// Derived from the implementation itself (assumption risk).
    ImplementationDerived,
    /// Independent specification, ticket or human oracle.
    Independent,
    /// Generated, unconfirmed — counts only after human confirmation.
    GeneratedUnconfirmed,
}

/// One imported external check result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCheck {
    pub check_id: String,
    pub source: String,
    pub expected_result_source: ExpectedResultSource,
    pub expected: String,
    pub observed: String,
    pub passed: bool,
    /// Weakened vs the original regression obligation? (recorded, and the
    /// pass is downgraded to `weakened` below when true.)
    pub assertion_weakened: bool,
    /// Human confirmed a generated expectation? (generated-only gates).
    pub human_confirmed: bool,
    /// Revision the check ran against.
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalVerdict {
    Pass,
    WeakenedPass,
    Failed,
    Unconfirmed,
}

/// Merge external checks into honest verdicts (US-119).
pub fn external_verdicts(checks: &[ExternalCheck]) -> Vec<(String, ExternalVerdict)> {
    checks
        .iter()
        .map(|c| {
            let v = match c.expected_result_source {
                ExpectedResultSource::ImplementationDerived => {
                    if c.passed && !c.assertion_weakened {
                        ExternalVerdict::Pass
                    } else if c.passed {
                        ExternalVerdict::WeakenedPass
                    } else {
                        ExternalVerdict::Failed
                    }
                }
                ExpectedResultSource::Independent => {
                    if c.passed && !c.assertion_weakened {
                        ExternalVerdict::Pass
                    } else if c.passed {
                        ExternalVerdict::WeakenedPass
                    } else {
                        ExternalVerdict::Failed
                    }
                }
                ExpectedResultSource::GeneratedUnconfirmed => {
                    if !c.human_confirmed {
                        ExternalVerdict::Unconfirmed
                    } else if c.passed && !c.assertion_weakened {
                        ExternalVerdict::Pass
                    } else if c.passed {
                        ExternalVerdict::WeakenedPass
                    } else {
                        ExternalVerdict::Failed
                    }
                }
            };
            (c.check_id.clone(), v)
        })
        .collect()
}

// ------------------------------------------------------ US-120 readiness

/// Four-state readiness (unknowns appear as unknowns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    Pass,
    Attention,
    Blocked,
    NotDetermined,
}

impl ReadinessStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReadinessStatus::Pass => "pass",
            ReadinessStatus::Attention => "attention",
            ReadinessStatus::Blocked => "blocked",
            ReadinessStatus::NotDetermined => "not_determined",
        }
    }
}

/// One readiness check row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessCheck {
    pub section: ReadinessSection,
    pub check: String,
    pub status: ReadinessStatus,
    /// For not_determined: the missing tool / nonrepresentative
    /// environment, named.
    pub missing: Option<String>,
    /// True when a waiver exists — disclosed, never relabelled as pass.
    pub waiver: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadinessSection {
    Security,
    Dependency,
    Performance,
    Operational,
    Functional,
}

/// A waiver record (separate from checks, so relabelling is impossible).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    pub check: String,
    pub reason: String,
    pub owner: String,
}

/// Assemble the readiness rollup (US-120): statuses are respected as
/// given; waivers attach as disclosure; the overall tone is the worst
/// status present (blocked > not_determined > attention > pass ordering
/// for severity: blocked worst, then not_determined, then attention).
pub fn readiness_rollup(checks: &[ReadinessCheck], waivers: &[Waiver]) -> BTreeMap<String, Vec<ReadinessCheck>> {
    let mut out: BTreeMap<String, Vec<ReadinessCheck>> = BTreeMap::new();
    for c in checks {
        let mut row = c.clone();
        if let Some(w) = waivers.iter().find(|w| w.check == c.check) {
            row.waiver = Some(format!("waiver by {}: {}", w.owner, w.reason));
            // a waiver NEVER upgrades the status; it only discloses
        }
        out.entry(format!("{:?}", row.section).to_lowercase())
            .or_default()
            .push(row);
    }
    out
}

/// Overall tone: worst status wins (blocked > not_determined > attention
/// > pass). Empty input is `not_determined` — unknowns stay unknowns.
pub fn overall_tone(checks: &[ReadinessCheck]) -> ReadinessStatus {
    if checks.is_empty() {
        return ReadinessStatus::NotDetermined;
    }
    checks
        .iter()
        .map(|c| c.status)
        .max()
        .unwrap_or(ReadinessStatus::NotDetermined)
}

// ------------------------------------------------ US-100 capacity sandbox

/// The supported prediction domain: linear queueing bound on a single
/// resource. Outside it the engine refuses numbers (exploratory only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityModel {
    /// Measured/assumed service capacity (requests per second).
    pub capacity_rps: f64,
    /// Source of the capacity number (measured workload or assumption).
    pub capacity_provenance: String,
}

/// A what-if scenario over traffic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityScenario {
    pub name: String,
    pub offered_rps: f64,
    /// Failure assumption text (e.g. "one replica down").
    pub failure_assumption: String,
    /// Remaining capacity fraction after the failure assumption.
    pub remaining_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapacityOutcome {
    pub scenario: String,
    /// Predicted headroom ratio (capacity/offered) — linear domain only.
    pub headroom_ratio: f64,
    /// Utilisation estimate 0..1 (offered / effective capacity).
    pub utilisation: f64,
    /// Label: always `exploratory prediction (linear domain)`.
    pub label: &'static str,
    /// Unknowns the caller must display (failure assumptions, provenance).
    pub unknowns: Vec<String>,
}

/// Evaluate a scenario inside the supported domain. `None` when the
/// scenario is outside it (non-positive numbers) — the engine refuses to
/// invent numbers for unsupported inputs.
pub fn capacity_outcome(model: &CapacityModel, scenario: &CapacityScenario) -> Option<CapacityOutcome> {
    if model.capacity_rps <= 0.0 || scenario.offered_rps <= 0.0 {
        return None;
    }
    let frac = scenario.remaining_fraction.clamp(0.0, 1.0);
    let effective = model.capacity_rps * frac;
    Some(CapacityOutcome {
        scenario: scenario.name.clone(),
        headroom_ratio: effective / scenario.offered_rps,
        utilisation: (scenario.offered_rps / effective).min(1.0),
        label: "exploratory prediction (linear domain)",
        unknowns: vec![
            format!("capacity provenance: {}", model.capacity_provenance),
            format!("failure assumption: {}", scenario.failure_assumption),
            "single-resource linear model; no queueing dynamics".into(),
        ],
    })
}

// ------------------------------------------------- US-102 calibration

/// One observed histogram bucket (imported from a runtime feed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedBucket {
    pub name: String,
    pub lower_ns: u64,
    pub upper_ns: u64,
    pub count: u64,
}

/// A calibration record: observations bound to revision + environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calibration {
    pub revision: u64,
    pub environment: String,
    pub captured_at_ns: u64,
    pub buckets: Vec<ObservedBucket>,
    pub source: String,
}

impl Calibration {
    /// Is a modelled state observed (calibrated), assumed, or unmodelled?
    pub fn state_for(&self, state: &str) -> ModelState {
        let total: u64 = self.buckets.iter().map(|b| b.count).sum();
        if total == 0 {
            return ModelState::Unmodelled;
        }
        if self.buckets.iter().any(|b| b.name == state) {
            ModelState::Observed
        } else {
            ModelState::Assumed
        }
    }

    /// Freshness: capture recency relative to a comparison timestamp.
    pub fn staleness_ns(&self, now_ns: u64) -> u64 {
        now_ns.saturating_sub(self.captured_at_ns)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelState {
    Observed,
    Assumed,
    Unmodelled,
}

impl ModelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelState::Observed => "observed",
            ModelState::Assumed => "assumed",
            ModelState::Unmodelled => "unmodelled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_weakening_and_generation_are_visible_verdicts() {
        let checks = vec![
            ExternalCheck {
                check_id: "C1".into(),
                source: "qa-harness".into(),
                expected_result_source: ExpectedResultSource::Independent,
                expected: "201".into(),
                observed: "201".into(),
                passed: true,
                assertion_weakened: false,
                human_confirmed: false,
                revision: 41,
            },
            ExternalCheck {
                check_id: "C2".into(),
                source: "qa-harness".into(),
                expected_result_source: ExpectedResultSource::Independent,
                expected: "full list".into(),
                observed: "first page".into(),
                passed: true,
                assertion_weakened: true,
                human_confirmed: false,
                revision: 41,
            },
            ExternalCheck {
                check_id: "C3".into(),
                source: "gen".into(),
                expected_result_source: ExpectedResultSource::GeneratedUnconfirmed,
                expected: "auto".into(),
                observed: "auto".into(),
                passed: true,
                assertion_weakened: false,
                human_confirmed: false,
                revision: 41,
            },
            ExternalCheck {
                check_id: "C4".into(),
                source: "gen".into(),
                expected_result_source: ExpectedResultSource::GeneratedUnconfirmed,
                expected: "auto".into(),
                observed: "auto".into(),
                passed: true,
                assertion_weakened: false,
                human_confirmed: true,
                revision: 41,
            },
        ];
        let v = external_verdicts(&checks);
        assert_eq!(v[0].1, ExternalVerdict::Pass);
        assert_eq!(v[1].1, ExternalVerdict::WeakenedPass);
        assert_eq!(v[2].1, ExternalVerdict::Unconfirmed); // generated, unconfirmed
        assert_eq!(v[3].1, ExternalVerdict::Pass); // human confirmed
    }

    #[test]
    fn waiver_discloses_without_upgrading_status() {
        let checks = vec![ReadinessCheck {
            section: ReadinessSection::Security,
            check: "sast-scan".into(),
            status: ReadinessStatus::Blocked,
            missing: Some("scanner not installed".into()),
            waiver: None,
        }];
        let waivers = vec![Waiver {
            check: "sast-scan".into(),
            reason: "accepted for this milestone".into(),
            owner: "release-lead".into(),
        }];
        let rollup = readiness_rollup(&checks, &waivers);
        let row = &rollup["security"][0];
        assert_eq!(row.status, ReadinessStatus::Blocked); // NOT relabelled
        assert!(row.waiver.as_deref().unwrap().contains("release-lead"));
        assert_eq!(overall_tone(&checks), ReadinessStatus::Blocked);
        assert_eq!(overall_tone(&[]), ReadinessStatus::NotDetermined);
    }

    #[test]
    fn missing_tool_is_not_determined_with_named_tool() {
        let checks = vec![ReadinessCheck {
            section: ReadinessSection::Performance,
            check: "load-profile".into(),
            status: ReadinessStatus::NotDetermined,
            missing: Some("load-profile tool absent".into()),
            waiver: None,
        }];
        assert_eq!(overall_tone(&checks), ReadinessStatus::NotDetermined);
    }

    #[test]
    fn capacity_refuses_unsupported_and_labels_predictions() {
        let model = CapacityModel {
            capacity_rps: 100.0,
            capacity_provenance: "measured: order_listing_bench".into(),
        };
        let ok = CapacityScenario {
            name: "2x traffic, one replica down".into(),
            offered_rps: 150.0,
            failure_assumption: "1 of 3 replicas down".into(),
            remaining_fraction: 2.0 / 3.0,
        };
        let out = capacity_outcome(&model, &ok).unwrap();
        // offered 150 rps vs effective capacity 66.7 rps → saturated
        assert!((out.utilisation - 1.0).abs() < 1e-9);
        assert!((out.headroom_ratio - (100.0 * 2.0 / 3.0) / 150.0).abs() < 1e-9);
        assert_eq!(out.label, "exploratory prediction (linear domain)");
        assert!(out.unknowns.iter().any(|u| u.contains("measured: order_listing_bench")));
        // unsupported: zero/negative inputs → no invented numbers
        let bad = CapacityScenario {
            name: "nonsense".into(),
            offered_rps: 0.0,
            failure_assumption: String::new(),
            remaining_fraction: 1.0,
        };
        assert!(capacity_outcome(&model, &bad).is_none());
    }

    #[test]
    fn calibration_separates_observed_assumed_unmodelled_with_freshness() {
        let cal = Calibration {
            revision: 41,
            environment: "staging".into(),
            captured_at_ns: 1_000,
            buckets: vec![ObservedBucket {
                name: "charging".into(),
                lower_ns: 0,
                upper_ns: 1_000,
                count: 5,
            }],
            source: "runtime feed import".into(),
        };
        assert_eq!(cal.state_for("charging"), ModelState::Observed);
        assert_eq!(cal.state_for("refunding"), ModelState::Assumed);
        let empty = Calibration { buckets: vec![], ..cal.clone() };
        assert_eq!(empty.state_for("charging"), ModelState::Unmodelled);
        assert_eq!(cal.staleness_ns(4_000), 3_000);
    }
}
