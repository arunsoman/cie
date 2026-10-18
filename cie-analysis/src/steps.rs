//! Transaction step assembly with async hand-off marking (CIE-US US-034
//! engine close).
//!
//! "Walk through a transaction step by step": step ORDER from the call
//! graph ships (RelationGraph / resolved calls are the walk input); what
//! was partial is async hand-off marking. This module assembles the
//! engine half: a linear step list for a transaction's call chain where
//! every step is labelled with its kind — call, async hand-off
//! (spawn/await/channel from the adapter's async facts), or return — and
//! lane transitions are DATA for the host swim-lane view.
//!
//! Honesty: a hand-off is marked only where the adapter classified one
//! (AsyncSite); the engine never invents async-ness from naming alone.

use std::collections::BTreeMap;

use cie_core::types::SymbolId;
use cie_semantic::facts::{AsyncKind, Callee, SymbolStore, TypedFacts};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Call,
    /// Spawn / await / channel — an async hand-off (lane transition).
    AsyncHandoff,
    Return,
}

impl StepKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepKind::Call => "call",
            StepKind::AsyncHandoff => "async_handoff",
            StepKind::Return => "return",
        }
    }
}

/// One step of the walk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionStep {
    pub symbol: SymbolId,
    pub symbol_name: String,
    pub line: u32,
    pub kind: StepKind,
    /// For async hand-offs: the adapter-classified kind (spawn/await/
    /// channel) — the lane the work moves to.
    pub handoff: Option<String>,
}

/// The assembled walk: steps in call-graph order; every async hand-off
/// the adapters classified is marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionWalk {
    pub entry: SymbolId,
    pub steps: Vec<TransactionStep>,
    pub handoffs: usize,
}

