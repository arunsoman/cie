//! JavaScript deep adapter (TASK-013, §16 #2): prototype-chain dispatch,
//! promises/closures, `this` binding, CommonJS/ESM imports.
//!
//! Built on the shared light engine (symbols, calls, imports, literals,
//! loops, taint sites, tests) plus the JS-specific deep passes required by
//! §16.1: prototype/`this` dispatch marking, try/catch handler pairing,
//! unhandled promise rejections, and `eval`/`with`/dynamic-`import()`
//! evidence.
//!
//! Semantics tables (§16 #2, vouched here — FR-3):
//! - dispatch: `Dynamic` (prototype chain, `this` binding)
//! - numeric: every number is an IEEE-754 double (`Precision::Binary64`,
//!   the §16.2 `F64Only` row), implicit numeric coercion, no integer
//!   overflow (TASK-022 refuses overflow rules here)
//! - exceptions: `throw`/`try`/`catch`/`finally` + unhandled promise
//!   rejections (recorded with no handler — never silent, FR-4)
//! - async: `await` → `Await`, `Promise`/`then` → `Async`, timer/queue
//!   callbacks → `Spawn`/`Channel`

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_semantic::facts::{Callee, ExceptionSite, LangFacts};
use cie_semantic::LangId;
use cie_core::types::SymbolId;
use tree_sitter::Parser;

pub struct JavaScriptAdapter;

/// §16.1: prototype/`this`/computed-property dispatch is never statically
/// resolved — the adapter marks it unresolved so Stage 1 never guesses (FR-4).
fn mark_dynamic_dispatch(callee_text: &str) -> Option<&'static str> {
    let t = callee_text.trim();
    if t.starts_with("this.") || t.starts_with("super.") {
        Some("this_binding")
    } else if t.contains('[') || t.contains("prototype") {
        Some("dynamic_property")
    } else if t.contains('.') {
        // Any property access (`a.authorize`, `console.log`) dispatches
        // through the prototype chain at runtime — the static target is
        // unknown (§16 #2, FR-4: vouched by this adapter's Dynamic row).
        Some("property_access")
    } else {
        None
    }
}

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

impl JavaScriptAdapter {
    /// Deep extraction: light-engine base + JS deep passes (§16.1).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut facts = extract_light(path, src, LightLang::JavaScript);
        let bytes = src.as_bytes();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("javascript grammar");
        let Some(tree) = parser.parse(src, None) else {
            return facts;
        };
        let root = tree.root_node();

        // ---- Pass 1: try/catch handler pairing ----
        // Throws/awaits inside a try body with a catch/finally are marked
        // handled (handler_end = handler end line); otherwise the rejection
        // has NO handler — swallow patterns stay visible to Stage 2 (FR-12).
        let mut handlers: Vec<(u32, u32)> = Vec::new(); // (body_end_line, handler_end_line)
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "try_statement" {
                let mut body_end = 0u32;
                let mut handler_end = 0u32;
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    match child.kind() {
                        "statement_block" => {
                            body_end = cie_core::types::line_of(bytes, child.end_byte() as u32);
                        }
                        "catch_clause" | "finally_clause" => {
                            handler_end =
                                cie_core::types::line_of(bytes, child.end_byte() as u32);
                        }
                        _ => {}
                    }
                }
                if handler_end > 0 && body_end > 0 {
                    handlers.push((body_end, handler_end));
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack.push(child);
            }
        }
        // Handler pairing runs after all exception sources exist (throw,
        // await rejections, promise rejections).
        let mark = |facts: &mut LangFacts| {
            for exc in &mut facts.exceptions {
                for (body_end, handler_end) in &handlers {
                    if exc.line <= *body_end {
                        exc.in_handler = true;
                        exc.handler_end = *handler_end;
                        break;
                    }
                }
            }
        };
        mark(&mut facts);

        // ---- Pass 2: statement-level evidence ----
        let mut stack2 = vec![root];
        while let Some(node) = stack2.pop() {
            match node.kind() {
                "expression_statement" => {
                    // Unhandled promise rejection: `.then(` chain without a
                    // `.catch` — recorded with NO handler (FR-4, never silent).
                    let txt = text_of(node, bytes);
                    if txt.contains(".then(") && !txt.contains(".catch") {
                        facts.exceptions.push(ExceptionSite {
                            symbol: SymbolId(0),
                            kind_text: "promise_rejection".into(),
                            is_bare: false,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                            in_handler: false,
                            handler_end: 0,
                        });
                    }
                }
                "with_statement" => {
                    facts.unsupported.push(format!(
                        "with statement at byte {} — scope object is dynamic (FR-4)",
                        node.start_byte()
                    ));
                }
                "await_expression" => {
                    // An await can reject; recorded as an exception fact so
                    // conversions-to-success stay visible (§13 #5 groundwork).
                    facts.exceptions.push(ExceptionSite {
                        symbol: SymbolId(0),
                        kind_text: "await_rejection".into(),
                        is_bare: false,
                        line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                        in_handler: false,
                        handler_end: 0,
                    });
                }
                _ => {}
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack2.push(child);
            }
        }

        // Re-pair: pass 2 may have added await/promise rejection facts.
        mark(&mut facts);

        // ---- Pass 3: dynamic dispatch marking + eval/import evidence ----
        for call in &mut facts.calls {
            if !matches!(call.callee, Callee::Unresolved(ref r) if r == "pending") {
                continue; // resolved or finally-marked already
            }
            if let Some(reason) = mark_dynamic_dispatch(&call.callee_text) {
                call.callee = Callee::Unresolved(reason.to_string());
            }
            if call.callee_text.contains("eval(") || call.callee_text.starts_with("eval") {
                facts
                    .unsupported
                    .push(format!("eval call at line {} (FR-4 evidence)", call.line));
            }
        }
        // dynamic import() — a call expression with callee text exactly
        // `import` (light engine records `import(...)` calls too)
        let mut stack3 = vec![root];
        while let Some(node) = stack3.pop() {
            if node.kind() == "call_expression" {
                let fn_node = node.child(0).unwrap_or(node);
                if text_of(fn_node, bytes) == "import" {
                    facts.unsupported.push(format!(
                        "dynamic import() at line {} — module unknown until runtime (FR-4)",
                        cie_core::types::line_of(bytes, node.start_byte() as u32)
                    ));
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack3.push(child);
            }
        }
        facts
    }
}

