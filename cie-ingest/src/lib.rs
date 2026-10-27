//! Engine orchestration (PRD §12 pipeline): walk → hash → plan → extract →
//! analyze → persist snapshot → report. Incremental by construction.

pub mod churn;
pub mod cochange;
pub mod history;
pub mod pipeline;
pub mod sample;
pub mod stream;
pub use cie_storage::relations_store;
pub use cie_storage::stack_store;
pub use cie_storage::state_model_store;
pub mod navigation;
pub use cie_storage::org_store;
pub use cie_storage::vector_store;

pub use stream::{FactsSource, SourceReceipt};

pub use churn::{
    churn_factors, co_change, co_change_coupling, ChurnExclusions, ChurnFactor, ChurnReport,
    CoChangeCoupling, CoChangeCouplingReport, CoChangeEdge, ExclusionEffect,
};
use cie_analysis::{self, AnalysisConfig};
use cie_claims::persist::ClaimStoreExt;
use cie_core::types::{content_hash, now_ns, Hash, Ns, RevId, SymbolId};
use cie_drift::persist::DriftStoreExt;
use cie_incremental::snapshot::{RevSnapshot, SnapshotStoreExt};
use cie_incremental::{plan_ingest, FileIndex, FileRecord, IngestPlan};
use cie_llm::persist::LabelStoreExt;
use cie_llm::{Budget, Labeler, OfflineProvider, ProviderKind};
use cie_semantic::facts::SymbolStore;
use cie_semantic::lang_for_path;
use cie_semantic::LangId;
use cie_store::Store;
use cie_summary::{ReuseStats, SummaryCacheEntry};
pub use history::{
    archaeology_chain, bind_claims_to_history, parse_blame, parse_log, parse_numstat, BlameLine,
    CommitMeta, FileTouch, GitHistoryProvider, HistoryFacts, HistoryOptions, HistoryUnavailable,
    SystemGit,
};
use relations_store::RelationGraphStoreExt;
use stack_store::StackStoreExt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use vector_store::EmbeddingStoreExt;

/// RFC-0024 (concepts-tier staleness): the concepts shape the current
/// engine builds. Bump ONLY when a full ingest must re-run to refresh
/// older stores' concepts trees. The tier was introduced with the
/// stack-fed C4 tiers (RFC-0019 stage 4); snapshots carrying a smaller
/// marker (or none — older engines) are re-derived once, then the fast
/// path holds again.
/// RFC-0024: which concepts shape the snapshot carries. v2 = the
/// stack-fed C4 tiers; v3 = + the induction chain (A1) and the
/// usecase-first projection (A5). Bumping the version sends every
/// folder's next ingest through ONE full pass, which rebuilds the
/// concepts tree and fills `usecase_view`, then the fast path holds.
pub const CONCEPTS_TIER_VERSION: u8 = 4;

/// RFC-0024: does the head revision's persisted snapshot predate the
/// current concepts-tier version? True when it loads but carries a
/// smaller (or absent) marker — the fast path then refuses to serve it
/// and lets the full pass rebuild. Load/store errors answer false: the
/// fast path serves what it has, exactly as before this check.
fn head_concepts_tier_stale(store: &Store) -> bool {
    let last = store.last_rev();
    if last == 0 {
        return false;
    }
    store
        .load_snapshot(last)
        .ok()
        .flatten()
        .map(|s| s.concepts_tier != Some(CONCEPTS_TIER_VERSION))
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub langs: Option<Vec<LangId>>,
    pub budget_ms: Option<u64>,
    pub no_llm: bool,
    pub llm: ProviderKind,
    pub llm_budget_usd: f64,
    pub include_coverage_gaps: bool,
    /// Reuse hash-matched semantic facts or force fresh extraction. Both
    /// modes use the same shared-tree pipeline and language adapters.
    pub facts_from: FactsSource,
}

impl Default for IngestOptions {
    fn default() -> Self {
        IngestOptions {
            langs: None,
            budget_ms: None,
            no_llm: true,
            llm: ProviderKind::Offline,
            llm_budget_usd: 1.0,
            include_coverage_gaps: true,
            facts_from: FactsSource::Polyglot,
        }
    }
}

pub struct IngestOutcome {
    pub rev: RevId,
    pub plan: IngestPlan,
    pub snapshot: RevSnapshot,
    pub store: SymbolStore,
    pub project_dir: PathBuf,
    pub reverse_deps: std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>>,
    pub test_map: cie_analysis::testing::TestMap,
    /// TASK-030: per-file summary reuse counters for this pass.
    pub summary_reuse: cie_analysis::behavior::SummaryReuseStats,
    /// File reads, shared parses and semantic cache reuse (None on snapshot-only outcomes).
    pub source_receipt: Option<SourceReceipt>,
}

/// Rebuild the call graph + test map from stored summaries.
type GraphTriple = (
    std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>>,
    cie_analysis::testing::TestMap,
    std::collections::BTreeMap<SymbolId, Vec<SymbolId>>,
);

fn rebuild_graphs(store: &SymbolStore, summaries: &[cie_summary::FunctionSummary]) -> GraphTriple {
    let mut callees_of: std::collections::BTreeMap<SymbolId, Vec<SymbolId>> =
        std::collections::BTreeMap::new();
    let mut reverse_deps: std::collections::BTreeMap<
        SymbolId,
        std::collections::BTreeSet<SymbolId>,
    > = std::collections::BTreeMap::new();
    for s in summaries {
        callees_of.insert(s.sym, s.callees.clone());
        for c in &s.callees {
            reverse_deps.entry(*c).or_default().insert(s.sym);
        }
    }
    let test_map = cie_analysis::testing::build_test_map(store, &callees_of);
    (reverse_deps, test_map, callees_of)
}

pub const IGNORED: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    ".venv",
    "venv",
    "dist",
    "build",
    "__pycache__",
    ".cie",
    ".idea",
    ".vscode",
    ".github",
];

/// RFC-0019 §6.2: the walk's result — language files for the adapter lens
/// PLUS the stack-lens inputs (manifests/docker/config) in their own bounded
/// map, same walk, no second traversal.
#[derive(Debug, Clone, Default)]
pub struct Walked {
    pub files: BTreeMap<String, (Hash, LangId)>,
    pub stack_inputs: BTreeMap<String, Hash>,
    /// The already-read BODIES for `stack_inputs`, captured by the same walk
    /// (utf8 text, ≤ MAX_FILE_BYTES — the walk's own gates). The stack lens
    /// consumes these instead of re-opening the files from disk: the read
    /// phase paid for these bytes once, so step 5 never pays again. Anything
    /// absent here (non-utf8, walk-skipped) falls back to the lens' disk
    /// read, byte-identical to before.
    pub stack_input_sources: BTreeMap<String, String>,
}

/// Walk the folder and hash every analyzable source file.
pub fn walk_and_hash(root: &Path, langs: Option<&Vec<LangId>>) -> std::io::Result<Walked> {
    let mut out = Walked::default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !IGNORED.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path)?;
            // stack-lens inputs ride the same walk (RFC-0019 §6.2) — never
            // filtered by `langs`: a python-only analysis still reads the
            // compose file that says how its service is governed.
            if cie_stack::is_stack_input(&rel) {
                out.stack_inputs.insert(rel.clone(), content_hash(&bytes));
                // same handoff as the live walk: keep the already-read body
                // for the stack lens (utf8 only — otherwise the lens falls
                // back to its own disk read, same failure as before)
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    out.stack_input_sources.insert(rel.clone(), text.into());
                }
                continue;
            }
            let lang = lang_for_path(&name);
            if lang == LangId::Unknown {
                continue;
            }
            if let Some(want) = langs {
                if !want.contains(&lang) {
                    continue;
                }
            }
            out.files.insert(rel, (content_hash(&bytes), lang));
        }
    }
    Ok(out)
}

