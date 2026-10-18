//! Engine orchestration (PRD §12 pipeline): walk → hash → plan → extract →
//! analyze → persist snapshot → report. Incremental by construction.


pub mod churn;
pub mod cochange;
pub mod sample;
pub mod history;
pub mod pipeline;

pub use history::{
    archaeology_chain, bind_claims_to_history, parse_blame, parse_log, parse_numstat, BlameLine,
    CommitMeta, FileTouch, GitHistoryProvider, HistoryFacts, HistoryOptions, HistoryUnavailable,
    SystemGit,
};
pub use churn::{
    churn_factors, co_change, ChurnExclusions, ChurnFactor, ChurnReport, CoChangeEdge,
    ExclusionEffect,
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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub langs: Option<Vec<LangId>>,
    pub budget_ms: Option<u64>,
    pub no_llm: bool,
    pub llm: ProviderKind,
    pub llm_budget_usd: f64,
    pub include_coverage_gaps: bool,
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
}

/// Rebuild the call graph + test map from stored summaries.
type GraphTriple = (
    std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>>,
    cie_analysis::testing::TestMap,
    std::collections::BTreeMap<SymbolId, Vec<SymbolId>>,
);

fn rebuild_graphs(
    store: &SymbolStore,
    summaries: &[cie_summary::FunctionSummary],
) -> GraphTriple {
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

/// Walk the folder and hash every analyzable source file.
pub fn walk_and_hash(
    root: &Path,
    langs: Option<&Vec<LangId>>,
) -> std::io::Result<BTreeMap<String, (Hash, LangId)>> {
    let mut out = BTreeMap::new();
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
            let lang = lang_for_path(&name);
            if lang == LangId::Unknown {
                continue;
            }
            if let Some(want) = langs {
                if !want.contains(&lang) {
                    continue;
                }
            }
            let bytes = std::fs::read(&path)?;
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, (content_hash(&bytes), lang));
        }
    }
    Ok(out)
}

fn read_file(root: &Path, rel: &str) -> std::io::Result<String> {
    std::fs::read_to_string(root.join(rel))
}

