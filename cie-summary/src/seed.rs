//! Per-symbol summary seeds, extracted once in the file-holding pass.
//!
//! The per-file ingest pass (step 3, `put_file` while the parse tree is in
//! hand) groups a file's own behavior facts into one `SummarySeed` per
//! function-like symbol and persists it beside the file's facts. A seed holds
//! exactly what is derivable from a single file in isolation — declared
//! parameters, return shapes, non-handler raises, branch conditions, loop
//! presence, and receiver-independent builtin effect claims. The summary
//! stage then consumes the seed for those local fields instead of re-deriving
//! them by scanning every file's facts again.
//!
//! What is deliberately NOT in a seed, because no single file can decide it:
//! resolved `callees` and the callers inversion (repo-wide resolution
//! ladder), the reasoned strings of `unresolved_callees` (reasons come from
//! the ladder), db receiver verdicts for `reads`/`writes`/db-shaped
//! `effects` (receiver-gated classification), and `complete` (a closure
//! property). Those stay join-owned; everything they need was already
//! persisted by step 3, so nothing re-reads source. A seed still captures
//! `pending_callees` — the callee texts extraction could not resolve — as
//! pre-join evidence.

use cie_core::types::SymbolId;
use cie_semantic::facts::{BindingKind, BuiltinKind, Callee, LangFacts, RhsKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{builtin_effects, EffectKind};

/// The per-symbol payload extracted while a file's facts are in hand.
/// Serialized into the file's `CachedFacts` payload at `put_file` time, so a
/// summary's local fields are derived once per changed file and the summary
/// stage consumes them instead of re-deriving them from scratch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummarySeed {
    /// Symbol id local to the file at extraction time. `assemble_facts`
    /// rewrites it to the global id with the same map it applies to the
    /// facts, so consumers always see global ids and never re-map.
    pub sym: SymbolId,
    /// Declared parameters — `name` or `name: type_text` — in source order.
    pub inputs: Vec<String>,
    /// Distinct return expressions, in first-occurrence order.
    pub outputs: Vec<String>,
    /// Effect claims independent of any receiver judgment: non-db builtins.
    /// Db-shaped calls stay out — their effect depends on the receiver's
    /// resolved class, which only the repo-wide join can classify.
    pub effects: BTreeSet<EffectKind>,
    /// Non-handler raise sites' exception names, in source order.
    pub exceptions: Vec<String>,
    /// Branch conditions, in source order (bounded like the summary stage).
    pub path_conditions: Vec<String>,
    /// Distinct constant return values.
    pub returns_constants: BTreeSet<String>,
    /// True when the symbol has return sites and every one is constant.
    pub all_returns_constant: bool,
    /// A non-handler raise, or an explicit `false`/`none` return.
    pub has_failure_path: bool,
    pub has_loops: bool,
    /// Local evidence tags: `reads_input` (assignment from an input source),
    /// `entrypoint` (route/app/command/task/endpoint decorator text).
    pub tags: BTreeSet<String>,
    /// Callee texts the extraction could not resolve, in source order.
    /// Pre-join evidence: the resolution ladder replaces the pending state —
    /// resolved callees become `callees` edges, the rest gain reasons — so
    /// the summary's final `unresolved_callees` strings carry those reasons.
    #[serde(default)]
    pub pending_callees: Vec<String>,
}

/// Entry-point shape of a call text (route/app.*/command/task/endpoint).
/// One predicate, shared by the seed builder and the behavior stage's
/// fallback derivation, so seeded and fallback summaries always agree.
pub fn is_entrypoint_call_text(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("route")
        || t.contains("app.get")
        || t.contains("app.post")
        || t.contains("app.put")
        || t.contains("app.delete")
        || t.contains("command")
        || t.contains("task")
        || t.contains("endpoint")
}

/// Group a file's own behavior facts into per-symbol local summary seeds.
/// Purely a per-file grouping — it never consults other files or the global
/// symbol store, and therefore never disagrees with the local derivation
/// loops in the summary stage on the same file.
pub fn summary_seeds(facts: &LangFacts) -> Vec<SummarySeed> {
    let mut out = Vec::new();
    for sym in &facts.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let sid = sym.id;
        let mut seed = SummarySeed {
            sym: sid,
            inputs: Vec::new(),
            outputs: Vec::new(),
            effects: BTreeSet::new(),
            exceptions: Vec::new(),
            path_conditions: Vec::new(),
            returns_constants: BTreeSet::new(),
            all_returns_constant: false,
            has_failure_path: false,
            has_loops: false,
            tags: BTreeSet::new(),
            pending_callees: Vec::new(),
        };

        // Declared parameters, in source order.
        let mut params: Vec<&cie_semantic::facts::BindingFact> = facts
            .bindings
            .iter()
            .filter(|b| b.owner == sid && b.kind == BindingKind::Param)
            .collect();
        params.sort_by_key(|b| b.line);
        for p in params {
            seed.inputs.push(match &p.type_text {
                Some(t) => format!("{}: {}", p.name, t),
                None => p.name.clone(),
            });
        }

        // Distinct return shapes; constant-return bookkeeping.
        let mut returns = 0usize;
        let mut constant_returns = 0usize;
        for r in facts.returns.iter().filter(|r| r.symbol == sid) {
            if !seed.outputs.contains(&r.expr) {
                seed.outputs.push(r.expr.clone());
            }
            if let Some(c) = &r.constant {
                seed.returns_constants.insert(c.clone());
                constant_returns += 1;
            }
            returns += 1;
        }
        if returns > 0 {
            seed.all_returns_constant = constant_returns == returns;
        }

        // Non-handler raises: failure path + exception names.
        for x in facts
            .exceptions
            .iter()
            .filter(|x| x.symbol == sid && !x.in_handler)
        {
            seed.exceptions.push(x.kind_text.clone());
            seed.has_failure_path = true;
        }
        // returning false/none explicitly is also a failure path
        if seed.returns_constants.contains("false") || seed.returns_constants.contains("none") {
            seed.has_failure_path = true;
        }

        if facts.loops.iter().any(|l| l.symbol == sid) {
            seed.has_loops = true;
        }

        // Assignment from a declared input source.
        if facts
            .assignments
            .iter()
            .any(|a| a.symbol == sid && a.rhs_kind == RhsKind::Input)
        {
            seed.tags.insert("reads_input".into());
        }

        // Entry points from route/app/task/command/endpoint decorator texts.
        if facts
            .calls
            .iter()
            .any(|c| c.caller == sid && is_entrypoint_call_text(&c.callee_text))
        {
            seed.tags.insert("entrypoint".into());
        }

        // Branch conditions, bounded like the summary stage.
        for i in facts.ifs.iter().filter(|i| i.symbol == sid).take(16) {
            seed.path_conditions.push(i.cond.clone());
        }

        // Receiver-independent builtin effects; the db-shaped claims are
        // left to the receiver-gated join. Pending (unresolved) callee
        // texts are captured per site as pre-join evidence.
        for c in facts.calls.iter().filter(|c| c.caller == sid) {
            if let Callee::Builtin(b) = &c.callee {
                if !matches!(b, BuiltinKind::DbQuery | BuiltinKind::DbExecute) {
                    for e in builtin_effects(*b) {
                        seed.effects.insert(*e);
                    }
                }
            }
            if let Callee::Unresolved(_) = &c.callee {
                seed.pending_callees.push(c.callee_text.clone());
            }
        }

        out.push(seed);
    }
    out
}
