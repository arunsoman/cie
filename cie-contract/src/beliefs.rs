//! Belief inference — candidate rules from deviant behavior (TASK-040,
//! §7 Engler et al. + Kremenek rows, FR-12).
//!
//! Engler-style signal scan over summaries + relations: inconsistent
//! checks (a result honored at N−1 call sites), cleanup pairing
//! (acquire/release), and API-usage co-occurrence regularities. The
//! majority pattern is the implicit specification; the minority sites are
//! the high-value audit targets. Heterogeneous signals combine in the
//! Kremenek-style factor graph ([`crate::beliefs::FactorGraph`]).
//!
//! Rules honored:
//! - **§8/global rule 4**: every output is `Authority::InferredCandidate`;
//!   the guarded registry ([`crate::registry::ContractRegistry`]) makes any
//!   other authority unreachable from this module.
//! - **global rule 2 / FR-4**: every candidate cites its support sites and
//!   deviant sites in `Evidence.provenance`.
//! - **FR-6**: beliefs are expected-behavior candidates stored as
//!   contracts, never merged into observed-fact records.
//! - **Determinism**: potentials are deterministic given the same input
//!   relations — same inputs produce byte-identical candidates.

use crate::{Authority, Contract, ObligationKind};
use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SymbolId};
use serde::{Deserialize, Serialize};

/// One call site: (caller, callee).
pub type SiteRef = (SymbolId, SymbolId);

/// The predicate a belief asserts (sketch: `Predicate::retval_checked(..)`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Predicate {
    /// Return value of the named function is checked at call sites.
    RetvalChecked(String),
    /// Acquire is paired with its release.
    CleanupPaired { acquire: String, release: String },
    /// Callers of `a` also call `b` (usage regularity).
    CoOccurs { a: String, b: String },
}

impl Predicate {
    pub fn retval_checked(f: impl Into<String>) -> Self {
        Predicate::RetvalChecked(f.into())
    }
    pub fn as_str(&self) -> String {
        match self {
            Predicate::RetvalChecked(f) => format!("retval_checked({f})"),
            Predicate::CleanupPaired { acquire, release } => {
                format!("cleanup_paired({acquire}, {release})")
            }
            Predicate::CoOccurs { a, b } => format!("co_occurs({a}, {b})"),
        }
    }
}

/// Signal family (Kremenek: heterogeneous sources get family-specific
/// reliabilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Family {
    InconsistentCheck,
    CleanupPairing,
    UsageRegulariry,
}

/// One normalized factor: a signal distilled into support/deviant sites
/// plus its family potential weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefFactor {
    pub family: Family,
    pub predicate: Predicate,
    /// Sites honoring the pattern (the majority ⇒ the belief).
    pub support_sites: Vec<SiteRef>,
    /// Sites deviating from the pattern (audit targets).
    pub deviant_sites: Vec<SiteRef>,
    /// Revision the signal was collected at (evidence scoping).
    pub rev: RevId,
    /// Family reliability prior (log-odds weight); deterministic.
    pub weight: f64,
}

impl BeliefFactor {
    pub fn support(&self) -> usize {
        self.support_sites.len()
    }
}

/// A posterior belief — the candidate rule the factor graph emits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Belief {
    pub predicate: Predicate,
    pub authority: Authority,
    pub support_sites: Vec<SiteRef>,
    pub deviant_sites: Vec<SiteRef>,
    /// Sigmoid of the summed log-odds (deterministic posterior).
    pub posterior: f64,
    pub evidence: Evidence,
}

impl Belief {
    /// FR-6: the belief as a registry contract — ALWAYS InferredCandidate
    /// (the guarded registry rejects anything else for rule-mined rules).
    pub fn to_contract(&self, symbols: Vec<SymbolId>) -> Contract {
        Contract {
                verified_by: vec![],
                introduced: None,
            id: 0, // assigned by the registry
            obligation: ObligationKind::Custom(format!("belief:{}", self.predicate.as_str())),
            expr: format!(
                "{} [posterior {:.3}]",
                self.predicate.as_str(),
                self.posterior
            ),
            authority: Authority::InferredCandidate,
            symbols,
            source: crate::ContractSource::NamingConvention,
        }
    }
}

/// Kremenek-style factor graph: heterogeneous signals combined into one
/// deterministic posterior per candidate rule.
///
/// Combination model (deterministic, documented): each factor votes for
/// its predicate with log-odds `weight × reliability(support, deviants)`;
/// the posterior for a predicate is the sigmoid of the sum over its
/// factors, where `reliability = support / (support + deviants + 1)`.
/// Same inputs ⇒ byte-identical output (sorted factor order, fixed
/// summation order).
#[derive(Debug, Clone, Default)]
pub struct FactorGraph {
    factors: Vec<BeliefFactor>,
}

/// Family reliability priors (Kremenek: some signals are more trustworthy).
const FAMILY_WEIGHTS: [(Family, f64); 3] = [
    (Family::InconsistentCheck, 2.0),
    (Family::CleanupPairing, 1.5),
    (Family::UsageRegulariry, 1.0),
];

impl FactorGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, factor: BeliefFactor) {
        self.factors.push(factor);
    }

    pub fn factors(&self) -> &[BeliefFactor] {
        &self.factors
    }

    /// Deterministic posterior inference. Output is sorted by predicate
    /// string; factors for the same predicate accumulate log-odds in
    /// sorted (family, subject) order.
    pub fn infer(&self) -> Vec<Belief> {
        use std::collections::BTreeMap;
        // group factors by predicate string
        let mut groups: BTreeMap<String, Vec<&BeliefFactor>> = BTreeMap::new();
        for f in &self.factors {
            groups.entry(f.predicate.as_str()).or_default().push(f);
        }
        let mut out = vec![];
        for (_key, mut fs) in groups {
            fs.sort_by(|a, b| {
                (a.family, a.predicate.as_str()).cmp(&(b.family, b.predicate.as_str()))
            });
            let mut log_odds = 0.0;
            for f in &fs {
                let w = FAMILY_WEIGHTS
                    .iter()
                    .find(|(fam, _)| *fam == f.family)
                    .map(|(_, w)| *w)
                    .unwrap_or(1.0);
                let reliability =
                    f.support() as f64 / (f.support() + f.deviant_sites.len() + 1) as f64;
                log_odds += w * reliability;
            }
            let posterior = 1.0 / (1.0 + (-log_odds).exp());
            // merge sites (sorted, deduped — determinism)
            let mut support: Vec<SiteRef> = fs.iter().flat_map(|f| f.support_sites.clone()).collect();
            support.sort();
            support.dedup();
            let mut deviant: Vec<SiteRef> = fs.iter().flat_map(|f| f.deviant_sites.clone()).collect();
            deviant.sort();
            deviant.dedup();
            let first = fs[0];
            let evidence = Evidence {
                rev: first.rev,
                provenance: format!(
                    "belief inference: {} support={} deviant={} (factor graph: {} factors)",
                    first.predicate.as_str(),
                    support.len(),
                    deviant.len(),
                    fs.len()
                ),
                assumptions: vec![
                    "kremenek combination: sigmoid(sum(w_i * reliability_i))".into(),
                ],
                unsupported: vec![],
                unresolved: vec![],
                generated_at_ns: 0,
            };
            out.push(Belief {
                predicate: first.predicate.clone(),
                authority: Authority::InferredCandidate,
                support_sites: support,
                deviant_sites: deviant,
                posterior,
                evidence,
            });
        }
        out
    }
}
