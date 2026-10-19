//! Manifest detection — "which stack is this?" from the build files the
//! project itself ships. Plain reads, no interpretation beyond the words.
//! Every claim names the file that grounded it.

use crate::types::BuildInfo;
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
}

/// what the build tool files at the project root say
pub fn detect(root: &Path) -> Detect {
    let mut d = Detect::default();

    let mut read = |name: &str| -> Option<String> {
        let p = root.join(name);
        match fs::read_to_string(&p) {
            Ok(s) => {
                d.manifests_read.push(name.to_string());
                Some(s)
            }
            Err(_) => None,
        }
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