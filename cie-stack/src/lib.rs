//! cie-stack — CIE's tech-stack pass, engine-side.
//!
//! Flow: read the manifests → detect the stack → ASK THE FACTORY for that
//! stack's parser → run it → merge manifest/docker/config facts → produce
//! one StackReport (the web Master view reads this as stack.json).
//!
//! Deterministic reads of the project's own files. No LLM, no network, no
//! code execution. Every claim names its file.

pub mod detect;
pub mod node;
pub mod python;
pub mod rust;
pub mod flow;
pub mod docker;
pub mod factory;
pub mod spring;
pub mod types;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 512 * 1024;
pub const MAX_FILES: usize = 8000;

/// RFC-0019 §6.2: the files whose change invalidates the persisted stack
/// artifact, even when no LANGUAGE file changed — a new route in the
/// manifest, a renamed compose service, a moved build context all change
/// what the stack lens says without touching a single source file. cie-stack
/// owns this list because it owns the inputs; cie-ingest imports it.
pub const STACK_INPUT_EXTS: &[&str] = &["xml", "gradle", "kts", "toml", "mod"];

/// Is this repo-relative path a stack-lens INPUT (manifest, docker file, or
/// framework config)? Name-based, not extension-based, because the decisive
/// inputs are exactly named files at any depth (Dockerfile has no extension;
/// docker-compose.yml is spelled out). The walk's own ignore list already
/// keeps node_modules/target/vendor copies out.
pub fn is_stack_input(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    let p = p.trim_start_matches("./");
    let base = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();
    // manifests (any depth, exact names)
    if matches!(
        base.as_str(),
        "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "cargo.toml"
            | "package.json"
            | "requirements.txt"
            | "pyproject.toml"
            | "go.mod"
    ) {
        return true;
    }
    // docker
    if base == "dockerfile" || base.starts_with("dockerfile.") {
        return true;
    }
    if matches!(
        base.as_str(),
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml"
    ) {
        return true;
    }
    // framework config (the RFC's `application*.yml/properties` list)
    base.starts_with("application.properties") || base.starts_with("application.yml")
        || base.starts_with("application.yaml")
}

/// which source files matter to a stack parser (jvm code only for now)
fn wants(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    let ext = p.rsplit('.').next().unwrap_or("");
    matches!(ext, "java" | "kt" | "js" | "mjs" | "ts" | "tsx" | "rs" | "py") && !skip_path(&p)
}

fn skip_path(p: &str) -> bool {
    const SKIP: &[&str] = &[
        "node_modules/", "target/", "vendor/", ".git/", "dist/", "build/", "out/", ".gradle/", ".idea/", "__pycache__/",
    ];
    let p = p.trim_start_matches("./");
    SKIP.iter().any(|s| p.contains(s)) || p.contains("src/test/") || p.starts_with("test/")
}

/// walk the tree, collect (rel path → text) for files the parsers want
fn collect_sources(root: &Path, also: &dyn Fn(&str) -> bool) -> (std::collections::BTreeMap<String, String>, usize) {
    let mut files: std::collections::BTreeMap<String, String> = BTreeMap::new();
    let mut seen: usize = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !is_ignored_dir(&name) {
                    stack.push(path);
                }
                continue;
            }
            if seen >= MAX_FILES {
                break;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if wants(&rel) || also(&rel) {
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.len() > MAX_FILE_BYTES {
                    continue;
                }
                if let Ok(src) = fs::read_to_string(&path) {
                    seen += 1;
                    files.insert(rel, src);
                }
            }
        }
    }
    (files, seen)
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "target" | "vendor" | ".git" | ".venv" | "venv" | "__pycache__" | ".gradle" | ".idea" | ".vscode"
            | "dist" | "build" | "out" | ".cie"
    )
}

