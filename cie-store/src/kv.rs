//! KV layer with backend swap (TASK-024, §10/§15.1/§15.3, FR-22).
//!
//! One [`KvStore`] trait: `sled` is the default backend, `redb` sits
//! behind the `backend-redb` cargo feature, and both pass the IDENTICAL
//! tree/key round-trip suite (`kv_suite` — the swap needs zero call-site
//! changes). The in-memory [`MemKv`] backend is always available for
//! tests and dry runs.
//!
//! - Tree management follows §15.1 exactly — one tree per namespace; the
//!   [`TreeRegistry`] enumerates every namespace and FAILS FAST on
//!   unknown prefixes (global rule 7).
//! - Numeric key components (`RevId`, `SymbolId`, `FileId`, `RecordId`)
//!   are BIG-ENDIAN encoded so lexicographic order equals numeric order
//!   (range scans stay correct); `Hash` keys are raw 32-byte blake3
//!   (FR-21).
//! - L0 hot cache per §15.3: `DashMap<SymbolId, SymbolFact>` hot facts +
//!   `FxHashMap<Hash, SymbolId>` index + a `quick_cache` LRU bounding the
//!   hot set; the lookup timing path allocates nothing (§15.5) and is
//!   gated at p99 ≤ 50 ns × CI_TOLERANCE (§15.4, global rule 5).


