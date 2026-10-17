//! TASK-073 validation — persisted ClaimStore with (doc, span, kind,
//! modality) provenance and a content-derived dedup key.

use cie_claims::persist::ClaimStoreExt;
use cie_ingest::{self, IngestOptions};
use cie_store::Store;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t073-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

#[test]
fn task_073_claim_store_persists_dedups_and_roundtrips() {
    let dir = tmp_dir("store");
    fs::write(
        dir.join("svc.py"),
        "def charge(card):\n    print(\"ok\")\n    return True\n",
    )
    .unwrap();
    let doc = "The system must charge the card.\n".to_string();
    let docs = vec![("p.md".to_string(), doc)];

    // First drift run persists the claim store.
    let (r1, _) = cie_ingest::run_drift(&dir, &docs, &opts(), None, false).unwrap();
    assert!(!r1.claims.is_empty());

    let store = Store::open(&dir).unwrap();
    let cs1 = store.load_claim_store().unwrap();
    assert_eq!(cs1.len(), r1.claims.len(), "one entry per extracted claim");
    assert!(
        dir.join(".cie/claims.json").exists(),
        "claims.json persisted"
    );

    // Provenance: (doc, span, kind, modality) recorded per claim.
    for (_, stored) in &cs1.entries {
        assert_eq!(stored.doc, "p.md");
        assert!(stored.span.0 >= 1, "span start line present");
        assert!(!stored.kind.is_empty() && !stored.modality.is_empty());
        assert_eq!(
            stored.dedup_key,
            cie_claims::persist::ClaimStore::dedup_key(&stored.doc, &stored.claim.text)
        );
    }

    // Second identical run: same claims → dedup keeps the store stable.
    let _ = cie_ingest::run_drift(&dir, &docs, &opts(), None, false).unwrap();
    let cs2 = Store::open(&dir).unwrap().load_claim_store().unwrap();
    assert_eq!(cs1.len(), cs2.len(), "identical (doc, text) claims dedup");

    // A changed doc adds claims without losing the old ones.
    let docs2 = vec![
        (
            "p.md".to_string(),
            "The system must charge the card.\n".to_string(),
        ),
        (
            "q.md".to_string(),
            "The gateway must validate input.\n".to_string(),
        ),
    ];
    let _ = cie_ingest::run_drift(&dir, &docs2, &opts(), None, false).unwrap();
    let cs3 = Store::open(&dir).unwrap().load_claim_store().unwrap();
    assert!(cs3.len() > cs2.len(), "new doc adds entries");
    assert!(
        cs3.entries.values().any(|s| s.doc == "p.md"),
        "old claims retained"
    );

    // Roundtrip: serde roundtrip of the whole store is byte-stable.
    let bytes = serde_json::to_vec(&cs3).unwrap();
    let back: cie_claims::persist::ClaimStore = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cs3, back);
}
