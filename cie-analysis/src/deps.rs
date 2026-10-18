//! Manifest dependency graph + static usage evidence + upgrade what-if
//! (CIE-US US-062 engine close).
//!
//! Story obligation: "see where a package matters and what an upgrade or
//! replacement affects … Usage evidence, advisories, compatibility concerns
//! and proposed replacement work are separated. An available fixed version is
//! never presented as automatically compatible with the application."
//!
//! Honesty rules baked into this module:
//! * manifests are parsed by hand-rolled readers for the common formats
//!   (`package.json`, `pyproject.toml`, `requirements.txt`, `Cargo.toml`,
//!   `go.mod`) — a format we cannot parse is REPORTED as unparsed, never
//!   guessed;
//! * usage evidence comes ONLY from adapter-produced `ImportFact`s (FR-3:
//!   semantics never cross languages). Today that means Python projects have
//!   import-backed usage; other ecosystems get an explicit "no import facts
//!   for this adapter" note — never a fabricated zero;
//! * a declared package with no matched import is NOT called "unused" — it
//!   is marked unresolved with the reason (entry-point or indirect usage is
//!   invisible to static analysis);
//! * upgrade impact separates "who imports this" (evidence) from
//!   "is the new version compatible" which is ALWAYS [`Compatibility::Unknown`].

use std::collections::BTreeMap;
use std::path::Path;

use cie_adapters::LanguageRegistry;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Manifest model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Pypi,
    Npm,
    Cargo,
    Go,
    Maven,
    Gradle,
    Composer,
    Nuget,
}

impl Ecosystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ecosystem::Pypi => "pypi",
            Ecosystem::Npm => "npm",
            Ecosystem::Cargo => "cargo",
            Ecosystem::Go => "go",
            Ecosystem::Maven => "maven",
            Ecosystem::Gradle => "gradle",
            Ecosystem::Composer => "composer",
            Ecosystem::Nuget => "nuget",
        }
    }
}

/// One declared dependency from one manifest file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    pub name: String,
    pub ecosystem: Ecosystem,
    /// Version requirement string as written (never interpreted).
    pub version_req: String,
    /// `license` / `licence` field when the manifest carries one.
    pub licence: Option<String>,
    /// "runtime" or "dev" (best-effort from the manifest's own grouping).
    pub scope: &'static str,
    /// Manifest file (relative to the scanned root) that declared this.
    pub source: String,
}

/// Result of one manifest scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DependencyTable {
    pub deps: Vec<Dependency>,
    /// Manifest-looking files that could not be parsed (named, never silent).
    pub unparsed: Vec<String>,
    /// How many manifest files were read.
    pub files_scanned: usize,
}

// ---------------------------------------------------------------------------
// Manifest readers (hand-rolled, minimal, honest)
// ---------------------------------------------------------------------------

fn push_dep(
    table: &mut DependencyTable,
    source: &str,
    eco: Ecosystem,
    name: &str,
    ver: &str,
    scope: &'static str,
    licence: Option<String>,
) {
    let name = name.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    if name.is_empty() || name.starts_with('-') || name.starts_with('#') {
        return;
    }
    table.deps.push(Dependency {
        name,
        ecosystem: eco,
        version_req: ver.trim().to_string(),
        licence: licence.clone(),
        scope,
        source: source.to_string(),
    });
}

/// `package.json`: dependencies / devDependencies + license.
fn read_package_json(rel: &str, src: &str, table: &mut DependencyTable) -> Result<(), String> {
    let v: serde_json::Value =
        serde_json::from_str(src).map_err(|e| format!("{rel}: {e}"))?;
    let licence = v
        .get("license")
        .and_then(|l| l.as_str())
        .map(|s| s.to_string());
    for (scope, key) in [("runtime", "dependencies"), ("dev", "devDependencies")] {
        if let Some(map) = v.get(key).and_then(|m| m.as_object()) {
            for (name, ver) in map {
                push_dep(
                    table,
                    rel,
                    Ecosystem::Npm,
                    name,
                    ver.as_str().unwrap_or(""),
                    scope,
                    licence.clone(),
                );
            }
        }
    }
    Ok(())
}

