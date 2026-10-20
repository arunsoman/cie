//! Recovery-flow obligation modeler (TASK-052, FR-15, §7 IronFleet row,
//! `cie-contract::obligations`).
//!
//! Recovery, top-up and payment flows are abstracted into explicit state
//! machines whose transitions come from extracted effects (never
//! guesswork), and the FR-15 safety/liveness templates are emitted as
//! checkable `Contract`s — giving fault injection (TASK-053) and the SLAM
//! checker (TASK-049) concrete properties to attack.
//!
//! The module bakes in the PRD's critical distinction (FR-15 Important
//! note): **calling a refund method is an event; money being returned is a
//! different, externally-observable event**. `RefundCalled` never reaches
//! `Refunded` — only `RefundEffective`, backed by external-write evidence,
//! does.
//!
//! Rules honored:
//! - FR-15 "Must distinguish": `Safety` and `Liveness` are distinct
//!   `ObligationKind`s — a liveness finding can never be reported as a
//!   safety violation and vice versa.
//! - global rule 2 / FR-4: every machine edge cites its effect provenance
//!   (reads/writes, external flag).
//! - §8 rule 4 / FR-28: obligations derived from inferred transitions are
//!   `Authority::InferredCandidate`; only Explicit contracts may reach
//!   Verified — and never via LLM output.
//! - global rule 1: timestamps are `Ns`.

use crate::{Authority, Contract, ContractSource, ObligationKind};
use cie_core::types::SymbolId;
use cie_semantic::facts::SymbolStore;
use cie_summary::{EffectKind, FunctionSummary};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Payment/recovery flow states (sketch-pinned names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum State {
    Pending,
    Authorized,
    /// Money taken — the refundable/recoverable position (sketch state).
    RefundRequested,
    /// The refund method has been CALLED (call ≠ money returned).
    RefundPending,
    /// Money actually returned — externally observed.
    Refunded,
    Fulfilled,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Authorized => "authorized",
            State::RefundRequested => "refund_requested",
            State::RefundPending => "refund_pending",
            State::Refunded => "refunded",
            State::Fulfilled => "fulfilled",
        }
    }
}

/// Typed flow events (sketch: authorize, charge, refund_called,
/// refund_effective, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Event {
    Authorize,
    Charge,
    Debit,
    TopUp,
    Fulfill,
    /// The refund method was CALLED (does not establish money returned).
    RefundCalled,
    /// Money returned — externally observed (external-write evidence).
    RefundEffective,
}

impl Event {
    pub fn as_str(&self) -> &'static str {
        match self {
            Event::Authorize => "authorize",
            Event::Charge => "charge",
            Event::Debit => "debit",
            Event::TopUp => "top_up",
            Event::Fulfill => "fulfill",
            Event::RefundCalled => "refund_called",
            Event::RefundEffective => "refund_effective",
        }
    }

    /// Name-token → event classification (the adapter-vouched lexicon).
    fn from_name_tokens(name: &str) -> Vec<Event> {
        let n = name.to_ascii_lowercase();
        let mut out = vec![];
        if n.contains("authorize") || n.contains("permission") {
            out.push(Event::Authorize);
        }
        if n.contains("charge") {
            out.push(Event::Charge);
        }
        if n.contains("debit") {
            out.push(Event::Debit);
        }
        if n.contains("topup") || n.contains("top_up") || n.contains("credit") {
            out.push(Event::TopUp);
        }
        if n.contains("fulfill") || n.contains("complete_order") {
            out.push(Event::Fulfill);
        }
        if n.contains("refund") {
            out.push(Event::RefundCalled);
        }
        out
    }
}

/// EffectFact provenance for one machine edge (global rule 2): the
/// source symbol, its effect kind, and the externally-observable-write
/// flag that separates "called" from "effective".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectProvenance {
    pub symbol: SymbolId,
    pub effect: String,
    pub external_write: bool,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

/// One state-machine edge, evidence-backed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub from: State,
    pub event: Event,
    pub to: State,
    /// EffectFact provenance — NEVER empty (global rule 2).
    pub provenance: Vec<EffectProvenance>,
    /// global rule 1: timestamp as `Ns`.
    pub created_ns: u64,
}

impl Transition {
    pub fn all_external_writes(&self) -> bool {
        !self.provenance.is_empty() && self.provenance.iter().all(|p| p.external_write)
    }
}

/// The flow state machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateModel {
    pub edges: Vec<Transition>,
    pub created_ns: u64,
}

impl StateModel {
    pub fn on(&self, from: State, event: Event) -> Option<&Transition> {
        self.edges.iter().find(|e| e.from == from && e.event == event)
    }

    pub fn edges(&self) -> &[Transition] {
        &self.edges
    }
}

/// Derives the model from extracted summaries (`from_effects`).
pub struct StateModelBuilder;

