//! §14.2 typed fact families (TASK-006) — the canonical adapter vocabulary.
//!
//! These are the EXACT §14.2 shapes the instructor sketches pin:
//! `CallFact { caller, callee, kind, loc, unresolved }`,
//! `EffectFact { op, reads, writes, external }` with
//! `SmallVec<[ResourceId; 4]>`, `ExceptionFact { op, kind, handler }`,
//! `AsyncFact { op, kind }`. The richer v0.1 per-language records in
//! [`crate::facts`] remain the extended model adapters also emit; both
//! serialize side by side (§15.1).
//!
//! # ExceptionKind ↔ §16 language models (FR-3 groundwork)
//!
//! | Adapter | Emits | Never emits |
//! |---|---|---|
//! | Python (TASK-012) | `Raised`, `Propagated`, `Caught` (incl. bare-except → `Caught` with untyped handler evidence) | `ErrorValue`, `Panic` |
//! | JavaScript/TypeScript (013/014) | `Raised`, `Propagated`, `Caught` (throw/Promise.reject) | `ErrorValue`, `Panic` |
//! | Java (015) | `Raised`, `Propagated`, `Caught` (checked exceptions carry `Propagated` with throws-clause evidence) | `ErrorValue`, `Panic` |
//! | C (016) | `Panic` (abort/assert paths); errno-style returns are `ErrorValue` effects, not exceptions | `Raised`, `Propagated`, `Caught` |
//! | C++ (017) | `Raised`, `Propagated`, `Caught` (throw/try/catch) | `ErrorValue`, `Panic` |
//! | C# (018) | `Raised`, `Propagated`, `Caught` | `ErrorValue`, `Panic` |
//! | Go (019) | `ErrorValue` (error returns are values); `Panic` for panic/recover | `Raised`, `Propagated`, `Caught` |
//! | Rust (020) | `ErrorValue` (Result/Option); `Panic` for unwrap/panic!/assert | `Raised`, `Propagated`, `Caught` |
//! | PHP (021) | `Raised`, `Propagated`, `Caught` | `ErrorValue`, `Panic` |
//!
//! Error-value languages (Go, Rust per §16) never emit `Raised` — a
//! returned `error`/`Err` is a value, not an exception (FR-3: per-language
//! judgment stays with the adapter that vouches for it).
//!
//! A call that cannot be resolved at extraction time is kept with
//! `unresolved: true` — dropping it is a contract violation (FR-4, §13 #6).

use cie_core::types::{FileId, OperationId, ResourceId, SourceLocation, SymbolId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// How a call site dispatches (§14.2 comment). `unresolved` is orthogonal:
/// a `Dynamic` call that cannot be resolved is recorded, never dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum CallKind {
    Direct,
    Virtual,
    Dynamic,
    Reflective,
}

/// §14.2 `CallFact` — exact shape: `{ caller, callee, kind, loc, unresolved }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct CallFact {
    pub caller: SymbolId,
    pub callee: SymbolId,
    pub kind: CallKind,
    pub loc: SourceLocation,
    /// FR-4/§13#6 honesty flag: `true` when the callee could not be
    /// resolved at extraction time (dynamic attribute, external module…).
    /// Unresolved calls are marked, never dropped.
    pub unresolved: bool,
}

/// §14.2 `EffectFact` — exact shape: `{ op, reads, writes, external }`;
/// `reads`/`writes` capacities are exactly 4 (`SmallVec<[ResourceId; 4]>`)
/// so hot paths stay allocation-free (§15.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct EffectFact {
    pub op: OperationId,
    pub reads: SmallVec<[ResourceId; 4]>,
    pub writes: SmallVec<[ResourceId; 4]>,
    pub external: bool,
}

/// Closed over the ten languages' exception models (§14.2 comment, FR-3):
/// error-value languages (Go, Rust) never emit `Raised`; see the module
/// table for the per-adapter vouch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ExceptionKind {
    Raised,
    Propagated,
    Caught,
    ErrorValue,
    Panic,
}

/// §14.2 `ExceptionFact` — exact shape: `{ op, kind, handler }` where
/// `handler` is the handler's source location when one exists (`Caught`
/// always has one; `ErrorValue` never does).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ExceptionFact {
    pub op: OperationId,
    pub kind: ExceptionKind,
    pub handler: Option<SourceLocation>,
}

/// §14.2 `AsyncKind` — exactly `{ Async, Await, Spawn, Channel }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AsyncKind {
    Async,
    Await,
    Spawn,
    Channel,
}

/// §14.2 `AsyncFact` — exact shape: `{ op, kind }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AsyncFact {
    pub op: OperationId,
    pub kind: AsyncKind,
}

/// Serde roundtrip for all four §14.2 families (TASK-006 DoD).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_14_2_shapes_roundtrip() {
        let loc = SourceLocation::span_bytes(FileId(0), 10, 40);
        let call = CallFact {
            caller: SymbolId(1),
            callee: SymbolId(2),
            kind: CallKind::Dynamic,
            loc,
            unresolved: true,
        };
        let json = serde_json::to_string(&call).unwrap();
        assert_eq!(serde_json::from_str::<CallFact>(&json).unwrap(), call);

        let effect = EffectFact {
            op: OperationId(3),
            reads: smallvec::smallvec![ResourceId(9)],
            writes: smallvec::smallvec![],
            external: true,
        };
        assert!(effect.reads.len() <= 4, "§14.2 SmallVec<[ResourceId; 4]> bound");
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(serde_json::from_str::<EffectFact>(&json).unwrap(), effect);

        let raised = ExceptionFact {
            op: OperationId(3),
            kind: ExceptionKind::Raised,
            handler: Some(loc),
        };
        let errval = ExceptionFact {
            op: OperationId(4),
            kind: ExceptionKind::ErrorValue,
            handler: None,
        };
        assert!(raised.handler.is_some());
        assert!(errval.handler.is_none());
        let json = serde_json::to_string(&raised).unwrap();
        assert_eq!(serde_json::from_str::<ExceptionFact>(&json).unwrap(), raised);

        let asyncs = [
            AsyncFact { op: OperationId(5), kind: AsyncKind::Spawn },
            AsyncFact { op: OperationId(6), kind: AsyncKind::Channel },
        ];
        assert!(asyncs.iter().any(|a| matches!(a.kind, AsyncKind::Spawn)));
        let json = serde_json::to_string(&asyncs[0]).unwrap();
        assert_eq!(serde_json::from_str::<AsyncFact>(&json).unwrap(), asyncs[0]);
    }
}
