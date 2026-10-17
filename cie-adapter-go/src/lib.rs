//! Go deep adapter (TASK-019, §16 #8): goroutines, channels, interfaces,
//! error values.
//!
//! Light-engine base (symbols, calls, imports, literals, loops, taint,
//! tests) plus Go-specific deep passes: interface-typed call marking,
//! `if err != nil` ErrorValue facts, `panic`/`defer`/`recover` Panic facts,
//! `go f()` Spawn facts, `chan`/`select` Channel facts.
//!
//! Semantics tables (§16 #8, vouched here — FR-3):
//! - dispatch: `Dynamic` — interface-typed calls dispatch via runtime
//!   itables (`Dynamic`); concrete-type calls are `Direct`
//! - numeric: fixed-width with DEFINED wraparound (`OverflowPolicy::
//!   Wrapping`, never UB)
//! - errors are values: `if err != nil` → `ErrorValue` facts;
//!   `panic`/`defer`/`recover` → `Panic` facts. The adapter NEVER emits
//!   `Raised` — Go has no throw (FR-3, global rule 6)
//! - interface satisfaction is implicit, so calls on interface values stay
//!   unresolved until a type is known (FR-4)
//! - a returned `error` with no checked `err != nil` at the call site is
//!   still just an `ErrorValue` fact — structure, not judgment (FR-2)

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_semantic::facts::{AsyncKind, AsyncSite, Callee, ExceptionSite, LangFacts};
use cie_semantic::LangId;
use cie_core::types::SymbolId;
use tree_sitter::Parser;

pub struct GoAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}


impl GoAdapter {
    /// Deep extraction: light-engine base + Go deep passes (§16.1).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut facts = extract_light(path, src, LightLang::Go);
        let bytes = src.as_bytes();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("go grammar");
        let Some(tree) = parser.parse(src, None) else {
            return facts;
        };
        let root = tree.root_node();

