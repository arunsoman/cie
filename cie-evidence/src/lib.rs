//! cie-evidence — the §12 evidence/coverage substrate (TASK-023): assembly
//! of `Evidence` records from extracted projects and analysis outputs.
//! The scalar §14.8 shapes (`Evidence`, `Coverage`) live in `cie-core`
//! (the §9 protocol needs them cycle-free); this crate re-exports them and
//! adds the fact-dependent builders.

pub mod environment;
pub mod profile;
pub mod readiness;
pub mod runtime;
pub mod workload;

pub use environment::{compare_environments, explore_flag_change, EnvironmentComparison, EnvironmentSnapshot, FlagDifference};
pub use runtime::{join_structure_with_runtime, JoinExactness, RuntimeObservation, StructureRuntimeJoin};

pub use profile::{
    attribute_hotspots, category_breakdown, compare_profiles, regression_report,
    CategoryBreakdown, ComparisonVerdict, EquivalenceDeclaration, Hotspot, HotspotAttribution,
    Profile, ProfileComparison, ProfileSample, RegressionReport, SampleCategory,
};

pub use readiness::{
    capacity_outcome, external_verdicts, overall_tone, readiness_rollup, Calibration,
    CapacityModel, CapacityOutcome, CapacityScenario, ExpectedResultSource, ExternalCheck,
    ExternalVerdict, ModelState, ObservedBucket, ReadinessCheck, ReadinessSection,
    ReadinessStatus, Waiver,
};
pub use workload::{
    classify_claim, ClaimClass, ClassifiedCandidate, PerfCandidate, TimingEntry, Workload,
    WorkloadManifest,
};
pub use cie_core::evidence::{Coverage, Evidence};

use cie_core::types::SymbolId;
use cie_semantic::facts::LangFacts;
use std::collections::BTreeMap;

/// Assemble the §14.8 evidence record for one extraction pass (FR-4: an
/// output cannot be constructed without it). Unresolved and unsupported
/// notes are truncated to keep evidence bounded (§15.5 hot-path budget).
pub fn assemble(
    rev: cie_core::types::RevId,
    provenance: impl Into<String>,
    now: cie_core::types::Ns,
    per_file: &[LangFacts],
    unresolved: &BTreeMap<SymbolId, Vec<String>>,
    limit: usize,
) -> Evidence {
    let mut ev = Evidence::new(rev, provenance, now);
    for f in per_file {
        for u in &f.unsupported {
            if ev.unsupported.len() >= limit {
                break;
            }
            ev.unsupported.push(format!("{}: {}", f.path, u));
        }
        for (sym, reasons) in unresolved {
            for u in reasons {
                if ev.unresolved.len() >= limit {
                    break;
                }
                ev.unresolved.push(format!("symbol {} — {}", sym.raw(), u));
            }
        }
    }
    ev
}

#[test]
fn assemble_bounds_and_paths() {
    let mut f = LangFacts::default();
    f.path = "a.py".into();
    f.unsupported.push("walrus".into());
    let mut unresolved = std::collections::BTreeMap::new();
    unresolved.insert(cie_core::types::SymbolId(3), vec!["dyn_call".to_string()]);
    let ev = assemble(7, "test", 1, &[f], &unresolved, 200);
    assert_eq!(ev.rev, 7);
    assert!(ev.unsupported[0].starts_with("a.py:"));
    assert!(ev.unresolved[0].starts_with("symbol 3"));
}
