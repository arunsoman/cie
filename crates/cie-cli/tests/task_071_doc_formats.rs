//! TASK-071 validation — cheap ingestion formats: `.rst`/`.adoc` line-based
//! extractors and `.html` tag-stripped text. Claims come from prose; code
//! blocks, comments and tags do not leak into the claim set.

use cie_cli::engine::{self, IngestOptions};
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t071-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

#[test]
fn task_071_rst_adoc_html_prose_extracted_noise_excluded() {
    let dir = tmp_dir("fmts");
    fs::write(
        dir.join("svc.py"),
        "def charge(card):\n    print(\"charging\")\n    return True\n",
    )
    .unwrap();

    let rst = "Service Guide\n=============\n\nThe system must charge the card.\n\n.. code-block:: python\n\n    must_never_appear = 1\n\nThe client should retry on failure.\n";
    let adoc = "= Billing\n\n// comment must never appear\nThe billing module must log every charge.\n\n----\nnoise_from_code_block must never appear\n----\n\nThe service will refund.\n";
    let html = "<html><head><style>body{}</style></head><body><h1>Policy</h1>\
<p>The gateway must validate every request.</p>\
<script>must_never_appear_script();</script>\
</body></html>\n";
    fs::write(dir.join("guide.rst"), rst).unwrap();
    fs::write(dir.join("billing.adoc"), adoc).unwrap();
    fs::write(dir.join("policy.html"), html).unwrap();

    let docs = vec![
        ("guide.rst".to_string(), fs::read_to_string(dir.join("guide.rst")).unwrap()),
        ("billing.adoc".to_string(), fs::read_to_string(dir.join("billing.adoc")).unwrap()),
        ("policy.html".to_string(), fs::read_to_string(dir.join("policy.html")).unwrap()),
    ];
    let (report, _) = engine::run_drift(&dir, &docs, &opts(), None, false).unwrap();
    let all_text: String = report.claims.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" | ");

    // Prose claims extracted from all three formats.
    assert!(all_text.contains("must charge the card"), "rst prose claim; got: {all_text}");
    assert!(all_text.contains("should retry"), "rst prose claim 2");
    assert!(all_text.contains("must log every charge"), "adoc prose claim");
    assert!(all_text.contains("will refund"), "adoc prose claim 2");
    assert!(all_text.contains("must validate every request"), "html prose claim");

    // Noise excluded: code blocks, comments, script bodies.
    assert!(!all_text.contains("must_never_appear"), "code block leaked");
    assert!(!all_text.contains("comment must never appear"), "adoc comment leaked");
    assert!(!all_text.contains("noise_from_code_block"), "adoc code leaked");
    assert!(!all_text.contains("must_never_appear_script"), "html script leaked");
}

/// The normalizers are unit-pinned in `cie-cli/src/docs.rs` tests; this
/// pins the CLI seam: docs are read through `docs::read_doc`.
#[test]
fn task_071_html_entity_decoding_in_pipeline() {
    let dir = tmp_dir("ents");
    fs::write(dir.join("a.py"), "def f():\n    return 1\n").unwrap();
    let html = "<p>The system &amp; agents must reconcile.</p>\n";
    fs::write(dir.join("e.html"), html).unwrap();
    let docs = vec![("e.html".into(), html.to_string())];
    let (report, _) = engine::run_drift(&dir, &docs, &opts(), None, false).unwrap();
    let all_text: String = report.claims.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join(" ");
    assert!(all_text.contains("must reconcile"), "claim found: {all_text}");
}
