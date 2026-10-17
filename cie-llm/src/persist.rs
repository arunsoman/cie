//! Label-cache persistence (PRD §63) over the generic store (TASK-001
//! layering — no crate cycle).

use cie_store::Store;
use serde::{Deserialize, Serialize};

/// Label cache (PRD §63): content-addressed by blake3(prompt + model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelCache {
    pub entries: std::collections::BTreeMap<String, CachedLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedLabel {
    pub label: String,
    pub model_id: String,
    pub source: String,
    pub created_ns: u64,
}

/// Extension trait: label-cache persistence over the generic store.
pub trait LabelStoreExt {
    fn load_labels(&self) -> std::io::Result<LabelCache>;
    fn save_labels(&self, cache: &LabelCache) -> std::io::Result<()>;
}

impl LabelStoreExt for Store {
    fn load_labels(&self) -> std::io::Result<LabelCache> {
        Ok(self
            .read_json::<LabelCache>(&self.root().join("labels.json"))?
            .unwrap_or_default())
    }

    fn save_labels(&self, cache: &LabelCache) -> std::io::Result<()> {
        self.write_json(&self.root().join("labels.json"), cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn labels_roundtrip() {
        let root = std::env::temp_dir().join(format!(
            "cie-llm-labels-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        let mut labels = LabelCache::default();
        labels.entries.insert(
            "abc".into(),
            CachedLabel {
                label: "Authorize user".into(),
                model_id: "offline-v1".into(),
                source: "offline".into(),
                created_ns: 42,
            },
        );
        store.save_labels(&labels).unwrap();
        let loaded = store.load_labels().unwrap();
        assert_eq!(loaded.entries["abc"].label, "Authorize user");
        let _ = fs::remove_dir_all(&root);
    }
}
