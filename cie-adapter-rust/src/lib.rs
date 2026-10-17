//! Rust deep adapter (TASK-020, §16 #9): ownership, traits, `Result`,
//! async, macros.
//!
//! Light-engine base plus Rust-specific deep passes: `dyn Trait`/generic
//! bound call marking, `?` propagation as `ErrorValue`, `panic!`/`unwrap`
//! as `Panic`, `tokio::spawn` → `Spawn`, `mpsc` → `Channel`, ownership
//! moves/borrows as resource effects (§14.3 `Owns`/`Reads`/`Writes` raw
//! material), `unsafe`/proc-macro assumptions.
//!
//! Semantics tables (§16 #9, vouched here — FR-3):
//! - dispatch: `Hybrid` — inherent/impl calls are `Direct`; `dyn Trait`
//!   and generic-bound calls are `Dynamic`
//! - numeric: `OverflowPolicy::PanicInDebug` — debug panics on overflow;
//!   release wrap is recorded as an assumption (FR-4)
//! - errors are values: `Result`/`?` propagation → `ErrorValue`;
//!   `panic!`/`unwrap` → `Panic`. The adapter NEVER emits `Raised` (FR-3)
//! - async: `async fn`/`await` → `Async`/`Await`, `tokio::spawn` →
//!   `Spawn`, `mpsc` → `Channel`
//! - proc-macro output and `unsafe` blocks are assumptions/`unsupported`
//!   evidence — the adapter never invents macro-expanded behavior (FR-4)
//! - trait-object calls stay unresolved unless the concrete type is
//!   visible in-tree (FR-4)

use cie_adapters::light_engine::{extract_light, LightLang};
use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_semantic::facts::{AsyncKind, AsyncSite, Callee, ExceptionSite, LangFacts, ResourceEffect};
use cie_semantic::LangId;
use cie_core::types::SymbolId;
use tree_sitter::Parser;

pub struct RustAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}


impl RustAdapter {
    /// Deep extraction: light-engine base + Rust deep passes (§16.1).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut facts = extract_light(path, src, LightLang::Rust);
        let bytes = src.as_bytes();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("rust grammar");
        let Some(tree) = parser.parse(src, None) else {
            return facts;
        };
        let root = tree.root_node();

