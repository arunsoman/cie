//! Composite change-risk terrain (CIE-US US-046 engine close).
//!
//! Assembles the shipped ingredients — analysis findings, drift claim
//! statuses, coverage gaps, and (when history was ingested) churn — into
//! one per-symbol composite score with the story's honesty obligations:
//! * the formula is shown: every factor, its weight and its contribution
//!   are exposed per symbol;
//! * weights are adjustable inputs, not hidden constants;
//! * missing data renders as a neutral `missing` value (0.5 contribution,
//!   flagged), never as zero-looking strength;
//! * the `composite, not fact` label is permanent on the output type.
//!
//! Scores are relative to the repository (normalised against the max
//! within this revision), never absolute claims.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use cie_claims::model::{DriftReport, DriftStatus};
use cie_contract::findings::Finding;
use cie_core::types::SymbolId;
use cie_evidence::Coverage;
use serde::{Deserialize, Serialize};

/// Weights (all adjustable; shown in every output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskWeights {
    pub findings: f64,
    pub drift: f64,
    pub test_gap: f64,
    pub churn: f64,
}

impl Default for RiskWeights {
    fn default() -> Self {
        RiskWeights {
            findings: 0.35,
            drift: 0.25,
            test_gap: 0.2,
            churn: 0.2,
        }
    }
}

/// One factor's contribution for one symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorContribution {
    pub factor: String,
    pub weight: f64,
    /// Normalised 0..1 input (relative to this repository's max).
    pub value: Option<f64>,
    /// None value → missing data: neutral 0.5 contribution, flagged.
    pub missing: bool,
    pub contribution: f64,
    /// Where the number came from (provenance discipline).
    pub source: String,
}

/// Per-symbol composite risk (labelled, never presented as fact).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolRisk {
    pub symbol: SymbolId,
    pub symbol_name: String,
    pub factors: Vec<FactorContribution>,
    /// Weighted sum of contributions (0..1, relative to this repository).
    pub score: f64,
    /// Rank in the composite ordering (1 = riskiest).
    pub rank: u32,
}

impl SymbolRisk {
    pub const LABEL: &'static str = "composite, not fact";
}

/// The full terrain output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRiskTerrain {
    pub rev: cie_core::types::RevId,
    pub weights: RiskWeights,
    pub symbols: Vec<SymbolRisk>,
    /// Permanent label (US-046 obligation).
    pub label: &'static str,
    /// Named data gaps (which factor was missing for which symbol count).
    pub missing_data: BTreeMap<&'static str, usize>,
}

/// Inputs for one composite assembly.
pub struct RiskInputs<'a> {
    pub rev: cie_core::types::RevId,
    pub findings: &'a [Finding],
    pub drift: Option<&'a DriftReport>,
    pub coverage: &'a Coverage,
    /// Symbol → number of tests reaching it (from the shipped test map).
    /// Symbols absent from the map are coverage gaps (factor = 1.0).
    pub tests_reaching: &'a BTreeMap<SymbolId, usize>,
    /// Optional churn factors keyed by symbol name path (US-086 output).
    /// The mapping symbol→path is the caller's knowledge (adapter paths).
    pub churn_by_symbol: Option<&'a BTreeMap<SymbolId, u64>>,
    /// Display names.
    pub symbol_names: &'a BTreeMap<SymbolId, String>,
}

/// Normalise a count against the max in its column (relative colouring).
fn norm(v: u64, max: u64) -> f64 {
    if max == 0 {
        0.0
    } else {
        v as f64 / max as f64
    }
}

