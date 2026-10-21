//! TASK-054 validation: error-handling failures ranked by safety/liveness
//! impact (Yuan et al. detectors).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_error_handling_facts()` → `LangFacts` with adapter-provided
//!   `ExceptionSite`s (the TASK-036 layer) and returns/loops.
//! - `fixture_obligation_model()` → `ModelProjection` of the TASK-052
//!   model (event + provenance-symbol projection).
//! - `f.evidence.locs` → `f.locs` (the SourceLocation chain rides on the
//!   finding; the Evidence provenance cites it textually too).

use cie_fault::{DetectorKind, ErrorHandlingDetectors, ModelProjection, ModelTransitionRef, ThreatClass};
use cie_contract::Authority;
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_semantic::facts::LangFacts;
use cie_semantic::LangId;

const REV: RevId = 41;

/// (a) refund() swallowing its transfer error; (b) retry loop broken by an
/// early return; (c) misleading success; (d) empty handler; (e) bare
/// log-and-continue handler.
fn fixture_error_handling_facts() -> Vec<LangFacts> {
    let mut f = LangFacts::new("pay.py", LangId::Python, "test-analyzer");
    f.symbols = vec![
        cie_semantic::facts::SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(0),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: "refund".into(),
            qual_name: "pay.refund".into(),
            loc: SourceLocation::point(0.into(), 10),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
        cie_semantic::facts::SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(1),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: "retry_scheduler".into(),
            qual_name: "pay.retry_scheduler".into(),
            loc: SourceLocation::point(0.into(), 20),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
        cie_semantic::facts::SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(2),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: "settle_batch".into(),
            qual_name: "pay.settle_batch".into(),
            loc: SourceLocation::point(0.into(), 40),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
    ];
    // (a) refund: exception caught at line 12, handler spans 12..14, no
    // re-raise anywhere inside → swallowed (recovery-flavored → Safety)
    f.exceptions.push(cie_semantic::facts::ExceptionSite {
        symbol: SymbolId(0),
        kind_text: "except".into(),
        is_bare: false,
        line: 12,
        in_handler: false,
        handler_end: 14,
    });
    // (c) settle_batch: error path AND a success constant return
    // (two-sided evidence → MisleadingSuccess)
    f.exceptions.push(cie_semantic::facts::ExceptionSite {
        symbol: SymbolId(2),
        kind_text: "except".into(),
        is_bare: false,
        line: 42,
        in_handler: false,
        handler_end: 44,
    });
    f.returns.push(cie_semantic::facts::ReturnSite {
        symbol: SymbolId(2),
        line: 46,
        expr: "return True".into(),
        constant: Some("true".into()),
    });
    // (b) retry_scheduler: early return inside the retry loop
    f.loops.push(cie_semantic::facts::LoopSite {
        symbol: SymbolId(1),
        kind: cie_semantic::facts::LoopKind::While,
        line: 21,
        body_start: 22,
        body_end: 30,
        loc: SourceLocation::point(0.into(), 21),
    });
    f.returns.push(cie_semantic::facts::ReturnSite {
        symbol: SymbolId(1),
        line: 25,
        expr: "return done".into(),
        constant: Some("true".into()),
    });

    // (d)/(e) in a second file: empty handler + bare handler
    let mut g = LangFacts::new("misc.py", LangId::Python, "test-analyzer");
    g.symbols = vec![
        cie_semantic::facts::SymbolFact {
            annotations: Vec::new(),
            id: SymbolId(3),
            kind: cie_semantic::facts::SymbolKind::Function,
            name: "notify_user".into(),
            qual_name: "misc.notify_user".into(),
            loc: SourceLocation::point(1.into(), 5),
            lang: LangId::Python,
            parent: None,
            rev: REV,
            doc: None,
        },
    ];
    // empty handler: handler_end <= line + 1
    g.exceptions.push(cie_semantic::facts::ExceptionSite {
        symbol: SymbolId(3),
        kind_text: "except".into(),
        is_bare: false,
        line: 6,
        in_handler: false,
        handler_end: 7,
    });
    // bare handler → log-and-continue
    g.exceptions.push(cie_semantic::facts::ExceptionSite {
        symbol: SymbolId(3),
        kind_text: "except".into(),
        is_bare: true,
        line: 9,
        in_handler: false,
        handler_end: 11,
    });
    vec![f, g]
}

/// The TASK-052 model projection: refund_called provenance points at the
/// refund symbol → obligation-aware prioritization.
fn fixture_obligation_model() -> ModelProjection {
    ModelProjection {
        transitions: vec![ModelTransitionRef {
            event: "refund_called".into(),
            provenance_symbols: vec![SymbolId(0)],
        }],
    }
}

/// The sketch's `task_054_detects_ineffective_refund_and_interrupted_scheduler`.
#[test]
fn task_054_detects_ineffective_refund_and_interrupted_scheduler() {
    let facts = fixture_error_handling_facts();
    let model = fixture_obligation_model();
    let detectors = ErrorHandlingDetectors::new(&facts, &model, REV);

    let findings = detectors.run();

    // swallowed refund failure → safety-threatening potential ineffective refund
    let refund = findings
        .iter()
        .find(|f| f.detector == DetectorKind::SwallowedException && f.about() == SymbolId(0))
        .expect("swallowed refund error must be flagged");
    assert_eq!(refund.classification, ThreatClass::Safety);
    assert!(!refund.locs.is_empty(), "finding cites handler location");

    // early return inside the retry loop → liveness threat
    let sched = findings
        .iter()
        .find(|f| f.detector == DetectorKind::EarlyReturnInRetryLoop)
        .expect("broken retry scheduler must be flagged");
    assert_eq!(sched.classification, ThreatClass::Liveness);

    // candidates only — no authority upgrade; all findings carry evidence
    assert!(findings.iter().all(|f| f.authority == Authority::InferredCandidate));
    assert!(findings.iter().all(|f| !f.locs.is_empty()));

    // the full detector catalogue fires on this fixture
    for d in [
        DetectorKind::SwallowedException,
        DetectorKind::EmptyHandler,
        DetectorKind::LogAndContinue,
        DetectorKind::MisleadingSuccess,
        DetectorKind::EarlyReturnInRetryLoop,
    ] {
        assert!(
            findings.iter().any(|f| f.detector == d),
            "detector {d:?} must fire on the fixture"
        );
    }

    // safety findings rank first (prioritized by threat class)
    let first_safety_idx = findings
        .iter()
        .position(|f| f.classification == ThreatClass::Safety)
        .unwrap();
    let first_liveness_idx = findings
        .iter()
        .position(|f| f.classification == ThreatClass::Liveness)
        .unwrap();
    assert!(
        first_safety_idx <= first_liveness_idx,
        "safety-ranked findings come first: {:?}",
        findings.iter().map(|f| (f.detector, f.classification)).collect::<Vec<_>>()
    );
}

/// Determinism: same inputs → byte-identical findings (FR-4).
#[test]
fn task_054_deterministic_ranking() {
    let facts = fixture_error_handling_facts();
    let model = fixture_obligation_model();
    let a = ErrorHandlingDetectors::new(&facts, &model, REV).run();
    let b = ErrorHandlingDetectors::new(&facts, &model, REV).run();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