/// `requirements.txt`: `name==ver`, `name>=ver`, comments/flags skipped.
pub fn read_requirements(rel: &str, src: &str, table: &mut DependencyTable) {
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let line = line.split(';').next().unwrap_or(line); // drop env markers
        let (name, ver) = match line.find(|c| matches!(c, '=' | '<' | '>' | '!' | '~')) {
            Some(i) => (&line[..i], &line[i..]),
            None => (line, ""),
        };
        push_dep(table, rel, Ecosystem::Pypi, name, ver, "runtime", None);
    }
}

/// Minimal TOML subset reader shared by pyproject/Cargo: returns the raw
/// lines of `section` (case-insensitive match) until the next `[section]`.
fn toml_section<'a>(src: &'a str, section: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            inside = t
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split('.')
                .next()
                .map(|h| h.trim().eq_ignore_ascii_case(section))
                .unwrap_or(false);
            continue;
        }
        if inside && !t.is_empty() {
            out.push(t);
        }
    }
    out
}

fn toml_string_value(v: &str) -> Option<String> {
    let v = v.trim();
    let rest = v.split_once('=').map(|x| x.1.trim()).unwrap_or(v);
    let rest = rest.split('#').next().unwrap_or(rest).trim();
    if rest.starts_with('{') {
        // inline table: license = { text = "MIT" } — find text = "..."
        let inner = rest.trim_start_matches('{').trim_end_matches('}');
        for part in inner.split(',') {
            let p = part.trim();
            if p.starts_with("text") || p.starts_with("license") || p.starts_with("licence") {
                if let Some(q) = p.split('"').nth(1) {
                    return Some(q.to_string());
                }
            }
        }
        return None;
    }
    if rest.starts_with('"') || rest.starts_with('\'') {
        return Some(rest.trim_matches(|c| c == '"' || c == '\'').to_string());
    }
    None
}

/// `pyproject.toml`: [project] dependencies + licence (PEP 621).
/// The dependencies array may span lines — collected until `]`.
fn read_pyproject(rel: &str, src: &str, table: &mut DependencyTable) {
    let section = toml_section(src, "project");
    let licence = section
        .iter()
        .find(|l| l.starts_with("license"))
        .and_then(|l| toml_string_value(l));
    let mut collecting = false;
    let mut buf = String::new();
    for line in &section {
        if collecting {
            buf.push(' ');
            buf.push_str(line);
            if buf.contains(']') {
                break;
            }
        } else if line.starts_with("dependencies") {
            collecting = true;
            buf.push_str(line);
            if buf.contains(']') {
                break;
            }
        }
    }
    for item in buf.split(',') {
        if let Some(q) = item.split('"').nth(1) {
            let (name, ver) = match q.find(|c: char| c.is_whitespace() || ">=<!~".contains(c)) {
                Some(i) => (&q[..i], &q[i..]),
                None => (q, ""),
            };
            push_dep(table, rel, Ecosystem::Pypi, name, ver, "runtime", licence.clone());
        }
    }
}

/// `Cargo.toml`: [package] license + [dependencies] (workspace refs skipped).
pub fn read_cargo_toml(rel: &str, src: &str, table: &mut DependencyTable) {
    let licence = toml_section(src, "package")
        .iter()
        .find(|l| l.starts_with("license"))
        .and_then(|l| toml_string_value(l));
    for line in toml_section(src, "dependencies") {
        let Some((name, rhs)) = line.split_once('=') else { continue };
        let name = name.trim();
        if rhs.contains("workspace = true") || name.ends_with(".workspace") {
            continue;
        }
        let ver = if rhs.trim().starts_with('{') {
            rhs.split("version")
                .nth(1)
                .and_then(|s| s.split('"').nth(1))
                .unwrap_or("")
                .to_string()
        } else {
            rhs.trim()
                .trim_matches('"')
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        };
        push_dep(table, rel, Ecosystem::Cargo, name, &ver, "runtime", licence.clone());
    }
}