/// Run the ingest/analysis pipeline for a project folder.
pub fn run_ingest(project: &Path, opts: &IngestOptions) -> std::io::Result<IngestOutcome> {
    run_ingest_with_progress(project, opts, &|_, _, _| {})
}

/// Observe real phase boundaries without coupling ingestion to a transport or UI.
pub fn run_ingest_with_progress(
    project: &Path,
    opts: &IngestOptions,
    progress: &dyn Fn(&str, &str, &str),
) -> std::io::Result<IngestOutcome> {
    progress(
        "read",
        "running",
        "Streaming source files through parsing and cached fact reuse…",
    );
    let t0 = now_ns();
    let registry = default_registry();
    let stream =
        stream::StreamSession::capture_deferred_with_progress(project, &registry, opts, progress)?;
    progress(
        "read",
        "done",
        &stream.receipt.disclosure().unwrap_or_default(),
    );
    let store = Store::open(project)?;
    let walked = stream.walked.clone();
    let current = walked.files;
    let prev_index = store.load_index()?;
    let plan = plan_ingest(prev_index.as_ref(), &current, &walked.stack_inputs);

    // TASK-028: deletions leave tombstones (`file:{path}:tombstone`, §15.1)
    // so stale facts are purged explicitly; re-appearing paths clear theirs.
    // Batched (DS-EVAL §6.4): one load+write per run, not per file — the
    // per-record whole-file rewrite is O(n²) in bytes.
    store.mark_tombstones(plan.deleted.iter().cloned())?;
    store.clear_tombstones(plan.changed.iter().chain(plan.added.iter()).cloned())?;

    // Fast path: nothing changed (acceptance #10). Deliberately does NOT
    // load the analysis snapshot — re-ingest wall time is hashing + plan only.
    // RFC-0024 exception: the head snapshot may predate the concepts-tier
    // marker (built by an older engine — before the stack-fed C4 tiers, or
    // by any future one with a bumped tier version). Serving it forever
    // would hand consumers a pre-tier concepts tree though the code changed:
    // load the head snapshot ONCE here (a cheap json read next to the full
    // re-hash above) and, when it predates the marker, fall through to the
    // full pass — which rebuilds the concepts (and everything else) and
    // re-stamps; the fast path then holds again for every run after that.
    if opts.facts_from == FactsSource::Polyglot
        && !plan.has_work()
        && !head_concepts_tier_stale(&store)
        && stream.has_revision(store.last_rev())
    {
        let last = store.last_rev();
        let wall = now_ns() - t0;
        let mut index = prev_index.unwrap_or_default();
        index.last_reingest_ns = wall;
        // RFC-0019 §6.2: the stack-inputs map persists into the index —
        // the next plan diffs against it.
        index.stack_inputs = plan.stack_inputs.clone();
        store.save_index(&index)?;
        // RFC-0019 §6.1/§6.2: a no-op ingest answers the stack lens from
        // the persisted artifact — EXCEPT when the stack inputs themselves
        // changed (manifest/docker/config-only change): re-run ONLY the
        // stack stage, refresh the head revision's artifact in place (the
        // revision's language content is unchanged, so its identity holds),
        // and keep the summary-reuse fast path fully intact for the adapter
        // lens — its inputs didn't change.
        let prior_stack = if last > 0 {
            store.load_stack(last).ok().flatten()
        } else {
            None
        };
        let stack_snapshot = if !plan.stack_inputs_changed.is_empty() {
            progress(
                "stack",
                "running",
                "Refreshing framework evidence for the current revision…",
            );
            // §6.2: the bodies this run's read phase already captured feed the
            // lens — the refresh re-reads nothing it already read.
            let refreshed = cie_stack::stack_report_stage_with(
                project,
                Some(&stream.walked.stack_input_sources),
                None,
            );
            if last > 0 {
                store.save_stack(last, &refreshed)?;
                // keep the persisted snapshot's embedded copy consistent —
                // materialize/load_outcome read THAT one, and a stale stack
                // there would hand consumers yesterday's reading.
                if let Some(mut snap) = store.load_snapshot(last)? {
                    snap.stack = Some(refreshed.clone());
                    store.save_snapshot(&snap)?;
                }
            }
            Some(refreshed)
        } else {
            prior_stack
        };
        let snapshot = RevSnapshot {
            rev: last,
            incremental: true,
            ingest_wall_ns: wall,
            stack: stack_snapshot,
            ..Default::default()
        };
        progress(
            "stack",
            "done",
            "Framework evidence ready for the current revision.",
        );
        for key in ["resolve", "concepts", "storage"] {
            progress(
                key,
                "done",
                "Reused the current revision; no source changes require rebuilding.",
            );
        }
        return Ok(IngestOutcome {
            rev: last,
            plan,
            snapshot,
            store: SymbolStore::default(),
            project_dir: project.to_path_buf(),
            reverse_deps: Default::default(),
            test_map: cie_analysis::testing::TestMap {
                reach: Default::default(),
                tests_of: Default::default(),
            },
            summary_reuse: Default::default(),
            source_receipt: None,
        });
    }

    let deadline: Option<Ns> = opts.budget_ms.map(|ms| now_ns() + ms * 1_000_000);

    // TASK-030: load the prior snapshot to seed summary reuse.
    let prior_snapshot: Option<RevSnapshot> = match prev_index.as_ref() {
        Some(idx) if idx.last_rev > 0 => store
            .load_snapshot(idx.last_rev)?
            .filter(|s| s.concepts_tier == Some(CONCEPTS_TIER_VERSION)),
        _ => None,
    };

    progress(
        "resolve",
        "running",
        "Assembling repository facts and resolving cross-file calls…",
    );
    let extracted = stream.materialize()?;
    progress("resolve", "done", "Repository graph assembled.");
    let source_receipt = stream.receipt.clone();

    // TASK-030: assemble the reuse plan — per-file (blake3(file),
    // blake3(dep set)) keys against the prior analysis.
    let current_hashes: BTreeMap<String, [u8; 32]> =
        current.iter().map(|(p, (h, _))| (p.clone(), *h)).collect();
    let mut current_deps: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for facts in &extracted.per_file {
        let entry = current_deps.entry(facts.path.clone()).or_default();
        for c in &facts.calls {
            if let cie_semantic::facts::Callee::Resolved(id) = c.callee {
                if let Some(sym) = extracted.store.get(id) {
                    entry.insert(extracted.files.path(sym.loc.file).to_string());
                }
            }
            if let Some(receiver) = c.receiver.as_deref() {
                if let cie_analysis::receiver::ReceiverClass::InTree { symbol, .. } =
                    cie_analysis::receiver::resolve_receiver_project(
                        &extracted.store,
                        facts,
                        &extracted.per_file,
                        c.caller,
                        receiver,
                    )
                {
                    if let Some(sym) = extracted.store.get(symbol) {
                        entry.insert(extracted.files.path(sym.loc.file).to_string());
                    }
                }
            }
        }
    }
    let mut plan30 = cie_analysis::behavior::ReusePlan {
        prior_keys: BTreeMap::new(),
        prior_summaries: BTreeMap::new(),
        current_hashes: current_hashes.clone(),
        current_deps,
        reusable_symbols: Default::default(),
    };
    if let Some(prev) = &prior_snapshot {
        let qual_of: BTreeMap<u32, (String, String)> = prev
            .symbols
            .iter()
            .map(|s| {
                (
                    s.id.0,
                    (
                        prev.files
                            .get(s.loc.file.0 as usize)
                            .cloned()
                            .unwrap_or_default(),
                        s.qual_name.clone(),
                    ),
                )
            })
            .collect();
        // Prior numeric IDs are revision-local. Translate through unique
        // declaration identities before seeding either exact or candidate edges.
        let remap: BTreeMap<SymbolId, SymbolId> = prev
            .symbols
            .iter()
            .filter_map(|old| {
                let path = prev.files.get(old.loc.file.0 as usize)?;
                let candidates: Vec<_> = extracted
                    .store
                    .symbols
                    .iter()
                    .filter(|new| {
                        extracted.files.path(new.loc.file) == path && new.qual_name == old.qual_name
                    })
                    .collect();
                if candidates.len() == 1 {
                    Some((old.id, candidates[0].id))
                } else {
                    None
                }
            })
            .collect();
        for sum in &prev.summaries {
            if let (Some((path, qual)), Some(new_id)) =
                (qual_of.get(&sum.sym.0), remap.get(&sum.sym))
            {
                let mut seeded = sum.clone();
                seeded.sym = *new_id;
                seeded.callees = seeded
                    .callees
                    .iter()
                    .filter_map(|id| remap.get(id).copied())
                    .collect();
                seeded.callers = seeded
                    .callers
                    .iter()
                    .filter_map(|id| remap.get(id).copied())
                    .collect();
                for call in &mut seeded.call_sites {
                    call.target_symbols = call
                        .target_symbols
                        .iter()
                        .filter_map(|id| remap.get(id).copied())
                        .collect();
                }
                plan30
                    .prior_summaries
                    .insert((path.clone(), qual.clone()), seeded);
            }
        }
        for entry in &prev.summary_cache {
            plan30
                .prior_keys
                .insert(entry.path.clone(), (entry.file_hash, entry.deps_hash));
        }
    }
    // TASK-056: per-symbol local-fact hashes + dirty worklist.
    let mut current_local: BTreeMap<String, [u8; 32]> = BTreeMap::new();
    let mut id_of: BTreeMap<String, SymbolId> = BTreeMap::new();
    let mut fact_bufs: BTreeMap<SymbolId, String> = BTreeMap::new();
    let mut callers_of: BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>> = BTreeMap::new();
    for facts in &extracted.per_file {
        for c in &facts.calls {
            // Resolved callees enter the row by STABLE identity (decl file +
            // qual name), never by revision-local numeric id: a symbol
            // inserted earlier in the walk shifts ids but changes nothing
            // about this caller's facts. Likewise the row omits the call's
            // byte span and line — a purely positional shift (another
            // function gained a line above) is not a fact change, and the
            // seed contract says untouched facts stay untouched (TASK-030).
            // Receiver text, argument count and loop context stay: they
            // feed resolution and effects.
            let callee = match &c.callee {
                cie_semantic::facts::Callee::Resolved(id) => extracted
                    .store
                    .get(*id)
                    .map(|s| format!("r|{}|{}", extracted.files.path(s.loc.file), s.qual_name))
                    .unwrap_or_else(|| format!("r{}", id.0)),
                cie_semantic::facts::Callee::Builtin(b) => format!("b{:?}", b),
                cie_semantic::facts::Callee::Unresolved(reason) => format!("u{}", reason),
            };
            fact_bufs.entry(c.caller).or_default().push_str(&format!(
                "call|{}|{}|{}|{:?}|{:?}\n",
                c.callee_text, callee, c.in_loop, c.receiver, c.arguments
            ));
            if let cie_semantic::facts::Callee::Resolved(id) = c.callee {
                callers_of.entry(id).or_default().insert(c.caller);
            }
        }
        for r in &facts.returns {
            fact_bufs.entry(r.symbol).or_default().push_str(&format!(
                "ret|{}|{}\n",
                r.expr,
                r.constant.clone().unwrap_or_default()
            ));
        }
        for e in &facts.exceptions {
            fact_bufs.entry(e.symbol).or_default().push_str(&format!(
                "exc|{}|{}|{}\n",
                e.kind_text, e.is_bare, e.in_handler
            ));
        }
        for a in &facts.assignments {
            fact_bufs.entry(a.symbol).or_default().push_str(&format!(
                "asg|{}|{:?}|{}\n",
                a.var,
                a.rhs_kind,
                a.rhs_data.join(",")
            ));
        }
        for i in &facts.ifs {
            fact_bufs
                .entry(i.symbol)
                .or_default()
                .push_str(&format!("if|{}\n", i.cond));
        }
        for l in &facts.loops {
            fact_bufs
                .entry(l.symbol)
                .or_default()
                .push_str(&format!("loop|{:?}\n", l.kind));
        }
        for s in &facts.sources {
            fact_bufs
                .entry(s.symbol)
                .or_default()
                .push_str(&format!("src|{}|{:?}\n", s.var, s.kind));
        }
        for s in &facts.sinks {
            fact_bufs.entry(s.symbol).or_default().push_str(&format!(
                "snk|{}|{}|{:?}\n",
                s.call_text,
                s.arg_vars.join(","),
                s.kind
            ));
        }
    }
    // A symbol's OWN declared bindings (params/locals/self-attrs) are local
    // facts: a changed declared type changes the resolution basis of the
    // body that consumes it. Rows are identity-stable — names and texts,
    // never numeric ids or positions — so a symbol inserted elsewhere in the
    // walk or a line shift cannot dirty an unchanged body.
    for facts in &extracted.per_file {
        for b in &facts.bindings {
            fact_bufs.entry(b.owner).or_default().push_str(&format!(
                "bind|{:?}|{}|{:?}|{:?}\n",
                b.kind, b.name, b.type_text, b.rhs_text
            ));
        }
    }
    // Declaration basis per FILE: the non-function symbols (classes, modules,
    // interfaces) the file's methods hang off. Hashed into the persisted
    // summary cache and compared against the prior run below. A changed basis
    // poisons the file's own symbols' prior hashes — the dirty worklist then
    // recomputes them and cascades to their transitive callers even when a
    // caller's own method body is unchanged. This must NOT ride the
    // per-symbol local-fact hash itself: those rows are identity-stable local
    // facts, and folding repo-global material into them (serialized bindings
    // with revision-local numeric owner ids, dep files' content hashes) made
    // one file's edit poison untouched files' keys — TASK-030's
    // exact-invalidation contract.
    let mut current_decl: BTreeMap<String, [u8; 32]> = BTreeMap::new();
    for facts in &extracted.per_file {
        let declarations: Vec<(&str, &str)> = extracted
            .store
            .symbols
            .iter()
            .filter(|s| !s.is_function_like() && extracted.files.path(s.loc.file) == facts.path)
            .map(|s| (s.qual_name.as_str(), extracted.files.path(s.loc.file)))
            .collect();
        // Class-scoped binding rows ride the basis too: a FIELD's declared
        // type is the resolution basis of every method that receives a value
        // through it, so a field retype — or a `extends` change, or a
        // rewritten import — must invalidate unchanged bodies in the file.
        // Stably serialized: owner by (path, qual), never numeric ids.
        let class_bindings: Vec<(
            &str,
            &cie_semantic::facts::BindingKind,
            &str,
            &Option<String>,
            &Option<String>,
        )> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == cie_semantic::facts::BindingKind::Field)
            .filter_map(|b| {
                let owner = extracted.store.get(b.owner)?;
                Some((
                    extracted.files.path(owner.loc.file),
                    &b.kind,
                    b.name.as_str(),
                    &b.type_text,
                    &b.rhs_text,
                ))
            })
            .collect();
        let type_decls: Vec<(&str, &str)> = facts
            .type_decls
            .iter()
            .map(|t| (t.name.as_str(), t.supers.as_str()))
            .collect();
        let imports: Vec<(&str, &Vec<String>, &Option<String>)> = facts
            .imports
            .iter()
            .map(|i| (i.module.as_str(), &i.names, &i.alias))
            .collect();
        let profile = format!("{declarations:?} {class_bindings:?} {type_decls:?} {imports:?}");
        current_decl.insert(
            facts.path.clone(),
            cie_core::types::content_hash(profile.as_bytes()),
        );
    }
    for sym in &extracted.store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let key = format!("{}::{}", extracted.files.path(sym.loc.file), sym.qual_name);
        id_of.insert(key.clone(), sym.id);
        let empty = String::new();
        let buf = fact_bufs.get(&sym.id).unwrap_or(&empty);
        current_local.insert(key, cie_core::types::content_hash(buf.as_bytes()));
    }
    let mut prior_local: BTreeMap<String, [u8; 32]> = BTreeMap::new();
    if let Some(prev) = &prior_snapshot {
        for entry in &prev.summary_cache {
            for (qual, hash) in &entry.symbol_keys {
                prior_local.insert(format!("{}::{}", entry.path, qual), *hash);
            }
        }
        // Declaration-basis invalidation: a file whose declarations changed
        // poisons its own symbols' prior hashes — the worklist treats them as
        // locally changed and cascades to their callers. Snapshots without a
        // decl hash (older engines) skip the check.
        for entry in &prev.summary_cache {
            let Some(prev_decl) = entry.decl_hash else {
                continue;
            };
            let Some(cur_decl) = current_decl.get(&entry.path) else {
                continue;
            };
            if prev_decl != *cur_decl {
                let prefix = format!("{}::", entry.path);
                let poisoned = cie_core::types::content_hash(b"declaration-basis-changed");
                for (k, v) in prior_local.iter_mut() {
                    if k.starts_with(&prefix) {
                        *v = poisoned;
                    }
                }
            }
        }
    }
    let worklist =
        cie_incremental::symbol_dirty_worklist(&prior_local, &current_local, &id_of, &callers_of);
    for (key, cur_hash) in &current_local {
        if prior_local.get(key) == Some(cur_hash)
            && !worklist
                .dirty
                .contains(id_of.get(key).unwrap_or(&SymbolId::PLACEHOLDER))
            && plan30.prior_summaries.contains_key(
                &key.split_once("::")
                    .map(|(p, q)| (p.to_string(), q.to_string()))
                    .unwrap_or_default(),
            )
        {
            if let Some((p, q)) = key.split_once("::") {
                plan30
                    .reusable_symbols
                    .insert((p.to_string(), q.to_string()));
            }
        }
    }
    let reuse_ref = if plan30.prior_keys.is_empty() {
        None
    } else {
        Some(&plan30)
    };

    // Labeler: LLM or offline; labels never decide anything.
    let mut label_cache = store.load_labels().unwrap_or_default();
    let budget = Budget::new(opts.llm_budget_usd);
    let provider_box = if opts.no_llm {
        Box::new(OfflineProvider) as Box<dyn cie_llm::LlmProvider>
    } else {
        cie_llm::make_provider(opts.llm)
    };
    let labeler_cell = std::cell::RefCell::new(Labeler::new(
        provider_box.as_ref(),
        &mut label_cache,
        &budget,
    ));
    let label_fn = |intent: &[String],
                    fallback: &str,
                    evidence: &[cie_analysis::hierarchy::LabelSymbolEvidence]|
     -> (String, &'static str) {
        // A3: convert the analysis-side evidence into the LLM-side shape
        // (no dependency edge cie-analysis -> cie-llm) so the concept
        // labeler can name the story, not join tag words.
        let members: Vec<cie_llm::ConceptEvidence> = evidence
            .iter()
            .map(|e| cie_llm::ConceptEvidence {
                name: e.name.clone(),
                tags: e.tags.clone(),
                effects: e.effects.clone(),
            })
            .collect();
        let rec = labeler_cell
            .borrow_mut()
            .label_concept(intent, fallback, &members);
        (rec.label, rec.source)
    };

    // TASK-002: revision chain continues from the persisted meta head, so a
    // process restart never reuses a RevId (§15.1 global rule 7).
    // NOTE: the head pointer is NOT advanced here (issue #11) — meta.json may
    // only ever name a revision whose snapshot is durably on disk. The local
    // `rev` below is what every step in this call uses; the persisted head
    // moves only after save_snapshot succeeds, so a crash mid-ingest leaves
    // meta.json at the last fully-good revision and every read keeps working.
    let rev_alloc = store.allocator();
    let rev = rev_alloc.next();
    let cfg = AnalysisConfig {
        deadline_ns: deadline,
        include_coverage_gaps: opts.include_coverage_gaps,
    };

    // RFC-0019 §6.1: the stack lens runs INSIDE ingest, before analyze, so
    // the hierarchy can consume it in the same run. Being in the FULL path
    // means language files changed (§6.2 rule 1: facts may change) — the
    // stack artifact re-derives; the lens' own caps bound the work, under a
    // sub-deadline slice. The lens is infallible by construction (walk/parse
    // problems land as honest notes inside the report), so this cannot
    // block the analysis — Amendment 3's `unavailable` record exists for
    // callers that wrap the stage in a fallible boundary.
    let stack_budget_ms = opts
        .budget_ms
        .map(|total| std::cmp::max(total / 4, 1).min(20_000));
    progress(
        "stack",
        "running",
        "Detecting frameworks, routes and runtime structure…",
    );
    // RFC-0019 §6.1/§6.2: the lens runs on the bodies the read phase already
    // read for hashing — manifests/configs are never opened a second time.
    let stack = cie_stack::stack_report_stage_with(
        project,
        Some(&stream.walked.stack_input_sources),
        stack_budget_ms,
    );
    progress("stack", "done", "Framework analysis complete.");
    // Persist BEFORE save_snapshot so a crash mid-ingest can never leave a
    // snapshot naming a stack artifact that isn't durably on disk.
    store.save_stack(rev, &stack)?;

    // RFC-0022 §6.1: the one real call-graph substrate, built and persisted
    // for the first time outside a test. Same crash-safety rule as the
    // stack artifact above — written before save_snapshot.
    let relations =
        cie_semantic::graph::relation_graph_from_facts(&extracted.per_file, &extracted.store);
    // Saved after the tombstone compaction below, in the snapshot ID space.

    // RFC-0022 §6.2: embed()/HnswRsStore both existed, tested, with zero
    // production call sites between them. `last_rev()` reads the head
    // BEFORE this ingest's commit_rev below, so it's genuinely the prior
    // successful revision's embeddings to carry forward — unchanged
    // symbol text is never re-embedded (vector_store::update_embeddings).
    let prev_rev = store.last_rev();
    let prev_vectors = if prev_rev > 0 {
        store.load_embeddings(prev_rev).unwrap_or(None)
    } else {
        None
    };
    let embed_texts = vector_store::symbol_texts(&extracted.store);
    let vectors = vector_store::update_embeddings(
        prev_vectors.as_ref(),
        &embed_texts,
        &budget,
        provider_box.as_ref(),
    );
    store.save_embeddings(rev, &vectors)?;

    // RFC-0022 §6.4 (prerequisite): `SymbolStore.unresolved` was declared
    // ("symbols whose call resolution is still pending") but nothing in
    // the workspace ever wrote to it — confirmed by grep before this
    // change. Populating it for real, from each unresolved call site's
    // own `callee_text`, is what makes the cross-repo resolution pass in
    // `run_org_ingest` possible without inventing a parallel data path.
    let mut unresolved_calls: BTreeMap<SymbolId, Vec<String>> = BTreeMap::new();
    for facts in &extracted.per_file {
        for c in &facts.calls {
            if let cie_semantic::facts::Callee::Unresolved(_) = &c.callee {
                unresolved_calls
                    .entry(c.caller)
                    .or_default()
                    .push(c.callee_text.clone());
            }
        }
    }

    progress(
        "concepts",
        "running",
        "Analyzing behavior, security and tests; building FCA/Godin concept groups…",
    );
    let analysis = cie_analysis::analyze_with_history(
        &extracted.store,
        &extracted.per_file,
        rev,
        &cfg,
        &label_fn,
        reuse_ref,
        Some(project),
        Some(&stack),
        Some(&extracted.seeds),
    );

    progress(
        "concepts",
        "done",
        "Analysis and concept clustering complete.",
    );

    // Persist label cache (LLM provenance recorded per PRD §63).
    store.save_labels(&label_cache)?;

    let wall = now_ns() - t0;
    let files_list: Vec<String> = current.keys().cloned().collect();
    let mut file_symbols: BTreeMap<String, Vec<SymbolId>> = BTreeMap::new();
    for sym in &extracted.store.symbols {
        file_symbols
            .entry(extracted.files.path(sym.loc.file).to_string())
            .or_default()
            .push(sym.id);
    }

    let mut index = FileIndex {
        files: BTreeMap::new(),
        last_rev: rev,
        stack_inputs: plan.stack_inputs.clone(),
        last_full_ingest_ns: if prev_index.is_none() {
            wall
        } else {
            prev_index
                .as_ref()
                .map(|i| i.last_full_ingest_ns)
                .unwrap_or(wall)
        },
        last_reingest_ns: wall,
    };
    for (rel, (hash, lang)) in &current {
        index.files.insert(
            rel.clone(),
            FileRecord {
                path: rel.clone(),
                hash: *hash,
                lang: *lang,
                last_rev: rev,
                symbols: file_symbols.get(rel).cloned().unwrap_or_default(),
            },
        );
    }
    // Publish the index only after the new snapshot is safely written.

    let mut snapshot = RevSnapshot {
        rev,
        files: files_list,
        symbols: extracted.store.symbols.clone(),
        // `callers` is derived here, not extracted: a straight reverse of
        // the already-resolved `callees` graph (analysis.reverse_deps, built
        // once in cie-analysis::behavior alongside callees itself), so it's
        // always exactly consistent with callees and costs no new parsing.
        summaries: analysis
            .summaries
            .values()
            .cloned()
            .map(|mut s| {
                s.callers = analysis
                    .reverse_deps
                    .get(&s.sym)
                    .map(|set| set.iter().copied().collect())
                    .unwrap_or_default();
                s
            })
            .collect(),
        // Behavioural-evidence redesign (diagrams layer): typed records for
        // symbols with NO evidenced interactions — nothing calls them and
        // they invoke nothing (no resolved callees, no unresolved call
        // texts). Derived once here so every diagram consumer states the
        // gap instead of rendering an empty lifeline as if it explained a
        // process.
        interaction_gaps: {
            let mut gaps: Vec<cie_incremental::InteractionGap> = vec![];
            for s in analysis.summaries.values() {
                if s.callees.is_empty()
                    && s.unresolved_callees.is_empty()
                    && !analysis
                        .reverse_deps
                        .get(&s.sym)
                        .map(|set| !set.is_empty())
                        .unwrap_or(false)
                {
                    gaps.push(cie_incremental::InteractionGap {
                        sym: s.sym,
                        kind: "no_evidenced_interactions".into(),
                        reason: "no call records reference this symbol and it invokes none in this revision".into(),
                    });
                }
            }
            gaps.sort_by_key(|g| g.sym);
            gaps
        },
        findings: analysis.findings.clone(),
        contracts: analysis.contracts.clone(),
        concepts: analysis.concepts.clone(),
        coverage: {
            let mut coverage = analysis.coverage.clone();
            if !source_receipt.skipped.is_empty() || !source_receipt.partial.is_empty() {
                coverage.complete = false;
                coverage
                    .notes
                    .extend(source_receipt.skipped.iter().cloned());
                coverage
                    .notes
                    .extend(source_receipt.partial.iter().cloned());
            }
            coverage
        },
        evidence: analysis.evidence.clone(),
        warnings: source_receipt.disclosure().into_iter().collect(),
        ingest_wall_ns: wall,
        incremental: false,
        summary_cache: plan30
            .current_hashes
            .keys()
            .map(|path| SummaryCacheEntry {
                path: path.clone(),
                file_hash: plan30.current_hashes[path],
                deps_hash: plan30.dep_hash(path),
                symbol_keys: current_local
                    .iter()
                    .filter_map(|(k, h)| {
                        k.strip_prefix(&format!("{path}::"))
                            .map(|qual| (qual.to_string(), *h))
                    })
                    .collect(),
                // the file's declaration basis at save time — the next run
                // compares against it to invalidate changed declarations
                decl_hash: current_decl.get(path).copied(),
            })
            .collect(),
        summary_reuse: Some(ReuseStats {
            reused_files: analysis.summary_reuse.reused_files,
            recomputed_files: analysis.summary_reuse.recomputed_files,
            reused_symbols: analysis.summary_reuse.reused_symbols,
            recomputed_symbols: analysis.summary_reuse.recomputed_symbols,
        }),
        // RFC-0019 §6.1: the stack lens' reading rides with the snapshot —
        // the report/concepts consumers read ONE artifact family.
        stack: Some(stack),
        // RFC-0024: this snapshot's concepts were built by THIS engine, so
        // they carry the current tier shape — stamp it (the no-op path and
        // the concepts/report lenses read it as their freshness signal).
        concepts_tier: Some(CONCEPTS_TIER_VERSION),
        // A5: filled in below, after the purge fixes the id space.
        usecase_view: Vec::new(),
        state_model: None,
    };
    // TASK-028: explicit purge pass — no stale symbols/summaries/findings or
    // orphaned call edges survive a tombstoned deletion.
    let purged = store.purge_tombstoned(&mut snapshot)?;
    let model = cie_contract::obligations::StateModelBuilder::from_effects(
        &SymbolStore::build(snapshot.symbols.clone()),
        &snapshot
            .summaries
            .iter()
            .map(|s| (s.sym, s.clone()))
            .collect(),
    );
    let model = cie_contract::obligations::evidenced_model(&model);
    snapshot.state_model = cie_contract::obligations::to_view(&model);
    use state_model_store::StateModelStoreExt;
    store.save_state_model(rev, &model)?;
    let relations = navigation::remap_relations(&relations, &extracted.store, &snapshot);
    store.save_relations(rev, &relations)?;
    let revision_files = navigation::snapshot_files(&snapshot, &extracted.store.files);
    cie_storage::artifacts::put(
        &store,
        rev,
        cie_storage::artifacts::Artifact::FileTable,
        &revision_files,
    )?;
    let unresolved_calls =
        navigation::remap_unresolved(&unresolved_calls, &extracted.store, &snapshot);
    navigation::save_unresolved(&store, rev, &unresolved_calls)?;
    org_store::save(
        &store,
        &org_store::OrgMember::new(&snapshot, &unresolved_calls),
    )?;
    // A5: the usecase-first projection rides with the artifact — computed
    // AFTER the purge (so its symbol ids speak the final, possibly
    // renumbered id space of THIS snapshot) and BEFORE the save (so the
    // persisted artifact already carries it). The projection reads only
    // the symbol table + file table, so the graph rebuild below is not
    // needed for it.
    snapshot.usecase_view = cie_analysis::hierarchy::usecase_projection(
        &snapshot.concepts,
        &SymbolStore::build_with_files(snapshot.symbols.clone(), revision_files.clone()),
    );
    progress("storage", "running", "Storage module builds C4, state and workflow views, detects duplicate clusters and publishes the revision…");
    progress("storage", "running", "Building view: storage initializing…");
    let _publication = stream.finish_with_progress(project, rev, &extracted, progress)?;
    store.save_snapshot(&snapshot)?;
    // Issue #11: only now, with this revision's snapshot durably written, may
    // meta.json name it current. A crash anywhere above leaves the previous
    // revision intact and loadable (load_outcome reads this pointer); the
    // aborted run's rev id is simply reused by the next successful ingest.
    store.save_index(&index)?;
    store.commit_rev(rev)?;
    progress(
        "storage",
        "done",
        "Revision and derived views persisted by the storage module.",
    );

    // Issue #10 companion: when the purge actually removed symbols it also
    // RENUMBERED the id space inside `snapshot` (id == index must hold on the
    // persisted artifact, or a reload resolves ids to the wrong record). The
    // in-memory store and graphs must then move to the same numbering — or
    // in-process consumers (SARIF's store.get(finding.symbol), drift impact
    // lookups) would resolve remapped ids against the pre-purge table: the
    // same silently-wrong lookup the purge fix just eliminated for reloads.
    // With no purge, nothing moved and the extracted store stays authoritative.
    let (mut sym_store, reverse_deps, test_map) = if purged > 0 {
        let st = SymbolStore::build_with_files(snapshot.symbols.clone(), revision_files);
        let (rd, tm, _) = rebuild_graphs(&st, &snapshot.summaries);
        (st, rd, tm)
    } else {
        let (rd, tm, _) = rebuild_graphs(
            &extracted.store,
            &analysis.summaries.values().cloned().collect::<Vec<_>>(),
        );
        (extracted.store, rd, tm)
    };
    sym_store.unresolved = unresolved_calls;

    Ok(IngestOutcome {
        rev,
        plan,
        snapshot,
        store: sym_store,
        project_dir: project.to_path_buf(),
        reverse_deps,
        test_map,
        summary_reuse: analysis.summary_reuse,
        source_receipt: Some(source_receipt),
    })
}

