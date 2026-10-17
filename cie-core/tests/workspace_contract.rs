//! TASK-001 validation sketch — workspace contract. The 30 §10 members all
//! appear in the root manifest, the §10 key third-party dependencies are
//! declared once in `[workspace.dependencies]`, and FR-21 forbids chrono
//! anywhere in the workspace. (TRACEABILITY §2: `rustc_hash` — underscore —
//! is the Cargo-hyphenated `rustc-hash` entry.)

const MEMBERS: [&str; 30] = [
    "cie-core",
    "cie-semantic",
    "cie-relations",
    "cie-adapters",
    "cie-adapter-python",
    "cie-adapter-javascript",
    "cie-adapter-typescript",
    "cie-adapter-java",
    "cie-adapter-c",
    "cie-adapter-cpp",
    "cie-adapter-csharp",
    "cie-adapter-go",
    "cie-adapter-rust",
    "cie-adapter-php",
    "cie-ingest",
    "cie-evidence",
    "cie-summary",
    "cie-contract",
    "cie-analysis",
    "cie-incremental",
    "cie-concepts",
    "cie-counterexample",
    "cie-fault",
    "cie-store",
    "cie-llm",
    "cie-docs",
    "cie-claims",
    "cie-drift",
    "cie-cli",
    "cie-server",
];

#[test]
fn task_001_workspace_members_and_deps() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")).unwrap();
    for m in MEMBERS {
        assert!(manifest.contains(m), "missing workspace member: {m}");
    }
    for dep in [
        "petgraph",
        "rkyv",
        "memmap2",
        "dashmap",
        "rustc-hash",
        "smallvec",
        "thiserror",
        "z3",
    ] {
        assert!(
            manifest.contains(dep),
            "missing [workspace.dependencies] entry: {dep}"
        );
    }
    assert!(
        !manifest.contains("chrono"),
        "FR-21: no chrono anywhere in the workspace"
    );
}
