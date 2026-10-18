//! cie-analysis — the L4 analysis layer: behavior, conformance, security,
//! testing, concepts/hierarchy and drift, plus the orchestration engine.

pub mod deps;
pub mod beliefs;
pub mod behavior;
pub mod alternatives;
pub mod concurrency;
pub mod focus;
pub mod endpoints;
pub mod impact_dims;
pub mod query;
pub mod pathcond;
pub mod predicates;
pub mod conformance;
pub mod fca;
pub use cie_contract::findings;
pub use steps::{transaction_walk, StepKind, TransactionStep, TransactionWalk};
pub use pathcond::{PathCond, PathCondition, PathCondExtraction, Pred, Cmp};
pub mod hierarchy;
pub mod ifds;
pub mod risk;
pub mod steps;
pub mod security;
pub mod testing;

use crate::findings::Finding;
use cie_concepts::ConceptDag;
use cie_contract::Contract;
use cie_core::types::{Ns, RevId, SymbolId};
use cie_evidence::{Coverage, Evidence};
use cie_semantic::facts::{LangFacts, SymbolStore};
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

pub struct AnalysisOutput {
    pub summaries: BTreeMap<SymbolId, FunctionSummary>,
    /// TASK-030 counters: files whose summaries were reused vs recomputed.
    pub summary_reuse: behavior::SummaryReuseStats,
    pub reverse_deps: BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>>,
    pub callees_of: BTreeMap<SymbolId, Vec<SymbolId>>,
    pub findings: Vec<Finding>,
    pub contracts: Vec<Contract>,
    pub concepts: ConceptDag,
    pub coverage: Coverage,
    pub evidence: Evidence,
    pub test_map: testing::TestMap,
}

pub struct AnalysisConfig {
    pub deadline_ns: Option<Ns>,
    /// Skip architecture/coverage informational findings if desired.
    pub include_coverage_gaps: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            deadline_ns: None,
            include_coverage_gaps: true,
        }
    }
}

/// Full analysis pass over an extracted project.
pub fn analyze(
    store: &SymbolStore,
    per_file: &[LangFacts],
    rev: RevId,
    cfg: &AnalysisConfig,
    label_fn: hierarchy::LabelFn,
    reuse: Option<&behavior::ReusePlan>,
) -> AnalysisOutput {
    let start = cie_core::types::now_ns();

    // L4 stage 1: behavior extraction (TASK-030: reuse where deps unchanged).
    let beh = behavior::compute_summaries_with_reuse(store, per_file, rev, cfg.deadline_ns, reuse);
    let callees_of: BTreeMap<SymbolId, Vec<SymbolId>> = beh
        .summaries
        .iter()
        .map(|(id, s)| (*id, s.callees.clone()))
        .collect();

    // Stage 2: conformance (findings + contracts).
    let conf = conformance::run(store, per_file, &beh.summaries, rev);

    // Stage 3: security + performance slice.
    let mut findings = conf.findings;
    findings.extend(security::run(store, per_file, rev));

    // Stage 4: testing slice.
    let test_map = testing::build_test_map(store, &callees_of);
    if cfg.include_coverage_gaps {
        findings.extend(testing::coverage_gaps(store, &test_map, rev));
    }

    // Dead code (CAP-1.4): no callers, not test, not entrypoint.
    for sym in &store.symbols {
        if !sym.is_function_like() || sym.is_test() {
            continue;
        }
        if beh.reverse_deps.contains_key(&sym.id) {
            continue;
        }
        let summary = beh.summaries.get(&sym.id);
        let is_entry = summary
            .map(|s| s.tags.contains("entrypoint"))
            .unwrap_or(false);
        if is_entry || sym.name.starts_with("__") {
            continue;
        }
        findings.push(Finding {
            id: format!("F-{rev}-D{:04}", findings.len() + 1),
            kind: crate::findings::FindingKind::DeadCode,
            severity: crate::findings::FindingKind::DeadCode.default_severity(),
            symbol: sym.id,
            symbol_name: sym.qual_name.clone(),
            message: format!(
                "'{}' has no callers in the analyzed graph and is not an entry point",
                sym.qual_name
            ),
            evidence_lines: vec![store.files.line(sym.loc)],
            remediation: "Remove it or expose it through an entry point/test".into(),
            authority: cie_contract::Authority::Verified,
            rule: "arch/AR-002".into(),
            tags: vec!["architecture".into()],
        });
    }

    // Stage 5: concepts + hierarchy.
    let concepts = hierarchy::build_hierarchy(store, &beh, &conf.contracts, label_fn, start);

    // Evidence + coverage.
    let mut unresolved: Vec<String> = vec![];
    for s in beh.summaries.values() {
        for u in &s.unresolved_callees {
            unresolved.push(format!(
                "{}: {}",
                store
                    .get(s.sym)
                    .map(|x| x.qual_name.clone())
                    .unwrap_or_default(),
                u
            ));
        }
    }
    let mut unsupported: Vec<String> = vec![];
    for f in per_file {
        for u in &f.unsupported {
            unsupported.push(format!("{}: {}", f.path, u));
        }
    }

    let complete = cfg
        .deadline_ns
        .map(|dl| cie_core::types::now_ns() <= dl)
        .unwrap_or(true)
        && beh.summaries.values().all(|s| s.complete);

    let total_syms = store.symbols.len() as u32;
    let analyzed = beh.summaries.len() as u32;
    let files_total = per_file.len() as u32;

    let coverage = Coverage {
        files_total,
        files_analyzed: files_total,
        symbols_total: total_syms,
        symbols_analyzed: analyzed,
        complete,
        notes: vec![
            "taint analysis is intra-procedural (no heap model — PRD §74)".into(),
            "conformance rules run on deep facts for Python; structural facts for JS/TS/Go/Rust"
                .into(),
        ],
        wall_ns: (cie_core::types::now_ns() - start) as i64,
    };

    let mut evidence = Evidence::new(rev, format!("cie-v{}", env!("CARGO_PKG_VERSION")), start);
    evidence.unresolved = unresolved.iter().take(200).cloned().collect();
    evidence.unsupported = unsupported.iter().take(200).cloned().collect();
    evidence.assumptions = vec![
        "naming-convention auth/money tags are hypotheses (InferredCandidate)".into(),
        "print/log effect classification is name-based".into(),
    ];

    AnalysisOutput {
        summary_reuse: beh.reuse,
        summaries: beh.summaries,
        reverse_deps: beh.reverse_deps,
        callees_of,
        findings,
        contracts: conf.contracts,
        concepts,
        coverage,
        evidence,
        test_map,
    }
}