impl LanguageAdapter for JavaScriptAdapter {
    fn id(&self) -> LangId {
        LangId::JavaScript
    }
    /// §16 error model row (TASK-022): vouched by this adapter.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::Raise
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "jsx", "mjs", "cjs"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:javascript"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #2: prototype chain + `this` binding ⇒ runtime dispatch.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16 #2: every number is an IEEE-754 double (F64Only), implicit
    /// numeric coercion; no integer overflow exists in this language.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Unspecified,
            coercion: CoercionPolicy::ImplicitNumeric,
            precision: Precision::Binary64,
        }
    }
    /// §16 #2 conventions: closures, prototype methods, module systems,
    /// promise chains — vouched data, not folklore (FR-3).
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "js-prototype-method",
                convention: "prototype methods dispatch dynamically through the chain",
            },
            FrameworkRule {
                name: "js-closure",
                convention: "closures capture enclosing scope; free variables resolve at call time",
            },
            FrameworkRule {
                name: "js-commonjs",
                convention: "require() is a CommonJS import",
            },
            FrameworkRule {
                name: "js-esm",
                convention: "import/export is an ES module import",
            },
            FrameworkRule {
                name: "js-promise-chain",
                convention: "then/catch chains schedule async continuations",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(JavaScriptAdapter)
}

#[test]
fn task_013_javascript_adapter_facts() {
    let adapter = JavaScriptAdapter;
    assert_eq!(adapter.id(), LangId::JavaScript);
    assert_eq!(
        adapter.dispatch_kind(),
        DispatchKind::Dynamic,
        "§16 #2: prototype chain"
    );
    assert!(
        matches!(adapter.numeric_semantics().precision, Precision::Binary64),
        "§16 #2: every number is a double (F64Only row)"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = "\
class Auth { authorize(u) { return this.check(u) && fetch('/ok').then(r => r.ok); } }
const a = new Auth(); a.authorize('u');
async function login(u) { try { return await a.authorize(u); } catch (e) { return true; } }
fetch('/x').then(r => r.json());
";
    let facts = adapter.extract("app.js", src);

    // this.check(...) — this-binding dispatch is dynamic and unresolved.
    assert!(
        facts.calls.iter().any(|c| {
            matches!(c.callee, Callee::Unresolved(ref r) if r == "this_binding")
                && c.callee_text.starts_with("this.")
        }),
        "prototype/this dispatch is unresolved (FR-4)"
    );
    // try/catch around await: handled exception + Await async fact.
    assert!(
        facts
            .exceptions
            .iter()
            .any(|e| e.in_handler && e.handler_end > 0),
        "try/catch yields a handled exception fact"
    );
    assert!(
        facts
            .exceptions
            .iter()
            .any(|e| e.kind_text == "promise_rejection" && !e.in_handler),
        "then() without catch is an unhandled rejection"
    );
    assert!(
        facts
            .async_facts
            .iter()
            .any(|a| matches!(a.kind, cie_semantic::facts::AsyncKind::Await)),
        "await is an Await async fact"
    );
}