/// If a re-ingest was a no-op, load the persisted snapshot so analysis-facing
/// callers still see full data.
pub fn materialize(outcome: IngestOutcome) -> std::io::Result<IngestOutcome> {
    if outcome.snapshot.incremental {
        load_outcome(&outcome.project_dir, outcome.rev)
    } else {
        Ok(outcome)
    }
}

/// Load a full outcome (snapshot + graphs) for an existing revision — used
/// by analysis-facing commands after a no-op re-ingest.
pub fn load_outcome(project: &Path, rev: RevId) -> std::io::Result<IngestOutcome> {
    let store = Store::open(project)?;
    let snapshot = store
        .load_snapshot(rev)?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "snapshot missing"))?;
    let mut sym_store = SymbolStore::build_with_files(
        snapshot.symbols.clone(),
        navigation::load_files(&store, &snapshot, project)?,
    );
    sym_store.unresolved = navigation::load_unresolved(&store, rev)?.unwrap_or_default();
    let (reverse_deps, test_map, _) = rebuild_graphs(&sym_store, &snapshot.summaries);
    Ok(IngestOutcome {
        rev: snapshot.rev,
        plan: IngestPlan::default(),
        snapshot,
        store: sym_store,
        project_dir: project.to_path_buf(),
        reverse_deps,
        test_map,
        summary_reuse: Default::default(),
        source_receipt: None,
    })
}

