//! cie — Code Intelligence Engine CLI.
//!
//! Commands: ingest, analyze, concepts, drift, impact, overlap, report,
//! sample, ingest-org, viz, stories, cochange, deps, stack.

use cie_drift::persist::DriftStoreExt;
use cie_incremental::SnapshotStoreExt;
pub use cie_ingest;

use cie_core::types::SymbolId;
use cie_ingest::{FactsSource, IngestOptions, IngestOutcome};
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
    /// Static-model fault hypotheses (RFC-0020); no application is executed.
    Fault { path: PathBuf, #[arg(long, default_value_t=2)] max_faults: usize, #[arg(long, default_value_t=500)] budget_ms: u64, #[arg(long)] json: bool },
    /// Evidenced payment/recovery template transitions (RFC-0020), static only.
    StateModel { path: PathBuf, #[arg(long)] json: bool },
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
    /// Multi-hop call/data/control navigation over the persisted relation
    /// graph (RFC-0022 §6.1, TASK-007/009) — the first production use of
    /// `cie-relations::RelationGraph`, previously built only in a test.
    Trace {
        path: PathBuf,
        symbol: String,
        /// Production facts populate calls, data_flow, owns, depends_on.
        /// Other accepted kinds (returns, control_flow, reads, writes,
        /// throws, catches) currently have no production extractor.
        /// Use all to traverse every populated relation kind.
        #[arg(long, default_value = "calls")]
        kind: String,
        /// Hop count bound — a BFS budget, not a timeout.
        #[arg(long, default_value_t = 3)]
        depth: usize,
        /// Walk incoming edges (callers) instead of outgoing (callees).
        #[arg(long)]
        reverse: bool,
        /// Traversal time budget, excluding ingest/artifact loading.
        #[arg(long, default_value_t = 2000)]
        budget_ms: u64,
        #[arg(long)]
        json: bool,
    },
    /// Hybrid semantic search over symbol text (RFC-0022 §6.2) — the
    /// first production use of `cie-llm::embed()`/`cie-store::hnsw`,
    /// previously implemented three times and called by nothing. Each
    /// hit is re-attached to its concept labels and contracts, the join
    /// a plain vector index can't do on its own.
    Search {
        path: PathBuf,
        query: String,
        #[arg(long, default_value_t = 10)]
        k: usize,
        /// Embedding provider; external backends are explicit opt-in.
        #[arg(long, value_enum, default_value = "offline")]
        llm_provider: LlmChoice,
        /// Force offline embeddings even when a provider is selected.
        #[arg(long)]
        no_llm: bool,
        /// Estimated embedding spend cap, in USD; zero uses offline.
        #[arg(long, default_value_t = 1.0, value_parser = parse_budget_usd)]
        llm_budget_usd: f64,
        /// KNN time budget; zero reports an explicit timeout.
        #[arg(long, default_value_t = 2000)]
        budget_ms: u64,
        #[arg(long)]
        json: bool,
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
    /// Generate the built-in sample repository with real dated history (US-007).
    /// Refuses a path inside an existing git repository: the sample's commits
    /// must land in their own repo, never in the host's (with its dirty files).
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
        /// BRIDGE (backlog item 5): where the parse input comes from.
        /// `polyglot` (default) feeds the adapters hash-verified sources
        /// from the polyglot store, disk fallback per file; `own` is the
        /// disk-only read. Artifacts are byte-identical either way.
        #[arg(long, value_enum, default_value = "polyglot")]
        facts_from: FactsFromChoice,
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
        /// Estimated labeling spend cap, in USD (A4: was hardcoded 1.0).
        #[arg(long, default_value_t = 1.0, value_parser = parse_budget_usd)]
        llm_budget_usd: f64,
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
        /// Label provider (A4: `concepts` used to hard-wire offline);
        /// external backends are explicit opt-in. The persistent label
        /// cache makes this a one-time spend per concept — re-runs that
        /// hit the cache make no model call.
        #[arg(long, value_enum, default_value = "offline")]
        llm_provider: LlmChoice,
        /// Force offline labeling even when a provider is selected.
        #[arg(long)]
        no_llm: bool,
        /// Estimated labeling spend cap, in USD; the offline fallback on
        /// budget exhaustion is silent by design.
        #[arg(long, default_value_t = 1.0, value_parser = parse_budget_usd)]
        llm_budget_usd: f64,
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
        /// RFC-0024 stage 0.5: produce the flow-chart ViewSpec over the
        /// extracted facts (process/decision/store elements; calls/data/
        /// writes edges) instead of the concept view.
        #[arg(long)]
        flow: bool,
        /// Force a layout strategy (verbatim G6 name); default auto-selects
        #[arg(long)]
        layout: Option<String>,
        /// Write the spec to a file instead of stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// PRD v2.0 polyglot layer: parse with tree-sitter into the SQLite
    /// AST/UAST/IR schema, detect frameworks + C4 + state machines +
    /// workflows + duplicates, optionally ingest OTel runtime evidence.
    Polyglot {
        path: PathBuf,
        /// Ingest an OTel spans payload (OTLP/JSON or flat array) after the
        /// static pass — the §9 runtime join
        #[arg(long)]
        otel: Option<PathBuf>,
        /// Emit a produced ViewSpec artifact by name ("state-machine:*",
        /// "workflows") to stdout instead of the summary
        #[arg(long)]
        viewspec: Option<String>,
        #[arg(long)]
        json: bool,
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

fn parse_budget_usd(s: &str) -> Result<f64, String> {
    let value: f64 = s.parse().map_err(|_| "expected a nonnegative finite USD amount")?;
    if !value.is_finite() || value < 0.0 { return Err("expected a nonnegative finite USD amount".into()); }
    Ok(value)
}

/// CLI spelling of `cie_ingest::FactsSource` (BRIDGE, backlog item 5).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum FactsFromChoice {
    Polyglot,
    Own,
}

impl From<FactsFromChoice> for FactsSource {
    fn from(c: FactsFromChoice) -> Self {
        match c {
            FactsFromChoice::Polyglot => FactsSource::Polyglot,
            FactsFromChoice::Own => FactsSource::Own,
        }
    }
}

fn ingest_opts(
    langs: &Option<Vec<String>>,
    budget_ms: Option<u64>,
    no_llm: bool,
    llm_provider: LlmChoice,
    no_coverage_gaps: bool,
    llm_budget_usd: f64,
) -> IngestOptions {
    IngestOptions {
        langs: lang_list(langs),
        budget_ms,
        no_llm,
        llm: llm_provider.into(),
        llm_budget_usd,
        include_coverage_gaps: !no_coverage_gaps,
        facts_from: FactsSource::Polyglot,
    }
}

/// RFC-0022 §6.1: the CLI-facing counterpart to `RelKind::as_str()` — no
/// parser existed on the type itself, so `cie trace --kind <name>` needs
/// one at the call site.
fn parse_rel_kind(s: &str) -> Option<cie_relations::RelKind> {
    use cie_relations::RelKind::*;
    Some(match s {
        "calls" => Calls,
        "returns" => Returns,
        "control_flow" => ControlFlow,
        "data_flow" => DataFlow,
        "reads" => Reads,
        "writes" => Writes,
        "throws" => Throws,
        "catches" => Catches,
        "owns" => Owns,
        "depends_on" => DependsOn,
        _ => return None,
    })
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
use cie_ingest::vector_store::EmbeddingStoreExt;
use cie_store::hnsw::VectorStore;

fn main() {
    let cli = Cli::parse();
    let code = run(cli);
    std::process::exit(code);
}

fn run(cli: Cli) -> i32 {
    match cli.command {
        Command::Fault { path, max_faults, budget_ms, json } => {
            if !(1..=2).contains(&max_faults) || budget_ms>5000 {eprintln!("fault: max-faults must be 1–2, budget-ms at most 5000");return 1;}
            let result=(|| -> std::io::Result<_> {
                use cie_ingest::state_model_store::StateModelStoreExt;
                let o=cie_ingest::materialize(cie_ingest::run_ingest(&path,&IngestOptions::default())?)?;
                let store=cie_store::Store::open(&path)?;
                let model=store.load_state_model(o.rev)?.filter(|m|!m.edges.is_empty());
                let r=cie_fault::lineage_from_state_model::analyze_model(o.rev,model.as_ref(),cie_fault::RoundLimits{max_faults,budget_ns:budget_ms*1_000_000});
                store.write_json(&store.root().join("revs").join(o.rev.to_string()).join("fault.json"),&r)?;
                Ok(r)
            })();
            match result {
                Ok(r)=>{if json {println!("{}",serde_json::to_string_pretty(&r).unwrap());}else{println!("{} · static-model simulation, execution performed: false",r.status);if let Some(reason)=r.unknown_reason{println!("{reason}");}if let Some(p)=r.property{println!("{p}");}println!("{} bounded simulated hypotheses; no runtime recovery conclusion",r.experiments.len());}0},
                Err(e)=>{eprintln!("fault: {e}");1}
            }
        }
        Command::StateModel { path, json } => {
            match cie_ingest::run_ingest(&path, &IngestOptions::default()).and_then(|o| cie_ingest::materialize(o)) {
                Ok(o) => {
                    if json { println!("{}", serde_json::to_string_pretty(&o.snapshot.state_model).unwrap()); }
                    else if let Some(m) = o.snapshot.state_model {
                        println!("{}", m.domain_note);
                        for t in m.transitions { println!("{} --{}--> {} [static Fact] {}", t.from, t.event, t.to, t.provenance); }
                    } else { println!("No evidenced payment/recovery state model for this revision."); }
                    0
                }
                Err(e) => { eprintln!("state-model: {e}"); 1 }
            }
        }
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
            // RFC-0019 §6.1 (--force amendment): a forced re-derivation
            // PERSISTS what it derived, so the store, the head snapshot's
            // embedded copy, and this run's output agree — the obvious
            // "refresh the artifact" move must not be a silent no-op that
            // only the terminal sees. No store yet → nothing to update
            // (a bare folder stays a bare folder).
            let mut persisted_rev: Option<u64> = None;
            if force && path.join(".cie").exists() {
                if let Ok(store) = cie_store::Store::open(&path) {
                    let rev = store.last_rev();
                    if rev > 0 {
                        use cie_ingest::stack_store::StackStoreExt;
                        if store.save_stack(rev, &report).is_ok() {
                            if let Ok(Some(mut snap)) = store.load_snapshot(rev) {
                                // skip only a literally-identical copy (a stale
                                // note must win over a same-second timestamp)
                                let differs = match &snap.stack {
                                    Some(old) => serde_json::to_string(old).ok().as_deref()
                                        != serde_json::to_string(&report).ok().as_deref(),
                                    None => true,
                                };
                                if differs {
                                    snap.stack = Some(report.clone());
                                    let _ = store.save_snapshot(&snap);
                                }
                            }
                            persisted_rev = Some(rev);
                        }
                    }
                }
            }
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
            match persisted_rev {
                Some(rev) => println!(
                    "tech stack — recomputed with --force and PERSISTED to the revision store (rev {rev}); the head snapshot's embedded copy was synced too:"
                ),
                None => match store_stamp {
                    Some(rev) => println!(
                        "tech stack — from the revision store (rev {rev}); the exact artifact ingest persisted. Recompute with --force:"
                    ),
                    None => println!("tech stack — read from the project's own files; nothing inferred here:"),
                },
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
            facts_from,
            json,
        } => {
            let opts = IngestOptions {
                langs: lang_list(&langs),
                budget_ms,
                no_llm: !llm,
                llm: llm_provider.into(),
                llm_budget_usd,
                include_coverage_gaps: true,
                facts_from: facts_from.into(),
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
                            // BRIDGE receipt (backlog item 5): say where the
                            // parse inputs came from — or that the store was
                            // absent, so the default mode's fallback is named,
                            // never silent.
                            if let Some(rcpt) = &outcome.source_receipt {
                                match (&rcpt.store_path, rcpt.from_store) {
                                    (Some(db), n) => println!(
                                        "facts-from polyglot: {n} of {} sources from the store ({}) — {} from disk",
                                        n + rcpt.from_disk,
                                        db.display(),
                                        rcpt.from_disk
                                    ),
                                    (None, _) => println!(
                                        "facts-from polyglot: no store for this path — all {} sources read from disk (run `cie polyglot <path>` to seed it)",
                                        rcpt.from_disk
                                    ),
                                }
                            }
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
            llm_budget_usd,
            no_coverage_gaps,
        } => {
            let opts = ingest_opts(
                &None,
                budget_ms,
                no_llm,
                llm_provider,
                no_coverage_gaps,
                llm_budget_usd,
            );
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
            llm_provider,
            no_llm,
            llm_budget_usd,
        } => {
            let opts = IngestOptions {
                langs: None,
                budget_ms: None,
                no_llm,
                llm: llm_provider.into(),
                llm_budget_usd,
                include_coverage_gaps: true,
                facts_from: FactsSource::Polyglot,
            };
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
        Command::Trace {
            path,
            symbol,
            kind,
            depth,
            reverse,
            budget_ms,
            json,
        } => {
            let want_kind = parse_rel_kind(&kind);
            if want_kind.is_none() && kind != "all" {
                eprintln!(
                    "error: unknown --kind '{kind}' (expected one of: all, calls, returns, control_flow, data_flow, reads, writes, throws, catches, owns, depends_on)"
                );
                return 1;
            }
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            let mut outcome = match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let exact: Vec<_> = outcome.store.symbols.iter().filter(|s| s.qual_name == symbol).collect();
            let candidates: Vec<_> = if exact.is_empty() {
                outcome.store.symbols.iter().filter(|s| s.name == symbol).collect()
            } else { exact };
            let sym = match candidates.as_slice() {
                [sym] => *sym,
                [] => { eprintln!("error: symbol '{symbol}' not found"); return 1; }
                _ => { eprintln!("error: ambiguous symbol '{symbol}'; use a unique qualified name"); return 1; }
            };
            let symbol_id = sym.id;
            let graph = match cie_ingest::navigation::ensure_navigation(&path, &mut outcome) {
                Ok(g) => g,
                Err(e) => { eprintln!("error: {e}"); return 1; }
            };
            // Resolve AFTER compatibility repair, which updates the outcome.
            let sym = outcome.store.get(symbol_id).expect("symbol remains in snapshot");
            let walk = cie_ingest::navigation::trace(&graph, sym.id, want_kind, depth, reverse,
                std::time::Duration::from_millis(budget_ms));
            let hops = &walk.hops;
            let truncated = walk.truncated;


            let describe = |id: SymbolId| -> (String, String) {
                match outcome.store.get(id) {
                    Some(s) => (
                        s.qual_name.clone(),
                        format!("{}:{}", outcome.store.files.path(s.loc.file), outcome.store.files.line(s.loc)),
                    ),
                    None => (format!("{id}"), String::new()),
                }
            };

            if json {
                let hops_json: Vec<_> = hops
                    .iter()
                    .map(|hop| {
                        hop.iter()
                            .map(|&id| {
                                let (name, loc) = describe(id);
                                serde_json::json!({ "symbol": name, "evidence": loc, "evidence_kind": "symbol_definition" })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::json!({
                        "symbol": sym.qual_name,
                        "kind": kind.as_str(),
                        "reverse": reverse,
                        "hops": hops_json,
                        "truncated": truncated,
                        "limit_reason": walk.limit_reason,
                    })
                );
            } else {
                println!(
                    "{} {} from '{}' (depth {}):",
                    if reverse { "callers" } else { "callees" },
                    kind.as_str(),
                    sym.qual_name,
                    depth
                );
                for (i, hop) in hops.iter().enumerate() {
                    println!("  hop {}:", i + 1);
                    for &id in hop {
                        let (name, loc) = describe(id);
                        println!("    - {name} ({loc})");
                    }
                }
                if truncated {
                    println!("  … traversal incomplete ({}) — raise --depth or --budget-ms as appropriate.", walk.limit_reason.unwrap_or("budget"));
                }
            }
            0
        }
        Command::Search { path, query, k, llm_provider, no_llm, llm_budget_usd, budget_ms, json } => {
            let opts = IngestOptions::default();
            let path = path.canonicalize().unwrap_or(path);
            let outcome = match cie_ingest::run_ingest(&path, &opts).and_then(cie_ingest::materialize) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let store = match cie_store::Store::open(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let previous = match store.load_embeddings(outcome.rev) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let provider = cie_llm::make_provider(if no_llm { cie_llm::ProviderKind::Offline } else { llm_provider.into() });
            let texts = cie_ingest::vector_store::symbol_texts(&outcome.store);
            let (vectors, query_vec) = cie_ingest::vector_store::prepare_search(
                previous.as_ref(), &texts, &cie_llm::Budget::new(llm_budget_usd), provider.as_ref(), &query);
            if previous.as_ref() != Some(&vectors) {
                if let Err(e) = store.save_embeddings(outcome.rev, &vectors) {
                    eprintln!("error: {e}"); return 1;
                }
            }
            if vectors.records.is_empty() {
                if json { println!("{}", serde_json::json!({"query": query, "hits": [], "timed_out": false, "embedding_provider": vectors.provider, "fallback_reason": vectors.fallback_reason})); }
                else { println!("nothing to search — no symbols were embedded for this revision."); }
                return 0;
            }
            let index = vectors.rebuild_index();
            let hit_outcome = index.knn(&query_vec, k, budget_ms.saturating_mul(1_000_000).min(i64::MAX as u64) as i64);
            let hits = match hit_outcome {
                cie_store::hnsw::KnnOutcome::Neighbors(h) => h,
                cie_store::hnsw::KnnOutcome::Unknown(_) => {
                    if json {
                        println!("{}", serde_json::json!({ "query": query, "hits": [], "timed_out": true, "embedding_provider": vectors.provider, "fallback_reason": vectors.fallback_reason }));
                    } else {
                        println!("search timed out — no results (not the same as \"nothing matched\").");
                    }
                    return 0;
                }
                cie_store::hnsw::KnnOutcome::Error(e) => {
                    eprintln!("error: {e:?}");
                    return 1;
                }
            };

            // Rejoin through the same file-scoped keys used by ingestion;
            // equal qualified names in separate files remain separate hits.
            let sym_by_record: std::collections::BTreeMap<[u8; 32], SymbolId> = outcome
                .store
                .symbols
                .iter()
                .zip(texts.iter())
                .map(|(s, (key, _))| (cie_core::types::hash(key.as_bytes()), s.id))
                .collect();

            let hits_json: Vec<_> = hits
                .iter()
                .filter_map(|h| {
                    let sym_id = *sym_by_record.get(&h.record_id)?;
                    let sym = outcome.store.get(sym_id)?;
                    let concepts: Vec<String> = outcome
                        .snapshot
                        .concepts
                        .nodes
                        .iter()
                        .filter(|n| n.symbols.contains(&sym_id))
                        .map(|n| n.label.clone())
                        .collect();
                    let contracts: Vec<String> = outcome
                        .snapshot
                        .contracts
                        .iter()
                        .filter(|c| c.symbols.contains(&sym_id))
                        .map(|c| c.expr.clone())
                        .collect();
                    Some(serde_json::json!({
                        "symbol": sym.qual_name,
                        "distance": h.distance,
                        "evidence": format!("{}:{}", outcome.store.files.path(sym.loc.file), outcome.store.files.line(sym.loc)),
                        "concepts": concepts,
                        "contracts": contracts,
                    }))
                })
                .collect();

            if json {
                println!("{}", serde_json::json!({ "query": query, "hits": hits_json, "timed_out": false, "embedding_provider": vectors.provider, "fallback_reason": vectors.fallback_reason }));
            } else {
                if let Some(reason) = &vectors.fallback_reason { println!("embedding fallback: {reason} (offline)"); }
                println!("search: \"{query}\" ({} hit{}):", hits_json.len(), if hits_json.len() == 1 { "" } else { "s" });
                for h in &hits_json {
                    println!("  - {} ({}) [distance {:.3}]", h["symbol"], h["evidence"], h["distance"].as_f64().unwrap_or(0.0));
                    let concepts = h["concepts"].as_array().cloned().unwrap_or_default();
                    if !concepts.is_empty() {
                        println!(
                            "      concepts: {}",
                            concepts.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")
                        );
                    }
                    let contracts = h["contracts"].as_array().cloned().unwrap_or_default();
                    if !contracts.is_empty() {
                        println!(
                            "      contracts: {}",
                            contracts.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")
                        );
                    }
                }
            }
            0
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
            flow,
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
                    let json_result = if flow {
                        // RFC-0024 stage 0.5: the flow-chart ViewSpec over
                        // revision-verified facts (`extract_revision_files`
                        // hash-checks every source before extraction).
                        let extracted =
                            match cie_ingest::navigation::extract_revision_files(
                                &path,
                                outcome.rev,
                                &outcome.snapshot,
                            ) {
                                Ok(x) => x,
                                Err(e) => {
                                    eprintln!("error: flow facts unavailable: {e}");
                                    return 1;
                                }
                            };
                        let flow_opts = cie_viz::flow::FlowOptions {
                            max_elements: max_elements.unwrap_or(cie_viz::ELEMENT_CAP),
                            layout: layout.clone(),
                        };
                        cie_viz::flow::flow_chart(&extracted.per_file, &extracted.store, &flow_opts)
                            .map(|spec| serde_json::to_string_pretty(&spec).unwrap_or_default())
                    } else {
                        let produce_opts = cie_viz::ProduceOptions {
                            max_elements: max_elements.unwrap_or(cie_viz::ELEMENT_CAP),
                            layout: layout.clone(),
                        };
                        cie_viz::produce(&outcome.snapshot, &produce_opts)
                            .map(|spec| serde_json::to_string_pretty(&spec).unwrap_or_default())
                    };
                    match json_result {
                        Ok(json) => {
                            match out {
                                Some(f) => {
                                    if let Err(e) = std::fs::write(&f, json + "\n") {
                                        eprintln!("error writing {f:?}: {e}");
                                        return 1;
                                    }
                                    println!("wrote {} (semantic hash in the spec)", f.display());
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
        Command::Polyglot {
            path,
            otel,
            viewspec,
            json,
        } => {
            let path = path.canonicalize().unwrap_or(path);
            // stable per-path store name: FNV-1a over the canonical path
            let mut h: u64 = 0xcbf29ce484222325;
            for b in path.to_string_lossy().as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            let db_path = std::env::temp_dir().join(format!("cie-polyglot-{h:x}.db"));
            // DS-EVAL §6.2 — build-then-rename. The web opens `db_path`
            // read-only while a rebuild may run in another process: build at
            // `<final>.building`, then atomically promote on success, so
            // readers always see the previous complete store or the new one —
            // never a half-built DB (the old delete-then-rebuild-in-place
            // left both a no-DB window and a half-built window). The fresh
            // `.building` file per run keeps the honest-recomputation
            // contract: re-runs never collide with a previous revision's rows.
            let building = std::env::temp_dir().join(format!("cie-polyglot-{h:x}.building"));
            let building_sidecars = [
                building.clone(),
                std::env::temp_dir().join(format!("cie-polyglot-{h:x}.building-wal")),
                std::env::temp_dir().join(format!("cie-polyglot-{h:x}.building-shm")),
            ];
            // discard any half-build left by a previous crashed run
            for stale in &building_sidecars {
                let _ = std::fs::remove_file(stale);
            }
            match run_polyglot_build(
                &building,
                &path,
                otel.as_deref(),
                viewspec.as_deref(),
                json,
                &db_path,
            ) {
                Ok(rc) => match std::fs::rename(&building, &db_path) {
                    Ok(()) => {
                        // The rename replaced the inode; any `-wal`/`-shm` at
                        // the final path belonged to the OLD inode (readonly
                        // web readers create empty ones for coordination).
                        // Drop them so the promoted, cleanly-checkpointed DB
                        // always starts sidecar-free — a stale `-wal` must
                        // never be recoverable onto the NEW database.
                        let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
                            "cie-polyglot-{h:x}.db-wal"
                        )));
                        let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
                            "cie-polyglot-{h:x}.db-shm"
                        )));
                        rc
                    }
                    Err(e) => {
                        for stale in &building_sidecars {
                            let _ = std::fs::remove_file(stale);
                        }
                        eprintln!("error: promoting polyglot store: {e}");
                        1
                    }
                },
                Err(e) => {
                    for stale in &building_sidecars {
                        let _ = std::fs::remove_file(stale);
                    }
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Stories { cmd } => run_stories(cmd),
    }
}

/// DS-EVAL §6.2 — build the polyglot store at `building`; the caller promotes
/// it over `db_path` by rename ONLY on success. The web opens `db_path`
/// read-only while a rebuild may run in another process, so promotion must be
/// atomic and the build must be complete-or-nothing.
fn run_polyglot_build(
    building: &std::path::Path,
    path: &std::path::Path,
    otel: Option<&std::path::Path>,
    viewspec: Option<&str>,
    json: bool,
    db_path: &std::path::Path,
) -> Result<i32, String> {
    let conn = cie_polyglot::schema::open_db(building)?;
    let rc = (|| -> Result<i32, String> {
        let static_res =
            cie_polyglot::ir::ingest_and_lower(&conn, 1, 41, &path);
        let Ok((stats, lower)) = static_res else {
            return Err(static_res.err().unwrap_or_default());
        };
        let _ = cie_polyglot::frameworks::install_registry(&conn);
        let fw = cie_polyglot::frameworks::detect(&conn);
        // C4 needs the ingested root for DDD dir checks
        let c4 = cie_polyglot::c4::detect_c4(&conn, &path, 41);
        let sm = cie_polyglot::statemachines::detect(&conn);
        let wf = cie_polyglot::workflows::detect(&conn);
        let dup = cie_polyglot::duplicates::detect(&conn);
        let runtime = otel
            .map(|p| {
                std::fs::read_to_string(p)
                    .map_err(|e| e.to_string())
                    .and_then(|payload| cie_polyglot::runtime::ingest(&conn, &payload))
            })
            .map_or(Ok(None), |r| r.map(Some));

        if let Some(want) = viewspec {
            // `all` → one bundle document: every spec in one JSON,
            // the seam the web pipeline consumes with a single read
            if want == "all" {
                match cie_polyglot::viewspec::produce_bundle(&conn) {
                    Ok(bundle) => {
                        println!("{bundle}");
                        return Ok(0);
                    }
                    Err(e) => return Err(e),
                }
            }
            let specs = cie_polyglot::viewspec::produce(&conn);
            match specs {
                Ok(map) => {
                    let hits: Vec<_> = map
                        .iter()
                        .filter(|(k, _)| {
                            k.starts_with(&want) || k.contains(&want)
                        })
                        .collect();
                    if hits.is_empty() {
                        return Err(format!(
                            "no spec matches '{want}' (available: {})",
                            map.keys().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
                    for (k, v) in hits {
                        println!("{k}");
                        println!("{v}");
                    }
                    return Ok(0);
                }
                Err(e) => return Err(e),
            }
        }

        if json {
            let report = serde_json::json!({
                "db": db_path.display().to_string(),
                "ingest": {
                    "files": stats.files_ingested,
                    "parse_errors": stats.files_parse_errors,
                    "ast_nodes": stats.ast_nodes,
                    "symbols": stats.symbols,
                    "unsupported_files": stats.skipped_unsupported,
                },
                "ir": {
                    "symbols_lowered": lower.symbols_lowered,
                    "blocks": lower.blocks,
                    "instructions": lower.instructions,
                    "calls": lower.calls,
                    "calls_resolved": lower.calls_resolved,
                    "calls_unresolved": lower.calls_unresolved,
                },
                "frameworks": fw.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(serde_json::json!("failed")),
                "c4": c4.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(serde_json::json!("failed")),
                "state_machines": sm.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(serde_json::json!("failed")),
                "workflows": wf.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(serde_json::json!("failed")),
                "duplicates": dup.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(serde_json::json!("failed")),
                "runtime": runtime.as_ref().map(|r| r.as_ref().map(|x| serde_json::to_value(x).unwrap_or_default()).unwrap_or(serde_json::json!("failed"))).unwrap_or(serde_json::json!("not-requested")),
            });
            println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
        } else {
            println!("polyglot store: {}", db_path.display());
            println!(
                "  ingest: {} files ({} parse errors), {} ast nodes, {} symbols, {} unsupported",
                stats.files_ingested, stats.files_parse_errors, stats.ast_nodes, stats.symbols, stats.skipped_unsupported
            );
            println!(
                "  ir: {} symbols lowered, {} blocks, {} instructions, {} calls ({} resolved / {} unresolved)",
                lower.symbols_lowered, lower.blocks, lower.instructions, lower.calls, lower.calls_resolved, lower.calls_unresolved
            );
            if let Ok(r) = &fw {
                println!(
                    "  frameworks: {} role sites, {} declared + {} url-matched bindings ({} specs)",
                    r.roles, r.declared_bindings, r.url_matched_bindings, r.specs_parsed
                );
            }
            if let Ok(r) = &c4 {
                println!(
                    "  c4: {} containers, {} components, {} classes linked, {} relationships",
                    r.containers, r.components, r.classes_linked, r.relationships
                );
            }
            if let Ok(r) = &sm {
                println!(
                    "  state machines: {} T1 facts, {} T2 inference, {} T2 fog, {} T3 hypothesis",
                    r.t1, r.t2, r.t2_fog, r.t3
                );
            }
            if let Ok(r) = &wf {
                println!(
                    "  workflows: {} entries, {} nodes ({} async boundaries, {} gateways)",
                    r.workflows, r.nodes, r.async_boundaries, r.gateways
                );
            }
            if let Ok(r) = &dup {
                println!(
                    "  duplicates: {} pairs ({} canonical-hash facts, {} high, {} medium, {} low, {} hidden; {} class/function containment pairs excluded, {} same-file span-nested pairs excluded)",
                    r.pairs, r.canonical_hash, r.high, r.medium, r.low, r.hidden_gaps, r.skipped_kind_mismatch, r.skipped_nested
                );
            }
            if let Ok(Some(r)) = &runtime {
                println!(
                    "  runtime: {} spans ({} mapped / {} unresolved), {} drift edges, {} flaky",
                    r.spans, r.mapped, r.unresolved, r.drift_edges, r.flaky
                );
            }
        }
        Ok(0)
    })();
    // Best-effort: push every committed WAL frame into the main file so the
    // caller's rename can never orphan rows in a leftover `-wal` (a clean
    // close checkpoints anyway — this is the belt to its suspenders).
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    drop(conn); // clean close: SQLite checkpoints + deletes the -wal/-shm
    if rc.is_ok() {
        let wal = std::path::PathBuf::from(format!("{}-wal", building.display()));
        if wal.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return Err(
                "polyglot WAL did not checkpoint cleanly; refusing to promote".to_string(),
            );
        }
    }
    rc
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
