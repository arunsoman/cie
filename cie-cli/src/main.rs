//! cie — Code Intelligence Engine CLI.
//!
//! Commands: ingest, analyze, concepts, drift, impact, overlap, report,
//! sample, ingest-org, viz, stories, cochange, deps, stack.

use cie_drift::persist::DriftStoreExt;
use cie_incremental::SnapshotStoreExt;
pub use cie_ingest;

use cie_core::types::SymbolId;
use cie_ingest::{IngestOptions, IngestOutcome};
use cie_llm::ProviderKind;
use cie_semantic::LangId;
use clap::{Parser as ClapParser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LlmChoice {
    Offline,
    Ollama,
    Openai,
}

impl From<LlmChoice> for ProviderKind {
    fn from(c: LlmChoice) -> ProviderKind {
        match c {
            LlmChoice::Offline => ProviderKind::Offline,
            LlmChoice::Ollama => ProviderKind::Ollama,
            LlmChoice::Openai => ProviderKind::OpenAi,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutFormat {
    Human,
    Json,
    Md,
    Sarif,
    Dot,
    Mermaid,
}

#[derive(ClapParser, Debug)]
#[command(
    name = "cie",
    version,
    about = "Code Intelligence Engine — evidence-backed code analysis"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Dependency manifest scan + usage evidence + upgrade what-if (US-062)
    Deps {
        path: PathBuf,
        /// Upgrade what-if: "name=version" (e.g. requests=2.32.0)
        #[arg(long)]
        what_if: Option<String>,
        /// Licence-obligation report (US-098; advisory feeds stay unplanned-by-design offline)
        #[arg(long)]
        licences: bool,
        #[arg(long)]
        json: bool,
    },
    /// Tech-stack report: manifests + docker + framework code facts (US-100)
    Stack {
        path: PathBuf,
        #[arg(long)]
        json: bool,
        /// RFC-0019 §6.1: recompute even when the revision store already
        /// holds this revision's stack artifact (default off — the store
        /// answers first, compute-if-absent otherwise).
        #[arg(long)]
        force: bool,
    },
    /// Co-change coupling report from git history (US-087)
    Cochange {
        path: PathBuf,
        #[arg(long, default_value_t = 1)]
        min_shared: u64,
        #[arg(long, default_value_t = 20)]
        bulk: usize,
        #[arg(long)]
        max_commits: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Generate the built-in sample repository with real dated history (US-007)
    Sample {
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Ingest a folder path (full assimilation or incremental re-ingest)
    Ingest {
        path: PathBuf,
        #[arg(long)]
        watch: bool,
        #[arg(long, value_delimiter = ',')]
        langs: Option<Vec<String>>,
        #[arg(long)]
        budget_ms: Option<u64>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::SetFalse)]
        llm: bool,
        #[arg(long, value_enum, default_value = "offline")]
        llm_provider: LlmChoice,
        #[arg(long, default_value_t = 1.0)]
        llm_budget_usd: f64,
        #[arg(long)]
        json: bool,
    },
    /// Ingest + report findings (human / json / md / sarif)
    Analyze {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "human")]
        format: OutFormat,
        #[arg(long)]
        budget_ms: Option<u64>,
        #[arg(long, value_enum, default_value = "offline")]
        llm_provider: LlmChoice,
        #[arg(long)]
        no_llm: bool,
        #[arg(long)]
        no_coverage_gaps: bool,
    },
    /// Hierarchical concept behavior tree (FR-44)
    Concepts {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "md")]
        format: OutFormat,
        #[arg(long, default_value_t = true, action = clap::ArgAction::SetTrue)]
        with_behaviors: bool,
        #[arg(long, default_value_t = true, action = clap::ArgAction::SetTrue)]
        with_rules: bool,
        /// Render only the first N levels (root = level 1) — TASK-083.
        #[arg(long)]
        max_depth: Option<u32>,
    },
    /// Document vs code drift analysis (FR-31–39)
    Drift {
        /// Document files (markdown/text)
        #[arg(required = true)]
        docs: Vec<PathBuf>,
        #[arg(long)]
        code: PathBuf,
        #[arg(long, value_enum, default_value = "md")]
        format: OutFormat,
        #[arg(long)]
        from_rev: Option<u64>,
        #[arg(long)]
        to_rev: Option<u64>,
        #[arg(long, hide = true)]
        inject_verify_timeout: bool,
        #[arg(long)]
        budget_ms: Option<u64>,
    },
    /// Blast radius + minimum test set for a symbol (CAP-10.2/10.3, CAP-17.3)
    Impact {
        path: PathBuf,
        /// Symbol name or qualified name
        #[arg(long)]
        symbol: String,
        /// Code change mode: list changed files (git-style) instead
        #[arg(long)]
        json: bool,
    },

    /// Capability-overlap matrix over the project's own concept nodes
    /// (cie-concepts::overlap, US-106 engine close): which named parts of
    /// the project share real symbols, so "does this already exist" is a
    /// structural comparison, never a guess from names alone.
    Overlap {
        path: PathBuf,
        /// Minimum shared symbols for a pair to be reported.
        #[arg(long, default_value_t = 2)]
        min_shared: u64,
        #[arg(long)]
        json: bool,
    },
    /// Aggregate report for the latest (or an as-of) revision
    Report {
        path: PathBuf,
        #[arg(long)]
        as_of_rev: Option<u64>,
        #[arg(long)]
        json: bool,
        /// Write the JSON to a file instead of stdout. Large real repos
        /// produce reports far past any sane pipe buffer, and a stdout
        /// consumer with a byte cap silently truncates mid-JSON (issue #12).
        /// Same bytes, one file — the caller reads it back whole.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest multiple repositories and merge concepts (FR-46)
    IngestOrg {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// ViewSpec producer + epistemic gate (CIE-VIZ: FR-9 output protocol).
    /// Produces a sealed semantic-graph ViewSpec JSON for the G6 adapter, or
    /// validates an existing spec against the gate + content hash.
    Viz {
        path: PathBuf,
        /// Validate this ViewSpec JSON instead of producing one (exit 1 on
        /// any gate violation or hash mismatch).
        #[arg(long)]
        validate: Option<PathBuf>,
        /// Engine id for render-hash verification in --validate mode
        /// (default: engine-neutral baseline, VIZ-10: CIE names no engine)
        #[arg(long)]
        engine: Option<String>,
        /// Salience-bounded element cap (default 2000, VIZ-I-06)
        #[arg(long)]
        max_elements: Option<usize>,
        /// Force a layout strategy (verbatim G6 name); default auto-selects
        #[arg(long)]
        layout: Option<String>,
        /// Write the spec to a file instead of stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// User-story catalogue and coverage (CIE-US layer, 147 stories)
    Stories {
        #[command(subcommand)]
        cmd: StoriesCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum StoriesCmd {
    /// List stories with dispositions (filterable)
    List {
        #[arg(long)]
        section: Option<String>,
        #[arg(long)]
        disposition: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show one story in full with its coverage entry
    Show { id: String },
    /// Coverage report — honest totals, no fake percentages
    Coverage {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        md: bool,
    },
    /// Verify a story (registry audit; deep suites land per section)
    Verify {
        /// Story id (US-042); omit to verify all
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn lang_list(spec: &Option<Vec<String>>) -> Option<Vec<LangId>> {
    spec.as_ref()
        .map(|v| v.iter().map(|s| LangId::parse(s)).collect())
}

fn ingest_opts(
    langs: &Option<Vec<String>>,
    budget_ms: Option<u64>,
    no_llm: bool,
    llm_provider: LlmChoice,
    no_coverage_gaps: bool,
) -> IngestOptions {
    IngestOptions {
        langs: lang_list(langs),
        budget_ms,
        no_llm,
        llm: llm_provider.into(),
        llm_budget_usd: 1.0,
        include_coverage_gaps: !no_coverage_gaps,
    }
}

fn finding_summary(outcome: &IngestOutcome) -> String {
    let s = &outcome.snapshot;
    let mut counts = std::collections::BTreeMap::new();
    for f in &s.findings {
        *counts.entry(f.severity).or_insert(0u32) += 1;
    }
    format!(
        "rev {} | files {} | symbols {} | summaries {} | findings {} (critical {} high {} medium {} low {} info {}) | coverage {:.0}% | wall {:.1}ms{}",
        s.rev,
        s.files.len(),
        s.symbols.len(),
        s.summaries.len(),
        s.findings.len(),
        counts.get(&cie_analysis::findings::Severity::Critical).copied().unwrap_or(0),
        counts.get(&cie_analysis::findings::Severity::High).copied().unwrap_or(0),
        counts.get(&cie_analysis::findings::Severity::Medium).copied().unwrap_or(0),
        counts.get(&cie_analysis::findings::Severity::Low).copied().unwrap_or(0),
        counts.get(&cie_analysis::findings::Severity::Info).copied().unwrap_or(0),
        s.coverage.symbol_ratio() * 100.0,
        s.ingest_wall_ns as f64 / 1e6,
        if s.coverage.complete { "" } else { " | INCOMPLETE (budget)" }
    )
}

use cie_ingest::stack_store::StackStoreExt;

fn main() {
    let cli = Cli::parse();
    let code = run(cli);
    std::process::exit(code);
}

fn run(cli: Cli) -> i32 {
    match cli.command {
        Command::Stack { path, json, force } => {
            // RFC-0019 §6.1: store-first, compute-if-absent. When the repo
            // carries a .cie store whose head revision has a stack artifact,
            // answer FROM THE STORE — the exact bytes persisted at ingest,
            // honestly stamped, no second parse. A bare folder computes as
            // today. --force skips the store and re-derives.
            let mut from_store: Option<(cie_stack::types::StackReport, u64)> = None;
            if !force {
                if path.join(".cie").join("revs").exists() {
                    if let Ok(store) = cie_store::Store::open(&path) {
                        let rev = store.last_rev();
                        if rev > 0 {
                            match store.load_stack(rev) {
                                Ok(Some(report)) => from_store = Some((report, rev)),
                                _ => {} // no artifact / unreadable → compute below
                            }
                        }
                    }
                }
            }
            let (report, store_stamp): (cie_stack::types::StackReport, Option<u64>) =
                match from_store {
                    Some((r, rev)) => (r, Some(rev)),
                    None => (cie_stack::stack_report(&path), None),
                };
            if json {
                match store_stamp {
                    // identical bytes to what ingest persisted
                    Some(rev) if !force && path.join(".cie").join("revs").join(rev.to_string()).join("stack.json").exists() => {
                        match std::fs::read_to_string(path.join(".cie").join("revs").join(rev.to_string()).join("stack.json")) {
                            Ok(s) => {
                                println!("{s}");
                                return 0;
                            }
                            Err(_) => {
                                println!("{}", cie_stack::to_json(&report));
                                return 0;
                            }
                        }
                    }
                    _ => {
                        println!("{}", cie_stack::to_json(&report));
                        return 0;
                    }
                }
            }
            match store_stamp {
                Some(rev) => println!(
                    "tech stack — from the revision store (rev {rev}); the exact artifact ingest persisted. Recompute with --force:"
                ),
                None => println!("tech stack — read from the project's own files; nothing inferred here:"),
            }
            println!("  build      · {}", report.build.tool);
            println!("  artifact   · {}", if report.build.artifact_name.is_empty() { "(not stated)" } else { &report.build.artifact_name });
            for l in &report.languages {
                println!(
                    "  language   · {} ({} file(s){})",
                    l.family,
                    l.files,
                    if l.shared_runtime { ", one runtime with java+kotlin" } else { "" }
                );
            }
            for f in &report.frameworks {
                println!(
                    "  framework  · {}{}",
                    f.name,
                    if f.modules.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", f.modules.join(", "))
                    }
                );
            }
            if let Some(jvm) = &report.jvm {
                println!(
                    "  spring     · {} route(s), {} service(s), {} repo(s), {} entities, {} scheduled, {} listener(s)",
                    report.routes.len(),
                    jvm.services.len(),
                    jvm.repositories.len(),
                    jvm.entities.len(),
                    jvm.scheduled_in.len(),
                    jvm.listeners.len()
                );
                for f in &report.flows {
                    println!("  controller · {} ({})", f.name, f.rel);
                }
            }
            if report.docker.present {
                println!("  docker     · {}", report.docker.note);
            }
            for n in &report.notes {
                println!("  note       · {}", n);
            }
            return 0;
        }
        Command::Deps { path, what_if, licences, json } => {
            let table = cie_analysis::deps::scan_manifests(&path);
            if licences {
                let rep = cie_analysis::deps::licence_report(&table);
                if json {
                    println!("{}", serde_json::to_string_pretty(&rep).unwrap_or_default());
                    return 0;
                }
                println!("licence obligations (as written in manifests):");
                for e in &rep.entries {
                    println!(
                        "  {:<9} {:<28} {}",
                        e.ecosystem.as_str(),
                        e.name,
                        e.licence.as_deref().unwrap_or("UNDECLARED — provenance unavailable")
                    );
                }
                if !rep.unknown_licence.is_empty() {
                    println!("undeclared (named, never guessed):");
                    for u in &rep.unknown_licence {
                        println!("  - {u}");
                    }
                }
                println!("note: {}", rep.advisories);
                println!("note: {}", rep.provenance);
                return 0;
            }
            let reg = cie_ingest::default_registry();
            let imports = cie_analysis::deps::collect_imports(&path, &reg);
            if json {
                let usage = cie_analysis::deps::usage(&table, &imports);
                let whatif = what_if.as_deref().and_then(|w| {
                    let (n, v) = w.split_once('=')?;
                    Some(cie_analysis::deps::upgrade_what_if(
                        &table,
                        &imports,
                        n.trim(),
                        v.trim(),
                    ))
                });
                let out = serde_json::json!({
                    "table": table,
                    "usage": usage,
                    "what_if": whatif,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
                return 0;
            }
            println!(
                "manifests read: {} ({} dependencies declared{})",
                table.files_scanned,
                table.deps.len(),
                if table.unparsed.is_empty() {
                    String::new()
                } else {
                    format!(", {} UNPARSED: {}", table.unparsed.len(), table.unparsed.join("; "))
                }
            );
            let usage = cie_analysis::deps::usage(&table, &imports);
            for (d, u) in table.deps.iter().zip(usage.iter()) {
                println!(
                    "  {:<9} {:<28} {:<12} {} — {}",
                    d.ecosystem.as_str(),
                    d.name,
                    if d.version_req.is_empty() { "*" } else { &d.version_req },
                    if u.resolved {
                        format!("used by {} file(s)", u.imported_by.len())
                    } else {
                        "no import-backed usage".to_string()
                    },
                    u.note
                );
            }
            if let Some(w) = what_if.as_deref() {
                let Some((n, v)) = w.split_once('=') else {
                    eprintln!("error: --what-if expects name=version");
                    return 1;
                };
                match cie_analysis::deps::upgrade_what_if(&table, &imports, n.trim(), v.trim()) {
                    Some(imp) => {
                        println!(
                            "what-if: upgrade {} -> {} touches {} import-backed consumer(s); compatibility: UNKNOWN",
                            imp.name,
                            imp.to,
                            imp.consumers.len()
                        );
                        for st in &imp.statements {
                            println!("  - {st}");
                        }
                    }
                    None => {
                        println!("what-if: '{}' is not declared in any scanned manifest", n.trim());
                    }
                }
            }
            0
        }
        Command::Cochange { path, min_shared, bulk, max_commits, json } => {
            let opts = cie_ingest::cochange::CoChangeOptions {
                min_shared,
                bulk_edit_min_paths: bulk,
                max_commits,
            };
            match cie_ingest::cochange::co_change_report(&path, &opts) {
                Ok(rep) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&rep).unwrap_or_default());
                    } else {
                        println!(
                            "co-change for {} — sample {} commits ({} counted, {} bulk-excluded), {} renames resolved",
                            path.display(),
                            rep.sample_commits,
                            rep.counted_commits,
                            rep.excluded_bulk,
                            rep.rename_events
                        );
                        if rep.edges.is_empty() {
                            println!("  no co-change pairs at the given threshold");
                        }
                        for e in &rep.edges {
                            println!(
                                "  {:>4}x  {} <-> {}{}",
                                e.support,
                                e.a,
                                e.b,
                                if e.cross_directory { "  [cross-directory]" } else { "" }
                            );
                        }
                        println!("  note: {}", rep.note);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {}", e.reason);
                    1
                }
            }
        }
        Command::Sample { path, force } => {
            match cie_ingest::sample::generate(&path, &cie_ingest::sample::SampleOptions { force }) {
                Ok(rep) => {
                    println!(
                        "sample written: {} files, {} new commits ({} total) at {}",
                        rep.files,
                        rep.commits,
                        rep.total_commits,
                        rep.root.display()
                    );
                    println!(
                        "try: cie report {} | cie viz {} --out viewspec.json | cie cochange {}",
                        path.display(),
                        path.display(),
                        path.display()
                    );
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Ingest {
            path,
            watch,
            langs,
            budget_ms,
            llm,
            llm_provider,
            llm_budget_usd,
            json,
        } => {
            let opts = IngestOptions {
                langs: lang_list(&langs),
                budget_ms,
                no_llm: !llm,
                llm: llm_provider.into(),
                llm_budget_usd,
                include_coverage_gaps: true,
            };
            let path = path.canonicalize().unwrap_or(path);
            loop {
                match cie_ingest::run_ingest(&path, &opts) {
                    Ok(outcome) => {
                        if json {
                            let outcome = match cie_ingest::materialize(outcome) {
                                Ok(o) => o,
                                Err(e) => {
                                    eprintln!("error: {e}");
                                    return 1;
                                }
                            };
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&outcome.snapshot).unwrap_or_default()
                            );
                        } else {
                            let plan = &outcome.plan;
                            println!(
                                "ingest: {} unchanged, {} changed, {} added, {} deleted",
                                plan.unchanged.len(),
                                plan.changed.len(),
                                plan.added.len(),
                                plan.deleted.len()
                            );
                            if outcome.snapshot.incremental {
                                println!(
                                    "no changes — rev {} stands (re-ingest wall {:.3}ms)",
                                    outcome.rev,
                                    outcome.snapshot.ingest_wall_ns as f64 / 1e6
                                );
                            } else {
                                println!("{}", finding_summary(&outcome));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 1;
                    }
                }
                if !watch {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
                println!("--- watching for changes (Ctrl-C to stop) ---");
            }
            0
        }
        Command::Analyze {
            path,
            format,
            budget_ms,
            llm_provider,
            no_llm,
            no_coverage_gaps,
        } => {
            let opts = ingest_opts(&None, budget_ms, no_llm, llm_provider, no_coverage_gaps);
            let path = path.canonicalize().unwrap_or(path);
            let outcome =
                match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 1;
                    }
                };
            match format {
                OutFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&outcome.snapshot.findings).unwrap_or_default()
                ),
                OutFormat::Sarif => println!(
                    "{}",
                    serde_json::to_string_pretty(&cie_drift::sarif::findings_to_sarif(
                        &outcome.store,
                        &outcome.snapshot.findings
                    ))
                    .unwrap_or_default()
                ),
                OutFormat::Md => println!(
                    "{}",
                    cie_drift::outputs::findings_markdown(&outcome.snapshot.findings)
                ),
                _ => {
                    println!("{}", finding_summary(&outcome));
                    println!();
                    for f in &outcome.snapshot.findings {
                        println!(
                            "[{:8}] {:19} {} — {}",
                            f.severity.as_str(),
                            f.authority.as_str(),
                            f.symbol_name,
                            f.message
                        );
                    }
                }
            }
            exit_code_for_findings(&path)
        }
        Command::Concepts {
            path,
            format,
            with_behaviors,
            with_rules,
            max_depth,
        } => {
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(outcome) => {
                    let dag_view = match max_depth {
                        Some(d) => cie_drift::outputs::depth_limited_view(
                            &outcome.snapshot.concepts,
                            d.max(1),
                        ),
                        None => outcome.snapshot.concepts.clone(),
                    };
                    match format {
                        OutFormat::Json => println!(
                            "{}",
                            serde_json::to_string_pretty(&dag_view).unwrap_or_default()
                        ),
                        OutFormat::Dot => {
                            println!("{}", cie_drift::outputs::concepts_dot(&dag_view))
                        }
                        OutFormat::Mermaid => {
                            println!("{}", cie_drift::outputs::concepts_mermaid(&dag_view))
                        }
                        _ => {
                            let names: std::collections::BTreeMap<SymbolId, String> = outcome
                                .store
                                .symbols
                                .iter()
                                .map(|s| (s.id, s.qual_name.clone()))
                                .collect();
                            println!(
                                "{}",
                                cie_drift::outputs::concepts_markbook(
                                    &dag_view,
                                    with_behaviors,
                                    with_rules,
                                    &names
                                )
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            }
            0
        }
        Command::Drift {
            docs,
            code,
            format,
            from_rev,
            to_rev,
            inject_verify_timeout,
            budget_ms,
        } => {
            let mut doc_contents = vec![];
            for d in &docs {
                match std::fs::read_to_string(d) {
                    Ok(c) => doc_contents.push((d.to_string_lossy().to_string(), c)),
                    Err(e) => {
                        eprintln!("error reading doc {d:?}: {e}");
                        return 1;
                    }
                }
            }
            let code = code.canonicalize().unwrap_or(code);
            let opts = IngestOptions::default();
            let deadline = budget_ms.map(|ms| cie_core::types::now_ns() + ms * 1_000_000);

            // Delta mode: --from-rev / --to-rev compares stored reports (acceptance #23)
            if let (Some(from), Some(to)) = (from_rev, to_rev) {
                let store = match cie_store::Store::open(&code) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 1;
                    }
                };
                let (a, b) = match (store.load_drift(from), store.load_drift(to)) {
                    (Ok(Some(a)), Ok(Some(b))) => (a, b),
                    _ => {
                        eprintln!("error: drift reports for revs {from}/{to} not found; run `cie drift` first");
                        return 1;
                    }
                };
                let delta = a.delta(&b);
                if format == OutFormat::Json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&delta).unwrap_or_default()
                    );
                } else {
                    println!("drift delta {from} -> {to}: {} changed claims", delta.len());
                    for (claim, text, f, t) in delta {
                        println!("  #{claim} [{f} -> {t}] {text}");
                    }
                }
                return 0;
            }

            match cie_ingest::run_drift(
                &code,
                &doc_contents,
                &opts,
                deadline,
                inject_verify_timeout,
            ) {
                Ok((report, outcome)) => {
                    match format {
                        OutFormat::Json => println!(
                            "{}",
                            serde_json::to_string_pretty(&report).unwrap_or_default()
                        ),
                        _ => println!(
                            "{}",
                            cie_drift::outputs::drift_markdown(&report, &outcome.store)
                        ),
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Impact { path, symbol, json } => {
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(outcome) => {
                    let target = outcome
                        .store
                        .symbols
                        .iter()
                        .find(|s| s.qual_name == symbol || s.name == symbol);
                    let Some(sym) = target else {
                        eprintln!("error: symbol '{symbol}' not found");
                        return 1;
                    };
                    let affected =
                        cie_analysis::testing::affected_by_change(&outcome.reverse_deps, &[sym.id]);
                    let mut affected = affected;
                    affected.insert(sym.id);
                    let changed: Vec<SymbolId> = affected.iter().copied().collect();
                    let min_tests =
                        cie_analysis::testing::minimum_test_set(&outcome.test_map, &changed);
                    let affected_concepts: Vec<String> = outcome
                        .snapshot
                        .concepts
                        .nodes
                        .iter()
                        .filter(|n| n.symbols.iter().any(|s| affected.contains(s)))
                        .map(|n| n.label.clone())
                        .collect();
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "symbol": sym.qual_name,
                                "affected_symbols": affected.iter().filter_map(|i| outcome.store.get(*i).map(|s| s.qual_name.clone())).collect::<Vec<_>>(),
                                "minimum_test_set": min_tests.iter().filter_map(|i| outcome.store.get(*i).map(|s| s.qual_name.clone())).collect::<Vec<_>>(),
                                "affected_concepts": affected_concepts,
                            })
                        );
                    } else {
                        println!("impact of changing '{}':", sym.qual_name);
                        println!("  affected symbols: {}", affected.len());
                        for id in affected.iter().take(30) {
                            if let Some(s) = outcome.store.get(*id) {
                                println!("    - {}", s.qual_name);
                            }
                        }
                        println!("  minimum test set: {} tests", min_tests.len());
                        for t in &min_tests {
                            if let Some(s) = outcome.store.get(*t) {
                                println!("    - {}", s.qual_name);
                            }
                        }
                        println!("  affected concepts:");
                        for c in &affected_concepts {
                            println!("    - {c}");
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Overlap {
            path,
            min_shared,
            json,
        } => {
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(outcome) => {
                    let names: std::collections::BTreeMap<SymbolId, String> = outcome
                        .store
                        .symbols
                        .iter()
                        .map(|s| (s.id, s.qual_name.clone()))
                        .collect();
                    // Every non-Domain concept node is a named, comparable
                    // "capability" — the Domain root maps to every symbol in
                    // the project, which would trivially "overlap" with
                    // everything and say nothing.
                    let mappings: std::collections::BTreeMap<
                        String,
                        std::collections::BTreeSet<SymbolId>,
                    > = outcome
                        .snapshot
                        .concepts
                        .nodes
                        .iter()
                        .filter(|n| n.kind != cie_concepts::ConceptKind::Domain && !n.symbols.is_empty())
                        .map(|n| (n.label.clone(), n.symbols.iter().copied().collect()))
                        .collect();
                    let matrix = cie_concepts::overlap_matrix(&mappings, min_shared as usize);
                    if json {
                        let pairs: Vec<serde_json::Value> = matrix
                            .pairs
                            .iter()
                            .map(|p| {
                                let name_of =
                                    |ids: &std::collections::BTreeSet<SymbolId>| -> Vec<String> {
                                        ids.iter().filter_map(|i| names.get(i).cloned()).collect()
                                    };
                                serde_json::json!({
                                    "a": p.a,
                                    "b": p.b,
                                    "shared": name_of(&p.shared),
                                    "only_a": name_of(&p.only_a),
                                    "only_b": name_of(&p.only_b),
                                    "similarity": p.similarity(),
                                })
                            })
                            .collect();
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "pairs": pairs,
                                "unmapped": matrix.unmapped,
                                "min_shared": min_shared,
                            }))
                            .unwrap_or_default()
                        );
                    } else {
                        println!(
                            "capability overlap for {} — {} concept(s) compared",
                            path.display(),
                            mappings.len()
                        );
                        if matrix.pairs.is_empty() {
                            println!("  no overlapping pairs at the given threshold");
                        }
                        for p in &matrix.pairs {
                            println!(
                                "  {:>3.0}%  {} <-> {}  ({} shared)",
                                p.similarity() * 100.0,
                                p.a,
                                p.b,
                                p.shared.len()
                            );
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Report {
            path,
            as_of_rev,
            json,
            out,
        } => {
            let store = match cie_store::Store::open(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let rev = as_of_rev.unwrap_or_else(|| store.last_rev());
            match store.load_snapshot(rev) {
                Ok(Some(snap)) => {
                    if json {
                        // Serialize once; a failed serialization is an error,
                        // not an empty line that looks like valid-but-blank
                        // output downstream.
                        let payload = match serde_json::to_string_pretty(&snap) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("error: snapshot could not be serialized: {e}");
                                return 1;
                            }
                        };
                        match out {
                            Some(f) => {
                                if let Err(e) = std::fs::write(&f, payload + "\n") {
                                    eprintln!("error writing {f:?}: {e}");
                                    return 1;
                                }
                                println!(
                                    "wrote {}: {} files, {} symbols, {} findings",
                                    f.display(),
                                    snap.files.len(),
                                    snap.symbols.len(),
                                    snap.findings.len()
                                );
                            }
                            None => println!("{payload}"),
                        }
                    } else {
                        println!(
                            "report for rev {rev} (time travel: {})",
                            as_of_rev.is_some()
                        );
                        println!("  files: {}", snap.files.len());
                        println!("  symbols: {}", snap.symbols.len());
                        println!("  summaries: {}", snap.summaries.len());
                        println!("  findings: {}", snap.findings.len());
                        println!("  contracts: {}", snap.contracts.len());
                        println!("  concept nodes: {}", snap.concepts.nodes.len());
                        println!(
                            "  coverage: {:.0}% complete={}",
                            snap.coverage.symbol_ratio() * 100.0,
                            snap.coverage.complete
                        );
                        println!("  unresolved calls: {}", snap.evidence.unresolved.len());
                        println!(
                            "  unsupported constructs: {}",
                            snap.evidence.unsupported.len()
                        );
                    }
                    0
                }
                Ok(None) => {
                    eprintln!("error: no snapshot for rev {rev}");
                    1
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Viz {
            path,
            validate,
            engine,
            max_elements,
            layout,
            out,
        } => {
            // ---- validate mode: gate + hash recompute on an existing spec
            if let Some(spec_path) = validate {
                let raw = match std::fs::read_to_string(&spec_path) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("error reading spec {spec_path:?}: {e}");
                        return 1;
                    }
                };
                let spec: cie_viz::ViewSpec = match serde_json::from_str(&raw) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: spec is not a valid ViewSpec (VIZ-A-01 schema): {e}");
                        return 1;
                    }
                };
                let mut failed = false;
                match cie_viz::gate::validate(&spec) {
                    Ok(()) => println!(
                        "gate: OK ({} elements, {} edges)",
                        spec.elements.len(),
                        spec.edges.len()
                    ),
                    Err(violations) => {
                        failed = true;
                        for v in &violations {
                            println!("{v}");
                        }
                    }
                }
                // PRD §3 hash split: semantic is the cache key, render is
                // the artifact key (engine-bound; "" = producer baseline).
                let engine_id = engine.as_deref().unwrap_or("");
                if !cie_viz::hash::verify_semantic(&spec) {
                    failed = true;
                    println!(
                        "SEMANTIC_HASH_MISMATCH [spec] stored {} != computed {}",
                        spec.semantic_hash,
                        cie_viz::hash::compute_semantic(&spec)
                    );
                } else {
                    println!("semantic hash: OK ({})", spec.semantic_hash);
                }
                if !cie_viz::hash::verify_render(&spec, engine_id) {
                    failed = true;
                    println!(
                        "RENDER_HASH_MISMATCH [spec] stored {} != computed {} (engine '{engine_id}')",
                        spec.render_hash,
                        cie_viz::hash::compute_render(&spec, engine_id)
                    );
                } else if engine_id.is_empty() {
                    println!("render hash: OK (engine-neutral baseline)");
                } else {
                    println!("render hash: OK (engine '{engine_id}')");
                }
                return if failed { 1 } else { 0 };
            }

            // ---- produce mode: ingest → RevSnapshot → sealed ViewSpec
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(outcome) => {
                    let produce_opts = cie_viz::ProduceOptions {
                        max_elements: max_elements.unwrap_or(cie_viz::ELEMENT_CAP),
                        layout: layout.clone(),
                    };
                    match cie_viz::produce(&outcome.snapshot, &produce_opts) {
                        Ok(spec) => {
                            let json = serde_json::to_string_pretty(&spec).unwrap_or_default();
                            match out {
                                Some(f) => {
                                    if let Err(e) = std::fs::write(&f, json + "\n") {
                                        eprintln!("error writing {f:?}: {e}");
                                        return 1;
                                    }
                                    println!(
                                        "wrote {}: {} elements, {} edges, semantic {}",
                                        f.display(),
                                        spec.elements.len(),
                                        spec.edges.len(),
                                        spec.semantic_hash
                                    );
                                }
                                None => println!("{json}"),
                            }
                            0
                        }
                        Err(e) => {
                            eprintln!("error: producer output failed its own gate: {e}");
                            1
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::IngestOrg { paths, json } => {
            let opts = IngestOptions::default();
            let canonical: Vec<PathBuf> = paths
                .iter()
                .map(|p| p.canonicalize().unwrap_or(p.clone()))
                .collect();
            match cie_ingest::run_org_ingest(&canonical, &opts) {
                Ok(org) => {
                    if let Err(e) = store_org(&canonical, &org) {
                        eprintln!("warning: could not persist org graph: {e}");
                    }
                    if json {
                        println!("{}", serde_json::to_string_pretty(&org).unwrap_or_default());
                    } else {
                        println!(
                            "org graph: {} repos, {} merged concepts",
                            org.repos.len(),
                            org.merged_concepts.len()
                        );
                        for m in &org.merged_concepts {
                            println!("  - '{}' sources: {}", m.label, m.sources.len());
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Stories { cmd } => run_stories(cmd),
    }
}

fn run_stories(cmd: StoriesCmd) -> i32 {
    use cie_stories::{registry::Disposition, verify::Verdict};
    match cmd {
        StoriesCmd::List {
            section,
            disposition,
            json,
        } => {
            let disp_filter = disposition.as_deref().and_then(|d| match d {
                "delivered" => Some(Disposition::Delivered),
                "partial" => Some(Disposition::Partial),
                "planned" => Some(Disposition::Planned),
                _ => None,
            });
            if disposition.is_some() && disp_filter.is_none() {
                eprintln!("error: --disposition must be delivered|partial|planned");
                return 1;
            }
            let rows: Vec<_> = cie_stories::coverage::all_story_coverage()
                .into_iter()
                .filter(|r| section.as_deref().is_none_or(|s| r.section == s))
                .filter(|r| disp_filter.is_none_or(|d| r.disposition == d))
                .collect();
            if rows.is_empty() {
                eprintln!("no stories match the given filters");
                return 1;
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&rows).unwrap_or_default()
                );
            } else {
                for r in &rows {
                    println!(
                        "{:<7} {:<10} {} — {}",
                        r.id,
                        r.disposition.as_str(),
                        r.title,
                        r.section
                    );
                }
                println!("{} story(ies) listed", rows.len());
            }
            0
        }
        StoriesCmd::Show { id } => {
            let Some(story) = cie_stories::Story::by_id(&id) else {
                eprintln!("error: unknown story id {id} (expected US-001..US-147)");
                return 1;
            };
            println!("{}", cie_stories::format_story(story));
            if let Some(entry) = cie_stories::registry::entry_for(&id) {
                println!("\nEngine coverage: {}", entry.disposition.as_str());
                for cap in entry.capabilities {
                    println!("  capability: {cap}");
                }
                if !entry.note.is_empty() {
                    println!("  note: {}", entry.note);
                }
            }
            0
        }
        StoriesCmd::Coverage { json, md } => {
            let report = cie_stories::coverage::CoverageReport::build();
            if json {
                println!("{}", serde_json::to_string_pretty(&report.to_json()).unwrap_or_default());
            } else if md {
                print!("{}", report.render_markdown());
            } else {
                println!(
                    "{:<48} {:>9} {:>8} {:>8} {:>6}",
                    "section", "delivered", "partial", "planned", "total"
                );
                for s in &report.sections {
                    println!(
                        "{:<48} {:>9} {:>8} {:>8} {:>6}",
                        truncate(&s.section, 47),
                        s.delivered,
                        s.partial,
                        s.planned,
                        s.total
                    );
                }
                println!(
                    "{:<48} {:>9} {:>8} {:>8} {:>6}",
                    "TOTAL", report.delivered, report.partial, report.planned, report.total
                );
                println!(
                    "\ndispositions count engine-level delivery; partial stays partial, planned stays absent."
                );
            }
            0
        }
        StoriesCmd::Verify { id, json } => {
            let verdicts = match id {
                Some(id) => vec![cie_stories::verify::verify_story(&id)],
                None => cie_stories::verify::verify_all(),
            };
            // A failing deep check is a first-class FAIL verdict — automation
            // gates on this exit code, so FAIL must exit non-zero exactly
            // like an unknown id does (issue #8). Anything else (audit-pass,
            // pass, planned) stays exit 0.
            let failed = verdicts.iter().any(|v| match v {
                Verdict::Fail { .. } | Verdict::Unknown { .. } => true,
                _ => false,
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&verdicts).unwrap_or_default());
            } else {
                for v in &verdicts {
                    match v {
                        Verdict::AuditPass { story, disposition, evidence_count } => {
                            println!("{story}: audit-pass ({disposition}, {evidence_count} capability refs)");
                        }
                        Verdict::Pass { story, check } => {
                            println!("{story}: pass ({check})");
                        }
                        Verdict::Fail { story, check, error } => {
                            println!("{story}: FAIL ({check}) — {error}");
                        }
                        Verdict::Planned { story, reason } => {
                            println!("{story}: planned — {reason}");
                        }
                        Verdict::Unknown { id } => {
                            println!("{id}: unknown story id");
                        }
                    }
                }
            }
            if failed { 1 } else { 0 }
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

fn store_org(repos: &[PathBuf], org: &cie_store::OrgGraph) -> std::io::Result<()> {
    if let Some(first) = repos.first() {
        let store = cie_store::Store::open(first)?;
        store.save_org(org)?;
    }
    Ok(())
}

fn exit_code_for_findings(path: &std::path::Path) -> i32 {
    // CI-friendly: critical findings -> exit 2.
    let store = match cie_store::Store::open(path) {
        Ok(s) => s,
        Err(_) => return 1,
    };
    let rev = store.last_rev();
    match store.load_snapshot(rev) {
        Ok(Some(snap))
            if snap
                .findings
                .iter()
                .any(|f| f.severity == cie_analysis::findings::Severity::Critical)
            => {
                2
            }
        _ => 0,
    }
}