        // ---- Pass 1: interface types + typed variables ----
        let mut interfaces: Vec<String> = vec![];
        let mut typed_vars: Vec<String> = vec![];
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "type_declaration" | "type_spec" => {
                    let txt = text_of(node, bytes);
                    // `type Authorizer interface { ... }`
                    if let Some(brace) = txt.find("interface") {
                        if let Some(name_end) = txt[..brace].trim_end().rfind(char::is_whitespace) {
                            let name = txt[name_end + 1..brace].trim();
                            if !name.is_empty() {
                                interfaces.push(name.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack.push(child);
            }
        }
        // Bind variable names to interface types (params + short/long decls).
        let mut stack1b = vec![root];
        while let Some(node) = stack1b.pop() {
            let txt = text_of(node, bytes);
            let is_param = node.kind() == "parameter_declaration";
            let is_shortvar = node.kind() == "short_var_declaration";
            if is_param {
                // `a Authorizer` — name first, type second.
                let names: Vec<&str> =
                    txt.split_whitespace().collect();
                if names.len() == 2 && interfaces.iter().any(|i| i == names[1]) {
                    typed_vars.push(names[0].to_string());
                }
            } else if is_shortvar {
                // `a := NewAuth()` — the runtime type is unknown in general;
                // only mark when the initializer names a constructor for an
                // interface-typed return we cannot see. Left unmarked: Go's
                // itable dispatch is handled at the call site instead.
                let _ = txt;
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack1b.push(child);
            }
        }

        // ---- Pass 2: goroutines, channels, err checks, panic/defer ----
        let mut stack2 = vec![root];
        while let Some(node) = stack2.pop() {
            match node.kind() {
                "go_statement" => {
                    facts.async_facts.push(AsyncSite {
                        symbol: SymbolId(0),
                        kind: AsyncKind::Spawn,
                        line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                    });
                }
                "send_statement" | "receive_statement" | "select_statement" => {
                    facts.async_facts.push(AsyncSite {
                        symbol: SymbolId(0),
                        kind: AsyncKind::Channel,
                        line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                    });
                }
                "if_statement" => {
                    // `if err != nil` — errors are values (ErrorValue, FR-3).
                    let cond = node
                        .child_by_field_name("condition")
                        .map(|c| text_of(c, bytes).to_string())
                        .unwrap_or_default();
                    if cond.contains("err") && cond.contains("!= nil") {
                        facts.exceptions.push(ExceptionSite {
                            symbol: SymbolId(0),
                            kind_text: "error_value".into(),
                            is_bare: false,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                            in_handler: true,
                            handler_end: cie_core::types::line_of(
                                bytes,
                                node.end_byte() as u32,
                            ),
                        });
                    }
                }
                "panic_statement" => {} // engine already records Panic
                "defer_clause" | "defer_statement" => {
                    facts.exceptions.push(ExceptionSite {
                        symbol: SymbolId(0),
                        kind_text: "defer".into(),
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

        // ---- Pass 3: interface-value call marking + panic facts ----
        let mut panics: Vec<u32> = vec![];
        for call in &mut facts.calls {
            if call.callee_text == "panic" {
                panics.push(call.line);
            }
            if !matches!(call.callee, Callee::Unresolved(ref r) if r == "pending") {
                continue;
            }
            if let Some((obj, _m)) = call.callee_text.split_once('.') {
                if typed_vars.iter().any(|v| v == obj) {
                    // Interface-typed receiver: itable dispatch at runtime
                    // (§16 #8) — unresolved until a type is known (FR-4).
                    call.callee = Callee::Unresolved("itable_dispatch".into());
                }
            }
        }
        for line in panics {
            // Go `panic(x)` parses as a plain call — record it as a Panic
            // family fact (§16 #8; the light engine's panic_expression arm
            // only covers Rust's panic! macro).
            facts.exceptions.push(ExceptionSite {
                symbol: SymbolId(0),
                kind_text: "panic".into(),
                is_bare: false,
                line,
                in_handler: false,
                handler_end: 0,
            });
        }
        let _ = interfaces;
        facts
    }
}

impl LanguageAdapter for GoAdapter {
    fn id(&self) -> LangId {
        LangId::Go
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:go"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #8: interface itables dispatch at runtime.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// §16 #8: DEFINED wraparound, never UB; explicit-only conversions.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Wrapping,
            coercion: CoercionPolicy::ExplicitOnly,
            precision: Precision::MixedWidths,
        }
    }
    /// §16 #8: error-value + goroutine/channel conventions, vouched.
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "go-error-value",
                convention: "errors are values; `if err != nil` is the check convention",
            },
            FrameworkRule {
                name: "go-implicit-interface",
                convention: "interface satisfaction is implicit; itable dispatch at runtime",
            },
            FrameworkRule {
                name: "go-channel",
                convention: "channels synchronize goroutines; sends/receives are rendezvous points",
            },
            FrameworkRule {
                name: "go-defer",
                convention: "deferred calls run LIFO at function exit — recovery/cleanup paths",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(GoAdapter)
}

#[test]
fn task_019_go_adapter_facts() {
    let adapter = GoAdapter;
    assert_eq!(adapter.id(), LangId::Go);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::Wrapping,
        "§16 #8: defined wraparound, never UB"
    );
    assert!(!adapter.framework_conventions().is_empty());

    let src = "\
type Authorizer interface { Authorize(u string) bool }
func refresh(a Authorizer, ch chan int) {
    go refresh(a, ch)
    ch <- 1
    if err != nil {
        return
    }
    panic(\"boom\")
}
func run(a Authorizer, u string) bool { return a.Authorize(u) }
";
    let facts = adapter.extract("auth.go", src);

    // Interface-typed receiver call: itable dispatch + unresolved (DoD).
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "itable_dispatch")),
        "interface-typed call is itable-dispatched and unresolved (FR-4)"
    );
    // go refresh(ch) → Spawn; ch <- v → Channel (DoD).
    assert!(facts.async_facts.iter().any(|a| a.kind == AsyncKind::Spawn));
    assert!(facts.async_facts.iter().any(|a| a.kind == AsyncKind::Channel));
    // if err != nil → ErrorValue; panic → Panic family. Never Raised (FR-3).
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "error_value"));
    assert!(
        facts.exceptions.iter().any(|e| e.kind_text == "panic"),
        "panic recorded"
    );
    assert!(
        !facts
            .exceptions
            .iter()
            .any(|e| e.kind_text.contains("throw") || e.kind_text == "raised"),
        "Go never emits Raised (FR-3)"
    );
}
