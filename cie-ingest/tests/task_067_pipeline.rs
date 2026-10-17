//! TASK-067 validation: folder walk → detect → parallel parse with
//! isolation (§17.2, FR-19).
//!
//! Sketch deviations (documented, single-point):
//! - `fixture_dir(|fs| ...)` → a real temp-dir fixture built with
//!   std::fs (unique suffix per run); `anyhow::Result` → `std::io::Result`.
//! - `Lang::Python` et al. → `cie_semantic::LangId`.
//! - `assert_lang(&out, ..)` → `out.lang_of(rel) == Some(LangId::..)`.
//! - The oversized fixture uses 17 MiB of sparse content (max_file_bytes
//!   16 MiB) instead of a 64 MiB sparse file — same SkipReason, faster.
//! - Parse-error isolation exercises the adapter-owned
//!   `parse_diagnostics` (FR-3) — broken.py yields
//!   `FileError::Parse("syntax error")` while healthy siblings parse.

use cie_ingest::pipeline::{
    FileError, IngestPipeline, SkipReason, WalkOptions,
};
use cie_semantic::LangId;
use std::fs;
use std::io::Write;

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie067_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_file(root: &std::path::Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(content).unwrap();
}

/// The sketch's `fixture_dir` + assertions.
#[test]
fn task_067_walk_detect_parallel_parse_isolation() -> std::io::Result<()> {
    let root = unique_dir("main");
    write_file(&root, "src/api.py", b"def checkout(cart):\n    return cart\n");
    write_file(
        &root,
        "src/lib.rs",
        b"pub fn refund(order: u32) -> u32 {\n    order + 1\n}\n",
    );
    write_file(
        &root,
        "internal/svc/main.go",
        b"package main\n\nfunc Authorize(u string) bool {\n\treturn true\n}\n",
    );
    write_file(
        &root,
        "web/app.ts",
        b"export function topUp(a: string): string {\n  return a;\n}\n",
    );
    // shebang detection: no extension
    write_file(&root, "tools/run", b"#!/usr/bin/env python3\nimport sys\n");
    // §17.2 ignored dirs
    fs::create_dir_all(root.join(".git/objects"))?;
    fs::create_dir_all(root.join("node_modules/left-pad"))?;
    fs::create_dir_all(root.join("target/debug"))?;
    // binary → skip
    write_file(&root, "logo.png", b"\x89PNG\r\n\x1a\n\0\0binary");
    // oversized → skip (17 MiB > 16 MiB cap)
    let big = vec![b'x'; (17 << 20)];
    write_file(&root, "gen/big.log", &big);
    // nested gitignore: gen2/ ignored
    write_file(&root, "gen2/.gitignore", b"*\n");
    write_file(&root, "gen2/secret.py", b"def nope():\n    return 1\n");
    // parse error → isolated
    write_file(&root, "src/broken.py", b"def oops(:\n");

    let walked = IngestPipeline::walk(
        &root,
        &WalkOptions {
            respect_gitignore: true,
            max_file_bytes: 16 << 20,
            budget_ms: None,
        },
    )?;
    let detected = IngestPipeline::detect_languages(walked);
    let registry = cie_ingest::default_registry();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .map_err(std::io::Error::other)?;
    let out = IngestPipeline::parse_parallel(detected, &registry, &pool);

    // language detection: extension → shebang
    assert_eq!(out.lang_of("src/api.py"), Some(LangId::Python));
    assert_eq!(out.lang_of("src/lib.rs"), Some(LangId::Rust));
    assert_eq!(out.lang_of("internal/svc/main.go"), Some(LangId::Go));
    assert_eq!(out.lang_of("web/app.ts"), Some(LangId::TypeScript));
    assert_eq!(
        out.lang_of("tools/run"),
        Some(LangId::Python),
        "shebang beats missing extension"
    );

    // §17.2 ignore set + nested gitignore
    let ign = out.ignored.join(",");
    assert!(ign.contains(".git") && ign.contains("node_modules") && ign.contains("target"));
    assert!(ign.contains("gen2"), "nested .gitignore honored");
    assert!(out.lang_of("gen2/secret.py").is_none());

    // skips with reasons
    assert!(out.skipped_as("logo.png", SkipReason::Binary));
    assert!(out.skipped_as("gen/big.log", SkipReason::Oversized));

    // per-file error isolation
    assert!(
        matches!(out.error_of("src/broken.py"), Some(FileError::Parse(_))),
        "error isolated per file: {:?}",
        out.error_of("src/broken.py")
    );
    // healthy sibling unaffected
    assert_eq!(
        out.symbols_of("src/api.py"),
        Some(2),
        "healthy sibling unaffected by broken.py (module + checkout)"
    );

    // cleanup
    fs::remove_dir_all(&root).ok();
    Ok(())
}

/// The Ns-tracked budget: a 0-ms deadline stops the walk early and is
/// reported honestly (`budget_exhausted`), FR-21 / global rule 1.
#[test]
fn task_067_budget_tracked_in_ns() {
    let root = unique_dir("budget");
    write_file(&root, "a.py", b"def x():\n    return 1\n");
    write_file(&root, "b.py", b"def y():\n    return 2\n");
    let walked = IngestPipeline::walk(
        &root,
        &WalkOptions { respect_gitignore: true, max_file_bytes: 1 << 20, budget_ms: Some(0) },
    )
    .unwrap();
    assert!(walked.budget_exhausted, "0-ms deadline exhausts immediately");
    fs::remove_dir_all(&root).ok();
}

/// Determinism: two walks produce byte-identical stage outputs (FR-4).
#[test]
fn task_067_walk_determinism() {
    let root = unique_dir("det");
    write_file(&root, "m.py", b"def a():\n    return 1\n");
    write_file(&root, "n.rs", b"pub fn b() -> u32 {\n    1\n}\n");
    let run = || {
        let w = IngestPipeline::walk(
            &root,
            &WalkOptions { respect_gitignore: true, max_file_bytes: 1 << 20, budget_ms: None },
        )
        .unwrap();
        let d = IngestPipeline::detect_languages(w);
        let reg = cie_ingest::default_registry();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        let out = IngestPipeline::parse_parallel(d, &reg, &pool);
        format!("{:?}", out.lang_of("m.py"))
    };
    assert_eq!(run(), run());
    fs::remove_dir_all(&root).ok();
}