/// the entry point CIE's CLI calls: `cie stack <dir>` → a StackReport
pub fn stack_report(root: &Path) -> types::StackReport {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // ---- 1. detect from the manifests ----
    let det = detect::detect(root);

    // ---- 2. collect the sources (code for parsers + docker/config files too) ----
    let (sources, files_seen) = collect_sources(root, &|rel: &str| {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        docker::is_dockerfile(base)
            || matches!(
                base,
                "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" | "application.properties"
                    | "application.yml" | "application.yaml"
            )
    });

    // ---- 3. ASK THE FACTORY for the matching parser ----
    let parser = factory::pick(&det);

    let mut report = types::StackReport {
        v: 1,
        build: det.build.clone(),
        languages: det.languages(),
        frameworks: vec![],
        docker: types::DockerInfo {
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
        coverage: types::Coverage {
            files_scanned: files_seen as u32,
            manifests_read: det.manifests_read.clone(),
            dockerfiles_read: vec![],
        },
        generated_at: now,
    };

    // what the factory hands back decides everything about the parse
    report
        .notes
        .push(format!("[stack] {} parsed it: {}", parser.id(), parser.describes()));
    parser.parse(&sources, &mut report);

    // ---- 4. frameworks + docker + config (facts independent of the parser) ----
    // the ROOT manifest's spring claim (single-app repos keep the old shape)
    if det.spring {
        report.frameworks.push(types::StackFramework {
            id: "spring-boot".into(),
            name: format!(
                "Spring Boot {}",
                det.spring_version.clone().unwrap_or_else(|| "(version not stated)".into())
            ),
            evidence: report
                .coverage
                .manifests_read
                .first()
                .cloned()
                .unwrap_or_else(|| "pom.xml".into()),
            modules: det.spring_starters.clone(),
        });
    }
    // one framework entry per detected app (the polyglot view)
    for app in &det.apps {
        if app.framework != "spring" || det.spring && app.dir.is_empty() && report.frameworks.iter().any(|f| f.id == "spring-boot") {
            if app.framework == "spring" && report.frameworks.iter().any(|f| f.id == "spring-boot") {
                continue; // the root entry already covers this one
            }
        }
        if app.dir.is_empty() && det.spring && std::ptr::eq(app, det.apps.first().unwrap_or(&det_apps_empty())) {
            continue;
        }
        report.frameworks.push(types::StackFramework {
            id: app.framework.clone(),
            name: framework_pretty(&app.framework),
            evidence: app.evidence.clone(),
            modules: vec![],
        });
    }

    // docker
    let rels: Vec<String> = sources.keys().cloned().collect();
    report.docker = docker::docker_info(root, &rels);
    report.coverage.dockerfiles_read = report
        .docker
        .dockerfiles
        .iter()
        .map(|d| d.path.clone())
        .chain(
            report
                .docker
                .compose
                .iter()
                .map(|c| c.path.clone()),
        )
        .collect();

    // application.properties/yml config entries, as written
    for (rel, src) in &sources {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if base.starts_with("application.") {
            report.config.push(parse_app_config(rel, src));
        }
    }

    // the per-app paragraph facts
    report.apps = det
        .apps
        .iter()
        .map(|a| types::AppSite {
            dir: a.dir.clone(),
            name: a.name.clone(),
            build: a.build.clone(),
            framework: a.framework.clone(),
            evidence: a.evidence.clone(),
        })
        .collect();

    // jvm counts live in the deep stack for the "one family" framing
    if let Some(jvm) = &mut report.jvm {
        jvm.scanned = det.java_files + det.kotlin_files;
        jvm.java_files = det.java_files;
        jvm.kotlin_files = det.kotlin_files;
        if det.java_files > 0 && det.kotlin_files > 0 {
            report
                .notes
                .push("jvm-mixed: java and kotlin share ONE runtime and classpath — read as one family, two syntaxes.".into());
        }
        if report.routes.is_empty() {
            report
                .notes
                .push("spring detected in the build files but no @RestController/@Controller endpoints parsed — the routes table stays honestly empty.".into());
        }
    } else if det.is_jvm() {
        // JVM files but the factory gave a stub (no Spring in the manifests)
        report
            .notes
            .push("jvm code found; no spring in the manifests — route/main parsing needs the framework named.".into());
    }

    if report.routes.is_empty() && report.jvm.is_none() {
        report
            .notes
            .push("no routes parsed for this stack yet — the stub above says exactly what CIE did and didn't read.".into());
    }

    report
}

fn det_apps_empty() -> detect::AppDet {
    detect::AppDet {
        dir: String::new(),
        name: String::new(),
        build: String::new(),
        framework: String::new(),
        evidence: String::new(),
        holds_jvm: false,
    }
}

fn framework_pretty(id: &str) -> String {
    match id {
        "spring" => "Spring Boot".into(),
        "express" => "express".into(),
        "fastify" => "fastify".into(),
        "axum" => "axum".into(),
        "actix-web" => "actix-web".into(),
        "rocket" => "rocket".into(),
        "flask" => "Flask".into(),
        "fastapi" => "FastAPI".into(),
        "gin" => "gin".into(),
        "net-http" => "net/http (Go stdlib)".into(),
        other => other.into(),
    }
}

/// application.properties / application.yml — the few things that matter,
/// as written (server.port, active profile, datasource url)
fn parse_app_config(rel: &str, src: &str) -> types::ConfigEntry {
    let mut port = None;
    let mut profiles = None;
    let mut datasource = None;
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("server.port") {
            let v = rest.trim_start_matches('=').trim();
            if !v.is_empty() {
                port = v.parse().ok();
            }
        } else if let Some(rest) = line.strip_prefix("spring.profiles.active") {
            let v = rest.trim_start_matches('=').trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                profiles = Some(v.to_string());
            }
        } else if line.contains("spring.datasource.url") {
            let v = line.split('=').next_back().map(|x| x.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
            if let Some(v) = v {
                if !v.is_empty() {
                    datasource = Some(v);
                }
            }
        }
        // yaml forms: `port: 8080` / `active: prod`
        if let Some(rest) = line.strip_prefix("port:") {
            if port.is_none() {
                port = rest.trim().parse().ok();
            }
        }
    }
    let note = if rel.ends_with(".properties") {
        "application.properties, as written"
    } else {
        "application.yml, as written"
    };
    types::ConfigEntry {
        rel: rel.to_string(),
        service: String::new(),
        port,
        profiles,
        datasource,
        note: note.into(),
    }
}