/// Walk `root` for bounded documentation inputs (md/rst/adoc, ≤256KiB,
/// depth ≤14, first 20 by path) within `deadline`. Shared by the eager CLI
/// drift path and the on-demand retrieval builder (RFC-0026 §9) so neither
/// keeps its own copy of the discovery rules.
pub fn discover_docs(
    root: &std::path::Path,
    deadline: std::time::Instant,
) -> std::io::Result<Vec<(String, String)>> {
    let check = || {
        if std::time::Instant::now() >= deadline {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Documentation discovery budget exceeded",
            ))
        } else {
            Ok(())
        }
    };
    check()?;
    let mut paths = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        check()?;
        if depth > 14 {
            continue;
        }
        for entry in std::fs::read_dir(dir)? {
            check()?;
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                if !IGNORED.contains(&entry.file_name().to_string_lossy().as_ref()) {
                    dirs.push((entry.path(), depth + 1));
                }
            } else if ty.is_file()
                && matches!(
                    entry.path().extension().and_then(|x| x.to_str()),
                    Some("md" | "rst" | "adoc")
                )
                && entry.metadata()?.len() <= 256 * 1024
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort_by_key(|p| (p.components().count(), p.clone()));
    check()?;
    let mut docs = Vec::new();
    for path in paths.into_iter().take(20) {
        check()?;
        docs.push((
            path.to_string_lossy().to_string(),
            std::fs::read_to_string(path)?,
        ));
    }
    check()?;
    Ok(docs)
}

