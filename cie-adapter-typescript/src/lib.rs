//! TypeScript deep adapter (TASK-014, §16 #3): structural typing, generics,
//! decorators — with runtime type erasure making interface/virtual calls
//! dynamic at runtime.
//!
//! Built on the shared light engine plus TS-specific deep passes: interface
//! symbols, type-erasure dispatch marking, decorator/`implements`/
//! declaration-merging framework rules, and `any`/type-assertion/
//! `@ts-ignore` evidence.
//!
//! Semantics tables (§16 #3, vouched here — FR-3, re-declared in THIS crate
//! per the crate-boundary rule; no table sharing with the JS adapter):
//! - dispatch: `Hybrid` — static types exist, but erasure means
//!   interface/virtual calls resolve at runtime
//! - numeric: f64 doubles plus `BigInt` as a fact-worthy distinct type
//! - exceptions/async: same model as §16 #2, re-vouched here
//! - conventions: decorators, `implements`, declaration merging

use cie_adapters::light_engine::LightLang;
use cie_adapters::{
    CoercionPolicy, DispatchKind, FrameworkRule, LanguageAdapter, NumericSemantics,
    OverflowPolicy, Precision,
};
use cie_semantic::facts::{Callee, ExceptionSite, LangFacts, SymbolFact, SymbolKind};
use cie_semantic::LangId;
use cie_core::types::{SourceLocation, SymbolId};
use tree_sitter::Parser;

pub struct TypeScriptAdapter;

fn text_of<'a>(node: tree_sitter::Node<'a>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

/// §16.3: interface-typed or computed dispatch resolves at runtime after
/// erasure — vouched unresolved, never guessed (FR-4).
fn mark_ts_dispatch(
    callee_text: &str,
    typed_vars: &[String],
) -> Option<&'static str> {
    let t = callee_text.trim();
    if t.starts_with("this.") || t.starts_with("super.") {
        return Some("this_binding");
    }
    if t.contains('[') {
        return Some("dynamic_property");
    }
    if let Some((obj, _method)) = t.split_once('.') {
        if typed_vars.iter().any(|v| v == obj) {
            // Call through an interface-typed value: Virtual + unresolved —
            // the runtime implementor is unknown after erasure (FR-4).
            return Some("type_erasure");
        }
        if !obj.is_empty() {
            return Some("property_access");
        }
    }
    None
}

impl TypeScriptAdapter {
    /// Deep extraction: light-engine base + TS deep passes (§16.3).
    pub fn extract_deep(&self, path: &str, src: &str) -> LangFacts {
        let mut parser = Parser::new();
        parser
            .set_language(&self.grammar(path).expect("typescript grammar"))
            .expect("typescript grammar");
        let Some(tree) = parser.parse(src, None) else {
            return LangFacts::new(path, LangId::TypeScript, "parse-aborted");
        };
        self.extract_tree(path, src, &tree)
    }

