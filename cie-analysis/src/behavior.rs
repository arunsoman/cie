//! Behavior extraction (PRD §22 / FR-11): bottom-up interprocedural
//! summaries over the resolved call graph, with effect propagation to a
//! fixpoint. Budget-aware: a deadline cut marks remaining summaries
//! `complete = false` — never silently "no defect".

use cie_core::types::{Ns, RevId, SymbolId};
use cie_semantic::facts::{BuiltinKind, Callee, CallSite, LangFacts, RhsKind, SymbolFact, SymbolStore};
use cie_summary::{
    builtin_effects, CallSiteRecord, CallSpan, EffectClaim, EffectKind, FunctionSummary,
    LexicalHint,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Authorization annotation surfaces — a `auth` tag requires one of these
/// (or an authorization-library callee). Lexical name matches alone are
/// typed hints, never the tag (behavioural-evidence redesign).
const AUTHZ_ANN_PATTERNS: &[&str] = &[
    "preauthorize",
    "preauthenticate",
    "secured",
    "rolesallowed",
    "permitall",
    "denyall",
    "hasauthority",
    "hasrole",
];

/// Authorization-library callee surfaces (receiver-qualified texts match).
const AUTHZ_CALLEE_PATTERNS: &[&str] = &[
    "securitycontext",
    "accessdenied",
    "hasauthority",
    "hasrole",
    "authenticationmanager",
    "authorizationmanager",
    "grantedauthority",
    "authorize",
    "checkright",
    "checkaccess",
];

pub struct BehaviorResult {
    pub summaries: BTreeMap<SymbolId, FunctionSummary>,
    /// callee -> callers (reverse dependencies, PRD FR-8).
    pub reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// TASK-030 reuse counters for this pass.
    pub reuse: SummaryReuseStats,
}

/// TASK-030 reuse counters: `reused_files` had summaries carried over
/// verbatim (unchanged file AND unchanged dependency set); `recomputed_files`
/// were rebuilt this pass (changed file ⇒ recompute plus transitive
/// reverse-deps via the fixpoint).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryReuseStats {
    pub reused_files: usize,
    pub recomputed_files: usize,
    /// TASK-056: symbol-level counters (seeded vs invalidated worklist).
    pub reused_symbols: usize,
    pub recomputed_symbols: usize,
}

/// TASK-030 reuse decision inputs, assembled by the engine from the prior
/// snapshot. A file is reusable iff its content hash AND its dep-set hash
/// (`blake3` over the sorted content hashes of the files its resolved
/// callees live in) match the prior analysis.
#[derive(Debug, Clone, Default)]
pub struct ReusePlan {
    /// path -> (file hash, dep-set hash) valid at the prior analysis.
    pub prior_keys: BTreeMap<String, ([u8; 32], [u8; 32])>,
    /// Prior summaries keyed by (path, qual_name) — stable identity across
    /// revisions where raw symbol ids are not.
    pub prior_summaries: BTreeMap<(String, String), FunctionSummary>,
    /// Current per-file content hashes.
    pub current_hashes: BTreeMap<String, [u8; 32]>,
    /// Current dep-set: file -> sorted paths of files its callees live in.
    pub current_deps: BTreeMap<String, BTreeSet<String>>,
    /// TASK-056: symbols allowed to seed from prior summaries — the clean
    /// set under the symbol-level dirty worklist (changed symbols and their
    /// transitive callers are excluded).
    pub reusable_symbols: BTreeSet<(String, String)>,
}

impl ReusePlan {
    /// blake3 over the sorted dep file hashes (the "dep set" half of the
    /// (blake3(file), blake3(dep set)) cache key).
    pub fn dep_hash(&self, path: &str) -> [u8; 32] {
        let mut buf: Vec<u8> = vec![];
        if let Some(deps) = self.current_deps.get(path) {
            for d in deps {
                buf.extend_from_slice(self.current_hashes.get(d).unwrap_or(&[0; 32]));
            }
        }
        cie_core::types::content_hash(&buf)
    }

    /// True when `path` may reuse its prior summaries this pass.
    pub fn is_reusable(&self, path: &str) -> bool {
        let Some(file_hash) = self.current_hashes.get(path) else {
            return false;
        };
        let deps_hash = self.dep_hash(path);
        self.prior_keys.get(path) == Some(&(*file_hash, deps_hash))
    }
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
    compute_summaries_with_reuse(store, per_file, rev, deadline_ns, None)
}