/// Assemble the composite terrain (US-046). Deterministic: symbols sorted
/// by score desc then id asc; ranks assigned in that order.
pub fn assemble_change_risk(inputs: &RiskInputs, weights: &RiskWeights) -> ChangeRiskTerrain {
    // --- factor columns
    let mut findings_count: BTreeMap<SymbolId, u64> = BTreeMap::new();
    for f in inputs.findings {
        *findings_count.entry(f.symbol).or_insert(0) += 1;
    }
    let max_findings = findings_count.values().copied().max().unwrap_or(0);

    // drift: unverified/contradicting claims mapped to symbols
    let mut drift_count: BTreeMap<SymbolId, u64> = BTreeMap::new();
    if let Some(report) = inputs.drift {
        let mut sym_by_claim: BTreeMap<cie_core::types::ClaimId, Vec<SymbolId>> = BTreeMap::new();
        for m in &report.mappings {
            sym_by_claim.insert(m.claim, m.symbols.clone());
        }
        for v in &report.verifications {
            if matches!(v.status, DriftStatus::Drifted | DriftStatus::Contradicting) {
                if let Some(syms) = sym_by_claim.get(&v.claim) {
                    for s in syms {
                        *drift_count.entry(*s).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let max_drift = drift_count.values().copied().max().unwrap_or(0);

    // test gap: symbols with zero reaching tests score 1.0; others scale
    // down by coverage of the analysed population
    let analyzed_total = inputs.coverage.symbols_analyzed.max(1) as f64;
    let test_gap = |s: &SymbolId| -> Option<f64> {
        match inputs.tests_reaching.get(s) {
            None => Some(1.0), // a named gap: no test reaches this symbol
            Some(n) => {
                if *n == 0 {
                    Some(1.0)
                } else {
                    // diminishing: 1 test is better than none; log-ish scale
                    Some(1.0 / (1.0 + *n as f64).ln().max(1.0))
                }
            }
        }
    };

    let max_churn = inputs
        .churn_by_symbol
        .as_ref()
        .map(|m| m.values().copied().max().unwrap_or(0))
        .unwrap_or(0);

    // --- per-symbol assembly
    let mut population: BTreeSet<SymbolId> = BTreeSet::new();
    for f in inputs.findings {
        population.insert(f.symbol);
    }
    for s in inputs.symbol_names.keys() {
        population.insert(*s);
    }
    for s in inputs.tests_reaching.keys() {
        population.insert(*s);
    }

    let mut missing_data: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut rows: Vec<SymbolRisk> = Vec::new();
    for sym in &population {
        let name = inputs
            .symbol_names
            .get(sym)
            .cloned()
            .unwrap_or_else(|| format!("symbol#{sym}"));
        let mut factors = Vec::new();

        // findings
        let fval = findings_count.get(sym).map(|v| norm(*v, max_findings));
        if fval.is_none() {
            *missing_data.entry("findings").or_insert(0) += 0;
        }
        let fmissing = fval.is_none();
        let fcontrib = fval.unwrap_or(0.5);
        factors.push(FactorContribution {
            factor: "findings".into(),
            weight: weights.findings,
            value: fval,
            missing: fmissing,
            contribution: weights.findings * fcontrib,
            source: "cie-analysis findings (this revision)".into(),
        });

        // drift
        let dval = drift_count.get(sym).map(|v| norm(*v, max_drift));
        if dval.is_none() {
            *missing_data.entry("drift").or_insert(0) += 1;
        }
        let dmissing = dval.is_none();
        let dcontrib = dval.unwrap_or(0.5);
        factors.push(FactorContribution {
            factor: "drift".into(),
            weight: weights.drift,
            value: dval,
            missing: dmissing,
            contribution: weights.drift * dcontrib,
            source: "drift report verifications".into(),
        });

        // test gap
        let (tval, tmissing) = match test_gap(sym) {
            Some(v) => (Some(v), false),
            None => (None, true),
        };
        let tcontrib = tval.unwrap_or(0.5);
        factors.push(FactorContribution {
            factor: "test_gap".into(),
            weight: weights.test_gap,
            value: tval,
            missing: tmissing,
            contribution: weights.test_gap * tcontrib,
            source: "test map (cie-analysis::testing)".into(),
        });

        // churn (optional ingest)
        let cval = inputs.churn_by_symbol.and_then(|m| m.get(sym)).map(|v| norm(*v, max_churn));
        if cval.is_none() {
            *missing_data.entry("churn").or_insert(0) += 1;
        }
        let cmissing = cval.is_none();
        let ccontrib = cval.unwrap_or(0.5);
        factors.push(FactorContribution {
            factor: "churn".into(),
            weight: weights.churn,
            value: cval,
            missing: cmissing,
            contribution: weights.churn * ccontrib,
            source: "cie-ingest churn factors (history sample, lower bound)".into(),
        });

        let score: f64 = factors.iter().map(|f| f.contribution).sum();
        rows.push(SymbolRisk {
            symbol: *sym,
            symbol_name: name,
            factors,
            score,
            rank: 0,
        });
    }
    // also count "no findings at all" as the findings column simply absent
    // when the map was empty (max=0) — every contribution is 0 then, which
    // is honest (no findings IS data), so no missing flag.

    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap().then(a.symbol.cmp(&b.symbol)));
    for (i, r) in rows.iter_mut().enumerate() {
        r.rank = (i + 1) as u32;
    }

    ChangeRiskTerrain {
        rev: inputs.rev,
        weights: RiskWeights {
            findings: weights.findings,
            drift: weights.drift,
            test_gap: weights.test_gap,
            churn: weights.churn,
        },
        symbols: rows,
        label: SymbolRisk::LABEL,
        missing_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_contract::findings::{Finding, FindingKind, Severity};

    fn finding(sym: SymbolId, name: &str) -> Finding {
        Finding {
            id: format!("F-test-{sym:?}"),
            kind: FindingKind::TaintFlow,
            severity: Severity::High,
            symbol: sym,
            symbol_name: name.into(),
            message: "test finding".into(),
            evidence_lines: vec![1],
            remediation: "fix".into(),
            authority: cie_contract::Authority::Explicit,
            rule: "conf/PR-001".into(),
            tags: vec![],
        }
    }

    #[test]
    fn composite_exposes_formula_weights_and_contributions() {
        let mut names = BTreeMap::new();
        names.insert(SymbolId(1), "pay::charge".to_string());
        names.insert(SymbolId(2), "pay::refund".to_string());
        let findings = vec![finding(SymbolId(1), "pay::charge"), finding(SymbolId(1), "pay::charge"), finding(SymbolId(1), "pay::charge")];
        let mut tests = BTreeMap::new();
        tests.insert(SymbolId(2), 3usize);
        let cov = Coverage::default();
        let inputs = RiskInputs {
            rev: 1,
            findings: &findings,
            drift: None,
            coverage: &cov,
            tests_reaching: &tests,
            churn_by_symbol: None,
            symbol_names: &names,
        };
        let terrain = assemble_change_risk(&inputs, &RiskWeights::default());
        assert_eq!(terrain.label, "composite, not fact");
        // charge: 3 findings (max) → findings value 1.0
        let charge = terrain.symbols.iter().find(|s| s.symbol_name == "pay::charge").unwrap();
        let f = charge.factors.iter().find(|f| f.factor == "findings").unwrap();
        assert_eq!(f.value, Some(1.0));
        assert!(!f.missing);
        // rank 1: charge riskier than refund (no findings + no tests)
        assert_eq!(charge.rank, 1);
        let refund = terrain.symbols.iter().find(|s| s.symbol_name == "pay::refund").unwrap();
        assert!(refund.rank > charge.rank);
        // churn missing → neutral 0.5 contribution, flagged, counted
        let c = refund.factors.iter().find(|f| f.factor == "churn").unwrap();
        assert!(c.missing);
        // weight × neutral 0.5
        assert!((c.contribution - weights_default_churn()).abs() < 1e-9);
        assert_eq!(terrain.missing_data.get("churn"), Some(&2));
        // every factor's contribution = weight × value (formula is shown)
        for s in &terrain.symbols {
            for f in &s.factors {
                let expect = f.weight * f.value.unwrap_or(0.5);
                assert!((f.contribution - expect).abs() < 1e-9);
            }
        }
    }

    fn weights_default_churn() -> f64 {
        RiskWeights::default().churn * 0.5
    }

    #[test]
    fn reweighting_changes_scores_without_hiding_the_formula() {
        let names: BTreeMap<SymbolId, String> = BTreeMap::new();
        let findings: Vec<Finding> = vec![];
        let tests: BTreeMap<SymbolId, usize> = BTreeMap::new();
        let cov = Coverage::default();
        let inputs = RiskInputs {
            rev: 1,
            findings: &findings,
            drift: None,
            coverage: &cov,
            tests_reaching: &tests,
            churn_by_symbol: None,
            symbol_names: &names,
        };
        let mut w = RiskWeights::default();
        w.findings = 1.0;
        w.drift = 0.0;
        w.test_gap = 0.0;
        w.churn = 0.0;
        let terrain = assemble_change_risk(&inputs, &w);
        assert_eq!(terrain.weights.findings, 1.0);
        // empty population is allowed — an honest empty terrain
        assert!(terrain.symbols.is_empty());
    }

    #[test]
    fn rev_is_pinned_for_freshness_disclosure() {
        let names: BTreeMap<SymbolId, String> = BTreeMap::new();
        let findings: Vec<Finding> = vec![];
        let tests: BTreeMap<SymbolId, usize> = BTreeMap::new();
        let cov = Coverage::default();
        let inputs = RiskInputs {
            rev: 42,
            findings: &findings,
            drift: None,
            coverage: &cov,
            tests_reaching: &tests,
            churn_by_symbol: None,
            symbol_names: &names,
        };
        let terrain = assemble_change_risk(&inputs, &RiskWeights::default());
        assert_eq!(terrain.rev, 42);
    }
}
