//! The seed builder is the phase-3 statement of "extract while the tree is
//! in hand": group a file's OWN facts into per-symbol summary seeds. These
//! tests pin every field the seed owns and the ones it deliberately leaves
//! to the join (receiver-gated db claims).

use cie_core::types::{FileId, SourceLocation, SymbolId};
use cie_semantic::facts::*;
use cie_semantic::LangId;
use cie_summary::seed::summary_seeds;

fn loc() -> SourceLocation {
    SourceLocation {
        file: FileId(0),
        start: 0,
        end: 1,
    }
}

fn function_sym(id: u32, name: &str, qual: &str) -> SymbolFact {
    SymbolFact {
        id: SymbolId(id),
        kind: SymbolKind::Function,
        name: name.into(),
        qual_name: qual.into(),
        loc: loc(),
        lang: LangId::Python,
        parent: None,
        rev: 1,
        doc: None,
        annotations: vec![],
    }
}

fn class_sym(id: u32, name: &str, qual: &str) -> SymbolFact {
    SymbolFact {
        kind: SymbolKind::Class,
        ..function_sym(id, name, qual)
    }
}

#[test]
fn seed_carries_every_file_local_summary_field() {
    let facts = LangFacts {
        path: "app.py".into(),
        lang: LangId::Python,
        analyzer: "test".into(),
        symbols: vec![
            class_sym(0, "Cart", "Cart"), // not function-like -> no seed
            function_sym(1, "charge", "charge"),
        ],
        calls: vec![
            CallSite {
                caller: SymbolId(1),
                callee_text: "app.route('/charge')".into(),
                callee: Callee::Builtin(BuiltinKind::Other),
                line: 2,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 1),
            },
            CallSite {
                caller: SymbolId(1),
                callee_text: "db.query('select *')".into(),
                callee: Callee::Builtin(BuiltinKind::DbQuery),
                line: 3,
                in_loop: false,
                arg_vars: vec![],
                receiver: Some("db".into()),
                arguments: None,
                span: (1, 2),
            },
            CallSite {
                caller: SymbolId(1),
                callee_text: "print(report)".into(),
                callee: Callee::Builtin(BuiltinKind::Print),
                line: 4,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (2, 3),
            },
            CallSite {
                caller: SymbolId(1),
                callee_text: "fetch_all".into(),
                callee: Callee::Unresolved("pending".into()),
                line: 5,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (3, 4),
            },
        ],
        returns: vec![
            ReturnSite {
                symbol: SymbolId(1),
                line: 6,
                expr: "charge(card, amt)".into(),
                constant: None,
            },
            ReturnSite {
                symbol: SymbolId(1),
                line: 7,
                expr: "false".into(),
                constant: Some("false".into()),
            },
        ],
        exceptions: vec![ExceptionSite {
            symbol: SymbolId(1),
            kind_text: "CardDeclined".into(),
            is_bare: false,
            line: 8,
            in_handler: false,
            handler_end: 0,
        }],
        assignments: vec![AssignmentFact {
            symbol: SymbolId(1),
            var: "who".into(),
            line: 9,
            rhs_kind: RhsKind::Input,
            rhs_data: vec![],
        }],
        ifs: vec![
            IfSite {
                symbol: SymbolId(1),
                line: 10,
                cond: "amt > 0".into(),
                cond_vars: vec!["amt".into()],
                loc: Default::default(),
            },
            IfSite {
                symbol: SymbolId(1),
                line: 11,
                cond: "retry < 3".into(),
                cond_vars: vec!["retry".into()],
                loc: Default::default(),
            },
        ],
        loops: vec![LoopSite {
            symbol: SymbolId(1),
            kind: LoopKind::For,
            line: 12,
            body_start: 13,
            body_end: 14,
            loc: Default::default(),
        }],
        literals: vec![],
        sources: vec![],
        sinks: vec![],
        imports: vec![],
        async_facts: vec![],
        unsupported: vec![],
        bindings: vec![
            // intentionally out of source order: the seed must sort by line
            BindingFact {
                owner: SymbolId(1),
                name: "retries".into(),
                kind: BindingKind::Param,
                type_text: None,
                rhs_text: None,
                line: 3,
            },
            BindingFact {
                owner: SymbolId(1),
                name: "amount".into(),
                kind: BindingKind::Param,
                type_text: Some("Card".into()),
                rhs_text: None,
                line: 2,
            },
            BindingFact {
                owner: SymbolId(0), // class field: not this function's input
                name: "conn".into(),
                kind: BindingKind::Field,
                type_text: None,
                rhs_text: None,
                line: 1,
            },
        ],
        type_decls: vec![],
        resource_effects: vec![],
    };

    let seeds = summary_seeds(&facts);
    assert_eq!(seeds.len(), 1, "classes are not function-like: no seed");
    let s = &seeds[0];
    assert_eq!(s.sym, SymbolId(1));

    // inputs: declared params, sorted by line, typed when the syntax states it
    assert_eq!(
        s.inputs,
        vec!["amount: Card".to_string(), "retries".to_string()]
    );
    // outputs: distinct return shapes, first-occurrence order
    assert_eq!(
        s.outputs,
        vec!["charge(card, amt)".to_string(), "false".to_string()]
    );

    // constants / failure / loops
    assert_eq!(
        s.returns_constants,
        ["false".to_string()].into_iter().collect()
    );
    assert!(!s.all_returns_constant);
    assert!(s.has_failure_path, "non-handler raise is a failure path");
    assert_eq!(s.exceptions, vec!["CardDeclined".to_string()]);
    assert!(s.has_loops);
    assert_eq!(
        s.path_conditions,
        vec!["amt > 0".to_string(), "retry < 3".to_string()]
    );

    // local evidence tags
    assert_eq!(
        s.tags,
        ["reads_input".to_string(), "entrypoint".to_string()]
            .into_iter()
            .collect()
    );

    // db-shaped calls are NOT seeded as effects: the receiver-gated verdict
    // is the join's. Receiver-independent builtins are.
    assert_eq!(
        s.effects,
        [cie_summary::EffectKind::Print].into_iter().collect()
    );

    // pending (unresolved-at-extraction) callee evidence
    assert_eq!(s.pending_callees, vec!["fetch_all".to_string()]);
}

#[test]
fn seed_of_quiet_function_is_false_everywhere() {
    let facts = LangFacts {
        path: "a.py".into(),
        lang: LangId::Python,
        analyzer: "test".into(),
        symbols: vec![function_sym(0, "noop", "noop")],
        ..Default::default()
    };
    let seeds = summary_seeds(&facts);
    assert_eq!(seeds.len(), 1);
    let s = &seeds[0];
    assert!(s.inputs.is_empty() && s.outputs.is_empty() && s.exceptions.is_empty());
    assert!(s.path_conditions.is_empty() && s.pending_callees.is_empty());
    assert!(s.returns_constants.is_empty() && s.tags.is_empty() && s.effects.is_empty());
    assert!(!s.all_returns_constant && !s.has_failure_path && !s.has_loops);
}

#[test]
fn entrypoint_predicate_is_shared_single_source() {
    // the same predicate serves the seed builder and the summary stage's
    // fallback derivation — one function, exported from cie_summary
    assert!(cie_summary::is_entrypoint_call_text("@app.route('/x')"));
    assert!(cie_summary::is_entrypoint_call_text("COMMAND"));
    assert!(!cie_summary::is_entrypoint_call_text("helper.format(x)"));
}