/// Run the ingest/analysis pipeline for a project folder.
pub fn run_ingest(project: &Path, opts: &IngestOptions) -> std::io::Result<IngestOutcome> {
    let t0 = now_ns();
    let store = Store::open(project)?;
    let current = walk_and_hash(project, opts.langs.as_ref())?;
    let prev_index = store.load_index()?;
    let plan = plan_ingest(prev_index.as_ref(), &current);

    // TASK-028: deletions leave tombstones (`file:{path}:tombstone`, §15.1)
    // so stale facts are purged explicitly; re-appearing paths clear theirs.
    for path in &plan.deleted {
        store.mark_tombstone(path)?;
    }
    for path in plan.changed.iter().chain(plan.added.iter()) {
        store.clear_tombstone(path)?;
    }

    // Fast path: nothing changed (acceptance #10). Deliberately does NOT
    // load the analysis snapshot — re-ingest wall time is hashing + plan only.
    if !plan.has_work() {
        let last = store.last_rev();
        let wall = now_ns() - t0;
        let mut index = prev_index.unwrap_or_default();
        index.last_reingest_ns = wall;
        store.save_index(&index)?;
        let snapshot = RevSnapshot {
            rev: last,
            incremental: true,
            ingest_wall_ns: wall,
            ..Default::default()
        };
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
        });
    }

    let deadline: Option<Ns> = opts.budget_ms.map(|ms| now_ns() + ms * 1_000_000);

    // TASK-030: load the prior snapshot to seed summary reuse.
    let prior_snapshot: Option<RevSnapshot> = match prev_index.as_ref() {
        Some(idx) if idx.last_rev > 0 => store.load_snapshot(idx.last_rev)?,
        _ => None,
    };

    // Read + extract current files.
    let mut files: Vec<(String, LangId, String)> = vec![];
    for (rel, (hash, lang)) in &current {
        let src = read_file(project, rel).unwrap_or_default();
        // verify hash matches what we planned on (file changed mid-walk)
        let actual = content_hash(src.as_bytes());
        if actual != *hash {
            continue;
        }
        files.push((rel.clone(), *lang, src));
    }

    let registry = default_registry();
    let extracted = cie_adapters::extract_project(&files, &registry);

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
            .map(|s| (s.id.0, (extracted.files.path(s.loc.file).to_string(), s.qual_name.clone())))
            .collect();
        for sum in &prev.summaries {
            if let Some((path, qual)) = qual_of.get(&sum.sym.0) {
                plan30
                    .prior_summaries
                    .insert((path.clone(), qual.clone()), sum.clone());
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
            let callee = match &c.callee {
                cie_semantic::facts::Callee::Resolved(id) => format!("r{}", id.0),
                cie_semantic::facts::Callee::Builtin(b) => format!("b{:?}", b),
                cie_semantic::facts::Callee::Unresolved(reason) => format!("u{}", reason),
            };
            fact_bufs.entry(c.caller).or_default().push_str(&format!(
                "call|{}|{}|{}\n",
                c.callee_text, callee, c.in_loop
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
    let label_fn = |intent: &[String], fallback: &str| -> (String, &'static str) {
        let rec = labeler_cell.borrow_mut().label_concept(intent, fallback);
        (rec.label, rec.source)
    };

    // TASK-002: revision chain continues from the persisted meta head, so a
    // process restart never reuses a RevId (§15.1 global rule 7).
    let rev_alloc = store.allocator();
    let rev = rev_alloc.next();
    store.commit_rev(rev)?;
    let cfg = AnalysisConfig {
        deadline_ns: deadline,
        include_coverage_gaps: opts.include_coverage_gaps,
    };
    let analysis = cie_analysis::analyze(
        &extracted.store,
        &extracted.per_file,
        rev,
        &cfg,
        &label_fn,
        reuse_ref,
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
    store.save_index(&index)?;

    let mut snapshot = RevSnapshot {
        rev,
        files: files_list,
        symbols: extracted.store.symbols.clone(),
        summaries: analysis.summaries.values().cloned().collect(),
        findings: analysis.findings.clone(),
        contracts: analysis.contracts.clone(),
        concepts: analysis.concepts.clone(),
        coverage: analysis.coverage.clone(),
        evidence: analysis.evidence.clone(),
        warnings: vec![],
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
            })
            .collect(),
        summary_reuse: Some(ReuseStats {
            reused_files: analysis.summary_reuse.reused_files,
            recomputed_files: analysis.summary_reuse.recomputed_files,
            reused_symbols: analysis.summary_reuse.reused_symbols,
            recomputed_symbols: analysis.summary_reuse.recomputed_symbols,
        }),
    };
    // TASK-028: explicit purge pass — no stale symbols/summaries/findings or
    // orphaned call edges survive a tombstoned deletion.
    let _purged = store.purge_tombstoned(&mut snapshot)?;
    store.save_snapshot(&snapshot)?;

    let (reverse_deps, test_map, _) = rebuild_graphs(
        &extracted.store,
        &analysis.summaries.values().cloned().collect::<Vec<_>>(),
    );

    Ok(IngestOutcome {
        rev,
        plan,
        snapshot,
        store: extracted.store,
        project_dir: project.to_path_buf(),
        reverse_deps,
        test_map,
        summary_reuse: analysis.summary_reuse,
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
    let sym_store = SymbolStore::build(snapshot.symbols.clone());
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
    })
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

    // TASK-071: cheap ingestion formats — normalize rst/adoc/html to plain
    // prose before hashing and claim extraction (markdown/text pass through).
    let normalized_docs: Vec<(String, String)> = docs
        .iter()
        .map(|(name, content)| (name.clone(), cie_docs::normalize_doc(name, content)))
        .collect();
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
            if cached.rev == raw.rev {
                // Cache hit: skip materialize — the caller gets the report;
                // the un-materialized outcome carries rev/project only.
                return Ok((cached, raw));
            }
        }
    }

    let outcome = materialize(raw)?;
    let now = now_ns();

    let mut claims = vec![];
    for (name, content) in docs {
        claims.extend(cie_drift::pipeline::extract_claims(name, content, now));
    }

    // TASK-073: persist claims with (doc, span, kind, modality) provenance;
    // identical (doc, text) claims dedup onto one entry across runs.
    {
        let mut claim_store = drift_store.load_claim_store()?;
        for c in &claims {
            claim_store.insert(c.clone());
        }
        drift_store.save_claim_store(&claim_store)?;
    }
    let mappings = cie_drift::pipeline::map_claims(&claims, &outcome.store, &{
        let mut m = BTreeMap::new();
        for s in &outcome.snapshot.summaries {
            m.insert(s.sym, s.clone());
        }
        m
    });
    let verifications = cie_drift::pipeline::verify_claims(
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
    );
    let report = cie_drift::pipeline::build_report(
        outcome.rev,
        &docs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        claims,
        mappings,
        verifications,
        &outcome.store,
        true,
        now,
    );
    let store = Store::open(project)?;
    store.save_drift(&report)?;

    // Persist the cache entry for the next identical re-run (acc #24).
    if !inject_timeout {
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
    let mut key_bytes = code_hash.to_vec();
    key_bytes.extend_from_slice(&docs_hash);
    Ok(cie_core::types::hash_hex(&content_hash(&key_bytes)))
}

/// Merge concept DAGs across repositories (FR-46 / CAP-16, acceptance #37).
pub fn run_org_ingest(
    repos: &[PathBuf],
    opts: &IngestOptions,
) -> std::io::Result<cie_store::OrgGraph> {
    let mut org = cie_store::OrgGraph::default();
    let mut label_index: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
    for repo in repos {
        let outcome = run_ingest(repo, opts)?;
        org.repos.push(repo.to_string_lossy().to_string());
        for node in &outcome.snapshot.concepts.nodes {
            if node.kind == cie_concepts::ConceptKind::UseCase
                || node.kind == cie_concepts::ConceptKind::Aggregate
                || node.kind == cie_concepts::ConceptKind::Entity
            {
                label_index
                    .entry(node.label.to_ascii_lowercase())
                    .or_default()
                    .push((repo.to_string_lossy().to_string(), node.id));
            }
        }
    }
    let mut merged_id = 0u32;
    for (label, sources) in label_index {
        if sources.len() < 2 {
            continue; // repo-unique concepts are not merged
        }
        org.merged_concepts.push(cie_store::MergedConcept {
            merged_id,
            label: label.clone(),
            sources: sources.clone(),
            confidence: if sources.len() > 1 { 0.9 } else { 0.5 },
        });
        merged_id += 1;
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
    // New language goes HERE — one line, e.g.:
    // reg.register(cie_adapter_ruby::adapter());
    reg
}