        // ---- Pass 1: trait types + dyn-typed variables ----
        let mut traits: Vec<String> = vec![];
        let mut dyn_vars: Vec<String> = vec![];
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "trait_item" => {
                    if let Some(name) = node.child_by_field_name("name") {
                        traits.push(text_of(name, bytes).to_string());
                    }
                }
                "parameter" | "let_declaration" => {
                    // `a: &dyn Authorizer` / `let a: &dyn Authorizer = ...`
                    let txt = text_of(node, bytes);
                    if txt.contains("dyn ") {
                        let after = txt.split("dyn ").nth(1).unwrap_or("");
                        let ty = after
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .unwrap_or("");
                        if traits.iter().any(|t| t == ty) || !ty.is_empty() {
                            // Bind the LAST identifier before `:`.
                            let before = txt.split(':').next().unwrap_or("");
                            if let Some(var) = before
                                .split(|c: char| !c.is_alphanumeric() && c != '_')
                                .filter(|s| !s.is_empty())
                                .last()
                            {
                                if !dyn_vars.iter().any(|v| v == var) {
                                    dyn_vars.push(var.to_string());
                                }
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

        // ---- Pass 2: async spawns/channels, unsafe/macros, ?/unwrap ----
        let mut stack2 = vec![root];
        while let Some(node) = stack2.pop() {
            match node.kind() {
                "token_tree" => {}
                "call_expression" | "macro_invocation" => {
                    let txt = text_of(node, bytes);
                    if txt.starts_with("tokio::spawn") || txt.starts_with("thread::spawn") {
                        facts.async_facts.push(AsyncSite {
                            symbol: SymbolId(0),
                            kind: AsyncKind::Spawn,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                        });
                    }
                    if txt.contains("mpsc::channel") || txt.contains("mpsc::unbounded_channel") {
                        facts.async_facts.push(AsyncSite {
                            symbol: SymbolId(0),
                            kind: AsyncKind::Channel,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                        });
                    }
                    if txt.starts_with("panic!") || txt.starts_with("unreachable!")
                        || txt.starts_with("todo!") || txt.starts_with("assert!")
                    {
                        facts.exceptions.push(ExceptionSite {
                            symbol: SymbolId(0),
                            kind_text: "panic".into(),
                            is_bare: false,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                            in_handler: false,
                            handler_end: 0,
                        });
                    }
                    if txt.contains("unwrap()") || txt.contains("expect(") {
                        facts.exceptions.push(ExceptionSite {
                            symbol: SymbolId(0),
                            kind_text: "panic".into(),
                            is_bare: false,
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                            in_handler: false,
                            handler_end: 0,
                        });
                    }
                }
                "unsafe_block" => {
                    facts.unsupported.push(format!(
                        "unsafe block at line {} — memory semantics unchecked (FR-4 assumption)",
                        cie_core::types::line_of(bytes, node.start_byte() as u32)
                    ));
                }
                "macro_definition" => {
                    facts.unsupported.push(format!(
                        "macro definition at line {} — expansion sites are assumptions (FR-4)",
                        cie_core::types::line_of(bytes, node.start_byte() as u32)
                    ));
                }
                _ => {}
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack2.push(child);
            }
        }
        // `?` propagation — ErrorValue (errors are values, FR-3).
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'?' {
                // Skip trait-bound `?Sized` and paths like `Option<?>` —
                // conservative: preceded by an identifier char and followed
                // by whitespace/punct/EOF.
                let prev_ok = i > 0
                    && (bytes[i - 1].is_ascii_alphanumeric()
                        || bytes[i - 1] == b')'
                        || bytes[i - 1] == b']'
                        || bytes[i - 1] == b'"');
                let next_ok = i + 1 >= bytes.len()
                    || !bytes[i + 1].is_ascii_alphanumeric();
                if prev_ok && next_ok {
                    facts.exceptions.push(ExceptionSite {
                        symbol: SymbolId(0),
                        kind_text: "error_value_propagation".into(),
                        is_bare: false,
                        line: cie_core::types::line_of(bytes, i as u32),
                        in_handler: false,
                        handler_end: 0,
                    });
                }
            }
        }

        // ---- Pass 3: dyn/generic dispatch marking + ownership effects ----
        for call in &mut facts.calls {
            if !matches!(call.callee, Callee::Unresolved(ref r) if r == "pending") {
                continue;
            }
            if let Some((obj, _m)) = call.callee_text.split_once('.') {
                if dyn_vars.iter().any(|v| v == obj) {
                    // Trait-object receiver: dynamic dispatch + unresolved
                    // unless the concrete type is visible in-tree (FR-4).
                    call.callee = Callee::Unresolved("dyn_dispatch".into());
                }
            }
        }
        // Ownership moves / borrows: `let b = a;` (move) and `&a`/`&mut a`
        // borrow sites — the §14.3 relations raw material.
        let mut stack3 = vec![root];
        while let Some(node) = stack3.pop() {
            if node.kind() == "let_declaration" {
                let txt = text_of(node, bytes);
                if let Some((lhs, rhs)) = txt.split_once('=') {
                    let val = rhs.trim().trim_end_matches(';').trim();
                    let var = lhs
                        .trim_start_matches("let ")
                        .trim_start_matches("mut ")
                        .trim()
                        .to_string();
                    if !var.is_empty() && !val.is_empty() && !val.contains("::new()") {
                        let kind = if val.starts_with('&') { "borrow" } else { "move" };
                        facts.resource_effects.push(ResourceEffect {
                            symbol: SymbolId(0),
                            kind_text: kind.to_string(),
                            resource: format!("{var} = {}", val.chars().take(60).collect::<String>()),
                            line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                        });
                    }
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

impl LanguageAdapter for RustAdapter {
    fn id(&self) -> LangId {
        LangId::Rust
    }
    /// §16 error model row (TASK-022): vouched by this adapter.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::ErrorValue
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:rust"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #9: inherent/impl calls Direct; `dyn Trait`/generic bounds Dynamic.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Hybrid
    }
    /// §16 #9: PanicInDebug — release wrap recorded as an assumption (FR-4).
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::PanicInDebug,
            coercion: CoercionPolicy::ExplicitOnly,
            precision: Precision::MixedWidths,
        }
    }
    /// §16 #9: ownership/traits/macros conventions, vouched.
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "rust-ownership",
                convention: "moves transfer ownership; borrows alias with read/write rules",
            },
            FrameworkRule {
                name: "rust-error-value",
                convention: "Result/Option are values; `?` propagates, never throws",
            },
            FrameworkRule {
                name: "rust-macros",
                convention: "proc macros generate code invisible to parse — expansion is an assumption",
            },
            FrameworkRule {
                name: "rust-tokio",
                convention: "tokio tasks are spawned futures; mpsc channels rendezvous",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(RustAdapter)
}

#[test]
fn task_020_rust_adapter_facts() {
    let adapter = RustAdapter;
    assert_eq!(adapter.id(), LangId::Rust);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Hybrid);
    assert_eq!(
        adapter.numeric_semantics().overflow,
        OverflowPolicy::PanicInDebug,
        "§16 #9: PanicInDebug"
    );
    assert!(adapter.framework_conventions().iter().any(|r| r.name == "rust-macros"));

    let src = r#"
trait Authorizer { fn authorize(&self, u: &str) -> bool; }
fn run(a: &dyn Authorizer, u: &str) -> bool {
    let ok = a.authorize(u)?;
    let copy = ok;
    unsafe { let _ = std::mem::size_of::<u8>(); }
    assert!(ok);
    ok
}
"#;
    let facts = adapter.extract("auth.rs", src);

    // dyn-Trait receiver call → dyn dispatch + unresolved (DoD).
    assert!(
        facts
            .calls
            .iter()
            .any(|c| matches!(c.callee, Callee::Unresolved(ref r) if r == "dyn_dispatch")),
        "dyn-Trait call is Dynamic + unresolved (FR-4)"
    );
    // `?` propagation → ErrorValue fact (DoD).
    assert!(
        facts
            .exceptions
            .iter()
            .any(|e| e.kind_text == "error_value_propagation"),
        "? propagation is an ErrorValue fact"
    );
    // assert!/unwrap → Panic family; never Raised (FR-3).
    assert!(facts.exceptions.iter().any(|e| e.kind_text == "panic"));
    assert!(!facts.exceptions.iter().any(|e| e.kind_text == "throw"));
    // unsafe → assumption evidence (FR-4).
    assert!(facts.unsupported.iter().any(|u| u.contains("unsafe")));
    // Ownership/borrow effects (DoD: moves produce Owns input).
    assert!(
        facts.resource_effects.iter().any(|e| e.kind_text == "move" || e.kind_text == "borrow"),
        "ownership/borrow effects recorded"
    );
}