/// TASK-030: summary computation with per-file reuse. A file whose content
/// hash and dep-set hash both match the prior analysis reuses its stored
/// `FunctionSummary`s verbatim (only rev/sym re-stamped); every other file
/// is recomputed and the fixpoint re-propagates into its transitive
/// reverse-deps (absorb is monotone, so reused seeds converge to the same
/// fixpoint as a clean recompute — pinned by the reuse test).
pub fn compute_summaries_with_reuse(
    store: &SymbolStore,
    per_file: &[LangFacts],
    rev: RevId,
    deadline_ns: Option<Ns>,
    reuse: Option<&ReusePlan>,
) -> BehaviorResult {
    let now = cie_core::types::now_ns();
    let mut summaries: BTreeMap<SymbolId, FunctionSummary> = BTreeMap::new();
    let mut reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>> = BTreeMap::new();
    let mut stats = SummaryReuseStats::default();
    let mut seeded: BTreeSet<SymbolId> = BTreeSet::new();

    // Decide reuse once per file.
    let _reusable_paths: BTreeMap<String, bool> = match reuse {
        Some(p) => p
            .current_hashes
            .keys()
            .map(|k| (k.clone(), p.is_reusable(k)))
            .collect(),
        None => BTreeMap::new(),
    };
    // Seed per-function facts. Symbol facts live in the global store;
    // per_file carries the behavior facts keyed by global symbol id.
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let sid = sym.id;
        let prior = reuse.and_then(|p| {
            // TASK-056: seeding is per-symbol via the dirty worklist.
            let key = (store.files.path(sym.loc.file).to_string(), sym.qual_name.clone());
            if p.reusable_symbols.contains(&key) {
                p.prior_summaries.get(&key)
            } else {
                None
            }
        });
        let mut s = match prior {
            Some(prev) => {
                let mut s = prev.clone();
                s.sym = sid;
                s.rev = rev;
                s
            }
            None => FunctionSummary::empty(sid, rev, now),
        };
        let fresh = prior.is_none();
        if fresh {
            stats.recomputed_symbols += 1;
        } else {
            stats.reused_symbols += 1;
            seeded.insert(sid);
        }
        // this symbol's call sites with their file facts, in source order
        // (empty for reused summaries — their records ride in the prior copy).
        let mut sym_calls: Vec<(&LangFacts, &CallSite)> = vec![];
        if fresh {
            // ---- per-occurrence call-site records + receiver-aware effects ----
            for facts in per_file {
                for call in facts.calls.iter().filter(|c| c.caller == sid) {
                    sym_calls.push((facts, call));
                }
            }
            sym_calls.sort_by(|a, b| {
                (a.1.span.1, std::cmp::Reverse(a.1.span.0), a.1.line)
                    .cmp(&(b.1.span.1, std::cmp::Reverse(b.1.span.0), b.1.line))
            });
            let mut statement_lines: Vec<u32> = sym_calls.iter().map(|(_, c)| c.line).collect();
            statement_lines.sort_unstable();
            statement_lines.dedup();
            let mut occurrence: BTreeMap<String, u32> = BTreeMap::new();

            for (order, (facts, call)) in sym_calls.iter().enumerate() {
                let rclass = call
                    .receiver
                    .as_deref()
                    .map(|r| crate::receiver::resolve_receiver_project(store, facts, per_file, sid, r))
                    .unwrap_or(crate::receiver::ReceiverClass::Unknown);
                let method = call
                    .callee_text
                    .rsplit('.')
                    .next()
                    .unwrap_or(&call.callee_text)
                    .to_ascii_lowercase();

                // ---- resolution dimensions (declaration / dispatch) ----
                let (declaration, dispatch, receiver_type): (&str, &str, Option<String>) =
                    match &call.callee {
                        Callee::Resolved(_) => ("resolved", "exact", None),
                        Callee::Builtin(_) => match &rclass {
                            crate::receiver::ReceiverClass::Db { identity, .. } => {
                                ("external", "external", Some(identity.clone()))
                            }
                            crate::receiver::ReceiverClass::InMemory { identity } => {
                                ("external", "exact", Some(identity.clone()))
                            }
                            crate::receiver::ReceiverClass::InTree { .. } => {
                                ("external", "external", None)
                            }
                            crate::receiver::ReceiverClass::ExternalType { identity } => {
                                ("external", "external", Some(identity.clone()))
                            }
                            crate::receiver::ReceiverClass::Unknown => {
                                ("unresolved", "unknown", None)
                            }
                        },
                        Callee::Unresolved(reason) => match reason.as_str() {
                            "reflective" => ("unresolved", "reflective", None),
                            "external_module" => ("external", "external", None),
                            _ => match &rclass {
                                crate::receiver::ReceiverClass::Db { identity, .. } => {
                                    ("external", "external", Some(identity.clone()))
                                }
                                crate::receiver::ReceiverClass::InMemory { identity } => {
                                    ("external", "exact", Some(identity.clone()))
                                }
                                crate::receiver::ReceiverClass::InTree { identity, symbol } => {
                                    let declared = store
                                        .children_of(*symbol)
                                        .iter()
                                        .any(|c| {
                                            store.get(*c).map(|m| m.name == call.callee_text.rsplit('.').next().unwrap_or(&call.callee_text)).unwrap_or(false)
                                        });
                                    if declared {
                                        ("resolved", "candidate", Some(identity.clone()))
                                    } else {
                                        ("unresolved", "unknown", Some(identity.clone()))
                                    }
                                }
                                crate::receiver::ReceiverClass::ExternalType { identity } => {
                                    ("external", "external", Some(identity.clone()))
                                }
                                crate::receiver::ReceiverClass::Unknown => {
                                    ("unresolved", "unknown", None)
                                }
                            },
                        },
                    };

                // Declaration navigation must retain identity, not just a type label.
                // Candidate targets never enter exact `callees` or must-effect propagation.
                let target_symbols: Vec<SymbolId> = match &call.callee {
                    Callee::Resolved(id) => vec![*id],
                    Callee::Unresolved(reason) if reason == "reflective" => vec![],
                    _ => {
                        if let crate::receiver::ReceiverClass::InTree { symbol, .. } = &rclass {
                            let name = call.callee_text.rsplit('.').next().unwrap_or(&call.callee_text);
                            store.children_of(*symbol).iter().copied().filter(|id| {
                                store.get(*id).map(|m| m.is_function_like() && m.name == name).unwrap_or(false)
                            }).filter(|id| {
                                let params = per_file.iter().flat_map(|f| &f.bindings)
                                    .filter(|b| b.owner == *id && b.kind == cie_semantic::facts::BindingKind::Param).count();
                                call.arguments.map(|n| params == n as usize).unwrap_or(true)
                            }).collect()
                        } else { vec![] }
                    }
                };
                let (declaration, dispatch, receiver_type) = if !target_symbols.is_empty() && !matches!(call.callee, Callee::Resolved(_)) {
                    let identity = match &rclass { crate::receiver::ReceiverClass::InTree { identity, .. } => Some(identity.clone()), _ => receiver_type };
                    (if target_symbols.len() == 1 { "resolved" } else { "ambiguous" }, "candidate", identity)
                } else { (declaration, dispatch, receiver_type) };
                // ---- effects: receiver-gated, with provenance or demotion ----
                let mut rec_effect: Option<EffectClaim> = None;
                let mut rec_hint: Option<String> = None;
                let mut db_labels = |s: &mut FunctionSummary, k: BuiltinKind| {
                    match k {
                        BuiltinKind::DbQuery => {
                            s.reads.insert("db".into());
                        }
                        BuiltinKind::DbExecute => {
                            s.writes.insert("db".into());
                        }
                        _ => {}
                    }
                };
                match &call.callee {
                    Callee::Builtin(b) => {
                        let db_receiver = match &rclass {
                            crate::receiver::ReceiverClass::Db { identity, basis } => {
                                Some((identity.clone(), basis.to_string()))
                            }
                            _ => None,
                        };
                        match db_receiver {
                            Some((ident, basis)) => {
                                // the receiver IS a database API: persistence-
                                // shaped methods are db claims WITH provenance,
                                // superseding the spelling classification.
                                if let Some(k) =
                                    cie_adapters::effects::persistence_kind(&method)
                                {
                                    let kind = if k == BuiltinKind::DbExecute {
                                        EffectKind::DbWrite
                                    } else {
                                        EffectKind::DbRead
                                    };
                                    s.effects.insert(kind);
                                    db_labels(&mut s, k);
                                    s.receiver.insert(ident.clone());
                                    s.resolution_basis.insert(basis.clone());
                                    rec_effect = Some(EffectClaim {
                                        kind,
                                        receiver: ident,
                                        resolution_basis: basis,
                                    });
                                } else {
                                    for e in builtin_effects(*b) {
                                        s.effects.insert(*e);
                                    }
                                    db_labels(&mut s, *b);
                                }
                            }
                            None => {
                                if matches!(b, BuiltinKind::DbQuery | BuiltinKind::DbExecute)
                                    && matches!(
                                        rclass,
                                        crate::receiver::ReceiverClass::InMemory { .. }
                                            | crate::receiver::ReceiverClass::InTree { .. }
                                            | crate::receiver::ReceiverClass::ExternalType { .. }
                                            | crate::receiver::ReceiverClass::Unknown
                                    )
                                {
                                    // spelling alone on a non-DB/unknown receiver:
                                    // a typed lexical hint, never a db effect.
                                    let hk = if *b == BuiltinKind::DbQuery {
                                        "db_read"
                                    } else {
                                        "db_write"
                                    };
                                    let note = format!(
                                        "'{}' matches a db spelling but the receiver is {} — demoted to a lexical hint, not a db effect",
                                        call.callee_text,
                                        rclass.describe()
                                    );
                                    s.lexical_hints.push(LexicalHint {
                                        hint: hk.into(),
                                        basis: "lexical-spelling".into(),
                                        note,
                                    });
                                    rec_hint = Some(hk.to_string());
                                } else {
                                    for e in builtin_effects(*b) {
                                        s.effects.insert(*e);
                                    }
                                    db_labels(&mut s, *b);
                                }
                            }
                        }
                    }
                    Callee::Resolved(id) => {
                        s.callees.push(*id);
                    }
                    Callee::Unresolved(reason) => {
                        s.unresolved_callees
                            .push(format!("{} ({})", call.callee_text, reason));
                        if let crate::receiver::ReceiverClass::Db { identity, basis } = &rclass {
                            // Java-style repository access never reaches the
                            // builtin classifier — a db-typed receiver with a
                            // persistence-shaped method IS the db claim.
                            if let Some(k) = cie_adapters::effects::persistence_kind(&method) {
                                let kind = if k == BuiltinKind::DbExecute {
                                    EffectKind::DbWrite
                                } else {
                                    EffectKind::DbRead
                                };
                                s.effects.insert(kind);
                                db_labels(&mut s, k);
                                s.receiver.insert(identity.clone());
                                s.resolution_basis.insert(basis.to_string());
                                rec_effect = Some(EffectClaim {
                                    kind,
                                    receiver: identity.clone(),
                                    resolution_basis: basis.to_string(),
                                });
                            }
                        }
                    }
                }

                // ---- the record itself ----
                let occ = occurrence.entry(call.callee_text.clone()).or_insert(0);
                *occ += 1;
                let statement_index = statement_lines
                    .iter()
                    .position(|l| *l == call.line)
                    .map(|i| (i + 1) as u32)
                    .unwrap_or(0);
                s.call_sites.push(CallSiteRecord {
                    callee: call.callee_text.clone(),
                    target_symbols,
                    receiver: call.receiver.clone(),
                    receiver_type,
                    declaration: declaration.into(),
                    dispatch: dispatch.into(),
                    observation: "unobserved".into(),
                    span: CallSpan {
                        start: call.span.0,
                        end: call.span.1,
                    },
                    occurrence: *occ,
                    arguments: call.arguments,
                    guard: guard_for(facts, sid, call),
                    source_order: order as u32,
                    statement_index,
                    line: call.line,
                    effect: rec_effect,
                    hint: rec_hint,
                });
            }

            for facts in per_file {
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
        }

        // returning false/none explicitly is also a failure path
        if s.returns_constants.contains("false") || s.returns_constants.contains("none") {
            s.has_failure_path = true;
        }

        // Tags
        if sym.is_test() {
            s.tags.insert("test".into());
        }
        // auth: a semantic claim requiring non-lexical evidence — an
        // authorization annotation on the symbol or an authorization-library
        // callee. A name match alone is a typed lexical hint, never the tag.
        let authz_evidence = authz_evidence(store, sym, &sym_calls);
        if authz_evidence {
            s.tags.insert("auth".into());
        }
        if fresh {
            if !authz_evidence && is_auth_name(&sym.name) {
                s.lexical_hints.push(LexicalHint {
                    hint: "auth".into(),
                    basis: "lexical-name-match".into(),
                    note: "no authorization annotation and no authorization-library callee observed in this revision".into(),
                });
            }
            // the former naming-convention MustAuthorize contracts (money/
            // destructive names) are retired to hints — no normative claim is
            // asserted from a spelling alone.
            if (is_money_name(&sym.name) || is_destructive_name(&sym.name)) && !authz_evidence {
                s.lexical_hints.push(LexicalHint {
                    hint: "authorize-first".into(),
                    basis: "lexical-name-match".into(),
                    note: format!(
                        "'{}' matches a money/destructive naming convention; no authorization evidence is asserted — the former naming-convention contract is now this hint",
                        sym.name
                    ),
                });
            }
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

    // File-level stats: a file is reused iff every function-like symbol in
    // it was seeded; interface-only files follow the (file,deps) key check.
    if reuse.is_some() {
        let mut path_total: BTreeMap<&str, usize> = BTreeMap::new();
        let mut path_seeded: BTreeMap<&str, usize> = BTreeMap::new();
        for sym in &store.symbols {
            if !sym.is_function_like() {
                continue;
            }
            *path_total.entry(store.files.path(sym.loc.file)).or_default() += 1;
            if seeded.contains(&sym.id) {
                *path_seeded.entry(store.files.path(sym.loc.file)).or_default() += 1;
            }
        }
        for path in reuse.map(|p| p.current_hashes.keys()).unwrap_or_default() {
            let all_seeded = path_seeded.get(path.as_str()) == path_total.get(path.as_str());
            if all_seeded {
                stats.reused_files += 1;
            } else {
                stats.recomputed_files += 1;
            }
        }
    }

    // Reverse dependencies from the final callee lists (covers reused files).
    for (caller, s) in &summaries {
        for c in &s.callees {
            reverse_deps.entry(*c).or_default().insert(*caller);
        }
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
                let before = (
                    cs.effects.len(),
                    cs.reads.len(),
                    cs.writes.len(),
                    cs.has_failure_path,
                );
                cs.absorb_callee(&callee_summary);
                if (
                    cs.effects.len(),
                    cs.reads.len(),
                    cs.writes.len(),
                    cs.has_failure_path,
                ) != before
                {
                    changed = true;
                }
            }
        }
    }

    BehaviorResult {
        summaries,
        reverse_deps,
        reuse: stats,
    }
}

