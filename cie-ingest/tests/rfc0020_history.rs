use cie_ingest::{run_drift, run_ingest, IngestOptions};
use std::{fs, process::Command};
fn git(root: &std::path::Path, args: &[&str]) {
    assert!(Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid"
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
        .status
        .success());
}
#[test]
fn cited_history_binds_producer_records_but_not_dirty_or_unavailable_sites() {
    let root = std::env::temp_dir().join(format!(
        "cie-rfc20-history-{}-{}",
        std::process::id(),
        cie_core::types::now_ns()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("auth.py"),
        "def authorize():\n    print('hello')\n    return True\n",
    )
    .unwrap();
    let policy = "- The system must authorize every payment request.\n";
    fs::write(root.join("policy.md"), policy).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "auth.py", "policy.md"]);
    git(&root, &["commit", "-qm", "initial evidence"]);
    let o = run_ingest(&root, &IngestOptions::default()).unwrap();
    assert!(o.snapshot.contracts.iter().any(|c| c
        .introduced
        .as_ref()
        .is_some_and(|h| h[0].subject == "initial evidence")));
    assert!(o.snapshot.findings.iter().any(|f| f.introduced.is_some()));
    let (d, _) = run_drift(
        &root,
        &[("policy.md".into(), policy.into())],
        &IngestOptions::default(),
        None,
        false,
    )
    .unwrap();
    assert!(d.claims.iter().all(|c| c.introduced.is_some()));
    let absolute = root.join("policy.md").to_string_lossy().to_string();
    let (d, _) = run_drift(
        &root,
        &[(absolute, policy.into())],
        &IngestOptions::default(),
        None,
        false,
    )
    .unwrap();
    assert!(
        d.claims.iter().all(|c| c.introduced.is_some()),
        "CLI-style absolute document paths bind inside repository scope"
    );
    fs::write(
        root.join("auth.py"),
        "def authorize():\n    print('changed')\n    return True\n",
    )
    .unwrap();
    let h = cie_history::cited_history(&root, &[("auth.py".into(), 2)]);
    assert!(cie_history::archaeology_chain(&h, "auth.py", 2).is_empty());
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .unwrap();
    fs::write(root.join(".git/shallow"), head.stdout).unwrap();
    assert!(cie_history::cited_history(&root, &[("auth.py".into(), 1)])
        .commits
        .is_empty());
    fs::remove_dir_all(root.join(".git")).unwrap();
    assert!(cie_history::cited_history(&root, &[("auth.py".into(), 1)])
        .commits
        .is_empty());
    let mut claims = cie_drift::extract_claims("policy.md", policy, 0);
    cie_drift::bind_history(&root, &mut claims);
    assert!(claims.iter().all(|c| c.introduced.is_none()));
    fs::remove_dir_all(root).unwrap();
}
