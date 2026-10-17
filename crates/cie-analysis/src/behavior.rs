//! Behavior extraction (PRD §22 / FR-11): bottom-up interprocedural
//! summaries over the resolved call graph, with effect propagation to a
//! fixpoint. Budget-aware: a deadline cut marks remaining summaries
//! `complete = false` — never silently "no defect".

use cie_core::facts::{BuiltinKind, Callee, LangFacts, RhsKind, SymbolStore};
use cie_core::summary::{builtin_effects, EffectKind, FunctionSummary};
use cie_core::types::{Ns, RevId, SymbolId};
use std::collections::{BTreeMap, BTreeSet};

pub struct BehaviorResult {
    pub summaries: BTreeMap<SymbolId, FunctionSummary>,
    /// callee -> callers (reverse dependencies, PRD FR-8).
    pub reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
}

pub fn is_auth_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("auth")
        || n.contains("permission")
        || n.contains("is_allowed")
        || n.contains("has_access")
        || n.contains("can_")
        || n.contains("check_access")
        || n.contains("check_right")
        || n.contains("verify_")
        || n.contains("grant_")
        || n.contains("deny_")
}

pub fn is_money_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("charge")
        || n.contains("pay")
        || n.contains("refund")
        || n.contains("transfer")
        || n.contains("withdraw")
        || n.contains("debit")
        || n.contains("spend")
        || n.contains("invoice")
        || n.contains("billing")
}

pub fn is_destructive_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("delete")
        || n.starts_with("remove")
        || n.starts_with("drop_")
        || n.starts_with("purge")
        || n.contains("_delete")
}

pub fn is_entry_name(name: &str) -> bool {
    matches!(name, "main" | "index" | "handler" | "app" | "serve" | "run")
        || name.starts_with("handle_")
}

pub fn compute_summaries(
    store: &SymbolStore,
    per_file: &[LangFacts],
    rev: RevId,
    deadline_ns: Option<Ns>,
) -> BehaviorResult {
    let now = cie_core::types::now_ns();
    let mut summaries: BTreeMap<SymbolId, FunctionSummary> = BTreeMap::new();
    let mut reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();

    // Seed per-function facts. Symbol facts live in the global store;
    // per_file carries the behavior facts keyed by global symbol id.
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let sid = sym.id;
        let mut s = FunctionSummary::empty(sid, rev, now);

        for facts in per_file {
            for call in facts.calls.iter().filter(|c| c.caller == sid) {
                match &call.callee {
                    Callee::Builtin(b) => {
                        for e in builtin_effects(*b) {
                            s.effects.insert(*e);
                        }
                        match b {
                            BuiltinKind::DbQuery => {
                                s.reads.insert("db".into());
                            }
                            BuiltinKind::DbExecute => {
                                s.writes.insert("db".into());
                            }
                            BuiltinKind::FileRead => {
                                s.reads.insert("fs".into());
                            }
                            BuiltinKind::FileWrite => {
                                s.writes.insert("fs".into());
                            }
                            BuiltinKind::Env => {
                                s.reads.insert("env".into());
                            }
                            _ => {}
                        }
                    }
                    Callee::Resolved(id) => {
                        s.callees.push(*id);
                        reverse_deps.entry(*id).or_default().insert(sid);
                    }
                    Callee::Unresolved(reason) => {
                        s.unresolved_callees
                            .push(format!("{} ({})", call.callee_text, reason));
                    }
                }
            }

            for r in facts.returns.iter().filter(|r| r.symbol == sid) {
                if let Some(c) = &r.constant {
                    s.returns_constants.insert(c.clone());
                }
            }
            let returns: Vec<_> = facts.returns.iter().filter(|r| r.symbol == sid).collect();
            if !returns.is_empty() {
                s.all_returns_constant = returns.iter().all(|r| r.constant.is_some());
            }

            for x in facts.exceptions.iter().filter(|x| x.symbol == sid) {
                if !x.in_handler {
                    // raised exception = failure path
                    s.exceptions.push(x.kind_text.clone());
                    s.has_failure_path = true;
                }
            }

            if facts.loops.iter().any(|l| l.symbol == sid) {
                s.has_loops = true;
            }

            for a in facts.assignments.iter().filter(|a| a.symbol == sid) {
                if a.rhs_kind == RhsKind::Input {
                    s.tags.insert("reads_input".into());
                }
            }

            // decorators like @app.route make it an entry point
            for c in facts.calls.iter().filter(|c| c.caller == sid) {
                let t = c.callee_text.to_ascii_lowercase();
                if t.contains("route")
                    || t.contains("app.get")
                    || t.contains("app.post")
                    || t.contains("app.put")
                    || t.contains("app.delete")
                    || t.contains("command")
                    || t.contains("task")
                    || t.contains("endpoint")
                {
                    s.tags.insert("entrypoint".into());
                }
            }

            // path conditions from branch conditions (bounded)
            for i in facts.ifs.iter().filter(|i| i.symbol == sid).take(16) {
                s.path_conditions.push(i.cond.clone());
            }
        }

        // returning false/none explicitly is also a failure path
        if s.returns_constants.contains("false") || s.returns_constants.contains("none") {
            s.has_failure_path = true;
        }

        // Tags
        if sym.is_test() {
            s.tags.insert("test".into());
        }
        if is_auth_name(&sym.name) {
            s.tags.insert("auth".into());
        }
        if is_money_name(&sym.name) {
            s.tags.insert("money".into());
        }
        if is_destructive_name(&sym.name) {
            s.tags.insert("destructive".into());
        }
        if is_entry_name(&sym.name) {
            s.tags.insert("entrypoint".into());
        }
        if s.effects.contains(&EffectKind::DbRead) {
            s.tags.insert("query".into());
        }
        if s.effects.contains(&EffectKind::DbWrite) {
            s.tags.insert("mutation".into());
        }

        summaries.insert(sid, s);
    }

    // Fixpoint effect propagation over resolved edges (bounded).
    let mut changed = true;
    let mut iters = 0u32;
    while changed && iters < 64 {
        changed = false;
        iters += 1;
        if let Some(dl) = deadline_ns {
            if cie_core::types::now_ns() > dl {
                for s in summaries.values_mut() {
                    s.complete = false;
                }
                break;
            }
        }
        // Propagate into callers: for each edge callee -> caller.
        let edges: Vec<(SymbolId, SymbolId)> = summaries
            .iter()
            .flat_map(|(caller, s)| s.callees.iter().map(|callee| (*callee, *caller)))
            .collect();
        for (callee, caller) in edges {
            let callee_summary = match summaries.get(&callee) {
                Some(cs) => cs.clone(),
                None => continue,
            };
            if let Some(cs) = summaries.get_mut(&caller) {
                let before = (cs.effects.len(), cs.reads.len(), cs.writes.len(), cs.has_failure_path);
                cs.absorb_callee(&callee_summary);
                if (cs.effects.len(), cs.reads.len(), cs.writes.len(), cs.has_failure_path) != before {
                    changed = true;
                }
            }
        }
    }

    BehaviorResult {
        summaries,
        reverse_deps,
    }
}