/// Enclosing guard of a call site: the innermost `if` condition whose span
/// contains the call (facts.ifs carry §14.1 branch spans), else the loop
/// marker, else "unconditional". This is what makes per-occurrence guards
/// reconstructible without re-parsing.
fn guard_for(facts: &LangFacts, sid: SymbolId, call: &CallSite) -> String {
    let mut best: Option<&cie_semantic::facts::IfSite> = None;
    for i in facts.ifs.iter().filter(|i| i.symbol == sid) {
        let contains = if i.loc.end > i.loc.start {
            call.span.0 >= i.loc.start && call.span.1 <= i.loc.end
        } else {
            // adapters without branch spans: line equality fallback
            i.line == call.line
        };
        if contains && best.map(|b| i.loc.start >= b.loc.start).unwrap_or(true) {
            best = Some(i);
        }
    }
    if let Some(i) = best {
        return i.cond.chars().take(160).collect();
    }
    if call.in_loop {
        "loop".to_string()
    } else {
        "unconditional".to_string()
    }
}

/// Non-lexical authorization evidence for a symbol: an authorization
/// annotation on the declaration, or an authorization-library callee
/// (receiver-qualified call text or resolved callee name).
fn authz_evidence(
    store: &SymbolStore,
    sym: &SymbolFact,
    calls: &[(&LangFacts, &CallSite)],
) -> bool {
    let ann_hit = sym.annotations.iter().any(|a| {
        let al = a.to_ascii_lowercase();
        AUTHZ_ANN_PATTERNS.iter().any(|p| al.contains(p))
    });
    if ann_hit {
        return true;
    }
    for (_, call) in calls {
        let t = call.callee_text.to_ascii_lowercase();
        if AUTHZ_CALLEE_PATTERNS.iter().any(|p| t.contains(p)) {
            return true;
        }
        if let Callee::Resolved(id) = &call.callee {
            if let Some(cs) = store.get(*id) {
                let n = cs.name.to_ascii_lowercase();
                if AUTHZ_CALLEE_PATTERNS.iter().any(|p| n.contains(p)) {
                    return true;
                }
            }
        }
    }
    false
}
