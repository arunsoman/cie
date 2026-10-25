//! Manifest detection — "which stack is this?" from the build files the
//! project itself ships. Plain reads, no interpretation beyond the words.
//! Every claim names the file that grounded it.

use crate::types::BuildInfo;
use crate::CapturedInputs;
use regex::Regex;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Detect {
    pub build: BuildInfo,
    pub spring: bool,
    pub spring_version: Option<String>,
    /// spring starter modules named in the build file (starter-web, starter-data-jpa…)
    pub spring_starters: Vec<String>,
    pub kotlin_plugin: bool,
    pub java_files: u32,
    pub kotlin_files: u32,
    pub python_files: u32,
    pub rust_files: u32,
    pub js_files: u32,
    pub go_files: u32,
    pub manifests_read: Vec<String>,
    /// every detected APPLICATION in the repo (root included when it has a manifest)
    pub apps: Vec<AppDet>,
}

/// one detected application: the dir its manifest lives in, the build, and
/// the HTTP framework the manifest NAMES (as written, honest "unknown")
#[derive(Debug, Clone)]
pub struct AppDet {
    pub dir: String,
    pub name: String,
    pub build: String,
    pub framework: String,
    pub evidence: String,
    /// every manifest evidence record over this root: (build tool, manifest path)
    pub manifests: Vec<(String, String)>,
    /// the app's dir contains java/kotlin sources (spring must pair with one)
    pub holds_jvm: bool,
}

/// walk the tree and collect every application manifest (bounded)
fn find_apps(inputs: &CapturedInputs) -> Vec<AppDet> {
    let root = inputs.root;
    let mut out: Vec<AppDet> = vec![];
    let mut stack = vec![(root.to_path_buf(), String::new())];
    let mut visited = 0usize;
    const MAX_DIRS: usize = 8000;
    while let Some((dir, rel_dir)) = stack.pop() {
        visited += 1;
        if visited > MAX_DIRS {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !IGNORED.contains(&name.as_str()) {
                    let child_dir = if rel_dir.is_empty() { name.clone() } else { format!("{rel_dir}/{name}") };
                    stack.push((path, child_dir));
                }
                continue;
            }
            let rel_manifest = if rel_dir.is_empty() { name.clone() } else { format!("{rel_dir}/{name}") };
            let app = app_from_manifest(&name, &rel_manifest, inputs);
            if let Some(app) = app {
                out.push(app);
            }
        }
    }
    out.sort_by(|a, b| (&a.dir, &a.evidence).cmp(&(&b.dir, &b.evidence)));
    out
}

/// classify ONE manifest file into an app descriptor (or None). The file's
/// text comes from the capture when the walk already read it, disk otherwise.
fn app_from_manifest(base: &str, rel_manifest: &str, inputs: &CapturedInputs) -> Option<AppDet> {
    let dir = std::path::Path::new(rel_manifest)
        .parent()
        .filter(|p| p.as_os_str() != ".")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    match base {
        "pom.xml" | "build.gradle" | "build.gradle.kts" => {
            let text = inputs.read_input(rel_manifest)?;
            let is_spring = text.contains("spring-boot-starter") || text.contains("springframework") || text.contains("spring-boot");
            let build = if base == "pom.xml" { "maven" } else { "gradle" }.to_string();
            let name = crate::detect::manifest_name(base, rel_manifest, &text);
            Some(AppDet {
                dir: dir.clone(),
                name,
                build,
                framework: if is_spring { "spring".into() } else { "jvm-unknown".into() },
                evidence: rel_manifest.to_string(),
                manifests: vec![],
                holds_jvm: dir_has_jvm(&dir),
            })
        }
        "Cargo.toml" => {
            let text = inputs.read_input(rel_manifest)?;
            let framework = if text.contains("axum") {
                "axum"
            } else if text.contains("actix-web") {
                "actix-web"
            } else if text.contains("rocket") {
                "rocket"
            } else {
                "rust-unknown"
            };
            Some(AppDet {
                dir: dir.clone(),
                name: crate::detect::manifest_name(base, rel_manifest, &text),
                build: "cargo".into(),
                framework: framework.into(),
                evidence: rel_manifest.to_string(),
                manifests: vec![],
                holds_jvm: false,
            })
        }
        "package.json" => {
            // ignore bundled/fixture package files
            if rel_manifest.contains("node_modules") || rel_manifest.contains("dist/") {
                return None;
            }
            let text = inputs.read_input(rel_manifest)?;
            let framework = if text.contains(r#""express""#) {
                "express"
            } else if text.contains(r#""fastify""#) {
                "fastify"
            } else {
                "node-unknown"
            };
            Some(AppDet {
                dir: dir.clone(),
                name: crate::detect::manifest_name(base, rel_manifest, &text),
                build: "npm".into(),
                framework: framework.into(),
                evidence: rel_manifest.to_string(),
                manifests: vec![],
                holds_jvm: false,
            })
        }
        "requirements.txt" | "pyproject.toml" => {
            let text = inputs.read_input(rel_manifest)?;
            let framework = if text.contains("fastapi") {
                "fastapi"
            } else if text.contains("flask") {
                "flask"
            } else {
                "python-unknown"
            };
            Some(AppDet {
                dir: dir.clone(),
                name: crate::detect::manifest_name(base, rel_manifest, &text),
                build: "python".into(),
                framework: framework.into(),
                evidence: rel_manifest.to_string(),
                manifests: vec![],
                holds_jvm: false,
            })
        }
        "go.mod" => {
            let text = inputs.read_input(rel_manifest)?;
            let framework = if text.contains("gin-gonic") {
                "gin"
            } else if text.contains("echo") {
                "echo"
            } else {
                "net-http"
            };
            Some(AppDet {
                dir: dir.clone(),
                name: crate::detect::manifest_name(base, rel_manifest, &text),
                build: "go".into(),
                framework: framework.into(),
                evidence: rel_manifest.to_string(),
                manifests: vec![],
                holds_jvm: false,
            })
        }
        _ => None,
    }
}

