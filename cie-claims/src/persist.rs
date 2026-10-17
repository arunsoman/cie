//! ClaimStore persistence (TASK-073) over the generic store (TASK-001
//! layering — no crate cycle).

use crate::model::Claim;
use cie_store::Store;
use serde::{Deserialize, Serialize};

// ---- TASK-073: persisted ClaimStore (`claims.json`) ----

/// A claim as persisted: the typed claim plus its (doc, span, kind,
/// modality) provenance and a content-derived dedup key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredClaim {
    pub claim: Claim,
    pub doc: String,
    /// Doc span (start line, end line) — provenance into the document.
    pub span: (u32, u32),
    pub kind: String,
    pub modality: String,
    /// blake3(doc + text): identical claim text in the same doc dedups.
    pub dedup_key: String,
}

/// Cross-run claim store: claims survive drift re-runs and accumulate
/// provenance; identical (doc, text) claims collapse onto one entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClaimStore {
    pub entries: std::collections::BTreeMap<String, StoredClaim>,
}

impl ClaimStore {
    /// Dedup key: blake3 over doc name + claim text (§15.1 content identity).
    pub fn dedup_key(doc: &str, text: &str) -> String {
        let mut buf = doc.as_bytes().to_vec();
        buf.extend_from_slice(text.as_bytes());
        cie_core::types::hash_hex(&cie_core::types::content_hash(&buf))
    }

    /// Insert a claim (dedup: same doc+text replaces, keeping the newest).
    pub fn insert(&mut self, claim: Claim) -> bool {
        let key = Self::dedup_key(&claim.doc, &claim.text);
        let stored = StoredClaim {
            span: (claim.loc.line, claim.loc.char_end as u32),
            doc: claim.doc.clone(),
            kind: claim.kind.as_str().to_string(),
            modality: claim.modality.as_str().to_string(),
            dedup_key: key.clone(),
            claim,
        };
        self.entries.insert(key, stored).is_none()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Extension trait: ClaimStore persistence over the generic store.
pub trait ClaimStoreExt {
    fn load_claim_store(&self) -> std::io::Result<ClaimStore>;
    fn save_claim_store(&self, store: &ClaimStore) -> std::io::Result<()>;
}

impl ClaimStoreExt for Store {
    fn load_claim_store(&self) -> std::io::Result<ClaimStore> {
        Ok(self
            .read_json::<ClaimStore>(&self.root().join("claims.json"))?
            .unwrap_or_default())
    }

    fn save_claim_store(&self, store: &ClaimStore) -> std::io::Result<()> {
        self.write_json(&self.root().join("claims.json"), store)
    }
}