/// Run drift for the given docs against the project's latest revision.
pub fn run_drift(
    project: &Path,
    docs: &[(String, String)], // (name, content)
    opts: &IngestOptions,
    verify_deadline_ns: Option<Ns>,
    inject_timeout: bool,
) -> std::io::Result<(cie_claims::model::DriftReport, IngestOutcome)> {
    let raw = run_ingest(project, opts)?;
    run_drift_from_outcome(project, docs, raw, verify_deadline_ns, inject_timeout)
}

/// Check docs against an already captured revision; never re-ingest.
pub fn run_drift_from_outcome(
    project: &Path,
    docs: &[(String, String)],
    raw: IngestOutcome,
    verify_deadline_ns: Option<Ns>,
    inject_timeout: bool,
) -> std::io::Result<(cie_claims::model::DriftReport, IngestOutcome)> {
    run_drift_inner(project, docs, raw, verify_deadline_ns, inject_timeout, None)
}

/// Bound the entire optional documentation pipeline, not only verification.
/// Expiry never creates a completed cache entry or a clean drift result.
pub fn run_drift_from_outcome_with_budget(
    project: &Path,
    docs: &[(String, String)],
    raw: IngestOutcome,
    budget: std::time::Duration,
) -> std::io::Result<(cie_claims::model::DriftReport, IngestOutcome)> {
    let deadline = std::time::Instant::now() + budget;
    let verify_deadline = now_ns().saturating_add(budget.as_nanos().min(u64::MAX as u128) as u64);
    run_drift_inner(
        project,
        docs,
        raw,
        Some(verify_deadline),
        false,
        Some(deadline),
    )
}
fn run_drift_inner(
    project: &Path,
    docs: &[(String, String)],
    raw: IngestOutcome,
    verify_deadline_ns: Option<Ns>,
    inject_timeout: bool,
    deadline: Option<std::time::Instant>,
) -> std::io::Result<(cie_claims::model::DriftReport, IngestOutcome)> {
    use cie_drift::pipeline::check_deadline;
    check_deadline(deadline)?;
    // TASK-071: cheap ingestion formats — normalize rst/adoc/html to plain
    // prose before hashing and claim extraction (markdown/text pass through).
    let normalized_docs: Vec<(String, String)> = docs
        .iter()
        .map(|(name, content)| {
            check_deadline(deadline)?;
            Ok((name.clone(), cie_docs::normalize_doc(name, content)))
        })
        .collect::<std::io::Result<_>>()?;
    check_deadline(deadline)?;
    let docs: &[(String, String)] = &normalized_docs;

    // TASK-076 / acc #24: identical code + identical docs ⇒ reuse the
    // persisted drift report (re-run ≤ 5% of first-run wall time). Code
    // identity is the sorted (path, content hash) file index — cheap to
    // read, no snapshot reload. Timeout injection bypasses the cache (it
    // exists to test the live path).
    let drift_store = Store::open(project)?;
    let cache_key_str = drift_cache_key(&drift_store, docs)?;
    if !inject_timeout {
        if let Some(cached) = drift_store.load_drift_cache_entry(&cache_key_str)? {
            if cached.rev == raw.rev && cached.complete {
                // Cache hit: skip materialize — the caller gets the report;
                // the un-materialized outcome carries rev/project only.
                check_deadline(deadline)?;
                return Ok((cached, raw));
            }
        }
    }

    check_deadline(deadline)?;
    let outcome = materialize(raw)?;
    check_deadline(deadline)?;
    let now = now_ns();

    let mut claims = vec![];
    for (name, content) in docs {
        let extracted = cie_drift::pipeline::extract_claims_until(name, content, now, deadline)?;
        for mut claim in extracted {
            claim.id = claims.len() as u32;
            claims.push(claim);
        }
    }

    check_deadline(deadline)?;
    cie_drift::pipeline::bind_history_until(project, &mut claims, deadline);
    check_deadline(deadline)?;

    // TASK-073: persist claims with (doc, span, kind, modality) provenance;
    // identical (doc, text) claims dedup onto one entry across runs.
    {
        let mut claim_store = drift_store.load_claim_store()?;
        for c in &claims {
            check_deadline(deadline)?;
            claim_store.insert(c.clone());
        }
        drift_store.save_claim_store(&claim_store)?;
    }
    let mappings = cie_drift::pipeline::map_claims_until(
        &claims,
        &outcome.store,
        &{
            let mut m = BTreeMap::new();
            for s in &outcome.snapshot.summaries {
                m.insert(s.sym, s.clone());
            }
            m
        },
        deadline,
    )?;
    check_deadline(deadline)?;
    let verifications = cie_drift::pipeline::verify_claims_until(
        &claims,
        &mappings,
        &outcome.store,
        &Vec::new(), // per_file not needed for verify (uses summaries/findings)
        &{
            let mut m = BTreeMap::new();
            for s in &outcome.snapshot.summaries {
                m.insert(s.sym, s.clone());
            }
            m
        },
        &outcome.snapshot.findings,
        &cie_drift::pipeline::VerifyConfig {
            deadline_ns: verify_deadline_ns,
            inject_timeout,
        },
        deadline,
    )?;
    check_deadline(deadline)?;
    let complete = !verifications
        .iter()
        .any(|v| v.reason.as_deref() == Some("verification_timeout"));
    let report = cie_drift::pipeline::build_report_until(
        outcome.rev,
        &docs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        claims,
        mappings,
        verifications,
        &outcome.store,
        complete,
        now,
        deadline,
    )?;
    let store = Store::open(project)?;
    store.save_drift(&report)?;

    // Persist the cache entry for the next identical re-run (acc #24).
    if !inject_timeout && report.complete {
        store.save_drift_cache_entry(&cache_key_str, &report)?;
    }
    Ok((report, outcome))
}

