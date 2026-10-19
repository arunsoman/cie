//! TASK-022 validation sketch — cross-language FR-3 semantics guard over
//! all fourteen adapters (adapted to the composition root per TRACEABILITY
//! §2: the registry + all adapter crates assemble here because Rust
//! forbids trait-crate → adapter-crate cycles). SQL and Shell have no
//! authorize-shaped fixture yet, so they fall back to the guard's
//! no-fixture admission path (FR-4: recorded, not skipped).

use cie_adapters::{CandidateRule, LanguageRegistry, SemanticsGuard};
use cie_semantic::LangId;
use std::collections::BTreeMap;

fn v1_registry() -> LanguageRegistry {
    cie_ingest::default_registry()
}

fn fixture_repo_12_files() -> BTreeMap<String, String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../cie-adapters/tests/fixtures/guard");
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir).expect("fixture repo present") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let src = std::fs::read_to_string(&path).unwrap();
            out.insert(name, src);
        }
    }
    out
}

#[test]
fn task_022_cross_language_semantics_guard() {
    let registry = v1_registry();
    assert_eq!(
        registry.adapters().len(),
        14,
        "FR-17: the guard runs over all fourteen adapters"
    );
    let repo = fixture_repo_12_files();
    assert_eq!(
        repo.len(),
        12,
        "fixture dir carries one authorize-shaped file per language for the original ten \
         plus Kotlin and Lua (v1.2.2); SQL and Shell have no fixture yet and fall back to \
         the no-fixture admission path below"
    );

    // Candidate rule (Houdini-style, cf. §7): "a call to authorize
    // establishes authorization".
    let rule = CandidateRule {
        call_target: "authorize".to_string(),
        effect: "authorization established".to_string(),
    };
    let admissions = SemanticsGuard::evaluate(&rule, &registry, &repo);
    assert_eq!(
        admissions.len(),
        14,
        "matrix-driven: one admission per registered adapter"
    );

    let fired: Vec<LangId> = admissions.iter().filter(|a| a.fired).map(|a| a.lang).collect();
    assert!(
        !fired.contains(&LangId::Go),
        "FR-3: Go returns error values — a bare call does not establish success"
    );
    assert!(
        !fired.contains(&LangId::Rust),
        "FR-3: Result/? semantics vouch against firing"
    );
    assert!(
        !fired.contains(&LangId::C),
        "FR-3: C error-code conventions (-1/NULL/errno) vouch against firing"
    );
    assert!(
        fired.contains(&LangId::Python),
        "Python raise semantics + resolved call admit the rule"
    );
    for a in admissions.iter().filter(|a| !a.fired) {
        assert!(
            !a.evidence.assumptions.is_empty() || !a.evidence.unresolved.is_empty(),
            "FR-4: every refusal is recorded with evidence ({:?})",
            a.lang
        );
    }
}