    fn extract_shared(&self, path: &str, src: &str, tree: &tree_sitter::Tree) -> LangFacts {
        let lang = if path.ends_with(".tsx") {
            LightLang::Tsx
        } else {
            LightLang::TypeScript
        };
        let mut facts = cie_adapters::light_engine::extract_light_tree(path, src, lang, tree);
        let bytes = src.as_bytes();

        let root = tree.root_node();

        // ---- Pass 1: interface symbols + typed-variable bindings ----
        let mut interfaces: Vec<String> = vec![];
        let mut typed_vars: Vec<String> = vec![];
        let mut stack = vec![root];
        let mut next_extra = SymbolId(0);
        while let Some(node) = stack.pop() {
            match node.kind() {
                "interface_declaration" => {
                    let mut c = node.walk();
                    let name = node
                        .children(&mut c)
                        .find(|ch| ch.kind() == "type_identifier")
                        .map(|ch| text_of(ch, bytes).to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        interfaces.push(name.clone());
                        // Interfaces are first-class symbols (§16 #3 DoD).
                        if let Some(existing) = facts
                            .symbols
                            .iter_mut()
                            .find(|s| s.name == name && s.kind == SymbolKind::Class)
                        {
                            existing.kind = SymbolKind::Interface;
                        } else {
                            facts.symbols.push(SymbolFact {
                                id: SymbolId(0), // remapped by extract_project
                                kind: SymbolKind::Interface,
                                name: name.clone(),
                                qual_name: name.clone(),
                                loc: SourceLocation::span_bytes(
                                    cie_core::types::FileId(0),
                                    node.start_byte() as u32,
                                    node.end_byte() as u32,
                                ),
                                lang: LangId::TypeScript,
                                parent: None,
                                rev: 0,
                                doc: None,
                                // This fallback path only fires when the
                                // light engine's own pass (which already
                                // records decorators, see light_engine.rs)
                                // didn't already create this symbol as a
                                // class — a rare edge case, not the common
                                // interface-with-decorator path.
                                annotations: vec![],
                            });
                            next_extra.0 += 1;
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

        // ---- Pass 1b: typed-variable bindings (after interfaces known) ----
        let mut stack1b = vec![root];
        while let Some(node) = stack1b.pop() {
            if matches!(
                node.kind(),
                "required_parameter" | "public_field_definition" | "variable_declarator"
            ) {
                // `a: Authorizer`, `const x: Authorizer = ...` — bind the
                // variable name to its interface type (erasure marking).
                let txt = text_of(node, bytes);
                if let Some(colon) = txt.find(':') {
                    let (lhs, rhs) = txt.split_at(colon);
                    let ty = rhs[1..].trim();
                    let ty_name = ty
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    if interfaces.iter().any(|i| i == ty_name) {
                        let var = lhs
                            .trim()
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .last()
                            .unwrap_or("");
                        if !var.is_empty() && !typed_vars.iter().any(|v| v == var) {
                            typed_vars.push(var.to_string());
                        }
                    }
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack1b.push(child);
            }
        }

        // ---- Pass 2: try/catch pairing (re-declared per crate boundary) ----
        let mut handlers: Vec<(u32, u32)> = vec![];
        let mut stack2 = vec![root];
        while let Some(node) = stack2.pop() {
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
                stack2.push(child);
            }
        }
        let _ = next_extra;

        // ---- Pass 3: await rejections + TS evidence ----
        let mut stack3 = vec![root];
        while let Some(node) = stack3.pop() {
            match node.kind() {
                "await_expression" => {
                    facts.exceptions.push(ExceptionSite {
                        symbol: SymbolId(0),
                        kind_text: "await_rejection".into(),
                        is_bare: false,
                        line: cie_core::types::line_of(bytes, node.start_byte() as u32),
                        in_handler: false,
                        handler_end: 0,
                    });
                }
                "expression_statement" => {
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
                // `any`, type assertions, @ts-ignore — assumptions in
                // evidence (FR-4), never silently dropped.
                "type_annotation" => {
                    let txt = text_of(node, bytes);
                    if txt.contains("any") {
                        facts.unsupported.push(format!(
                            "`any` annotation at line {} — type information erased by vouch",
                            cie_core::types::line_of(bytes, node.start_byte() as u32)
                        ));
                    }
                }
                "as_expression" | "type_assertion" => {
                    facts.unsupported.push(format!(
                        "type assertion at line {} — compile-time override, runtime unchecked (FR-4)",
                        cie_core::types::line_of(bytes, node.start_byte() as u32)
                    ));
                }
                _ => {
                    if node.kind() == "comment"
                        && text_of(node, bytes).contains("@ts-ignore")
                    {
                        facts.unsupported.push(format!(
                            "@ts-ignore at line {} — checking explicitly disabled (FR-4)",
                            cie_core::types::line_of(bytes, node.start_byte() as u32)
                        ));
                    }
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                stack3.push(child);
            }
        }

        // ---- Pass 3b: handler pairing after all exception sources exist ----
        for exc in &mut facts.exceptions {
            for (body_end, handler_end) in &handlers {
                if exc.line <= *body_end {
                    exc.in_handler = true;
                    exc.handler_end = *handler_end;
                    break;
                }
            }
        }

        // ---- Pass 4: dispatch marking ----
        for call in &mut facts.calls {
            if !matches!(call.callee, Callee::Unresolved(ref r) if r == "pending") {
                continue;
            }
            if let Some(reason) = mark_ts_dispatch(&call.callee_text, &typed_vars) {
                call.callee = Callee::Unresolved(reason.to_string());
            }
        }
        facts
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn grammar(&self, path: &str) -> Option<tree_sitter::Language> {
        let _ = path;
        Some(if path.ends_with(".tsx") { tree_sitter_typescript::LANGUAGE_TSX.into() } else { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() })
    }
    fn extract_tree(&self, path: &str, src: &str, tree: &tree_sitter::Tree) -> LangFacts {
        self.extract_shared(path, src, tree)
    }

    fn id(&self) -> LangId {
        LangId::TypeScript
    }
    /// §16 error model row (TASK-022): vouched by this adapter.
    fn error_model(&self) -> cie_adapters::ErrorModel {
        cie_adapters::ErrorModel::Raise
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx"]
    }
    fn analyzer(&self) -> &'static str {
        "tree-sitter-deep:typescript"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_deep(path, src)
    }
    /// §16 #3: static types exist but are erased at runtime ⇒ Hybrid.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Hybrid
    }
    /// §16 #3: f64 doubles plus `BigInt` as a distinct fact-worthy type;
    /// implicit numeric coercion as in JS.
    fn numeric_semantics(&self) -> NumericSemantics {
        NumericSemantics {
            overflow: OverflowPolicy::Unspecified,
            coercion: CoercionPolicy::ImplicitNumeric,
            precision: Precision::Binary64,
        }
    }
    /// §16 #3: decorators, `implements`, declaration merging — vouched data.
    fn framework_conventions(&self) -> &'static [FrameworkRule] {
        &[
            FrameworkRule {
                name: "ts-decorator",
                convention: "class/method/parameter decorators inject behavior at runtime",
            },
            FrameworkRule {
                name: "ts-implements",
                convention: "`implements` vouches structural conformance; dispatch stays runtime",
            },
            FrameworkRule {
                name: "ts-declaration-merging",
                convention: "same-named interface/namespace declarations merge across scopes",
            },
        ]
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(TypeScriptAdapter)
}

#[test]
fn task_014_typescript_adapter_facts() {
    let adapter = TypeScriptAdapter;
    assert_eq!(adapter.id(), LangId::TypeScript);
    assert_eq!(
        adapter.dispatch_kind(),
        DispatchKind::Hybrid,
        "§16 #3: types erased at runtime"
    );
    assert!(
        !adapter.framework_conventions().is_empty()
            && adapter
                .framework_conventions()
                .iter()
                .any(|r| r.name == "ts-decorator"),
        "decorators are framework rules"
    );

    let src = "\
interface Authorizer { authorize(u: string): boolean; }
function login(a: Authorizer, u: string): boolean { return a.authorize(u); }
class SsoAuth implements Authorizer { authorize(u: string) { return true; } }
const x: any = fetch('/x').then(r => r.json());
async function h() { try { return await x; } catch (e) { return false; } }
";
    let facts = adapter.extract("app.ts", src);

    // Interfaces are symbols (§16 #3 DoD).
    assert!(
        facts.symbols.iter().any(|s| s.kind == SymbolKind::Interface && s.name == "Authorizer"),
        "interfaces are Interface symbols"
    );
    // Call via an interface-typed parameter: Virtual + unresolved.
    assert!(
        facts.calls.iter().any(|c| {
            matches!(c.callee, Callee::Unresolved(ref r) if r == "type_erasure")
        }),
        "interface-typed call is unresolved after erasure (FR-4)"
    );
    // `any` lands in unsupported evidence.
    assert!(
        facts.unsupported.iter().any(|u| u.contains("any")),
        "`any` recorded as evidence"
    );
    // Await + try/catch pairing re-declared in this crate.
    assert!(
        facts
            .exceptions
            .iter()
            .any(|e| e.kind_text == "await_rejection" && e.in_handler),
        "await rejection paired with its handler"
    );
}
