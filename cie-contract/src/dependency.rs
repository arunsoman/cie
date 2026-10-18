//! Dependency importance and upgrade risk (US-062). Usage evidence,
//! advisories, compatibility concerns and proposed replacement work are
//! kept as separate, honestly-labelled fields: an available fixed version
//! is never presented as automatically compatible with the application.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub current_version: String,
}

/// Where a dependency is actually used — never inferred from its name
/// alone, always a concrete usage site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvidence {
    pub dependency: String,
    pub using_symbol: String,
}

/// An imported advisory (CVE, deprecation notice, …) — never fabricated;
/// absence of an entry here means "none imported", not "none exist".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advisory {
    pub id: String,
    pub severity: String,
    pub summary: String,
}

/// Compatibility defaults to `Unknown` and only moves on explicit
/// evidence — a fixed/available version is never auto-promoted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityVerdict {
    #[default]
    Unknown,
    ConfirmedCompatible,
    ConfirmedIncompatible,
}

/// A candidate replacement/upgrade version with its own named concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeCandidate {
    pub version: String,
    pub compatibility: CompatibilityVerdict,
    pub concerns: Vec<String>,
}

impl UpgradeCandidate {
    /// A bare available version, compatibility unestablished — the only
    /// constructor callers get for "a version exists" alone.
    pub fn available(version: impl Into<String>) -> Self {
        UpgradeCandidate { version: version.into(), compatibility: CompatibilityVerdict::Unknown, concerns: Vec::new() }
    }
}

/// Usage/advisory/upgrade picture for one dependency, sections kept
/// separate so none is read as confirming another.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DependencyRiskMap {
    pub dependency: String,
    pub usage: Vec<UsageEvidence>,
    pub advisories: Vec<Advisory>,
    pub candidates: Vec<UpgradeCandidate>,
}

impl DependencyRiskMap {
    pub fn build(dep: &Dependency, usage: Vec<UsageEvidence>, advisories: Vec<Advisory>, candidates: Vec<UpgradeCandidate>) -> Self {
        DependencyRiskMap { dependency: dep.name.clone(), usage, advisories, candidates }
    }

    /// How many usage sites exist — "where a package matters" without
    /// claiming anything about advisories or upgrade safety.
    pub fn usage_count(&self) -> usize {
        self.usage.len()
    }

    /// Candidates whose compatibility is still unestablished — the set an
    /// upgrade decision cannot skip past.
    pub fn unresolved_candidates(&self) -> Vec<&UpgradeCandidate> {
        self.candidates.iter().filter(|c| c.compatibility == CompatibilityVerdict::Unknown).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_candidate_defaults_to_unknown_not_compatible() {
        let c = UpgradeCandidate::available("2.0.0");
        assert_eq!(c.compatibility, CompatibilityVerdict::Unknown);
    }

    #[test]
    fn risk_map_keeps_usage_advisories_and_candidates_separate() {
        let dep = Dependency { name: "left-pad".into(), current_version: "1.0.0".into() };
        let map = DependencyRiskMap::build(
            &dep,
            vec![UsageEvidence { dependency: "left-pad".into(), using_symbol: "fmt::pad".into() }],
            vec![Advisory { id: "GHSA-1".into(), severity: "high".into(), summary: "prototype pollution".into() }],
            vec![UpgradeCandidate::available("2.0.0")],
        );
        assert_eq!(map.usage_count(), 1);
        assert_eq!(map.advisories.len(), 1);
        assert_eq!(map.unresolved_candidates().len(), 1);
    }

    #[test]
    fn confirmed_incompatible_candidate_is_not_unresolved() {
        let dep = Dependency { name: "x".into(), current_version: "1.0.0".into() };
        let mut candidate = UpgradeCandidate::available("2.0.0");
        candidate.compatibility = CompatibilityVerdict::ConfirmedIncompatible;
        candidate.concerns.push("drops Node 14 support".into());
        let map = DependencyRiskMap::build(&dep, Vec::new(), Vec::new(), vec![candidate]);
        assert!(map.unresolved_candidates().is_empty());
    }

    #[test]
    fn no_imported_advisories_is_not_a_clean_bill_of_health() {
        let dep = Dependency { name: "x".into(), current_version: "1.0.0".into() };
        let map = DependencyRiskMap::build(&dep, Vec::new(), Vec::new(), Vec::new());
        // The empty vec is the honest representation of "none imported" —
        // callers must not read it as "no advisories exist".
        assert!(map.advisories.is_empty());
    }
}
