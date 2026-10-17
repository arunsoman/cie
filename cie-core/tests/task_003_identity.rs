//! TASK-003 validation sketch — instructor contract, adapted per
//! docs/TRACEABILITY.md §2: `SourceLocation` keeps its rendered line/col +
//! path shape (byte-span fields land with §14.2 shapes in v0.3), so the
//! location assertion preserves the sketch's intent via the span constructor.

use cie_core::{
    FileId, Interner, OperationId, ResourceId, SourceLocation, StrInterner, SymbolId, ValueId,
};

#[test]
fn task_003_identity_types_and_interners() {
    let mut names = StrInterner::new(); // Interner<Box<str>> convenience wrapper
    let a = names.intern("authorize");
    let b = names.intern("authorize");
    assert_eq!(a, b, "FR-1: same name, same id, forever");
    assert_eq!(names.lookup("authorize"), Some(a));
    assert_eq!(names.lookup("missing"), None);
    assert_eq!(names.get(a), Some("authorize"));

    let mut payloads: Interner<Box<str>> = Interner::new();
    let k = payloads.insert("auth.py", Box::from("auth.py"));
    assert_eq!(payloads.get(k).map(|s| s.as_ref()), Some("auth.py"));

    // Sketch: SourceLocation { file: FileId(7), start: 12, end: 34 } —
    // adapted to the rendered-location shape (see header note).
    let loc = SourceLocation::span(FileId(7), "auth.py", 12, 34);
    assert!(loc.start_line < loc.end_line);
    assert_eq!(loc.file, FileId(7));
    // Sketch note: the instructor array mixes four distinct types in one
    // array literal, which Rust cannot compile — the intent (every identity
    // newtype is a Copy+Hash+Eq+Ord hot key, §15.3 L0) is pinned via the
    // generic bound plus a homogeneous set.
    fn assert_hot_key<T: Copy + std::hash::Hash + Eq + Ord>(v: T) -> T {
        v
    }
    let _ = assert_hot_key(SymbolId(1));
    let _ = assert_hot_key(OperationId(2));
    let _ = assert_hot_key(ValueId(3));
    let _ = assert_hot_key(ResourceId(4));
    let ids = [SymbolId(1), SymbolId(2), SymbolId(3), SymbolId(4)];
    let set: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(set.len(), 4, "all ids are Copy+Hash+Eq hot keys (§15.3 L0)");
}

/// TASK-003 DoD: the 1M-key interner runs in debug mode (unit test
/// `interner::tests::intern_1m_no_collisions` pins it inside the lib).
#[test]
fn task_003_newtype_placeholder_sentinel() {
    assert_eq!(SymbolId::PLACEHOLDER, SymbolId(u32::MAX));
    assert_eq!(FileId::from(7u32), FileId(7));
    assert_eq!(SymbolId::from(9u32).raw(), 9);
}