/// acc #24 cache key: blake3 over the sorted (path, content hash) file index
/// (code identity) plus every doc's name+content (docs identity).
fn drift_cache_key(store: &Store, docs: &[(String, String)]) -> std::io::Result<String> {
    let index = store.load_index()?.unwrap_or_default();
    let mut code_bytes: Vec<u8> = vec![];
    for rec in index.files.values() {
        code_bytes.extend_from_slice(rec.path.as_bytes());
        code_bytes.extend_from_slice(&rec.hash);
    }
    let code_hash = content_hash(&code_bytes);
    let mut docs_bytes: Vec<u8> = vec![];
    for (name, content) in docs {
        docs_bytes.extend_from_slice(name.as_bytes());
        docs_bytes.extend_from_slice(content.as_bytes());
    }
    let docs_hash = content_hash(&docs_bytes);
    // Claim IDs are now unique across documents; do not reuse old colliding IDs.
    let mut key_bytes = b"rfc0020-history-v2-doc-ids".to_vec();
    key_bytes.extend_from_slice(&code_hash);
    key_bytes.extend_from_slice(&docs_hash);
    Ok(cie_core::types::hash_hex(&content_hash(&key_bytes)))
}

/// Implements the retrieval layer's on-demand build hook (RFC-0026 §9) for
/// `drift`: reconstructs an `IngestOutcome` purely from the persisted
/// revision via [`load_outcome`] — never a live re-ingest — discovers docs
/// fresh, and runs the same pipeline the eager CLI path used to run inline
/// at ingest time. Lives here (not in cie-storage, which cie-ingest already
/// depends on) and gets injected into the retrieval layer as a trait object
/// by whatever host crate can see both sides (cie-cli).
pub struct OnDemandDeferredBuilder;
impl cie_storage::retrieval::DeferredBuilder for OnDemandDeferredBuilder {
    fn build(
        &self,
        root: &std::path::Path,
        rev: RevId,
        kind: cie_storage::retrieval::ArtifactKind,
        budget: std::time::Duration,
    ) -> std::result::Result<serde_json::Value, String> {
        use cie_storage::retrieval::ArtifactKind;
        match kind {
            ArtifactKind::Drift => build_drift_on_demand(root, rev, budget),
            ArtifactKind::Overlap => build_overlap_on_demand(root, rev),
            other => Err(format!("OnDemandDeferredBuilder cannot build {other:?}")),
        }
    }
}