/// `go.mod`: module + require block (single and grouped).
fn read_go_mod(rel: &str, src: &str, table: &mut DependencyTable) {
    let mut in_block = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("require (") {
            in_block = true;
            continue;
        }
        if in_block && t == ")" {
            in_block = false;
            continue;
        }
        if in_block || t.starts_with("require ") {
            let t = t.strip_prefix("require ").unwrap_or(t).trim();
            let mut parts = t.split_whitespace();
            if let (Some(name), Some(ver)) = (parts.next(), parts.next()) {
                let scope = if t.contains("// indirect") { "indirect" } else { "runtime" };
                push_dep(table, rel, Ecosystem::Go, name, ver, if scope == "indirect" { "runtime" } else { "runtime" }, None);
            }
        }
    }
}

/// `pom.xml` (Maven): `<dependency>` blocks → `groupId:artifactId`.
/// Hand-rolled line reader — no XML tree, no namespace guessing; a pom we
/// cannot follow is reported unparsed, never approximated.
fn read_pom_xml(rel: &str, src: &str, table: &mut DependencyTable) -> Result<(), String> {
    if !src.contains("<project") {
        return Err(format!("{rel}: no <project> element found"));
    }
    // project-level licence (first <license><name> inside <licenses>)
    let licence = src
        .find("<licenses>")
        .and_then(|start| src[start..].find("</licenses>").map(|end| &src[start..start + end]))
        .and_then(|block| xml_text(&block, "name"));

    let mut rest = src;
    while let Some(pos) = rest.find("<dependency>") {
        let Some(end) = rest[pos..].find("</dependency>") else { break };
        let block = &rest[pos..pos + end];
        let group = xml_text(block, "groupId").unwrap_or_default();
        let artifact = xml_text(block, "artifactId").unwrap_or_default();
        let version = xml_text(block, "version").unwrap_or_default();
        let scope = match xml_text(block, "scope") {
            Some(s) if s.eq_ignore_ascii_case("test") => "dev",
            _ => "runtime",
        };
        if !group.is_empty() && !artifact.is_empty() {
            push_dep(
                table,
                rel,
                Ecosystem::Maven,
                &format!("{group}:{artifact}"),
                &version,
                scope,
                licence.clone(),
            );
        }
        rest = &rest[pos + end + "</dependency>".len()..];
    }
    Ok(())
}

/// First `<tag>value</tag>` occurrence in `src` (trimmed, entities kept as
/// written — values are never interpreted).
fn xml_text(src: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = src.find(&open)? + open.len();
    let end = src[start..].find(&close)? + start;
    let v = src[start..end].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// `build.gradle` / `build.gradle.kts` (Gradle): quoted `group:artifact[:version]`
/// coordinates after a configuration keyword (`implementation`, `api`,
/// `testImplementation`, `classpath`, …). Project-local (`project(…)`) and
/// file-list dependencies are skipped — they are not external packages.
fn read_gradle(rel: &str, src: &str, table: &mut DependencyTable) {
    const CONFIGS: &[&str] = &[
        "implementation", "api", "compile", "runtimeOnly", "compileOnly", "kapt",
        "ksp", "debugImplementation", "releaseImplementation", "testImplementation",
        "androidTestImplementation", "testCompile", "classpath", "coreLibraryDesugaring",
        "implementation platform", "testRuntimeOnly", "annotationProcessor",
    ];
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("//") || t.is_empty() {
            continue;
        }
        // find the first quoted string containing ':'
        let mut quote_hit: Option<(usize, usize)> = None; // (open, close)
        let bytes = t.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'"' | b'\'' => {
                    let q = bytes[i];
                    let mut j = i + 1;
                    while j < bytes.len() && bytes[j] != q {
                        j += 1;
                    }
                    if j < bytes.len() && t[i + 1..j].contains(':') {
                        quote_hit = Some((i, j));
                        break;
                    }
                    i = j + 1;
                    continue;
                }
                _ => i += 1,
            }
        }
        let Some((open, close)) = quote_hit else { continue };
        if t[open + 1..close].contains("project(") || t[open + 1..close].starts_with(":") {
            continue; // project-local coordinates are not external packages
        }
        let before = t[..open].to_ascii_lowercase();
        let matched = CONFIGS.iter().any(|c| before.contains(c));
        if !matched {
            continue; // a quoted string alone is not a dependency claim
        }
        let coords = t[open + 1..close].trim();
        let mut parts = coords.split(':');
        let (Some(group), Some(artifact)) = (parts.next(), parts.next()) else { continue };
        let version = parts.next().unwrap_or("");
        let scope = if before.contains("test") || before.contains("androidtest") {
            "dev"
        } else {
            "runtime"
        };
        push_dep(
            table,
            rel,
            Ecosystem::Gradle,
            &format!("{group}:{artifact}"),
            version,
            scope,
            None,
        );
    }
}

