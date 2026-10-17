//! Named-workload measurement protocol (CIE-US US-074 / US-085 close).
//!
//! The pattern classifiers ship performance *candidates* (repeated
//! per-element queries, I/O under locks, per-iteration locking). A
//! candidate becomes a **measured** claim only when a timing entry from a
//! named workload, build and revision supports it. This module is that
//! protocol:
//! * [`WorkloadManifest`] — imported workload definitions + timing
//!   entries with provenance (name, build, revision, measured_ns);
//! * [`classify_claim`] — split a candidate list into `measured` (backed
//!   by a manifest entry) and `potential` (no measurement yet, labelled
//!   as such — the US-085 "never a measured speedup without experiments"
//!   rule, enforced by construction);
//! * a correctness-constraints field travels with every candidate so an
//!   optimisation debate starts from what must not break.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One named workload (imported, not fabricated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workload {
    pub name: String,
    /// Build identifier the timings were taken on (e.g. `release-v1.2`).
    pub build: String,
    /// Revision the workload ran against.
    pub revision: u64,
    /// What the workload does (imported description).
    pub description: String,
}

/// One timing entry: measured on a named workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimingEntry {
    pub workload: String,
    /// Operation measured (e.g. `charge_hot_path`).
    pub op: String,
    pub measured_ns: u64,
    /// Where the number came from (bench harness, profiler export…).
    pub provenance: String,
}

/// The manifest: workloads + their timings, both imported with sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadManifest {
    pub workloads: BTreeMap<String, Workload>,
    pub timings: Vec<TimingEntry>,
}

impl WorkloadManifest {
    pub fn add_workload(&mut self, w: Workload) {
        self.workloads.insert(w.name.clone(), w);
    }

    pub fn add_timing(&mut self, t: TimingEntry) {
        self.timings.push(t);
    }

    /// Timing backing `op` on a named workload, if imported.
    pub fn timing_for(&self, workload: &str, op: &str) -> Option<&TimingEntry> {
        // a timing is only usable when its workload is defined too
        if !self.workloads.contains_key(workload) {
            return None;
        }
        self.timings
            .iter()
            .find(|t| t.workload == workload && t.op == op)
    }
}

/// A performance candidate as the pattern classifiers emit it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfCandidate {
    /// Pattern id (e.g. `n_plus_one`, `io_under_lock`).
    pub pattern: String,
    /// Where (symbol qualified name + line).
    pub site: String,
    pub line: u32,
    /// Affected behaviour description (US-085 obligation).
    pub affected_behaviour: String,
    /// Evidence the classifier cites.
    pub evidence: Vec<String>,
    /// Correctness constraints an optimisation must preserve.
    pub correctness_constraints: Vec<String>,
    /// When a measurement backs it: workload name.
    pub measured_on: Option<String>,
    /// When a measurement backs it: op name.
    pub measured_op: Option<String>,
}

/// The honest split (US-074): only manifest-backed candidates are
/// `measured`; the rest stay potential, labelled for what they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedCandidate {
    #[serde(flatten)]
    pub candidate: PerfCandidate,
    /// measured | potential
    pub claim_class: ClaimClass,
    /// For measured claims: the timing (with provenance) that backs it.
    pub measurement: Option<TimingEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimClass {
    Measured,
    Potential,
}

impl ClaimClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaimClass::Measured => "measured",
            ClaimClass::Potential => "potential",
        }
    }
}

/// Classify candidates against the manifest (US-074/US-085). A candidate
/// is measured iff its (workload, op) pair resolves to a timing entry
/// whose workload is defined. Everything else is potential — visibly.
pub fn classify_claim(candidates: &[PerfCandidate], manifest: &WorkloadManifest) -> Vec<ClassifiedCandidate> {
    candidates
        .iter()
        .map(|c| {
            let measurement = c
                .measured_on
                .as_ref()
                .zip(c.measured_op.as_ref())
                .and_then(|(w, o)| manifest.timing_for(w, o).cloned());
            ClassifiedCandidate {
                candidate: c.clone(),
                claim_class: if measurement.is_some() {
                    ClaimClass::Measured
                } else {
                    ClaimClass::Potential
                },
                measurement,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(measured: bool) -> PerfCandidate {
        PerfCandidate {
            pattern: "n_plus_one".into(),
            site: "shop::list_orders".into(),
            line: 42,
            affected_behaviour: "per-order query inside the listing loop".into(),
            evidence: vec!["call in_loop=true to DbQuery".into()],
            correctness_constraints: vec!["order totals stay consistent with items".into()],
            measured_on: if measured { Some("order_listing_bench".into()) } else { None },
            measured_op: if measured { Some("list_orders_hot".into()) } else { None },
        }
    }

    #[test]
    fn measured_requires_named_workload_and_timing() {
        let mut m = WorkloadManifest::default();
        m.add_workload(Workload {
            name: "order_listing_bench".into(),
            build: "release-v1.2".into(),
            revision: 41,
            description: "1000 orders, warm cache".into(),
        });
        m.add_timing(TimingEntry {
            workload: "order_listing_bench".into(),
            op: "list_orders_hot".into(),
            measured_ns: 1_500_000,
            provenance: "criterion bench @ release-v1.2".into(),
        });
        let out = classify_claim(&[candidate(true), candidate(false)], &m);
        assert_eq!(out[0].claim_class, ClaimClass::Measured);
        assert_eq!(out[0].measurement.as_ref().unwrap().measured_ns, 1_500_000);
        assert_eq!(out[1].claim_class, ClaimClass::Potential);
        assert!(out[1].measurement.is_none());
    }

    #[test]
    fn timing_without_defined_workload_is_not_a_measurement() {
        let mut m = WorkloadManifest::default();
        // timing present, workload never defined → orphan timing is not
        // usable (provenance discipline: the workload must be named AND
        // defined)
        m.add_timing(TimingEntry {
            workload: "ghost_bench".into(),
            op: "list_orders_hot".into(),
            measured_ns: 1,
            provenance: "unattributed".into(),
        });
        let out = classify_claim(&[candidate(true)], &m);
        assert_eq!(out[0].claim_class, ClaimClass::Potential);
    }

    #[test]
    fn potential_candidates_carry_constraints_not_speedup_claims() {
        let out = classify_claim(&[candidate(false)], &WorkloadManifest::default());
        let c = &out[0].candidate;
        assert_eq!(out[0].claim_class.as_str(), "potential");
        assert!(c.correctness_constraints.iter().any(|s| s.contains("consistent")));
        // no measured_ns anywhere in the potential classification
        assert!(serde_json::to_string(&out[0]).unwrap().contains("potential"));
        assert!(!serde_json::to_string(&out[0]).unwrap().contains("measured_ns"));
    }
}