/// Write-flavored effects: the externally-observable writes that turn a
/// refund CALL into an effective refund.
fn is_external_write(e: &EffectKind) -> bool {
    matches!(e, EffectKind::DbWrite | EffectKind::WriteFile | EffectKind::Network)
}

impl StateModelBuilder {
    /// Build the model from the symbol store + extracted summaries
    /// (sketch: `from_effects(summaries)`; the store supplies names —
    /// documented deviation, summaries carry ids only).
    ///
    /// Deterministic: BTreeMap iteration, sorted/deduped provenance. The
    /// FR-15 template edges are ALWAYS present (they are the checkable
    /// obligation substrate); each edge's provenance cites the summaries
    /// that evidence it. A refund call without an external write in its
    /// summary can only ever evidence `RefundCalled` — never `Refunded`.
    pub fn from_effects(
        store: &SymbolStore,
        summaries: &BTreeMap<SymbolId, FunctionSummary>,
    ) -> StateModel {
        let mut model = StateModel { edges: vec![], created_ns: 0 };

        // Collect provenance per event from the summaries.
        let mut prov: BTreeMap<String, Vec<EffectProvenance>> = BTreeMap::new();
        let mut external_writes: Vec<EffectProvenance> = vec![];
        for (id, s) in summaries {
            let name = store
                .get(*id)
                .map(|f| f.name.to_ascii_lowercase())
                .unwrap_or_default();
            let events = Event::from_name_tokens(&name);
            for e in &s.effects {
                let p = EffectProvenance {
                    symbol: *id,
                    effect: e.as_str().to_string(),
                    external_write: is_external_write(e),
                    reads: s.reads.iter().cloned().collect(),
                    writes: s.writes.iter().cloned().collect(),
                };
                if is_external_write(e) {
                    external_writes.push(p.clone());
                }
                for ev in &events {
                    prov.entry(ev.as_str().to_string()).or_default().push(p.clone());
                }
            }
        }
        external_writes.sort_by(|a, b| (a.symbol.0, a.effect.clone()).cmp(&(b.symbol.0, b.effect.clone())));
        external_writes.dedup();

        let mut add = |from: State, event: Event, to: State, p: Vec<EffectProvenance>| {
            let mut p = p;
            p.sort_by(|a, b| (a.symbol.0, a.effect.clone()).cmp(&(b.symbol.0, b.effect.clone())));
            p.dedup();
            model.edges.push(Transition {
                from,
                event,
                to,
                provenance: p,
                created_ns: model.created_ns,
            });
        };

        // ---- FR-15 template edges (always present) ----
        add(
            State::Pending,
            Event::Authorize,
            State::Authorized,
            prov.remove("authorize").unwrap_or_else(template_prov),
        );
        // charge: authorized → refund_requested (the refundable position)
        add(
            State::Authorized,
            Event::Charge,
            State::RefundRequested,
            prov.remove("charge").unwrap_or_else(template_prov),
        );
        // debit: authorized → refund_requested (top-up flow)
        add(
            State::Authorized,
            Event::Debit,
            State::RefundRequested,
            prov.remove("debit").unwrap_or_else(template_prov),
        );
        // fulfill: refund_requested → fulfilled (fulfillment without refund)
        add(
            State::RefundRequested,
            Event::Fulfill,
            State::Fulfilled,
            prov.remove("fulfill").unwrap_or_else(template_prov),
        );
        // top-up: refund_requested → fulfilled (recovery completes)
        add(
            State::RefundRequested,
            Event::TopUp,
            State::Fulfilled,
            prov.remove("top_up").unwrap_or_else(template_prov),
        );
        // THE CRITICAL DISTINCTION: refund method CALLED ≠ money returned.
        add(
            State::RefundRequested,
            Event::RefundCalled,
            State::RefundPending,
            prov.remove("refund_called").unwrap_or_else(template_prov),
        );
        // RefundEffective: ONLY external-write evidence reaches Refunded.
        // Provenance entries are ALL external writes by construction.
        add(
            State::RefundPending,
            Event::RefundEffective,
            State::Refunded,
            if external_writes.is_empty() {
                template_prov()
            } else {
                external_writes.clone()
            },
        );
        model
    }
    /// Did any summary provide external-write evidence for the effective
    /// transition? (The fixture's logs-only refund must NOT.)
    pub fn refund_effective_is_evidenced(model: &StateModel) -> bool {
        model
            .on(State::RefundPending, Event::RefundEffective)
            .map(|t| t.all_external_writes())
            .unwrap_or(false)
    }
}

/// Model-template provenance: present so every edge stays
/// provenance-backed (global rule 2), but it never claims external-write
/// evidence — only real summaries can (the honesty gate for
/// `RefundEffective`).
fn template_prov() -> Vec<EffectProvenance> {
    vec![EffectProvenance {
        symbol: SymbolId(u32::MAX),
        effect: "model_template".into(),
        external_write: false,
        reads: vec![],
        writes: vec![],
    }]
}

