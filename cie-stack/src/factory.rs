//! The factory this whole crate is organised around.
//!
//! You do NOT hand-code which parser runs. Detection reads the manifests,
//! then you ask the factory for the parser that matches, and call it
//! through one uniform interface. Parsers that don't exist yet return an
//! honest stub that says exactly that.
//!
//!   let parser = factory::pick(&detect);   // which stack's parser?
//!   parser.parse(&files)                    // → routes + mains + deep facts
//!
//! The Rust ENGINE does the parsing — tree-sitter, no LLM, no network,
//! nothing evaluated or executed. Files are read as written.

use crate::types::StackReport;
use std::collections::BTreeMap;
use std::sync::Arc;

/// per-file source the caller collected (path → text)
pub type Sources = BTreeMap<String, String>;

/// the one interface every stack parser answers to
pub trait StackParser: Send + Sync {
    /// stable id: "spring-jvm" | "rust-stub" | "node-stub" ...
    fn id(&self) -> &'static str;
    /// what this parser covers, in plain words
    fn describes(&self) -> String;
    /// run it: fill routes + mains; JVM parsers fill `jvm` too
    fn parse(&self, files: &Sources, report: &mut StackReport);
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

// ---- the real one, so far ----
pub struct SpringParser;

impl crate::factory::StackParser for SpringParser {
    fn id(&self) -> &'static str {
        "spring-jvm"
    }
    fn describes(&self) -> String {
        "Spring over the JVM (java + kotlin read as one family)".to_string()
    }
    fn parse(&self, files: &Sources, report: &mut StackReport) {
        let mut classes = Vec::new();
        let mut scanned = 0u32;
        let mut java = 0u32;
        let mut kotlin = 0u32;
        for (rel, src) in files {
            if skip_source(rel) {
                continue;
            }
            let lang: &'static str = if rel.ends_with(".java") {
                java += 1;
                "java"
            } else if rel.ends_with(".kt") {
                kotlin += 1;
                "kotlin"
            } else {
                continue;
            };
            scanned += 1;
            classes.push(crate::spring::parse_file(lang, rel, src));
        }
        let all: Vec<crate::spring::ClassNode> = classes.into_iter().flatten().collect();
        let asm = crate::spring::assemble(std::mem::take(&mut all.clone()));
        let _ = all;
        report.jvm = Some(asm.jvm);
        report.routes = asm.routes;
        report.mains = asm.mains;
        let _ = (scanned, java, kotlin);
    }
}

fn skip_source(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    p.contains("/src/test/") || p.starts_with("src/test/") || p.starts_with("test/") || p.contains("/test/")
}

// ---- the factory ----
/// ask which parser matches what detection found — never hand-routed
pub fn pick(d: &crate::detect::Detect) -> Arc<dyn StackParser> {
    if d.spring && d.is_jvm() {
        return Arc::new(SpringParser);
    }
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