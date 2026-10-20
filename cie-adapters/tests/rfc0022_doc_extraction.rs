//! RFC-0022 §6.3 — doc/comment extraction, shared by every LightLang
//! grammar (Go/Rust/TS/JS/Tsx) through one fix in `light_engine::
//! push_light_symbol`. Before this landed, `SymbolFact.doc` was `None`
//! for all five; this test proves it's now populated for real, and
//! stays `None` (never a guess) when nothing precedes the declaration.

use cie_adapters::light_engine::{extract_light, LightLang};

fn doc_of(facts: &cie_semantic::facts::LangFacts, name: &str) -> Option<String> {
    facts
        .symbols
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| s.doc.clone())
}

#[test]
fn go_doc_comment_extracted() {
    let src = "package main\n\n// Authorize checks the caller's permission.\n// Returns an error when denied.\nfunc Authorize(user string) error {\n\treturn nil\n}\n";
    let facts = extract_light("auth.go", src, LightLang::Go);
    let doc = doc_of(&facts, "Authorize").expect("doc comment found");
    assert!(doc.contains("Authorize checks"));
    assert!(doc.contains("Returns an error"));
}

#[test]
fn go_no_comment_is_none() {
    let src = "package main\n\nfunc Plain() {}\n";
    let facts = extract_light("plain.go", src, LightLang::Go);
    assert_eq!(doc_of(&facts, "Plain"), None);
}

#[test]
fn rust_triple_slash_doc_extracted() {
    let src = "/// Authorizes the caller.\n/// Never panics.\nfn authorize() {}\n";
    let facts = extract_light("auth.rs", src, LightLang::Rust);
    let doc = doc_of(&facts, "authorize").expect("doc comment found");
    assert!(doc.contains("Authorizes the caller"));
    assert!(doc.contains("Never panics"));
}

#[test]
fn rust_doc_survives_attribute_between_comment_and_fn() {
    let src = "/// A test helper.\n#[test]\nfn helper() {}\n";
    let facts = extract_light("auth.rs", src, LightLang::Rust);
    let doc = doc_of(&facts, "helper").expect("doc comment found despite #[test]");
    assert!(doc.contains("A test helper"));
}

#[test]
fn typescript_jsdoc_block_extracted() {
    let src = "/**\n * Authorizes the caller.\n * @param user the caller\n */\nfunction authorize(user) {}\n";
    let facts = extract_light("auth.ts", src, LightLang::TypeScript);
    let doc = doc_of(&facts, "authorize").expect("JSDoc block found");
    assert!(doc.contains("Authorizes the caller"));
}

#[test]
fn javascript_and_tsx_exports_keep_clean_jsdoc() {
    for (path, lang) in [
        ("auth.js", LightLang::JavaScript),
        ("auth.ts", LightLang::TypeScript),
        ("auth.tsx", LightLang::Tsx),
    ] {
        let src = "/**\n * Authorizes the caller.\n * @returns permission\n */\nexport function authorize() {}\n";
        let facts = extract_light(path, src, lang);
        assert_eq!(
            doc_of(&facts, "authorize").as_deref(),
            Some("Authorizes the caller.\n@returns permission"),
            "{path}"
        );
    }
}

#[test]
fn detached_comments_and_ordinary_non_doc_comments_are_not_docs() {
    for (path, lang, src) in [
        (
            "plain.go",
            LightLang::Go,
            "package main\n// A file note.\n\nfunc Plain() {}\n",
        ),
        (
            "plain.rs",
            LightLang::Rust,
            "// Internal note.\nfn Plain() {}\n",
        ),
        (
            "plain.js",
            LightLang::JavaScript,
            "// Internal note.\nfunction Plain() {}\n",
        ),
        (
            "plain.ts",
            LightLang::TypeScript,
            "/** Detached doc. */\n\nfunction Plain() {}\n",
        ),
        ("plain.tsx", LightLang::Tsx, "function Plain() {}\n"),
        (
            "plain.go",
            LightLang::Go,
            "package main\nvar x = 1 // belongs to x\nfunc Plain() {}\n",
        ),
    ] {
        assert_eq!(
            doc_of(&extract_light(path, src, lang), "Plain"),
            None,
            "{path}: {src}"
        );
    }
}
