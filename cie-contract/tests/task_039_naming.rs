//! TASK-039 validation: naming adapter for method-name/implementation
//! mismatch (§7 Høst & Østvold, FR-10/FR-11), including the guarded
//! registry contract (TASK-038 API) the naming rules flow through.
//!
//! Sketch deviations (documented, single-point):
//! - `fx.summaries()` → a `BTreeMap<SymbolId, FunctionSummary>` built
//!   inline; `fixture_naming()` is realized as two summary entries.
//! - `contract_registry_with_naming_rules()` → `ContractRegistry` from
//!   `cie-contract::registry` (the TASK-038 guarded write path).
//! - `RecordKind::NameMismatch | SuspectedStub` → `NamingKind` (the
//!   naming payload view; DerivedRecord wrapping is FR-9 store-side).
//! - `p.missing_action == Some(Action::CheckPermission)` →
//!   `rec.missing == Some(Action::CheckPermission)` (same field, owned
//!   by the record instead of a provenance entry; the provenance string
//!   also cites it per global rule 2).

use cie_contract::naming::{Action, Label, LabelSource, NamingAdapter, NamingKind};
use cie_contract::{Authority, Contract, ContractRegistry, ContractSource, ObligationKind};
use cie_core::evidence::Evidence;
use cie_core::types::{RevId, SourceLocation, SymbolId};
use cie_semantic::facts::{SymbolFact, SymbolKind, SymbolStore};
use cie_semantic::LangId;
use cie_summary::{EffectKind, FunctionSummary};
use std::collections::BTreeMap;

const REV: RevId = 7;

fn sym(id: u32, name: &str, lang: LangId) -> SymbolFact {
    SymbolFact {
        id: SymbolId(id),
        kind: SymbolKind::Function,
        name: name.to_string(),
        qual_name: format!("mod.{name}"),
        loc: SourceLocation::point(0.into(), 0),
        lang,
        parent: None,
        rev: REV,
        doc: None,
    }
}

/// (a) `fn authorize(u: User) -> bool { println!("auth {u}"); true }` —
/// print-only, constant return, no callees.
/// (b) `fn authorize2(u: User) -> bool { if !allowed(u) { return false; } true }`
/// — consults a permission source via a resolved callee.
fn fixture() -> (SymbolStore, BTreeMap<SymbolId, FunctionSummary>) {
    let store = SymbolStore::build(vec![
        sym(0, "authorize", LangId::Python),
        sym(1, "authorize2", LangId::Python),
        sym(2, "allowed", LangId::Python),
    ]);
    let mut summaries = BTreeMap::new();

    // (a) print-only authorize
    let mut a = FunctionSummary::empty(SymbolId(0), REV, 0);
    a.effects.insert(EffectKind::Print);
    a.returns_constants.insert("true".into());
    a.all_returns_constant = true;
    summaries.insert(SymbolId(0), a);

    // (b) authorize consulting `allowed` (permission-flavored callee)
    let mut b = FunctionSummary::empty(SymbolId(1), REV, 0);
    b.callees.push(SymbolId(2));
    b.returns_constants.insert("false".into());
    b.returns_constants.insert("true".into());
    b.all_returns_constant = true;
    summaries.insert(SymbolId(1), b);

    let mut h = FunctionSummary::empty(SymbolId(2), REV, 0);
    h.effects.insert(EffectKind::DbRead);
    summaries.insert(SymbolId(2), h);

    (store, summaries)
}

/// The sketch's `task_039_naming_mismatch`.
#[test]
fn task_039_naming_mismatch() {
    let (store, summaries) = fixture();
    let mut reg = ContractRegistry::new();
    let adapter = NamingAdapter::lexicon_default(); // deterministic fallback (FR-28)

    let recs = adapter.run(&store, &summaries, REV);
    let mis = recs
        .iter()
        .find(|r| r.subject == SymbolId(0))
        .expect("mismatch on print-only authorize");
    assert!(
        matches!(mis.kind, NamingKind::SuspectedStub | NamingKind::NameMismatch),
        "print-only authorize is a stub-shaped mismatch, got {:?}",
        mis.kind
    );
    assert_eq!(mis.missing, Some(Action::CheckPermission));
    // global rule 2: evidence cites name, observed set, missing action.
    assert!(mis.evidence.provenance.contains("authorize"));
    assert!(mis.evidence.provenance.contains("check_permission"));
    assert!(mis.evidence.provenance.contains("print"));

    // (b) real permission consult -> no mismatch
    assert!(recs.iter().all(|r| r.subject != SymbolId(1)));

    // Naming contracts enter the registry only as InferredCandidate —
    // and the registry GUARDS that invariant (TASK-038 API).
    let contract = mis.to_contract(vec![mis.subject]);
    assert_eq!(contract.authority, Authority::InferredCandidate);
    let id = reg.insert(contract).expect("naming candidate admitted");
    let naming = reg
        .contracts_for(SymbolId(0))
        .into_iter()
        .find(|c| c.obligation == ObligationKind::Naming)
        .expect("naming obligation registered for subject");
    assert_eq!(naming.id, id);
    assert_eq!(naming.authority, Authority::InferredCandidate); // never Explicit via lexicon/LLM

    // LLM-off path: deterministic label (FR-28).
    let lbl = adapter.label(mis, None); // None => LLM disabled
    assert_eq!(lbl.source, LabelSource::Deterministic);
    assert!(lbl.model_id.is_none() && lbl.prompt_hash.is_none());
    assert_eq!(Label::key("abc"), "label:abc");

    // LLM-on path: annotation only, still not influencing authority.
    let lbl2 = adapter.label(mis, Some("test-model"));
    assert_eq!(lbl2.source, LabelSource::Llm);
    assert!(lbl2.prompt_hash.is_some());
}

