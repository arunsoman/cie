//! String interning (PRD §47 `Interner<T>`). Strings are interned once and
//! referenced by index everywhere on hot paths.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Interner {
    map: HashMap<String, u32>,
    vec: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Interner::default()
    }

    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.map.get(s) {
            return idx;
        }
        let idx = self.vec.len() as u32;
        self.vec.push(s.to_string());
        self.map.insert(s.to_string(), idx);
        idx
    }

    pub fn get(&self, idx: u32) -> &str {
        self.vec
            .get(idx as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn lookup(&self, s: &str) -> Option<u32> {
        self.map.get(s).copied()
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedup() {
        let mut i = Interner::new();
        let a = i.intern("cart.add_item");
        let b = i.intern("cart.add_item");
        let c = i.intern("cart.total");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(i.get(a), "cart.add_item");
        assert_eq!(i.len(), 2);
    }
}
