//! Runtime-activity overlay over static structure (US-075). A join is
//! labelled exact only when the static revision and the observation's
//! deployment revision agree; everything weaker is Partial, and a symbol
//! with no joined observation is Unknown — never "unused". There is no
//! "unused" variant anywhere in this module: the type itself cannot
//! express that conclusion, so no workload-incomplete window can produce
//! it by construction.

use serde::{Deserialize, Serialize};

/// One imported runtime observation — never fabricated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeObservation {
    pub symbol: String,
    pub environment: String,
    pub deployment_revision: String,
    pub window_start_ns: u64,
    pub window_end_ns: u64,
    pub activity_count: u64,
}

/// How strongly a structure/runtime join can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinExactness {
    /// Revision, deployment and code location all agree.
    Exact,
    /// Some observation joined, but revision/deployment disagree.
    Partial,
    /// No observation joined this symbol at all this window.
    Unknown,
}

/// The joined result for one static symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureRuntimeJoin {
    pub symbol: String,
    pub exactness: JoinExactness,
    /// `None` only when nothing joined — never a fabricated zero.
    pub observed_activity: Option<u64>,
    pub reason: String,
}

/// Join one static symbol against imported runtime observations
/// (US-075). `static_revision` is the code revision being studied;
/// each observation's `deployment_revision` is compared against it.
pub fn join_structure_with_runtime(
    static_symbol: &str,
    static_revision: &str,
    observations: &[RuntimeObservation],
) -> StructureRuntimeJoin {
    let matches: Vec<&RuntimeObservation> = observations.iter().filter(|o| o.symbol == static_symbol).collect();
    if matches.is_empty() {
        return StructureRuntimeJoin {
            symbol: static_symbol.to_string(),
            exactness: JoinExactness::Unknown,
            observed_activity: None,
            reason: "no runtime observation joined for this window/workload — \
                     not evidence the symbol is unused"
                .into(),
        };
    }
    let exact = matches.iter().all(|m| m.deployment_revision == static_revision);
    let total: u64 = matches.iter().map(|m| m.activity_count).sum();
    StructureRuntimeJoin {
        symbol: static_symbol.to_string(),
        exactness: if exact { JoinExactness::Exact } else { JoinExactness::Partial },
        observed_activity: Some(total),
        reason: if exact {
            "static revision matches every joined observation's deployment revision".into()
        } else {
            format!(
                "static revision {static_revision} disagrees with at least one \
                 observation's deployment revision"
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(symbol: &str, rev: &str, count: u64) -> RuntimeObservation {
        RuntimeObservation {
            symbol: symbol.into(),
            environment: "prod".into(),
            deployment_revision: rev.into(),
            window_start_ns: 0,
            window_end_ns: 1,
            activity_count: count,
        }
    }

    #[test]
    fn matching_revision_is_exact() {
        let join = join_structure_with_runtime("charge", "rev-1", &[obs("charge", "rev-1", 5)]);
        assert_eq!(join.exactness, JoinExactness::Exact);
        assert_eq!(join.observed_activity, Some(5));
    }

    #[test]
    fn mismatched_revision_is_partial_not_exact() {
        let join = join_structure_with_runtime("charge", "rev-2", &[obs("charge", "rev-1", 5)]);
        assert_eq!(join.exactness, JoinExactness::Partial);
    }

    #[test]
    fn no_observation_is_unknown_never_unused() {
        let join = join_structure_with_runtime("charge", "rev-1", &[]);
        assert_eq!(join.exactness, JoinExactness::Unknown);
        assert_eq!(join.observed_activity, None);
        assert!(join.reason.contains("not evidence"));
    }
}
