//! The factory this whole crate is organised around.
//!
//! Detection reads the manifests at EVERY depth, then you ask the factory
//! for the matching parser and call it through one uniform interface.
//! A repo may HOST SEVERAL applications (a polyglot monorepo): the factory
//! then hands back a composite that runs one parser per detected app and
//! merges their results — routes/mains/flows across languages, grouped by
//! the app's own dir. Stacks without a parser return an honest stub.
//!
//!   let parser = factory::pick(&detect);   // which stack's parser(s)?
//!   parser.parse(&files, &mut report)        // → routes + mains + flows
//!
//! The Rust ENGINE does the parsing — tree-sitter, no LLM, no network,
//! nothing evaluated or executed. Files are read as written.

use crate::types::StackReport;
use std::collections::BTreeMap;
use std::sync::Arc;

/// per-file source the caller collected (repo-relative path → text)
pub type Sources = BTreeMap<String, String>;

/// the one interface every stack parser answers to
pub trait StackParser: Send + Sync {
    /// stable id: "spring-jvm" | "node-express" | "rust-axum" ...
    fn id(&self) -> &'static str;
    /// what this parser covers, in plain words
    fn describes(&self) -> String;
    /// run it: fill routes + mains + flows; JVM parsers fill `jvm` too
    fn parse(&self, files: &Sources, report: &mut StackReport);
}

/// keep only the files under this app's dir ("" = everything).
/// Paths stay REPO-RELATIVE — the source panel opens the real file.
pub fn scope(files: &Sources, dir: &str) -> Sources {
    if dir.is_empty() {
        return files.clone();
    }
    let prefix = format!("{dir}/");
    files
        .iter()
        .filter(|(rel, _)| rel.starts_with(&prefix) || rel.as_str() == dir.trim_start_matches("./"))
        .map(|(rel, src)| (rel.clone(), src.clone()))
        .collect()
}

// ---- the honest stub, for stacks without a parser yet ----
struct StubParser {
    id: &'static str,
    name: String,
}

impl StackParser for StubParser {
    fn id(&self) -> &'static str {
        self.id
    }
    fn describes(&self) -> String {
        format!(
            "{}: not mapped yet — CIE counts files but doesn't name routes or mains for this stack yet",
            self.name
        )
    }
    fn parse(&self, _files: &Sources, report: &mut StackReport) {
        report
            .notes
            .push(format!("{} — no route/main parser for this stack yet; the counts and manifests above are all CIE knows.", self.describes()));
    }
}

// ---- the real parsers (one per family) ----
pub struct SpringParser {
    pub dir: String,
}

impl crate::factory::StackParser for SpringParser {
    fn id(&self) -> &'static str {
        "spring-jvm"
    }
    fn describes(&self) -> String {
        "Spring over the JVM (java + kotlin read as one family)".to_string()
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        let scoped = scope(files, &self.dir);
        let mut classes = Vec::new();
        for (rel, src) in &scoped {
            let lang: &'static str = if rel.ends_with(".java") {
                "java"
            } else if rel.ends_with(".kt") {
                "kotlin"
            } else {
                continue;
            };
            if skip_source(rel) {
                continue;
            }
            classes.push(crate::spring::parse_file(lang, rel, src));
        }
        let all: Vec<crate::spring::ClassNode> = classes.into_iter().flatten().collect();
        let asm = crate::spring::assemble(all, &self.dir);
        // merge into the report (two JVM services in one repo both land here)
        if let Some(existing) = &mut report.jvm {
            existing.config_classes.extend(asm.jvm.config_classes);
            existing.services.extend(asm.jvm.services);
            existing.repositories.extend(asm.jvm.repositories);
            existing.entities.extend(asm.jvm.entities);
            existing.scheduled_in.extend(asm.jvm.scheduled_in);
            existing.listeners.extend(asm.jvm.listeners);
            existing.outbound.extend(asm.jvm.outbound);
        } else {
            report.jvm = Some(asm.jvm);
        }
        report.routes.extend(asm.routes);
        report.mains.extend(asm.mains);
        report.flows.extend(asm.flows);
    }
}

fn skip_source(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    p.contains("/src/test/") || p.starts_with("src/test/") || p.starts_with("test/") || p.contains("/test/")
}

// ---- the factory ----