fn build_drift_on_demand(
    root: &std::path::Path,
    rev: RevId,
    budget: std::time::Duration,
) -> std::result::Result<serde_json::Value, String> {
    let deadline = std::time::Instant::now() + budget;
    let docs = discover_docs(root, deadline).map_err(|e| e.to_string())?;
    if docs.is_empty() {
        return Ok(serde_json::json!({
            "rev": rev, "docs": Vec::<String>::new(), "claims": [], "mappings": [],
            "verifications": [], "reverse_drift": [], "aggregate": {},
            "complete": true, "generated_ns": now_ns(),
        }));
    }
    let outcome = load_outcome(root, rev).map_err(|e| e.to_string())?;
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    let (report, _) = run_drift_from_outcome_with_budget(root, &docs, outcome, remaining)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

/// Still the brute-force all-pairs matrix (RFC-0026 §§1-8's lattice
/// replacement is not implemented yet) — this only moves *when* it runs,
/// from blocking ingest to the first on-demand request, so ingest/publish
/// no longer wait on it. The algorithm's own cost is unchanged.
fn build_overlap_on_demand(
    root: &std::path::Path,
    rev: RevId,
) -> std::result::Result<serde_json::Value, String> {
    let outcome = load_outcome(root, rev).map_err(|e| e.to_string())?;
    let names: BTreeMap<SymbolId, String> = outcome
        .store
        .symbols
        .iter()
        .map(|s| (s.id, s.qual_name.clone()))
        .collect();
    let mappings: BTreeMap<String, std::collections::BTreeSet<SymbolId>> = outcome
        .snapshot
        .concepts
        .nodes
        .iter()
        .filter(|n| n.kind != cie_concepts::ConceptKind::Domain && !n.symbols.is_empty())
        .map(|n| (n.label.clone(), n.symbols.iter().copied().collect()))
        .collect();
    let matrix = cie_concepts::overlap_matrix(&mappings, 1);
    let name_of = |ids: &std::collections::BTreeSet<SymbolId>| -> Vec<String> {
        ids.iter().filter_map(|id| names.get(id).cloned()).collect()
    };
    let pairs: Vec<serde_json::Value> = matrix
        .pairs
        .iter()
        .map(|p| {
            serde_json::json!({"a":p.a,"b":p.b,"shared":name_of(&p.shared),"only_a":name_of(&p.only_a),"only_b":name_of(&p.only_b),"similarity":p.similarity()})
        })
        .collect();
    Ok(serde_json::json!({"pairs":pairs,"unmapped":matrix.unmapped,"min_shared":1}))
}

/// Merge concept DAGs across repositories (FR-46 / CAP-16, acceptance #37).
pub fn run_org_ingest(
    repos: &[PathBuf],
    opts: &IngestOptions,
) -> std::io::Result<cie_store::OrgGraph> {
    let mut org = cie_store::OrgGraph::default();
    let mut label_index: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
    // RFC-0022 §6.4: outcomes are kept (not discarded per-iteration) so the
    // cross-repo resolution pass below can read every repo's symbols and
    // unresolved calls, not just the current one's.
    let mut outcomes: Vec<(String, org_store::OrgMember)> = Vec::new();
    for repo in repos {
        let raw = run_ingest(repo, opts)?;
        let store = Store::open(repo)?;
        let member = match org_store::load(&store, raw.rev)? {
            Some(member) => member,
            None => {
                let mut outcome = materialize(raw)?;
                navigation::ensure_navigation(repo, &mut outcome)?;
                let member =
                    org_store::OrgMember::new(&outcome.snapshot, &outcome.store.unresolved);
                org_store::save(&store, &member)?;
                member
            }
        };
        let repo_name = repo.to_string_lossy().to_string();
        org.repos.push(repo_name.clone());
        for node in &member.concepts {
            // The org lens merges same-label concepts across repos. Facets
            // must join UseCase/Aggregate/Entity here: FCA groups are the
            // label-bearing business concepts of a route-less repo, and
            // demoting them to Facet (a shared attribute is never a business
            // story WITHIN one repo) must not kill the org-level view where
            // two repos sharing a concept is exactly the fact being claimed.
            if matches!(
                node.kind,
                cie_concepts::ConceptKind::UseCase
                    | cie_concepts::ConceptKind::Facet
                    | cie_concepts::ConceptKind::Aggregate
                    | cie_concepts::ConceptKind::Entity
            ) {
                label_index
                    .entry(node.label.to_ascii_lowercase())
                    .or_default()
                    .push((repo_name.clone(), node.id));
            }
        }
        outcomes.push((repo_name, member));
    }
    let mut merged_id = 0u32;
    for (label, sources) in label_index {
        let distinct_repos: std::collections::BTreeSet<_> =
            sources.iter().map(|(repo, _)| repo).collect();
        if distinct_repos.len() < 2 {
            continue; // multiple same-label nodes in ONE repo are not an org merge
        }
        org.merged_concepts.push(cie_store::MergedConcept {
            merged_id,
            label: label.clone(),
            sources: sources.clone(),
            confidence: if sources.len() > 1 { 0.9 } else { 0.5 },
        });
        merged_id += 1;
    }

    // RFC-0022 §6.4: `OrgGraph.api_edges` was declared and never
    // constructed anywhere (confirmed before this change) — this is the
    // real constructor. Each repo's `SymbolStore.unresolved` (just made
    // real above, in `run_ingest`) names the call text CIE couldn't
    // resolve within that repo; a second pass here checks whether a
    // SIBLING repo exports a same-named symbol. Only an UNAMBIGUOUS
    // match (exactly one candidate by short name in the other repo)
    // produces an edge — never a guess among several same-named symbols.
    // One org-wide name index retains indexed lookup after moving away
    // from full SymbolStores. Resolution never scans all sibling symbols
    // for every unresolved call.
    let mut exports: BTreeMap<&str, Vec<(usize, SymbolId)>> = BTreeMap::new();
    for (repo, (_, member)) in outcomes.iter().enumerate() {
        for symbol in &member.symbols {
            exports
                .entry(&symbol.name)
                .or_default()
                .push((repo, symbol.id));
        }
    }
    let mut seen_edges = std::collections::BTreeSet::new();
    for (i, (from_repo, from_outcome)) in outcomes.iter().enumerate() {
        for (&caller, callee_texts) in &from_outcome.unresolved {
            let Some(caller_sym) = from_outcome.symbols.iter().find(|s| s.id == caller) else {
                continue;
            };
            for callee_text in callee_texts {
                let short_name = callee_text.rsplit('.').next().unwrap_or(callee_text);
                // Ambiguity is checked ACROSS every sibling repo, not
                // per-repo: two different repos each exporting exactly
                // one same-named symbol is still two candidates overall,
                // and must resolve to neither (never a coin-flip guess).
                let candidates: Vec<_> = exports
                    .get(short_name)
                    .into_iter()
                    .flatten()
                    .filter(|(repo, _)| *repo != i)
                    .take(2)
                    .copied()
                    .collect();
                if let [(j, sym)] = candidates[..] {
                    if let Some(target_sym) = outcomes[j].1.symbols.iter().find(|s| s.id == sym) {
                        let edge = cie_store::ApiEdge {
                            from_repo: from_repo.clone(),
                            to_repo: outcomes[j].0.clone(),
                            from_symbol: caller_sym.qual_name.clone(),
                            to_symbol: target_sym.qual_name.clone(),
                            kind: "import".into(),
                            match_basis: Some("globally_unique_short_name; import binding and exported visibility unverified".into()),
                            claim_class: Some("inferred".into()),
                        };
                        if seen_edges.insert((i, caller, j, sym)) {
                            org.api_edges.push(edge);
                        }
                    }
                }
                // Zero or multiple candidates: stays unresolved, same as
                // within a single repo today — never a guessed edge.
            }
        }
    }

    Ok(org)
}

/// The v1 adapter registry (TASK-011's `default_registry`, composed at the
/// ingestion root because Rust forbids trait-crate → adapter-crate cycles;
/// see TRACEABILITY §2). Extension path (PRD §67 / acceptance #9): a new
/// language = one adapter crate + ONE `reg.register(...)` line here.
pub fn default_registry() -> cie_adapters::LanguageRegistry {
    let mut reg = cie_adapters::LanguageRegistry::new();
    reg.register(Box::new(cie_adapter_python::PythonAdapter));
    reg.register(cie_adapter_javascript::adapter());
    reg.register(cie_adapter_typescript::adapter());
    reg.register(cie_adapter_java::adapter());
    reg.register(cie_adapter_c::adapter());
    reg.register(cie_adapter_cpp::adapter());
    reg.register(cie_adapter_csharp::adapter());
    reg.register(cie_adapter_go::adapter());
    reg.register(cie_adapter_rust::adapter());
    reg.register(cie_adapter_php::adapter());
    reg.register(cie_adapter_sql::adapter());
    reg.register(cie_adapter_shell::adapter());
    reg.register(cie_adapter_kotlin::adapter());
    reg.register(cie_adapter_lua::adapter());
    // New language goes HERE — one line, e.g.:
    // reg.register(cie_adapter_ruby::adapter());
    reg
}