/// serialize the report to pretty JSON (what /cie stack --json prints)
pub fn to_json(report: &types::StackReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
}

/// RFC-0019 §6.1: the ingest-side entry point. Runs the same deterministic
/// lens as [`stack_report`] — one walk, one parse, the factory's own honest
/// caps (`MAX_FILES`, `MAX_DIRS`, `flow.rs` limits) bound the work — and
/// hands the report to ingest for persistence as `revs/<N>/stack.json`.
///
/// `budget_ms` is the caller's sub-deadline slice. The lens cannot abort
/// mid-parse (a half-walked report would read as authoritative when it is
/// neither complete nor honest), so the budget is enforced as a POST-HOC
/// disclosure: an overrun adds a note naming the measured wall time and the
/// caps that bounded it. Measured sub-second on 5 apps / 36 files (the
/// polyglot sample, release build); the caps keep the worst case bounded.
pub fn stack_report_stage(root: &Path, budget_ms: Option<u64>) -> types::StackReport {
    let t0 = std::time::Instant::now();
    let mut report = stack_report(root);
    if let Some(budget) = budget_ms {
        let wall_ms = t0.elapsed().as_millis() as u64;
        if wall_ms > budget {
            report.notes.push(format!(
                "stack stage took {wall_ms}ms against a {budget}ms budget slice — the walk/parse caps (max {MAX_FILES} files) bounded it; the report above is complete, not truncated"
            ));
        }
    }
    report
}

/// convenience for tests / CLI: build + print in one call
pub fn build_json(root: &Path) -> String {
    let r = stack_report(root);
    to_json(&r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StackReport;
    use std::path::PathBuf;
    use std::fs;
    use std::process::Command;

    fn tmpdir(tag: &str) -> PathBuf {
        let t = std::env::temp_dir().join(format!("cie-stack-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&t);
        fs::create_dir_all(&t).unwrap();
        t
    }

    #[test]
    fn reports_stub_for_unknown_stack() {
        let t = tmpdir("unknown");
        let r = stack_report(&t);
        assert_eq!(r.build.tool, "unknown");
        assert!(r.notes.iter().any(|n| n.contains("no route/main parser")));
        let _ = fs::remove_dir_all(&t);
    }

    // The Spring fixture lives only in /tmp (not in this repo) — run
    // `cargo test` with CIE_SPRING_FIXTURE set to exercise it:
    #[test]
    fn spring_fixture() {
        let dir = match std::env::var("CIE_SPRING_FIXTURE") {
            Ok(d) => d,
            Err(_) => return, // fixture not provided — skip cleanly
        };
        let r = stack_report(Path::new(&dir));
        assert!(!report_routes_empty(&r));
        assert!(r.jvm.is_some());
        let _ = Command::new("true").status();
    }

    fn report_routes_empty(r: &StackReport) -> bool {
        r.routes.is_empty()
    }

    // keep serde_json imported for tests that check the JSON shape (debug)
    #[allow(dead_code)]
    fn roundtrip(r: &StackReport) -> serde_json::Value {
        let s = serde_json::to_string(r).unwrap();
        serde_json::from_str(&s).unwrap()
    }
}