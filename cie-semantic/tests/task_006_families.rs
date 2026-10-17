//! TASK-006 validation sketch — §14.2 typed fact shapes, binding contract.

use cie_semantic::{AsyncFact, AsyncKind, CallFact, CallKind, EffectFact, ExceptionFact, ExceptionKind};
use cie_core::{FileId, OperationId, ResourceId, SourceLocation, SymbolId};

const LOC: SourceLocation = SourceLocation { file: FileId(0), start: 10, end: 40 };

#[test]
fn task_006_typed_fact_shapes() {
    let call = CallFact {
        caller: SymbolId(1), callee: SymbolId(2),
        kind: CallKind::Dynamic, loc: LOC, unresolved: true,
    };
    assert!(call.unresolved, "FR-4/§13#6: unresolved calls are marked, never dropped");

    let effect = EffectFact {
        op: OperationId(3),
        reads: smallvec::smallvec![ResourceId(9)],
        writes: smallvec::smallvec![],
        external: true,
    };
    assert!(effect.reads.len() <= 4, "§14.2: bounded SmallVec<[ResourceId; 4]>");

    let raised = ExceptionFact { op: OperationId(3), kind: ExceptionKind::Raised, handler: Some(LOC) };
    let errval = ExceptionFact { op: OperationId(4), kind: ExceptionKind::ErrorValue, handler: None };
    assert!(matches!(raised.kind, ExceptionKind::Raised) && raised.handler.is_some());
    assert!(matches!(errval.kind, ExceptionKind::ErrorValue));

    let asyncs = [AsyncFact { op: OperationId(5), kind: AsyncKind::Spawn },
                  AsyncFact { op: OperationId(6), kind: AsyncKind::Channel }];
    assert!(asyncs.iter().any(|a| matches!(a.kind, AsyncKind::Spawn)));
}

/// Roundtrip serialization for all four §14.2 structs (TASK-006 DoD).
#[test]
fn task_006_roundtrip_serialization() {
    let call = CallFact {
        caller: SymbolId(1),
        callee: SymbolId(2),
        kind: CallKind::Reflective,
        loc: LOC,
        unresolved: false,
    };
    let json = serde_json::to_string(&call).unwrap();
    assert_eq!(serde_json::from_str::<CallFact>(&json).unwrap(), call);

    let effect = EffectFact {
        op: OperationId(3),
        reads: smallvec::smallvec![ResourceId(1), ResourceId(2), ResourceId(3), ResourceId(4)],
        writes: smallvec::smallvec![ResourceId(5)],
        external: false,
    };
    let json = serde_json::to_string(&effect).unwrap();
    assert_eq!(serde_json::from_str::<EffectFact>(&json).unwrap(), effect);

    let exc = ExceptionFact { op: OperationId(7), kind: ExceptionKind::Caught, handler: Some(LOC) };
    let json = serde_json::to_string(&exc).unwrap();
    assert_eq!(serde_json::from_str::<ExceptionFact>(&json).unwrap(), exc);

    let a = AsyncFact { op: OperationId(8), kind: AsyncKind::Await };
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(serde_json::from_str::<AsyncFact>(&json).unwrap(), a);
}