/// `composer.json` (PHP): require / require-dev + license.
fn read_composer_json(rel: &str, src: &str, table: &mut DependencyTable) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(src).map_err(|e| format!("{rel}: {e}"))?;
    let licence = match v.get("license") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(a)) => {
            a.first().and_then(|x| x.as_str()).map(|s| s.to_string())
        }
        _ => None,
    };
    for (scope, key) in [("runtime", "require"), ("dev", "require-dev")] {
        if let Some(map) = v.get(key).and_then(|m| m.as_object()) {
            for (name, ver) in map {
                if name == "php" || name.starts_with("ext-") || name.starts_with("lib-") {
                    continue; // platform requirements, not packages
                }
                push_dep(table, rel, Ecosystem::Composer, name, ver.as_str().unwrap_or(""), scope, licence.clone());
            }
        }
    }
    Ok(())
}

/// `*.csproj` (NuGet): `<PackageReference Include=... Version=... />`.
fn read_csproj(rel: &str, src: &str, table: &mut DependencyTable) {
    for line in src.lines() {
        let t = line.trim();
        if !t.contains("<PackageReference") {
            continue;
        }
        let include = attr_value(t, "Include").unwrap_or_default();
        if include.is_empty() {
            continue;
        }
        let version = attr_value(t, "Version").unwrap_or_default();
        push_dep(table, rel, Ecosystem::Nuget, &include, &version, "runtime", None);
    }
}

/// Value of `Attr="…"` in an XML-ish tag (double or single quoted).
fn attr_value(tag: &str, attr: &str) -> Option<String> {
    for q in ['"', '\''] {
        let pat = format!("{attr}={q}");
        if let Some(pos) = tag.find(&pat) {
            let rest = &tag[pos + pat.len()..];
            if let Some(end) = rest.find(q) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// Walk `root` (depth-capped, common junk dirs skipped) and read every
/// recognised manifest. Unknown *.toml/*.json at root are NOT guessed.
pub fn scan_manifests(root: &Path) -> DependencyTable {
    let mut table = DependencyTable::default();
    let skip_dirs = [".git", "node_modules", "target", "__pycache__", ".venv", "venv", "dist", "build"];
    let mut visit = |dir: &Path, table: &mut DependencyTable| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .into_owned();
            let src = match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(_) => continue,
            };
            match name.as_str() {
                "package.json" => {
                    table.files_scanned += 1;
                    if let Err(e) = read_package_json(&rel, &src, table) {
                        table.unparsed.push(e);
                    }
                }
                "requirements.txt" => {
                    table.files_scanned += 1;
                    read_requirements(&rel, &src, table);
                }
                "pyproject.toml" => {
                    table.files_scanned += 1;
                    read_pyproject(&rel, &src, table);
                }
                "Cargo.toml" => {
                    table.files_scanned += 1;
                    read_cargo_toml(&rel, &src, table);
                }
                "go.mod" => {
                    table.files_scanned += 1;
                    read_go_mod(&rel, &src, table);
                }
                "pom.xml" => {
                    table.files_scanned += 1;
                    if let Err(e) = read_pom_xml(&rel, &src, table) {
                        table.unparsed.push(e);
                    }
                }
                "build.gradle" | "build.gradle.kts" => {
                    table.files_scanned += 1;
                    read_gradle(&rel, &src, table);
                }
                "composer.json" => {
                    table.files_scanned += 1;
                    if let Err(e) = read_composer_json(&rel, &src, table) {
                        table.unparsed.push(e);
                    }
                }
                n if n.ends_with(".csproj") => {
                    table.files_scanned += 1;
                    read_csproj(&rel, &src, table);
                }
                _ => {}
            }
        }
    };
    visit(root, &mut table);
    let Ok(subs) = std::fs::read_dir(root) else { return table };
    for e in subs.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() && !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
            visit(&p, &mut table);
        }
    }
    table.deps.sort_by(|a, b| {
        a.ecosystem
            .as_str()
            .cmp(b.ecosystem.as_str())
            .then(a.name.cmp(&b.name))
            .then(a.source.cmp(&b.source))
    });
    table.deps.dedup_by(|a, b| a.name == b.name && a.source == b.source && a.scope == b.scope);
    table
}