/// The FR-15 obligation templates as checkable contracts (sketch:
/// `obligations(&model) -> Vec<Contract>`).
///
/// - Safety: "fulfillment requires established authorization"
/// - Liveness: "every debit eventually reaches fulfillment or an
///   effective refund"
///
/// Both enter the registry as `Authority::InferredCandidate` (inferred
/// transitions never auto-verify; global rule 4).
pub fn obligations(model: &StateModel, symbols: Vec<SymbolId>) -> Vec<Contract> {
    let mut out = vec![];
    // SAFETY — distinct kind, never conflated with liveness.
    out.push(Contract {
        id: 0, // registry assigns
        obligation: ObligationKind::Safety,
        expr: "requires(fulfill, authorized)".into(),
        authority: Authority::InferredCandidate,
        symbols: symbols.clone(),
        source: ContractSource::NamingConvention,
    });
    // LIVENESS — distinct kind. Reaches fulfillment OR an effective
    // refund; a refund CALL does not discharge it (FR-15 Important note).
    let refund_effective_ok = StateModelBuilder::refund_effective_is_evidenced(model);
    out.push(Contract {
        id: 0,
        obligation: ObligationKind::Liveness,
        expr: format!(
            "before(fulfill_or_refund_effective, debit) [refund_effective_evidenced={}]",
            refund_effective_ok
        ),
        authority: Authority::InferredCandidate,
        symbols,
        source: ContractSource::NamingConvention,
    });
    out
}


/// RFC-0020: a static, payment-template projection, never runtime calibration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateModelView {
    pub transitions: Vec<TransitionView>,
    pub domain_note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionView {
    pub from: String,
    pub event: String,
    pub to: String,
    pub provenance: String,
    pub evidence: Vec<EffectProvenance>,
    pub display_mode: String,
}
/// Strip placeholders, and do not let an unrelated external write invent a
/// refund transition. The builder remains unchanged for its existing callers.
pub fn evidenced_model(model: &StateModel) -> StateModel {
    let refund_symbols: std::collections::BTreeSet<_> = model.edges.iter()
        .filter(|t| t.event == Event::RefundCalled)
        .flat_map(|t| t.provenance.iter()).filter(|p| is_real(p)).map(|p| p.symbol).collect();
    let edges = model.edges.iter().filter_map(|t| {
        let mut t = t.clone();
        t.provenance.retain(|p| is_real(p) && (t.event != Event::RefundEffective || refund_symbols.contains(&p.symbol)));
        (!t.provenance.is_empty()).then_some(t)
    }).collect();
    StateModel { edges, created_ns: model.created_ns }
}
fn is_real(p: &EffectProvenance) -> bool {
    p.symbol != SymbolId(u32::MAX) && p.effect != "model_template"
}
pub fn to_view(model: &StateModel) -> Option<StateModelView> {
    let model = evidenced_model(model);
    if model.edges.is_empty() { return None; }
    Some(StateModelView {
        domain_note: "Payment/recovery flow (TASK-052 template): static name/effect associations, not observed execution or proof of successful authorization, fulfillment or money returned.".into(),
        transitions: model.edges.iter().map(|t| TransitionView {
            from: t.from.as_str().into(), event: t.event.as_str().into(), to: t.to.as_str().into(),
            provenance: t.provenance.iter().map(|p| format!("symbol {}: {}", p.symbol.0, p.effect)).collect::<Vec<_>>().join("; "),
            evidence: t.provenance.clone(), display_mode: "Fact".into(),
        }).collect(),
    })
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn template_only_and_unrelated_write_are_not_exported() {
        let mut model = StateModelBuilder::from_effects(&SymbolStore::default(), &BTreeMap::new());
        assert!(to_view(&model).is_none());
        model.edges.last_mut().unwrap().provenance = vec![EffectProvenance {symbol: SymbolId(2), effect: "db_write".into(), external_write: true, reads: vec![], writes: vec![]}];
        assert!(to_view(&model).is_none());
        let old: StateModel = serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
        assert!(to_view(&old).is_none());
    }
    #[test]
    fn real_transition_exports_only_its_static_evidence() {
        let mut model = StateModelBuilder::from_effects(&SymbolStore::default(), &BTreeMap::new());
        model.edges[0].provenance.push(EffectProvenance {symbol: SymbolId(3), effect: "print".into(), external_write: false, reads: vec![], writes: vec![]});
        let view = to_view(&model).unwrap();
        assert_eq!(view.transitions.len(), 1);
        assert_eq!(view.transitions[0].evidence.len(), 1);
        assert_eq!(view.transitions[0].display_mode, "Fact");
        assert!(view.domain_note.contains("not observed"));
    }
}
