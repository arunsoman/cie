//! Engine orchestration (PRD §12 pipeline): walk → hash → plan → extract →
//! analyze → persist snapshot → report. Incremental by construction.

use cie_analysis::{self, AnalysisConfig};
use cie_core::facts::SymbolStore;
use cie_core::types::lang_for_path;
use cie_core::incremental::{plan_ingest, FileIndex, FileRecord, IngestPlan};
use cie_core::types::{content_hash, now_ns, Hash, LangId, Ns, RevId, SymbolId};
use cie_llm::{Budget, Labeler, OfflineProvider, ProviderKind};
use cie_storage::{RevSnapshot, Store};
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
}

/// Rebuild the call graph + test map from stored summaries.
fn rebuild_graphs(
    store: &SymbolStore,
    summaries: &[cie_core::summary::FunctionSummary],
) -> (
    std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>>,
    cie_analysis::testing::TestMap,
    std::collections::BTreeMap<SymbolId, Vec<SymbolId>>,
) {
    let mut callees_of: std::collections::BTreeMap<SymbolId, Vec<SymbolId>> = std::collections::BTreeMap::new();
    let mut reverse_deps: std::collections::BTreeMap<SymbolId, std::collections::BTreeSet<SymbolId>> =
        std::collections::BTreeMap::new();
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
    ".git", "node_modules", "target", "vendor", ".venv", "venv", "dist", "build",
    "__pycache__", ".cie", ".idea", ".vscode", ".github",
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
        });
    }

    let deadline: Option<Ns> = opts
        .budget_ms
        .map(|ms| now_ns() + ms * 1_000_000);

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

    let registry = cie_langs::default_registry();
    let extracted = cie_langs::extract_project(&files, &registry);

    // Labeler: LLM or offline; labels never decide anything.
    let mut label_cache = store.load_labels().unwrap_or_default();
    let budget = Budget::new(opts.llm_budget_usd);
    let provider_box = if opts.no_llm {
        Box::new(OfflineProvider::default()) as Box<dyn cie_llm::LlmProvider>
    } else {
        cie_llm::make_provider(opts.llm)
    };
    let labeler_cell = std::cell::RefCell::new(Labeler::new(
        provider_box.as_ref(),
        &mut label_cache,
        &budget,
    ));
    let label_fn = |intent: &[String], fallback: &str| -> String {
        labeler_cell
            .borrow_mut()
            .label_concept(intent, fallback)
            .label
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
    let analysis = cie_analysis::analyze(&extracted.store, &extracted.per_file, rev, &cfg, &label_fn);

    // Persist label cache (LLM provenance recorded per PRD §63).
    store.save_labels(&label_cache)?;

    let wall = now_ns() - t0;
    let files_list: Vec<String> = current.keys().cloned().collect();
    let mut file_symbols: BTreeMap<String, Vec<SymbolId>> = BTreeMap::new();
    for sym in &extracted.store.symbols {
        file_symbols
            .entry(sym.loc.path.clone())
            .or_default()
            .push(sym.id);
    }

    let mut index = FileIndex {
        files: BTreeMap::new(),
        last_rev: rev,
        last_full_ingest_ns: if prev_index.is_none() { wall } else { prev_index.as_ref().map(|i| i.last_full_ingest_ns).unwrap_or(wall) },
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

    let snapshot = RevSnapshot {
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
    };
    store.save_snapshot(&snapshot)?;

    let (reverse_deps, test_map, _) = rebuild_graphs(&extracted.store, &analysis.summaries.values().cloned().collect::<Vec<_>>());

    Ok(IngestOutcome {
        rev,
        plan,
        snapshot,
        store: extracted.store,
        project_dir: project.to_path_buf(),
        reverse_deps,
        test_map,
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
    })
}

/// Run drift for the given docs against the project's latest revision.
pub fn run_drift(
    project: &Path,
    docs: &[(String, String)], // (name, content)
    opts: &IngestOptions,
    verify_deadline_ns: Option<Ns>,
    inject_timeout: bool,
) -> std::io::Result<(cie_core::drift::DriftReport, IngestOutcome)> {
    let outcome = materialize(run_ingest(project, opts)?)?;
    let now = now_ns();

    let mut claims = vec![];
    for (name, content) in docs {
        claims.extend(cie_analysis::drift::extract_claims(name, content, now));
    }
    let mappings = cie_analysis::drift::map_claims(&claims, &outcome.store, &{
        let mut m = BTreeMap::new();
        for s in &outcome.snapshot.summaries {
            m.insert(s.sym, s.clone());
        }
        m
    });
    let verifications = cie_analysis::drift::verify_claims(
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
        &cie_analysis::drift::VerifyConfig {
            deadline_ns: verify_deadline_ns,
            inject_timeout,
        },
    );
    let report = cie_analysis::drift::build_report(
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
    Ok((report, outcome))
}

/// Merge concept DAGs across repositories (FR-46 / CAP-16, acceptance #37).
pub fn run_org_ingest(
    repos: &[PathBuf],
    opts: &IngestOptions,
) -> std::io::Result<cie_storage::OrgGraph> {
    let mut org = cie_storage::OrgGraph::default();
    let mut label_index: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
    for repo in repos {
        let outcome = run_ingest(repo, opts)?;
        org.repos.push(repo.to_string_lossy().to_string());
        for node in &outcome.snapshot.concepts.nodes {
            if node.kind == cie_core::concept::ConceptKind::UseCase
                || node.kind == cie_core::concept::ConceptKind::Aggregate
                || node.kind == cie_core::concept::ConceptKind::Entity
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
        org.merged_concepts.push(cie_storage::MergedConcept {
            merged_id,
            label: label.clone(),
            sources: sources.clone(),
            confidence: if sources.len() > 1 { 0.9 } else { 0.5 },
        });
        merged_id += 1;
    }
    Ok(org)
}