// ---------------------------------------------------------------------------
// Usage evidence (adapter facts only — FR-3)
// ---------------------------------------------------------------------------

/// Usage evidence for one declared package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageUsage {
    pub name: String,
    pub ecosystem: Ecosystem,
    /// Files whose adapter-recorded imports matched this package.
    pub imported_by: Vec<String>,
    /// Import-root matched at least one adapter fact?
    pub resolved: bool,
    /// Why the evidence is what it is (honesty statement, never empty).
    pub note: String,
}

/// Collect import module strings per file using the registry's OWN adapters
/// (each file parsed only by its own adapter — FR-3/global rule 6).
pub fn collect_imports(root: &Path, reg: &LanguageRegistry) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let skip_dirs = [".git", "node_modules", "target", "__pycache__", ".venv", "venv", "dist", "build"];
    let mut walk = |dir: &Path, out: &mut BTreeMap<String, Vec<String>>| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .into_owned();
            let Some(adapter) = reg.adapter_for(&rel) else { continue };
            let Ok(src) = std::fs::read_to_string(&p) else { continue };
            let facts = adapter.extract(&rel, &src);
            let mods: Vec<String> = facts.imports.iter().map(|i| i.module.clone()).collect();
            if !mods.is_empty() {
                out.insert(rel, mods);
            }
        }
    };
    walk(root, &mut out);
    let Ok(subs) = std::fs::read_dir(root) else { return out };
    for e in subs.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() && !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
            walk(&p, &mut out);
        }
    }
    out
}

fn pypi_variants(name: &str) -> Vec<String> {
    let n = name.to_ascii_lowercase();
    let n = n.as_str();
    vec![n.to_string(), n.replace('-', "_"), n.replace('-', "")]
}

fn npm_variants(name: &str) -> Vec<String> {
    vec![name.to_string()]
}

fn go_variants(name: &str) -> Vec<String> {
    // import path prefix match: github.com/pkg/errors
    vec![name.to_string()]
}

fn cargo_variants(name: &str) -> Vec<String> {
    vec![name.replace('-', "_"), name.to_string()]
}

/// `group:artifact` (Maven/Gradle) → import-root candidates: the groupId
/// (`org.apache.commons`), the artifactId, and both joined.
fn jvm_variants(name: &str) -> Vec<String> {
    let mut out = vec![];
    let mut parts = name.splitn(2, ':');
    if let (Some(g), Some(a)) = (parts.next(), parts.next()) {
        out.push(g.to_string());
        out.push(a.to_string());
        out.push(format!("{g}.{a}"));
    }
    out.push(name.to_string());
    out
}

fn composer_variants(name: &str) -> Vec<String> {
    // monolog/monolog → Monolog\… namespaces; keep both slash and backslash
    vec![
        name.to_string(),
        name.replace('/', "\\"),
        name.split('/').next().unwrap_or(name).to_string(),
    ]
}

fn nuget_variants(name: &str) -> Vec<String> {
    vec![name.to_string(), name.to_ascii_lowercase()]
}