/// the app name: artifact/name in the manifest, else the dir's own name
fn manifest_name(_base: &str, rel_manifest: &str, text: &str) -> String {
    let art = Regex::new(r#"^\s*name\s*=\s*"([^"]+)""#).ok();
    if rel_manifest.ends_with("Cargo.toml") || rel_manifest.ends_with("pyproject.toml") {
        if let Some(re) = art {
            if let Some(m) = re.captures(text) {
                return m[1].to_string();
            }
        }
    }
    let pkg = Regex::new(r#""name"\s*:\s*"([^"]+)""#).ok();
    if rel_manifest.ends_with("package.json") {
        if let Some(re) = pkg {
            if let Some(m) = re.captures(text) {
                return m[1].to_string();
            }
        }
    }
    let artifact = Regex::new(r"<artifactId>([^<]+)</artifactId>").ok();
    if rel_manifest.ends_with("pom.xml") {
        if let Some(re) = artifact {
            if let Some(m) = re.captures(text) {
                return m[1].to_string();
            }
        }
    }
    std::path::Path::new(rel_manifest)
        .parent()
        .map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
        .unwrap_or_default()
}

/// does this dir hold java/kotlin files? (cheap bounded scan, used by find_apps)
fn dir_has_jvm(dir: &str) -> bool {
    dir.is_empty() // root manifests can't prove it — callers gate spring on is_jvm()
}

/// Behavioural-evidence redesign (identity layer): manifests are EVIDENCE,
/// never identity. Several manifests over the SAME dir (pom.xml +
/// build.gradle at one root) are one physical application — merge them into
/// ONE AppDet whose members ride along for the evidence list. Deterministic:
/// input is sorted by (dir, evidence).
fn merge_same_root(apps: Vec<AppDet>) -> Vec<AppDet> {
    let mut out: Vec<AppDet> = vec![];
    for a in apps {
        match out.last_mut() {
            Some(last) if last.dir == a.dir => {
                // same root: fold into the first record (deterministic: the
                // lexicographically-first manifest names the app/build).
                if last.name.is_empty() && !a.name.is_empty() {
                    last.name = a.name.clone();
                }
                if last.framework != "spring" && a.framework == "spring" {
                    last.framework = a.framework.clone();
                }
                last.manifests.push((a.build.clone(), a.evidence.clone()));
                last.holds_jvm = last.holds_jvm || a.holds_jvm;
            }
            _ => {
                let mut first = a;
                first.manifests = vec![(first.build.clone(), first.evidence.clone())];
                out.push(first);
            }
        }
    }
    out
}

/// what the build files say — over a capture when given, disk otherwise
pub(crate) fn detect_with(inputs: &CapturedInputs) -> Detect {
    let root = inputs.root;
    let mut d = Detect::default();
    d.apps = merge_same_root(find_apps(inputs));
    // root-manifest spring gates on the REAL jvm files (is_jvm checks counts)
    for app in d.apps.iter_mut() {
        if app.framework == "spring" && !app.dir.is_empty() {
            // per-app jvm check: any java/kt under that dir?
            app.holds_jvm = subdir_jvm(root, &app.dir);
        }
    }
    d.apps.retain(|app| {
        // spring without jvm sources in the app dir is mis-detected — honest stub instead
        !(app.framework == "spring" && !app.holds_jvm)
    });
    d.apps.iter_mut().for_each(|app| {
        if app.framework == "spring" && !app.holds_jvm {
            app.framework = "jvm-unknown".into();
        }
    });
    let _ = dir_has_jvm("");

    let mut read = |name: &str| -> Option<String> {
        inputs.read_input(name).map(|s| {
            d.manifests_read.push(name.to_string());
            s
        })
    };

    let module_re = Regex::new(r"<module>([^<]+)</module>").unwrap();
    let artifact_re = Regex::new(r"<artifactId>([^<]+)</artifactId>").unwrap();
    let parent_v_re = Regex::new(r"spring-boot-starter-parent[\s\S]{0,400}?<version>([\d.]+)</version>").unwrap();
    let dep_v_re =
        Regex::new(r"<groupId>org\.springframework\.boot</groupId>\s*<artifactId>[\w-]+</artifactId>\s*<version>([\d.]+)</version>").unwrap();

    if let Some(pom) = read("pom.xml") {
        d.build.tool = "maven".to_string();
        for m in module_re.captures_iter(&pom) {
            d.build.modules.push(m[1].to_string());
        }
        if let Some(a) = artifact_re.captures(&pom) {
            d.build.artifact_name = a[1].to_string();
        }
        for m in artifact_re.captures_iter(&pom) {
            let a = m[1].to_lowercase();
            if a.starts_with("spring-boot") || a.contains("spring") {
                d.spring_starters.push(m[1].to_string());
                if a.starts_with("spring-boot-starter") {
                    d.spring = true;
                }
            }
        }
        if let Some(v) = parent_v_re.captures(&pom) {
            d.spring_version = Some(v[1].to_string());
        }
        if let Some(v) = dep_v_re.captures(&pom) {
            if d.spring_version.is_none() {
                d.spring_version = Some(v[1].to_string());
            }
        }
    }

    let include_re = Regex::new(r#"include\("([^"]+)"\)"#).unwrap();
    let gradle_spring_re = Regex::new(r#""([^"]*spring[^"]*)""#).unwrap();
    let gradle_ver_re = Regex::new(r#"org\.springframework\.boot"\)\s*version\s+"([\d.]+)""#).unwrap();

    if let Some(gradle) = read("build.gradle").or_else(|| read("build.gradle.kts")) {
        if d.build.tool == "unknown" {
            d.build.tool = "gradle".to_string();
        }
        for m in include_re.captures_iter(&gradle) {
            d.build.modules.push(m[1].to_string());
        }
        for m in gradle_spring_re.captures_iter(&gradle) {
            let a = m[1].to_lowercase();
            if a.contains("springframework") || a.contains("spring-boot") {
                d.spring = true;
                d.spring_starters.push(m[1].to_string());
            }
        }
        if let Some(v) = gradle_ver_re.captures(&gradle) {
            if d.spring_version.is_none() {
                d.spring_version = Some(v[1].to_string());
            }
        }
        if gradle.contains("org.jetbrains.kotlin") || gradle.contains("kotlin(\"") || gradle.contains("kotlin(\"jvm\")") {
            d.kotlin_plugin = true;
        }
    }

    if let Some(settings) = read("settings.gradle").or_else(|| read("settings.gradle.kts")) {
        for m in include_re.captures_iter(&settings) {
            d.build.modules.push(m[1].to_string());
        }
        if d.build.tool == "unknown" {
            d.build.tool = "gradle".to_string();
        }
    }

    let cargo_name_re = Regex::new(r#"\[package\][\s\S]{0,800}?\nname\s*=\s*"([^"]+)""#).unwrap();
    let ws_member_re = Regex::new(r#""([^"]+)""#).unwrap();
    if let Some(cargo) = read("Cargo.toml") {
        d.build.tool = "cargo".to_string();
        if cargo.contains("[workspace]") {
            // ONLY the members array — never every quoted string in the file
            if let Some(i) = cargo.find("members") {
                if let Some(open) = cargo[i..].find('[') {
                    if let Some(close) = cargo[i + open..].find(']') {
                        let block = &cargo[i + open..i + open + close];
                        for m in ws_member_re.captures_iter(block) {
                            d.build.modules.push(m[1].to_string());
                        }
                    }
                }
            }
        }
        if let Some(a) = cargo_name_re.captures(&cargo) {
            d.build.artifact_name = a[1].to_string();
        }
    }

    let pkg_name_re = Regex::new(r#""name"\s*:\s*"([^"]+)""#).unwrap();
    if let Some(pkg) = read("package.json") {
        if d.build.tool == "unknown" {
            d.build.tool = "npm".to_string();
        }
        if let Some(name) = pkg_name_re.captures(&pkg) {
            d.build.artifact_name = name[1].to_string();
        }
    }
    if read("go.mod").is_some() && d.build.tool == "unknown" {
        d.build.tool = "go".to_string();
    }
    if (read("requirements.txt").is_some() || read("pyproject.toml").is_some()) && d.build.tool == "unknown" {
        d.build.tool = "python".to_string();
    }

    // java/kotlin presence from the file walk (cheap extension scan)
    walk_count(root, &mut d);
    // kotlin sources with java ones = one runtime
    if d.kotlin_files > 0 && d.java_files > 0 {
        d.kotlin_plugin = true;
    }
    d
}

fn walk_count(root: &Path, d: &mut Detect) {
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
                if !IGNORED.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            let ext = name.rsplit('.').next().unwrap_or("");
            match ext {
                "java" => d.java_files += 1,
                "kt" | "kts" => d.kotlin_files += 1,
                "py" => d.python_files += 1,
                "rs" => d.rust_files += 1,
                "js" | "jsx" | "ts" | "tsx" => d.js_files += 1,
                "go" => d.go_files += 1,
                _ => {}
            }
        }
    }
}

const IGNORED: &[&str] = &[
    "node_modules", "target", "vendor", ".git", ".venv", "venv", "__pycache__", ".gradle", ".idea", ".vscode", "dist", "build",
    "out", ".cie",
    // the project's own test material isn't the code the user runs —
    // counting fixture files as real code would lie
    "tests", "test", "fixtures", "testdata", "spec",
];

impl Default for Detect {
    fn default() -> Self {
        Self {
            build: crate::types::BuildInfo {
                tool: "unknown".into(),
                modules: vec![],
                artifact_name: String::new(),
            },
            spring: false,
            spring_version: None,
            spring_starters: vec![],
            kotlin_plugin: false,
            java_files: 0,
            kotlin_files: 0,
            python_files: 0,
            rust_files: 0,
            js_files: 0,
            go_files: 0,
            manifests_read: vec![],
            apps: vec![],
        }
    }
}

impl Detect {
    pub fn languages(&self) -> Vec<crate::types::StackLang> {
        let mut out = Vec::new();
        if self.java_files > 0 && self.kotlin_files > 0 {
            out.push(crate::types::StackLang {
                family: "jvm-mixed".into(),
                files: self.java_files + self.kotlin_files,
                shared_runtime: true,
            });
        } else if self.java_files > 0 {
            out.push(crate::types::StackLang {
                family: "java".into(),
                files: self.java_files,
                shared_runtime: false,
            });
        } else if self.kotlin_files > 0 {
            out.push(crate::types::StackLang {
                family: "kotlin".into(),
                files: self.kotlin_files,
                shared_runtime: false,
            });
        }
        // non-jvm stacks get their counts too — plain facts the stub
        // parsers can still be honest with
        for (family, files) in [
            ("rust", self.rust_files),
            ("javascript", self.js_files),
            ("python", self.python_files),
            ("go", self.go_files),
        ] {
            if files > 0 {
                out.push(crate::types::StackLang {
                    family: family.into(),
                    files,
                    shared_runtime: false,
                });
            }
        }
        out
    }

    pub fn is_jvm(&self) -> bool {
        self.java_files > 0 || self.kotlin_files > 0
    }
}

/// one plain sentence summarising the build, for the chips row
pub fn build_raw(d: &Detect) -> String {
    let mut s = format!("{} project", d.build.tool);
    if let Some(v) = &d.spring_version {
        s += &format!(" · Spring Boot {v}");
    }
    if !d.build.modules.is_empty() {
        s += &format!(" · {} module(s)", d.build.modules.len());
    }
    if d.kotlin_files > 0 && d.java_files > 0 {
        s += &format!(" · mixed java+kotlin ({} + {})", d.java_files, d.kotlin_files);
    }
    s
}
/// any java/kt file under one subdir (bounded, stops at the first hit)
fn subdir_jvm(root: &Path, dir: &str) -> bool {
    let mut stack = vec![root.join(dir)];
    let mut visited = 0usize;
    while let Some(d) = stack.pop() {
        visited += 1;
        if visited > 4000 {
            break;
        }
        let entries = match fs::read_dir(&d) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !IGNORED.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if name.ends_with(".java") || name.ends_with(".kt") {
                return true;
            }
        }
    }
    false
}
