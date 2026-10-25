//! Parity pin: the summary stage fed by phase-3 seeds must produce summary
//! fields identical to the fallback derivation that scans the same facts.
//! The seed builder mirrors the fallback loops exactly (same predicates,
//! same order/limits); this file proves it field by field.
//!
//! `inputs`/`outputs` are excluded from the parity assertion: before seeds
//! existed the summary stage never populated them at all (empty `vec![]`
//! from `FunctionSummary::empty`). Seeding is the FIRST population of those
//! fields, so the assertion pins them against the seed builder directly.

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

fn project_facts() -> LangFacts {
    LangFacts {
        path: "app.py".into(),
        lang: LangId::Python,
        analyzer: "parity".into(),
        symbols: vec![
            function_sym(0, "run", "run"),
            function_sym(1, "helper", "helper"),
            function_sym(2, "charge", "charge"),
        ],
        calls: vec![
            // charge: name-tag + decorator + db claim subject to the receiver gate
            CallSite {
                caller: SymbolId(2),
                callee_text: "app.route('/charge')".into(),
                callee: Callee::Builtin(BuiltinKind::Other),
                line: 1,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (0, 10),
            },
            CallSite {
                caller: SymbolId(2),
                callee_text: "db.execute('delete *')".into(),
                callee: Callee::Builtin(BuiltinKind::DbExecute),
                line: 2,
                in_loop: false,
                arg_vars: vec![],
                receiver: Some("db".into()),
                arguments: None,
                span: (10, 20),
            },
            CallSite {
                caller: SymbolId(2),
                callee_text: "fetch_pending".into(),
                callee: Callee::Unresolved("pending".into()),
                line: 3,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (20, 30),
            },
            // run: print + cross-file call into helper
            CallSite {
                caller: SymbolId(0),
                callee_text: "print(msg)".into(),
                callee: Callee::Builtin(BuiltinKind::Print),
                line: 4,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (30, 40),
            },
            CallSite {
                caller: SymbolId(0),
                callee_text: "helper".into(),
                callee: Callee::Resolved(SymbolId(1)),
                line: 5,
                in_loop: false,
                arg_vars: vec![],
                receiver: None,
                arguments: None,
                span: (40, 50),
            },
        ],
        returns: vec![
            ReturnSite {
                symbol: SymbolId(2),
                line: 6,
                expr: "none".into(),
                constant: Some("none".into()),
            },
            ReturnSite {
                symbol: SymbolId(0),
                line: 7,
                expr: "x + 1".into(),
                constant: None,
            },
            ReturnSite {
                symbol: SymbolId(1),
                line: 8,
                expr: "true".into(),
                constant: Some("true".into()),
            },
        ],
        exceptions: vec![
            ExceptionSite {
                symbol: SymbolId(2),
                kind_text: "CardDeclined".into(),
                is_bare: false,
                line: 9,
                in_handler: false,
                handler_end: 0,
            },
            ExceptionSite {
                symbol: SymbolId(1),
                kind_text: "ValueError".into(),
                is_bare: false,
                line: 10,
                in_handler: false,
                handler_end: 0,
            },
        ],
        assignments: vec![AssignmentFact {
            symbol: SymbolId(2),
            var: "who".into(),
            line: 11,
            rhs_kind: RhsKind::Input,
            rhs_data: vec![],
        }],
        ifs: vec![
            IfSite {
                symbol: SymbolId(2),
                line: 12,
                cond: "amount > 0".into(),
                cond_vars: vec!["amount".into()],
                loc: Default::default(),
            },
            IfSite {
                symbol: SymbolId(0),
                line: 13,
                cond: "msg is None".into(),
                cond_vars: vec!["msg".into()],
                loc: Default::default(),
            },
        ],
        loops: vec![LoopSite {
            symbol: SymbolId(0),
            kind: LoopKind::While,
            line: 14,
            body_start: 15,
            body_end: 16,
            loc: Default::default(),
        }],
        literals: vec![],
        sources: vec![],
        sinks: vec![],
        imports: vec![],
        async_facts: vec![],
        unsupported: vec![],
        bindings: vec![
            BindingFact {
                owner: SymbolId(2),
                name: "amount".into(),
                kind: BindingKind::Param,
                type_text: Some("Card".into()),
                rhs_text: None,
                line: 1,
            },
            // owner 0 (run) has no params: inputs stays empty for it
        ],
        type_decls: vec![],
        resource_effects: vec![],
    }
}

#[test]
fn seeded_summaries_match_fallback_derivation_field_by_field() {
    let facts = project_facts();
    let store = cie_semantic::facts::SymbolStore::build_with_files(
        facts.symbols.clone(),
        cie_core::types::FileTable::new(),
    );
    let per_file = vec![facts.clone()];

    // Fallback path: no seeds — the original per-file derivation loops.
    let fallback = cie_analysis::behavior::compute_summaries_with_reuse(
        &store, &per_file, 7, None, None, None,
    );
    // Seeded path: seeds captured by the file-holding pass, global ids.
    let seeded = cie_analysis::behavior::compute_summaries_with_reuse(
        &store,
        &per_file,
        7,
        None,
        None,
        Some(&vec![summary_seeds(&facts)]),
    );

    let mut fallback_ids: Vec<_> = fallback.summaries.keys().copied().collect();
    fallback_ids.sort();
    let mut seeded_ids: Vec<_> = seeded.summaries.keys().copied().collect();
    seeded_ids.sort();
    assert_eq!(fallback_ids, seeded_ids, "same symbol set either way");

    for sid in fallback_ids {
        // Both runs may carry different wall clocks; zero `compute_ns`. Also
        // zero `inputs`/`outputs`: the legacy derivation NEVER populated them
        // (empty from `FunctionSummary::empty`) while the seed does — that is
        // the population this change adds, asserted separately below. Every
        // other field must match exactly.
        let mut a = fallback.summaries[&sid].clone();
        let mut b = seeded.summaries[&sid].clone();
        a.compute_ns = 0;
        b.compute_ns = 0;
        a.inputs = vec![];
        a.outputs = vec![];
        b.inputs = vec![];
        b.outputs = vec![];
        assert_eq!(a, b, "summary of {sid:?} identical with and without seeds");
    }
}

#[test]
fn seed_populates_inputs_and_outputs_which_fallback_leaves_empty() {
    let facts = project_facts();
    let store = cie_semantic::facts::SymbolStore::build_with_files(
        facts.symbols.clone(),
        cie_core::types::FileTable::new(),
    );
    let per_file = vec![facts.clone()];
    let fallback = cie_analysis::behavior::compute_summaries_with_reuse(
        &store, &per_file, 7, None, None, None,
    );
    let seeded = cie_analysis::behavior::compute_summaries_with_reuse(
        &store,
        &per_file,
        7,
        None,
        None,
        Some(&vec![summary_seeds(&facts)]),
    );

    // the legacy derivation never populated these fields at all
    assert!(fallback.summaries[&SymbolId(2)].inputs.is_empty());
    assert!(fallback.summaries[&SymbolId(2)].outputs.is_empty());

    // the seed does — declared params + distinct return shapes
    assert_eq!(
        seeded.summaries[&SymbolId(2)].inputs,
        vec!["amount: Card".to_string()]
    );
    assert_eq!(
        seeded.summaries[&SymbolId(2)].outputs,
        vec!["none".to_string()]
    );
    // run has no params: empty inputs, real output shape
    assert!(seeded.summaries[&SymbolId(0)].inputs.is_empty());
    assert_eq!(
        seeded.summaries[&SymbolId(0)].outputs,
        vec!["x + 1".to_string()]
    );
}