/// Assemble the walk from `entry` following resolved calls (DFS in
/// line order, depth-bounded, cycle-safe).
pub fn transaction_walk(
    entry: SymbolId,
    facts: &TypedFacts,
    store: &SymbolStore,
    max_depth: usize,
) -> TransactionWalk {
    let mut async_by_symbol: BTreeMap<SymbolId, Vec<(u32, String)>> = BTreeMap::new();
    for a in &facts.async_facts {
        let k = match a.kind {
            AsyncKind::AsyncFn => "async_fn",
            AsyncKind::Await => "await",
            AsyncKind::Spawn => "spawn",
            AsyncKind::Channel => "channel",
        };
        async_by_symbol
            .entry(a.symbol)
            .or_default()
            .push((a.line, k.to_string()));
    }
    let mut callees: BTreeMap<SymbolId, Vec<(SymbolId, u32)>> = BTreeMap::new();
    for c in &facts.calls {
        if let Callee::Resolved(t) = c.callee {
            callees.entry(c.caller).or_default().push((t, c.line));
        }
    }
    for v in callees.values_mut() {
        v.sort_by_key(|(_, l)| *l);
    }

    let name_of = |id: SymbolId| {
        store
            .get(id)
            .map(|s| s.qual_name.clone())
            .unwrap_or_else(|| format!("symbol#{id}"))
    };

    let mut steps: Vec<TransactionStep> = Vec::new();
    let mut handoffs = 0usize;
    let mut visited: std::collections::BTreeSet<SymbolId> = std::collections::BTreeSet::new();

    fn walk(
        sym: SymbolId,
        depth: usize,
        max_depth: usize,
        visited: &mut std::collections::BTreeSet<SymbolId>,
        callees: &BTreeMap<SymbolId, Vec<(SymbolId, u32)>>,
        async_by_symbol: &BTreeMap<SymbolId, Vec<(u32, String)>>,
        name_of: &dyn Fn(SymbolId) -> String,
        steps: &mut Vec<TransactionStep>,
        handoffs: &mut usize,
    ) {
        if depth > max_depth || !visited.insert(sym) {
            return;
        }
        let name = name_of(sym);
        match async_by_symbol.get(&sym).and_then(|v| v.first()) {
            Some((line, kind)) => {
                steps.push(TransactionStep {
                    symbol: sym,
                    symbol_name: name,
                    line: *line,
                    kind: StepKind::AsyncHandoff,
                    handoff: Some(kind.clone()),
                });
                *handoffs += 1;
            }
            None => steps.push(TransactionStep {
                symbol: sym,
                symbol_name: name,
                line: 0,
                kind: StepKind::Call,
                handoff: None,
            }),
        }
        match callees.get(&sym) {
            Some(nexts) => {
                for (t, _) in nexts {
                    walk(*t, depth + 1, max_depth, visited, callees, async_by_symbol, name_of, steps, handoffs);
                }
            }
            None => steps.push(TransactionStep {
                symbol: sym,
                symbol_name: name_of(sym),
                line: 0,
                kind: StepKind::Return,
                handoff: None,
            }),
        }
    }

    walk(
        entry,
        0,
        max_depth,
        &mut visited,
        &callees,
        &async_by_symbol,
        &name_of,
        &mut steps,
        &mut handoffs,
    );
    TransactionWalk {
        entry,
        steps,
        handoffs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::{FileId, SourceLocation};
    use cie_semantic::facts::{AsyncSite, CallSite, SymbolFact, SymbolKind};
    use cie_semantic::lang::LangId;

    fn fact(id: u32, qual: &str) -> SymbolFact {
        SymbolFact {
            id: SymbolId(id),
            kind: SymbolKind::Function,
            name: qual.rsplit("::").next().unwrap().into(),
            qual_name: qual.into(),
            loc: SourceLocation::span_bytes(FileId(0), 0, 10),
            lang: LangId::Python,
            parent: None,
            rev: 1,
            doc: None,
        }
    }

    fn call(caller: u32, callee: u32, line: u32) -> CallSite {
        CallSite {
            caller: SymbolId(caller),
            callee_text: String::new(),
            callee: Callee::Resolved(SymbolId(callee)),
            line,
            in_loop: false,
            arg_vars: vec![],
        }
    }

    #[test]
    fn steps_walk_in_order_and_mark_async_handoffs() {
        // entry(1) → charge(2) [await @ line 7] → post(3)
        let facts = TypedFacts {
            symbols: vec![fact(1, "api::entry"), fact(2, "pay::charge"), fact(3, "ledger::post")],
            calls: vec![call(1, 2, 3), call(2, 3, 9)],
            async_facts: vec![AsyncSite {
                symbol: SymbolId(2),
                kind: AsyncKind::Await,
                line: 7,
            }],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let w = transaction_walk(SymbolId(1), &facts, &store, 8);
        assert_eq!(w.handoffs, 1);
        assert_eq!(w.steps.len(), 4); // entry, charge(handoff), post, post-return
        assert_eq!(w.steps[0].kind, StepKind::Call);
        assert_eq!(w.steps[1].kind, StepKind::AsyncHandoff);
        assert_eq!(w.steps[1].handoff.as_deref(), Some("await"));
        assert_eq!(w.steps[1].line, 7);
        assert_eq!(w.steps[3].kind, StepKind::Return);
        // step order follows the call graph
        assert_eq!(w.steps[0].symbol, SymbolId(1));
        assert_eq!(w.steps[1].symbol, SymbolId(2));
        assert_eq!(w.steps[2].symbol, SymbolId(3));
    }

    #[test]
    fn no_async_facts_means_no_invented_handoffs() {
        let facts = TypedFacts {
            symbols: vec![fact(1, "a::f"), fact(2, "a::g")],
            calls: vec![call(1, 2, 1)],
            ..Default::default()
        };
        let store = SymbolStore::build(facts.symbols.clone());
        let w = transaction_walk(SymbolId(1), &facts, &store, 8);
        assert_eq!(w.handoffs, 0);
        assert!(w.steps.iter().all(|s| s.kind != StepKind::AsyncHandoff));
    }
}
