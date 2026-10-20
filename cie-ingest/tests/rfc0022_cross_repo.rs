//! RFC-0022 §6.4 — `OrgGraph.api_edges` gets a real constructor: a call
//! `cie-ingest` couldn't resolve within its own repo is checked against
//! every sibling repo's exported symbols by short name, producing an
//! edge only on an unambiguous match.

use cie_ingest::{run_org_ingest, IngestOptions};
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-rfc0022-xrepo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_unresolved_call_matching_exactly_one_sibling_export_becomes_an_edge() {
    let a = tmp_dir("a-match");
    let b = tmp_dir("b-match");
    std::fs::write(
        a.join("caller.py"),
        "import notifications\n\ndef trigger():\n    return notifications.deliver_webhook(1)\n",
    )
    .unwrap();
    std::fs::write(
        b.join("handler.py"),
        "def deliver_webhook(event_id):\n    return event_id\n",
    )
    .unwrap();

    let org = run_org_ingest(&[a.clone(), b.clone()], &IngestOptions::default()).unwrap();
    assert_eq!(org.repos.len(), 2);
    assert!(
        org.api_edges
            .iter()
            .any(|e| e.to_symbol.contains("deliver_webhook")),
        "expected an edge resolving to deliver_webhook, got: {:?}",
        org.api_edges
    );

    std::fs::remove_dir_all(&a).ok();
    std::fs::remove_dir_all(&b).ok();
}

#[test]
fn an_ambiguous_match_across_two_repos_resolves_to_neither() {
    let a = tmp_dir("a-ambig");
    let b = tmp_dir("b-ambig");
    let c = tmp_dir("c-ambig");
    std::fs::write(
        a.join("caller.py"),
        "import svc\n\ndef trigger():\n    return svc.handle(1)\n",
    )
    .unwrap();
    // Two sibling repos BOTH export a function named `handle` — the
    // short-name match is ambiguous, so neither may be guessed.
    std::fs::write(b.join("one.py"), "def handle(x):\n    return x\n").unwrap();
    std::fs::write(c.join("two.py"), "def handle(x):\n    return x + 1\n").unwrap();

    let org = run_org_ingest(
        &[a.clone(), b.clone(), c.clone()],
        &IngestOptions::default(),
    )
    .unwrap();
    assert!(
        !org.api_edges.iter().any(|e| e.to_symbol.contains("handle")),
        "an ambiguous short name must never produce a guessed edge: {:?}",
        org.api_edges
    );

    std::fs::remove_dir_all(&a).ok();
    std::fs::remove_dir_all(&b).ok();
    std::fs::remove_dir_all(&c).ok();
}

#[test]
fn a_call_with_no_matching_export_anywhere_stays_unresolved() {
    let a = tmp_dir("a-none");
    let b = tmp_dir("b-none");
    std::fs::write(
        a.join("caller.py"),
        "import ghost\n\ndef trigger():\n    return ghost.nonexistent_fn(1)\n",
    )
    .unwrap();
    std::fs::write(b.join("real.py"), "def real_fn():\n    return 1\n").unwrap();

    let org = run_org_ingest(&[a.clone(), b.clone()], &IngestOptions::default()).unwrap();
    assert!(
        org.api_edges.is_empty(),
        "no export matches — zero edges, never a guess"
    );

    std::fs::remove_dir_all(&a).ok();
    std::fs::remove_dir_all(&b).ok();
}

#[test]
fn unchanged_org_reload_preserves_edges_and_merged_concepts() {
    let a = tmp_dir("reload-a");
    let b = tmp_dir("reload-b");
    std::fs::write(
        a.join("caller.py"),
        "import svc\n\ndef trigger():\n    return svc.deliver(1)\n",
    )
    .unwrap();
    std::fs::write(b.join("service.py"), "def deliver(x):\n    return x\n").unwrap();
    let repos = [a.clone(), b.clone()];
    let opts = IngestOptions::default();
    let first = run_org_ingest(&repos, &opts).unwrap();
    assert_eq!(first.api_edges.len(), 1);
    let second = run_org_ingest(&repos, &opts).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    // A pre-RFC store has neither navigation artifact. It must repair
    // them without changing the revision or losing the unresolved call.
    for repo in &repos {
        let store = cie_store::Store::open(repo).unwrap();
        let rev = store.last_rev();
        for name in ["relations.json", "unresolved.json", "org-member.json"] {
            std::fs::remove_file(store.root().join("revs").join(rev.to_string()).join(name))
                .unwrap();
        }
    }
    let repaired = run_org_ingest(&repos, &opts).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&repaired).unwrap()
    );
    for repo in repos {
        std::fs::remove_dir_all(repo).unwrap();
    }
}

#[test]
fn repeated_calls_are_one_explicitly_inferred_candidate_edge() {
    let a = tmp_dir("repeated-a");
    let b = tmp_dir("repeated-b");
    std::fs::write(
        a.join("caller.py"),
        "import svc\n\ndef trigger():\n    svc.deliver(1)\n    return svc.deliver(2)\n",
    )
    .unwrap();
    std::fs::write(b.join("service.py"), "def deliver(x):\n    return x\n").unwrap();
    let org = run_org_ingest(&[a.clone(), b.clone()], &IngestOptions::default()).unwrap();
    assert_eq!(org.api_edges.len(), 1);
    assert_eq!(org.api_edges[0].claim_class.as_deref(), Some("inferred"));
    assert!(org.api_edges[0]
        .match_basis
        .as_ref()
        .unwrap()
        .contains("unverified"));
    let single = run_org_ingest(&[a.clone()], &IngestOptions::default()).unwrap();
    assert!(single.merged_concepts.is_empty());
    // Previously persisted edges load without new metadata.
    let mut old = serde_json::to_value(&org.api_edges[0]).unwrap();
    old.as_object_mut().unwrap().remove("claim_class");
    old.as_object_mut().unwrap().remove("match_basis");
    let old: cie_store::ApiEdge = serde_json::from_value(old).unwrap();
    assert!(old.claim_class.is_none());
    std::fs::remove_dir_all(a).unwrap();
    std::fs::remove_dir_all(b).unwrap();
}