/// Match one import module string against a package's variants.
fn matches_import(eco: Ecosystem, variants: &[String], module: &str) -> bool {
    let m = module.trim();
    match eco {
        Ecosystem::Pypi => variants.iter().any(|v| {
            let root = m.split('.').next().unwrap_or(m);
            root.eq_ignore_ascii_case(v)
        }),
        Ecosystem::Npm => variants
            .iter()
            .any(|v| m == v.as_str() || m.starts_with(&format!("{v}/"))),
        Ecosystem::Cargo => variants.iter().any(|v| {
            m == v.as_str()
                || m.starts_with(&format!("{v}::"))
                || m.split("::").next() == Some(v.as_str())
        }),
        Ecosystem::Go => variants
            .iter()
            .any(|v| m == v.as_str() || m.starts_with(&format!("{v}/"))),
        Ecosystem::Maven | Ecosystem::Gradle => variants.iter().any(|v| {
            m == v.as_str()
                || m.starts_with(&format!("{v}."))
                || m.starts_with(&format!("{v}/"))
        }),
        Ecosystem::Composer => variants
            .iter()
            .any(|v| m.eq_ignore_ascii_case(v) || m.to_ascii_lowercase().starts_with(&format!("{}\\", v.to_ascii_lowercase()))),
        Ecosystem::Nuget => variants
            .iter()
            .any(|v| m.eq_ignore_ascii_case(v) || m.to_ascii_lowercase().starts_with(&format!("{}.", v.to_ascii_lowercase()))),
    }
}

