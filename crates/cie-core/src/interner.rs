//! String interning (PRD §47 / TASK-003 `Interner<T>`). Strings are interned
//! once and referenced by index everywhere on hot paths (§15.5). The map side
//! sits on `rustc_hash::FxHashMap<Box<str>, u32>` per the PRD key-crates list
//! (§10); `StrInterner` is the thin `Interner<Box<str>>` wrapper whose
//! `InternedStr` handles are what §14.2 fact fields store.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// A handle to an interned key — what fact structs store instead of strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InternedStr(pub u32);

impl InternedStr {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for InternedStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{}", self.0)
    }
}

/// TASK-003 generic interner: string key → dense `u32` index, payload `T`
/// (FR-1 stable identity). Interning the same key twice yields the same
/// index. `get_by_key` takes `&str` and never allocates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Interner<T> {
    map: FxHashMap<Box<str>, u32>,
    #[serde(bound = "T: Serialize, T: serde::de::DeserializeOwned")]
    vec: Vec<T>,
}

impl<T> Interner<T> {
    pub fn new() -> Self {
        Interner {
            map: FxHashMap::default(),
            vec: Vec::new(),
        }
    }

    /// Insert `key -> value`; returns the stable index (idempotent per key).
    pub fn insert(&mut self, key: &str, value: T) -> u32 {
        if let Some(&idx) = self.map.get(key) {
            return idx;
        }
        let idx = self.vec.len() as u32;
        self.vec.push(value);
        self.map.insert(key.into(), idx);
        idx
    }

    /// Payload lookup by index.
    pub fn get(&self, idx: u32) -> Option<&T> {
        self.vec.get(idx as usize)
    }

    /// Index lookup by key — no allocation.
    pub fn get_by_key(&self, key: &str) -> Option<u32> {
        self.map.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }
}

/// String interner: `Interner<Box<str>>` with ergonomic `InternedStr`
/// handles (TASK-003 §15.5 "strings interned once").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrInterner {
    inner: Interner<Box<str>>,
}

impl StrInterner {
    pub fn new() -> Self {
        StrInterner {
            inner: Interner::new(),
        }
    }

    /// Intern a string; the same bytes always yield the same `InternedStr`.
    pub fn intern(&mut self, s: &str) -> InternedStr {
        InternedStr(self.inner.insert(s, s.into()))
    }

    /// Handle for an already-interned string — no allocation.
    pub fn lookup(&self, s: &str) -> Option<InternedStr> {
        self.inner.get_by_key(s).map(InternedStr)
    }

    /// Resolve a handle back to the string slice.
    pub fn get(&self, id: InternedStr) -> Option<&str> {
        self.inner.get(id.0).map(|b| b.as_ref())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedup() {
        let mut i = StrInterner::new();
        let a = i.intern("cart.add_item");
        let b = i.intern("cart.add_item");
        let c = i.intern("cart.total");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(i.get(a), Some("cart.add_item"));
        assert_eq!(i.len(), 2);
        assert_eq!(i.lookup("cart.add_item"), Some(a));
        assert_eq!(i.lookup("missing"), None);
    }

    /// TASK-003 DoD: 1M distinct keys interned with zero id collisions
    /// (debug-mode test per the sketch).
    #[test]
    fn intern_1m_no_collisions() {
        let mut names = StrInterner::new();
        for i in 0..1_000_000u32 {
            let id = names.intern(&format!("sym_{i}"));
            assert_eq!(id.raw(), i, "dense, collision-free ids");
        }
        assert_eq!(names.len(), 1_000_000);
        assert_eq!(names.lookup("sym_999999"), Some(InternedStr(999_999)));
    }
}