/// build the parser for ONE detected app
pub fn make_for_app(app: &crate::detect::AppDet) -> Box<dyn StackParser> {
    let dir = app.dir.clone();
    match app.framework.as_str() {
        "spring" => {
            if app.holds_jvm {
                Box::new(SpringParser { dir })
            } else {
                Box::new(StubParser {
                    id: "spring-stub",
                    name: format!("Spring app in {} — no java/kotlin files under its dir found", app_label(app)),
                })
            }
        }
        "express" | "fastify" | "node-unknown" => Box::new(crate::node::NodeParser { dir, framework: node_framework_label(app) }),
        "axum" | "actix-web" | "rocket" | "rust-unknown" => Box::new(crate::rust::RustParser { dir, framework: app.framework.to_string() }),
        "flask" | "fastapi" => Box::new(crate::python::PyParser { dir, framework: app.framework.to_string() }),
        other => Box::new(StubParser {
            id: "stub",
            name: format!("{} app in {} ({})", app_label(app), app.build, other),
        }),
    }
}

fn app_label(app: &crate::detect::AppDet) -> String {
    if app.dir.is_empty() {
        "the repo root".to_string()
    } else {
        app.dir.to_string()
    }
}

fn node_framework_label(app: &crate::detect::AppDet) -> String {
    match app.framework.as_str() {
        "express" => "express".into(),
        "fastify" => "fastify".into(),
        _ => "express-or-similar".into(),
    }
}

/// ask which parser matches what detection found — never hand-routed.
/// Several detected apps → a composite (one parser per app, merged).
pub fn pick(d: &crate::detect::Detect) -> Arc<dyn StackParser> {
    if d.apps.len() > 1 {
        return Arc::new(CompositeParser { apps: d.apps.clone() });
    }
    if let Some(app) = d.apps.first() {
        return Arc::from(make_for_app(app));
    }
    // no manifests anywhere — the old root-level stub path
    match d.build.tool.as_str() {
        "cargo" => Arc::new(StubParser {
            id: "rust-stub",
            name: "Rust".into(),
        }),
        "npm" => Arc::new(StubParser {
            id: "node-stub",
            name: "Node".into(),
        }),
        "maven" | "gradle" if d.is_jvm() => Arc::new(StubParser {
            id: "jvm-stub",
            name: "JVM (no Spring detected)".into(),
        }),
        _ => Arc::new(StubParser {
            id: "generic-stub",
            name: "This stack".into(),
        }),
    }
}

/// one parser per detected app, results merged into ONE report
struct CompositeParser {
    apps: Vec<crate::detect::AppDet>,
}

impl StackParser for CompositeParser {
    fn id(&self) -> &'static str {
        "composite"
    }
    fn describes(&self) -> String {
        format!(
            "a composite — {} app(s), one parser each, merged: {}",
            self.apps.len(),
            self.apps
                .iter()
                .map(|a| format!("{} ({})", if a.dir.is_empty() { "root" } else { &a.dir }, a.framework))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        for app in &self.apps {
            let parser = make_for_app(app);
            let mut one = types_fresh(report);
            parser.parse(files, &mut one);
            merge(report, one);
        }
    }
}

fn types_fresh(reference: &StackReport) -> StackReport {
    StackReport {
        v: reference.v,
        build: crate::types::BuildInfo {
            tool: String::new(),
            modules: vec![],
            artifact_name: String::new(),
        },
        languages: vec![],
        frameworks: vec![],
        docker: crate::types::DockerInfo {
            present: false,
            dockerfiles: vec![],
            compose: None,
            note: String::new(),
        },
        jvm: None,
        routes: vec![],
        mains: vec![],
        flows: vec![],
        apps: vec![],
        config: vec![],
        notes: vec![],
        coverage: crate::types::Coverage {
            files_scanned: 0,
            manifests_read: vec![],
            dockerfiles_read: vec![],
        },
        generated_at: 0,
    }
}

fn merge(main: &mut StackReport, one: StackReport) {
    main.routes.extend(one.routes);
    main.mains.extend(one.mains);
    main.flows.extend(one.flows);
    main.config.extend(one.config);
    if let Some(incoming) = one.jvm {
        if let Some(existing) = &mut main.jvm {
            existing.config_classes.extend(incoming.config_classes);
            existing.services.extend(incoming.services);
            existing.repositories.extend(incoming.repositories);
            existing.entities.extend(incoming.entities);
            existing.scheduled_in.extend(incoming.scheduled_in);
            existing.listeners.extend(incoming.listeners);
            existing.outbound.extend(incoming.outbound);
        } else {
            main.jvm = Some(incoming);
        }
    }
    for n in one.notes {
        main.notes.push(n);
    }
}