use crate::records::RecordId;
use cie_core::types::{FileId, Hash, RevId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;

/// Every §15.1 namespace, exactly once (global rule 7).
pub const NAMESPACES: &[&str] = &[
    "rev",
    "file",
    "sym",
    "sym_by_hash",
    "rel:csr",
    "sum",
    "contract",
    "concept",
    "hier",
    "label",
    "vec",
    "evidence",
    "record",
    "deps",
    "rev_deps",
    "doc",
    "claim",
    "claim_by_doc",
    "map",
    "verify",
    "drift",
    "drift_delta",
    "reverse",
];

/// Fail-fast registry: `ensure(ns)` errors on any prefix outside
/// [`NAMESPACES`].
#[derive(Debug, Clone, Default)]
pub struct TreeRegistry {
    open: BTreeMap<String, ()>,
}

impl TreeRegistry {
    pub fn new() -> Self {
        let mut open = BTreeMap::new();
        for ns in NAMESPACES {
            open.insert(ns.to_string(), ());
        }
        TreeRegistry { open }
    }

    /// All §15.1 namespaces (sketch: `enumerates every namespace`).
    pub fn namespaces(&self) -> Vec<String> {
        NAMESPACES.iter().map(|s| s.to_string()).collect()
    }

    /// Fail fast on unknown prefixes.
    pub fn ensure(&self, ns: &str) -> Result<(), String> {
        if self.open.contains_key(ns) {
            Ok(())
        } else {
            Err(format!(
                "unknown namespace `{ns}` — §15.1 has exactly {} trees (global rule 7)",
                NAMESPACES.len()
            ))
        }
    }
}

/// Big-endian key codec (FR-21): lexicographic order == numeric order.
pub mod key {
    use super::*;

    /// u64 → 8 BE bytes (RevId/SymbolId/FileId wide keys).
    pub fn u64_be(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    pub fn rev(r: RevId) -> [u8; 8] {
        u64_be(r)
    }

    pub fn sym(s: SymbolId) -> [u8; 8] {
        u64_be(s.0 as u64)
    }

    pub fn file(f: FileId) -> [u8; 8] {
        u64_be(f.0 as u64)
    }

    /// RecordId keys are raw 32 bytes (content-addressed).
    pub fn record(r: &RecordId) -> Vec<u8> {
        r.to_vec()
    }

    /// Hash keys are raw 32-byte blake3 (FR-21).
    pub fn hash(h: &Hash) -> Vec<u8> {
        h.to_vec()
    }

    /// Composite `{rev}:{sym}` key (rel:csr, sum namespaces).
    pub fn rev_sym(r: RevId, s: SymbolId) -> Vec<u8> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(&u64_be(r));
        v.extend_from_slice(&u64_be(s.0 as u64));
        v
    }
}

/// The one KV interface (§10: sled/redb are THE KV layer; the swap is
/// invisible to callers).
pub trait KvStore: Send + Sync {
    fn put(&self, ns: &str, key: &[u8], value: &[u8]) -> Result<(), String>;
    fn get(&self, ns: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String>;
    fn delete(&self, ns: &str, key: &[u8]) -> Result<(), String>;
    /// Lexicographic scan under a prefix (range scans for `sum:{rev}:*`).
    fn scan_prefix(&self, ns: &str, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String>;
    fn registry(&self) -> &TreeRegistry;
}

/// In-memory backend (always compiled; tests + dry runs).
#[derive(Debug, Default)]
pub struct MemKv {
    trees: RwLock<BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>>,
    registry: TreeRegistry,
}

impl MemKv {
    pub fn new() -> Self {
        MemKv { trees: RwLock::new(BTreeMap::new()), registry: TreeRegistry::new() }
    }
}

impl KvStore for MemKv {
    fn put(&self, ns: &str, key: &[u8], value: &[u8]) -> Result<(), String> {
        self.registry.ensure(ns)?;
        let mut t = self.trees.write().unwrap();
        t.entry(ns.into()).or_default().insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn get(&self, ns: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.registry.ensure(ns)?;
        Ok(self
            .trees
            .read()
            .unwrap()
            .get(ns)
            .and_then(|m| m.get(key).cloned()))
    }

    fn delete(&self, ns: &str, key: &[u8]) -> Result<(), String> {
        self.registry.ensure(ns)?;
        if let Some(m) = self.trees.write().unwrap().get_mut(ns) {
            m.remove(key);
        }
        Ok(())
    }

    fn scan_prefix(&self, ns: &str, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
        self.registry.ensure(ns)?;
        let t = self.trees.read().unwrap();
        Ok(t.get(ns)
            .map(|m| {
                m.range(prefix.to_vec()..)
                    .take_while(|(k, _)| k.starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn registry(&self) -> &TreeRegistry {
        &self.registry
    }
}

/// A §14.2-shaped hot symbol fact (allocation-light clone target for the
/// DashMap; the full SymbolFact shape stays in the semantic layer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolFactLite {
    pub id: SymbolId,
    pub name: String,
    pub rev: RevId,
}

/// The L0 hot cache (§15.3): DashMap hot facts + FxHashMap hash index +
/// quick_cache bounded LRU. `lookup` allocates nothing on the timing path.
pub struct L0Cache {
    pub hot: dashmap::DashMap<SymbolId, SymbolFactLite>,
    pub index: RwLock<rustc_hash::FxHashMap<Hash, SymbolId>>,
    pub lru: quick_cache::sync::Cache<(u64, u64), Vec<u8>>,
}

impl Default for L0Cache {
    fn default() -> Self {
        L0Cache {
            hot: dashmap::DashMap::new(),
            index: RwLock::new(rustc_hash::FxHashMap::default()),
            lru: quick_cache::sync::Cache::new(65_536),
        }
    }
}

impl L0Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hot-fact lookup (convenience; clones — NOT the gated timing path).
    pub fn lookup(&self, id: SymbolId) -> Option<SymbolFactLite> {
        self.hot.get(&id).map(|e| e.value().clone())
    }

    /// §15.4/§15.5 hot lookup probe: the map access itself, allocating
    /// nothing (the gated 50 ns timing path).
    pub fn lookup_id(&self, id: SymbolId) -> Option<SymbolId> {
        self.hot.get(&id).map(|e| e.id)
    }

    /// Hash → id index — the §15.4 30 ns path.
    pub fn id_by_hash(&self, h: &Hash) -> Option<SymbolId> {
        self.index.read().unwrap().get(h).copied()
    }

    pub fn insert_fact(&self, fact: SymbolFactLite) {
        self.hot.insert(fact.id, fact);
    }

    pub fn insert_hash(&self, h: Hash, id: SymbolId) {
        self.index.write().unwrap().insert(h, id);
    }
}

#[cfg(feature = "backend-sled")]
pub mod sled_backend {
    //! sled backend (default). Identical semantics to [`super::MemKv`] and
    //! the redb backend — one suite covers all (TASK-024 DoD).

    use super::{key, KvStore, TreeRegistry};
    use std::path::Path;

    pub struct SledStore {
        db: sled::Db,
        registry: TreeRegistry,
    }

    impl SledStore {
        pub fn open(path: &Path) -> Result<Self, String> {
            Ok(SledStore { db: sled::open(path).map_err(|e| e.to_string())?, registry: TreeRegistry::new() })
        }

        pub fn open_temporary() -> Result<Self, String> {
            let dir = std::env::temp_dir().join(format!(
                "cie024_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            Self::open(&dir)
        }

        fn tree(&self, ns: &str) -> Result<sled::Tree, String> {
            self.registry.ensure(ns)?;
            self.db.open_tree(ns).map_err(|e| e.to_string())
        }
    }

    impl KvStore for SledStore {
        fn put(&self, ns: &str, key: &[u8], value: &[u8]) -> Result<(), String> {
            self.tree(ns)?.insert(key, value).map(|_| ()).map_err(|e| e.to_string())
        }

        fn get(&self, ns: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
            Ok(self.tree(ns)?.get(key).map_err(|e| e.to_string())?.map(|v| v.to_vec()))
        }

        fn delete(&self, ns: &str, key: &[u8]) -> Result<(), String> {
            self.tree(ns)?.remove(key).map(|_| ()).map_err(|e| e.to_string())
        }

        fn scan_prefix(&self, ns: &str, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
            let t = self.tree(ns)?;
            Ok(t.scan_prefix(prefix)
                .filter_map(|r| r.ok())
                .map(|(k, v)| (k.to_vec(), v.to_vec()))
                .collect())
        }

        fn registry(&self) -> &TreeRegistry {
            &self.registry
        }
    }
}

#[cfg(feature = "backend-redb")]
pub mod redb_backend {
    //! redb backend behind `backend-redb`. Identical semantics — the same
    //! `kv_suite` covers it (TASK-024 DoD).

    use super::{key, KvStore, TreeRegistry};
    use std::path::Path;
    use std::sync::RwLock;

    pub struct RedbStore {
        db: redb::Database,
        registry: TreeRegistry,
        /// redb table creation needs &mut; serialize table opens.
        lock: RwLock<()>,
    }

    const VALUE: redb::TableDefinition<&[u8], &[u8]> = redb::TableDefinition::new("kv");

    impl RedbStore {
        pub fn open(path: &Path) -> Result<Self, String> {
            Ok(RedbStore {
                db: redb::Database::create(path).map_err(|e| e.to_string())?,
                registry: TreeRegistry::new(),
                lock: RwLock::new(()),
            })
        }

        pub fn open_temporary() -> Result<Self, String> {
            let dir = std::env::temp_dir().join(format!(
                "cie024redb_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            Self::open(&dir.join("kv.redb"))
        }
    }

    impl KvStore for RedbStore {
        fn put(&self, ns: &str, k: &[u8], v: &[u8]) -> Result<(), String> {
            self.registry.ensure(ns)?;
            let mut key = ns.as_bytes().to_vec();
            key.push(0xFF); // namespace separator (never a key byte prefix issue: keys are BE numerics/hashes)
            key.extend_from_slice(k);
            let _g = self.lock.read().unwrap();
            let wb = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut t = wb.open_table(VALUE).map_err(|e| e.to_string())?;
                t.insert(key.as_slice(), v).map_err(|e| e.to_string())?;
            }
            wb.commit().map_err(|e| e.to_string())
        }

        fn get(&self, ns: &str, k: &[u8]) -> Result<Option<Vec<u8>>, String> {
            self.registry.ensure(ns)?;
            let mut key = ns.as_bytes().to_vec();
            key.push(0xFF);
            key.extend_from_slice(k);
            let _g = self.lock.read().unwrap();
            let rb = self.db.begin_read().map_err(|e| e.to_string())?;
            let t = rb.open_table(VALUE).map_err(|e| e.to_string())?;
            Ok(t.get(key.as_slice()).map_err(|e| e.to_string())?.map(|v| v.value().to_vec()))
        }

        fn delete(&self, ns: &str, k: &[u8]) -> Result<(), String> {
            self.registry.ensure(ns)?;
            let mut key = ns.as_bytes().to_vec();
            key.push(0xFF);
            key.extend_from_slice(k);
            let _g = self.lock.read().unwrap();
            let wb = self.db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut t = wb.open_table(VALUE).map_err(|e| e.to_string())?;
                t.remove(key.as_slice()).map_err(|e| e.to_string())?;
            }
            wb.commit().map_err(|e| e.to_string())
        }

        fn scan_prefix(&self, ns: &str, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
            self.registry.ensure(ns)?;
            let mut key = ns.as_bytes().to_vec();
            key.push(0xFF);
            key.extend_from_slice(prefix);
            let _g = self.lock.read().unwrap();
            let rb = self.db.begin_read().map_err(|e| e.to_string())?;
            let t = rb.open_table(VALUE).map_err(|e| e.to_string())?;
            let mut out = vec![];
            for r in t.range(key.as_slice()..).map_err(|e| e.to_string())? {
                let (k, v) = r.map_err(|e| e.to_string())?;
                if !k.value().starts_with(&key) {
                    break;
                }
                out.push((k.value()[key.len() - prefix.len()..].to_vec(), v.value().to_vec()));
            }
            Ok(out)
        }

        fn registry(&self) -> &TreeRegistry {
            &self.registry
        }
    }
}

/// The shared TASK-024 suite — every backend passes the identical
/// round-trips (sled/redb/MemKv: zero call-site changes on swap).
pub mod kv_suite {
    use super::{key, KvStore, NAMESPACES};

    pub fn run_suite(store: &dyn KvStore) {
        // 1. registry enumerates every §15.1 namespace + fails fast
        let nses = store.registry().namespaces();
        assert_eq!(nses.len(), NAMESPACES.len());
        for ns in &nses {
            store.registry().ensure(ns).expect("namespace registered");
        }
        assert!(store.registry().ensure("bogus").is_err(), "fail fast on unknown prefix");

        // 2. typed round-trip at exact §15.1 keys
        let fact = b"{\"id\":7,\"name\":\"pay\",\"rev\":41}";
        store.put("sym", &key::sym(cie_core::types::SymbolId(7)), fact).unwrap();
        assert_eq!(
            store.get("sym", &key::sym(cie_core::types::SymbolId(7))).unwrap().unwrap(),
            fact
        );

        // 3. BE codec: numeric order == lexicographic order
        assert!(key::u64_be(9) < key::u64_be(10), "BE encoding required for range scans");
        assert!(key::u64_be(41) < key::u64_be(42));
        assert!(key::rev_sym(1u64, cie_core::types::SymbolId(2)) < key::rev_sym(1u64, cie_core::types::SymbolId(3)));
        assert!(key::rev_sym(41u64, cie_core::types::SymbolId(9)) < key::rev_sym(42u64, cie_core::types::SymbolId(1)));
        let h: cie_core::types::Hash = cie_core::types::hash(b"auth.py");
        store.put("sym_by_hash", &key::hash(&h), &key::sym(cie_core::types::SymbolId(7))).unwrap();
        assert_eq!(
            store.get("sym_by_hash", &key::hash(&h)).unwrap().unwrap(),
            key::sym(cie_core::types::SymbolId(7))
        );

        // 4. record keys are raw 32-byte content ids
        let rid: [u8; 32] = cie_core::types::hash(b"record-payload");
        store.put("record", &key::record(&rid), b"payload").unwrap();
        assert_eq!(store.get("record", &key::record(&rid)).unwrap().unwrap(), b"payload");

        // 5. delete + range scan
        store.put("sum", &key::rev_sym(42u64, cie_core::types::SymbolId(7)), b"s1").unwrap();
        store.put("sum", &key::rev_sym(42u64, cie_core::types::SymbolId(8)), b"s2").unwrap();
        store.put("sum", &key::rev_sym(43u64, cie_core::types::SymbolId(9)), b"s3").unwrap();
        let rows = store.scan_prefix("sum", &key::rev(42u64)).unwrap();
        assert_eq!(rows.len(), 2, "range scan stays within the revision");
        store.delete("sum", &key::rev_sym(42u64, cie_core::types::SymbolId(7))).unwrap();
        assert!(store.get("sum", &key::rev_sym(42u64, cie_core::types::SymbolId(7))).unwrap().is_none());
    }
}