/// FR-3/global rule 6: a language that does not vouch the naming rule is
/// skipped — rules do not leak across languages.
#[test]
fn task_039_language_vouch_gate() {
    let (store, summaries) = fixture();
    let mut adapter = NamingAdapter::lexicon_default();
    adapter.set_vouch(LangId::Python, false); // withdraw the vouch

    let recs = adapter.run(&store, &summaries, REV);
    assert!(
        recs.is_empty(),
        "unvouched language must produce no naming records, got {:?}",
        recs.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
    );
}

/// Token normalization: snake/camel/Pascal/kebab all reduce to the same
/// promised-action set (deterministic lexicon, per-language normalization).
#[test]
fn task_039_name_token_normalization() {
    for name in ["check_permission", "checkPermission", "CheckPermission", "check-permission"] {
        let acts = NamingAdapter::promised_actions(name);
        assert_eq!(acts, vec![Action::CheckPermission], "name {name}");
    }
    // non-lexical names promise nothing
    assert!(NamingAdapter::promised_actions("do_stuff").is_empty());
}

/// The registry guards (TASK-038 API): naming/protocol obligations can
/// never enter as decisive authorities; LlmCandidate is inert; Explicit
/// cannot come from NamingConvention; Verified upgrade needs a proof.
#[test]
fn task_039_registry_guards() {
    let mut reg = ContractRegistry::new();
    let naming = Contract {
        id: 0,
        obligation: ObligationKind::Naming,
        expr: "`authorize` must check permission".into(),
        authority: Authority::Explicit, // FORBIDDEN for Naming
        symbols: vec![SymbolId(0)],
        source: ContractSource::NamingConvention,
    };
    let err = reg.insert(naming).unwrap_err();
    assert!(matches!(err, cie_core::CieError::UnsupportedConstruct(_)));

    // Explicit from NamingConvention is equally rejected.
    let fake_explicit = Contract {
        id: 0,
        obligation: ObligationKind::MustAuthorize,
        expr: "x".into(),
        authority: Authority::Explicit,
        symbols: vec![],
        source: ContractSource::NamingConvention,
    };
    assert!(reg.insert(fake_explicit).is_err());

    // A legit naming candidate inserts, then upgrades ONLY via proof.
    let ok = Contract {
        id: 0,
        obligation: ObligationKind::Naming,
        expr: "`authorize` must check permission".into(),
        authority: Authority::InferredCandidate,
        symbols: vec![SymbolId(0)],
        source: ContractSource::NamingConvention,
    };
    let id = reg.insert(ok).expect("candidate admitted");
    assert!(reg.upgrade_to_verified(id, "").is_err(), "empty proof rejected");
    assert_eq!(
        reg.upgrade_to_verified(id, "bounded check survived 3 traces").unwrap(),
        Authority::Verified
    );

    // LlmCandidate is inert (acceptance #28): never Verified.
    let llm = Contract {
        id: 0,
        obligation: ObligationKind::MustValidateInput,
        expr: "llm says so".into(),
        authority: Authority::LlmCandidate,
        symbols: vec![],
        source: ContractSource::Llm,
    };
    let id2 = reg.insert(llm).expect("llm candidate admitted (inert)");
    assert!(reg.upgrade_to_verified(id2, "proof").is_err());

    // ...and an LLM source can never pose as decisive on insert either.
    let llm_explicit = Contract {
        id: 0,
        obligation: ObligationKind::MustValidateInput,
        expr: "llm says so".into(),
        authority: Authority::Explicit,
        symbols: vec![],
        source: ContractSource::Llm,
    };
    assert!(reg.insert(llm_explicit).is_err());
}

/// Determinism: same inputs → byte-identical records (FR-4).
#[test]
fn task_039_determinism() {
    let (store, summaries) = fixture();
    let adapter = NamingAdapter::lexicon_default();
    let a = adapter.run(&store, &summaries, REV);
    let b = adapter.run(&store, &summaries, REV);
    let ja = serde_json::to_string(&a).unwrap();
    let jb = serde_json::to_string(&b).unwrap();
    assert_eq!(ja, jb);
}