/// Usage evidence for every declared package, from adapter import facts.
pub fn usage(table: &DependencyTable, imports: &BTreeMap<String, Vec<String>>) -> Vec<PackageUsage> {
    let mut out = Vec::new();
    for d in &table.deps {
        let variants = match d.ecosystem {
            Ecosystem::Pypi => pypi_variants(&d.name),
            Ecosystem::Npm => npm_variants(&d.name),
            Ecosystem::Cargo => cargo_variants(&d.name),
            Ecosystem::Go => go_variants(&d.name),
            Ecosystem::Maven | Ecosystem::Gradle => jvm_variants(&d.name),
            Ecosystem::Composer => composer_variants(&d.name),
            Ecosystem::Nuget => nuget_variants(&d.name),
        };
        let mut imported_by: Vec<String> = Vec::new();
        for (file, mods) in imports {
            if mods.iter().any(|m| matches_import(d.ecosystem, &variants, m)) {
                imported_by.push(file.clone());
            }
        }
        imported_by.sort();
        // Honest scoping: ImportFact coverage is adapter-driven. Today only
        // the Python adapter records import facts (FR-3: no cross-language
        // guessing), so other ecosystems carry a named-gap note instead of
        // a fabricated zero.
        let (resolved, note) = if !imported_by.is_empty() {
            (
                true,
                "usage evidence from adapter-recorded imports".to_string(),
            )
        } else if matches!(
            d.ecosystem,
            Ecosystem::Pypi | Ecosystem::Npm | Ecosystem::Cargo | Ecosystem::Go | Ecosystem::Maven | Ecosystem::Gradle | Ecosystem::Composer | Ecosystem::Nuget
        ) {
            (
                false,
                "no import matched this name — entry-point or indirect usage is invisible to static analysis; NOT proof of being unused".to_string(),
            )
        } else {
            (
                false,
                "no import facts exist for this adapter yet — usage here is unevidenced, not zero".to_string(),
            )
        };
        out.push(PackageUsage {
            name: d.name.clone(),
            ecosystem: d.ecosystem,
            imported_by,
            resolved,
            note,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Upgrade what-if
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Compatibility {
    /// The ONLY value this engine will ever report: static analysis cannot
    /// verify that an available version is compatible with the application.
    Unknown,
}

/// What an upgrade/replacement of `name` to `to` would touch.
#[derive(Debug, Clone, Serialize)]
pub struct UpgradeImpact {
    pub name: String,
    pub to: String,
    /// Files with import-backed usage of this package (may be empty — see
    /// the usage note; an empty list is NOT proof of no consumers).
    pub consumers: Vec<String>,
    pub compatibility: Compatibility,
    /// Statements the view must keep visible (separated concerns).
    pub statements: Vec<String>,
}

/// Upgrade/replace what-if for a declared package. Returns None for a name
/// that is not declared in the scanned manifests (nothing invented).
pub fn upgrade_what_if(
    table: &DependencyTable,
    imports: &BTreeMap<String, Vec<String>>,
    name: &str,
    to: &str,
) -> Option<UpgradeImpact> {
    let dep = table
        .deps
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))?;
    let usage = usage(table, imports)
        .into_iter()
        .find(|u| u.name == dep.name);
    let consumers = usage.map(|u| u.imported_by).unwrap_or_default();
    let mut statements = vec![
        format!(
            "declared in {} as '{}' (requirement as written: '{}')",
            dep.source, dep.scope, dep.version_req
        ),
        "compatibility is UNKNOWN: static analysis cannot verify that the new version works with this application — an available fixed version is never presented as automatically compatible".to_string(),
    ];
    if consumers.is_empty() {
        statements.push(
            "no import-backed consumers found (see the usage note — this is not proof the package is unused)"
                .to_string(),
        );
    }
    Some(UpgradeImpact {
        name: dep.name.clone(),
        to: to.to_string(),
        consumers,
        compatibility: Compatibility::Unknown,
        statements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_and_package_json_parse() {
        let mut t = DependencyTable::default();
        read_requirements(
            "requirements.txt",
            "# comment\nrequests==2.31.0\nflask>=3.0\n-r other.txt\n",
            &mut t,
        );
        read_package_json(
            "package.json",
            r#"{"license":"MIT","dependencies":{"lodash":"^4.0.0"},"devDependencies":{"jest":"^29"}}"#,
            &mut t,
        )
        .unwrap();
        assert_eq!(t.deps.len(), 4);
        let req = t.deps.iter().find(|d| d.name == "requests").unwrap();
        assert_eq!(req.version_req, "==2.31.0");
        let lodash = t.deps.iter().find(|d| d.name == "lodash").unwrap();
        assert_eq!(lodash.licence.as_deref(), Some("MIT"));
        assert_eq!(lodash.scope, "runtime");
        let jest = t.deps.iter().find(|d| d.name == "jest").unwrap();
        assert_eq!(jest.scope, "dev");
    }

    #[test]
    fn cargo_and_pyproject_parse_with_licence() {
        let mut t = DependencyTable::default();
        read_cargo_toml(
            "Cargo.toml",
            "[package]\nname=\"x\"\nlicense = \"MIT OR Apache-2.0\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nclap = \"4\"\n",
            &mut t,
        );
        read_pyproject(
            "pyproject.toml",
            "[project]\nname = \"x\"\nlicense = {text = \"Apache-2.0\"}\ndependencies = [\n  \"requests==2.31.0\",\n  \"pydantic>=2\",\n]\n",
            &mut t,
        );
        let serde = t.deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde.version_req, "1");
        assert_eq!(serde.licence.as_deref(), Some("MIT OR Apache-2.0"));
        let req = t.deps.iter().find(|d| d.name == "requests").unwrap();
        assert_eq!(req.licence.as_deref(), Some("Apache-2.0"));
        assert_eq!(t.deps.iter().find(|d| d.name == "pydantic").unwrap().version_req, ">=2");
    }

    #[test]
    fn go_mod_parse() {
        let mut t = DependencyTable::default();
        read_go_mod(
            "go.mod",
            "module example.com/m\n\ngo 1.21\n\nrequire (\n\tgithub.com/pkg/errors v0.9.1\n\tgolang.org/x/text v0.14.0 // indirect\n)\n",
            &mut t,
        );
        assert_eq!(t.deps.len(), 2);
        assert_eq!(t.deps[0].name, "github.com/pkg/errors");
        assert_eq!(t.deps[0].version_req, "v0.9.1");
    }

    #[test]
    fn usage_matching_and_upgrade_impact() {
        let mut t = DependencyTable::default();
        read_requirements("requirements.txt", "requests==2.31\nunusedpkg>=1\n", &mut t);
        read_cargo_toml("Cargo.toml", "[dependencies]\nserde = \"1\"\n", &mut t);
        let mut imports = BTreeMap::new();
        imports.insert(
            "app.py".to_string(),
            vec!["requests".to_string(), "os".to_string()],
        );
        imports.insert("lib.py".to_string(), vec!["requests.adapters".to_string()]);
        let u = usage(&t, &imports);
        let r = u.iter().find(|x| x.name == "requests").unwrap();
        assert!(r.resolved);
        assert_eq!(r.imported_by, vec!["app.py".to_string(), "lib.py".to_string()]);
        let unused = u.iter().find(|x| x.name == "unusedpkg").unwrap();
        assert!(!unused.resolved);
        assert!(unused.note.contains("NOT proof of being unused"));
        // every adapter records imports now, so a cargo package with no
        // matching import gets the honest "no import matched" note — never
        // a fabricated zero, never a fake "no facts exist" claim
        let s = u.iter().find(|x| x.name == "serde").unwrap();
        assert!(s.note.contains("NOT proof of being unused"));
        let wi = upgrade_what_if(&t, &imports, "requests", "2.32.0").unwrap();
        assert_eq!(wi.consumers.len(), 2);
        assert_eq!(wi.compatibility, Compatibility::Unknown);
        assert!(wi.statements.iter().any(|x| x.contains("UNKNOWN")));
        assert!(upgrade_what_if(&t, &imports, "not-declared", "1").is_none());
    }
    #[test]
    fn licence_report_names_unknowns_and_states_gaps() {
        let mut t = DependencyTable::default();
        read_cargo_toml("Cargo.toml", "[package]\nlicense=\"MIT\"\n\n[dependencies]\nserde = \"1\"\n", &mut t);
        read_requirements("requirements.txt", "mysterypkg==1.0\n", &mut t);
        let r = licence_report(&t);
        assert_eq!(r.entries.len(), 2);
        let serde = r.entries.iter().find(|e| e.name == "serde").unwrap();
        assert_eq!(serde.licence.as_deref(), Some("MIT"));
        assert_eq!(
            r.unknown_licence,
            vec!["mysterypkg (pypi, declared in requirements.txt)".to_string()]
        );
        assert!(r.advisories.contains("NOT scanned"));
        assert!(r.provenance.contains("not verified"));
    }
}

// ---------------------------------------------------------------------------
// Licence obligations (US-098 — the licence slice of the supply-chain story)
// ---------------------------------------------------------------------------

/// One package's licence obligation record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LicenceEntry {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub version_req: String,
    /// As written in the manifest (never resolved against SPDX).
    pub licence: Option<String>,
    pub source: String,
}

/// The licence-obligation report (US-098).
#[derive(Debug, Clone, Serialize)]
pub struct LicenceReport {
    pub entries: Vec<LicenceEntry>,
    /// Declared packages whose manifest carries NO licence field — named,
    /// never guessed (the obligation is "unavailable provenance is stated").
    pub unknown_licence: Vec<String>,
    /// Honesty statement: advisory/vulnerability feeds are NOT scanned
    /// offline; a fabricated feed would be worse than none.
    pub advisories: &'static str,
    /// Honesty statement: licence strings are read as written and are not
    /// verified against licence registries.
    pub provenance: &'static str,
}

/// Licence obligations for every declared package.
pub fn licence_report(table: &DependencyTable) -> LicenceReport {
    let mut entries = Vec::new();
    let mut unknown = Vec::new();
    for d in &table.deps {
        match &d.licence {
            Some(l) => entries.push(LicenceEntry {
                name: d.name.clone(),
                ecosystem: d.ecosystem,
                version_req: d.version_req.clone(),
                licence: Some(l.clone()),
                source: d.source.clone(),
            }),
            None => {
                unknown.push(format!("{} ({}, declared in {})", d.name, d.ecosystem.as_str(), d.source));
                entries.push(LicenceEntry {
                    name: d.name.clone(),
                    ecosystem: d.ecosystem,
                    version_req: d.version_req.clone(),
                    licence: None,
                    source: d.source.clone(),
                });
            }
        }
    }
    LicenceReport {
        entries,
        unknown_licence: unknown,
        advisories: "advisory/vulnerability feeds are NOT scanned: offline-first refuses to fabricate feed data; supply-chain advisories stay a named gap (US-098)",
        provenance: "licence strings are read from manifests as written; they are not verified against licence registries or resolved transitively",
    }
}
