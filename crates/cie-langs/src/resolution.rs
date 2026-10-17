//! Cross-file call resolution (PRD §12: "build cross-file relations").
//!
//! Replaces `Callee::Unresolved("pending")` placeholders with either
//! `Resolved(SymbolId)` or a final, explicit unresolved reason:
//! - `same_file` / `unique_global` / `imported` / `self_method` /
//!   `typed_variable` / `class_member` — resolved variants
//! - `unresolved_name` / `dynamic_attribute` / `external_module` —
//!   recorded in evidence (acceptance #6)

use cie_core::facts::{Callee, ImportFact, LangFacts, RhsKind, SymbolKind, SymbolStore};
use cie_core::types::SymbolId;
use std::collections::BTreeMap;

pub fn resolve_project(store: &SymbolStore, per_file: &mut [LangFacts]) {
    // Builtin shadowing: a project-defined function shadows same-named
    // library/builtin classification (e.g. `verify_password` defined in the
    // project must resolve to it even though the name looks crypto-ish).
    let shadow: BTreeMap<String, SymbolId> = store
        .symbols
        .iter()
        .filter(|s| s.is_function_like())
        .map(|s| (s.name.clone(), s.id))
        .collect();

    for facts in per_file.iter_mut() {
        for call in &mut facts.calls {
            if let Callee::Builtin(_) = call.callee {
                if !call.callee_text.contains('.') {
                    if let Some(id) = shadow.get(&call.callee_text) {
                        call.callee = Callee::Resolved(*id);
                    }
                }
            }
        }
    }

    let imports: BTreeMap<String, Vec<ImportFact>> = per_file
        .iter()
        .map(|f| (f.path.clone(), f.imports.clone()))
        .collect();

    for facts in per_file.iter_mut() {
        let path_imports = imports.get(&facts.path).cloned().unwrap_or_default();
        let caller_path = facts.path.clone();
        // Phase 1: resolve against immutable view; Phase 2: apply mutations.
        let mut resolved: Vec<Callee> = Vec::with_capacity(facts.calls.len());
        for call in &facts.calls {
            if matches!(call.callee, Callee::Unresolved(ref r) if r == "pending") {
                resolved.push(resolve_one(
                    store,
                    &path_imports,
                    &caller_path,
                    call.caller,
                    &call.callee_text,
                    facts,
                ));
            } else {
                resolved.push(call.callee.clone());
            }
        }
        for (call, r) in facts.calls.iter_mut().zip(resolved) {
            call.callee = r;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    store: &SymbolStore,
    imports: &[ImportFact],
    path: &str,
    caller: SymbolId,
    callee_text: &str,
    facts: &LangFacts,
) -> Callee {
    let segments: Vec<&str> = callee_text.split('.').collect();

    // Single-segment bare call.
    if segments.len() == 1 {
        let name = segments[0];
        // 1. same-file function whose parent is the module (or none)
        let same_file = store
            .in_path(path)
            .iter()
            .filter_map(|id| store.get(*id))
            .find(|s| s.name == name && s.id != caller && matches!(s.kind, SymbolKind::Function | SymbolKind::Test));
        if let Some(s) = same_file {
            return Callee::Resolved(s.id);
        }
        // 2. enclosing-class method (implicit self call)
        if let Some(caller_sym) = store.get(caller) {
            if let Some(cls) = caller_sym.parent {
                let method = store
                    .children_of(cls)
                    .iter()
                    .filter_map(|id| store.get(*id))
                    .find(|s| s.name == name);
                if let Some(m) = method {
                    return Callee::Resolved(m.id);
                }
            }
        }
        // 3. imported name: `from mod import name`
        for imp in imports {
            if imp.names.iter().any(|n| n == name) {
                if let Some(id) = store.resolve_unique_name(name) {
                    return Callee::Resolved(id);
                }
            }
        }
        // 4. unique global match
        if let Some(id) = store.resolve_unique_name(name) {
            return Callee::Resolved(id);
        }
        // 5. module imported by name: `import mod` then `mod()`? rare -> unresolved
        return Callee::Unresolved("unresolved_name".into());
    }

    // self.method / cls.method
    if segments.len() >= 2 && (segments[0] == "self" || segments[0] == "cls") {
        if let Some(caller_sym) = store.get(caller) {
            if let Some(cls) = caller_sym.parent {
                let method = store
                    .children_of(cls)
                    .iter()
                    .filter_map(|id| store.get(*id))
                    .find(|s| s.name == segments[1]);
                if let Some(m) = method {
                    return Callee::Resolved(m.id);
                }
            }
        }
        return Callee::Unresolved("dynamic_attribute".into());
    }

    // var.method — try typed variable: `var = ClassName()`
    if segments.len() == 2 {
        let var = segments[0];
        let method = segments[1];
        // find same-function assignment var = ClassName()
        let typed_class = facts
            .assignments
            .iter()
            .filter(|a| a.symbol == caller && a.var == var && a.rhs_kind == RhsKind::Call)
            .filter_map(|a| a.rhs_data.first().cloned())
            .find_map(|class_name| {
                store
                    .in_path(path)
                    .iter()
                    .filter_map(|id| store.get(*id))
                    .find(|s| s.name == class_name && s.kind.is_type_like())
                    .map(|s| s.id)
            });
        if let Some(cls) = typed_class {
            let m = store
                .children_of(cls)
                .iter()
                .filter_map(|id| store.get(*id))
                .find(|s| s.name == method);
            if let Some(m) = m {
                return Callee::Resolved(m.id);
            }
        }
        // seg0 is a class in this file -> classmethod / static
        let class_in_file = store
            .in_path(path)
            .iter()
            .filter_map(|id| store.get(*id))
            .find(|s| s.name == var && s.kind.is_type_like());
        if let Some(cls) = class_in_file {
            let m = store
                .children_of(cls.id)
                .iter()
                .filter_map(|id| store.get(*id))
                .find(|s| s.name == method);
            if let Some(m) = m {
                return Callee::Resolved(m.id);
            }
        }
        // seg0 is an imported module -> cross-file: try "mod.method" / "a.b.method"
        for imp in imports {
            let module_alias = imp.alias.clone().unwrap_or_else(|| {
                imp.module.rsplit('.').next().unwrap_or(&imp.module).to_string()
            });
            if module_alias == var {
                let qual = format!("{}.{}", imp.module, method);
                if let Some(id) = store.resolve_qual(&qual) {
                    return Callee::Resolved(id);
                }
                // from-imported names inside that module
                if let Some(id) = store.resolve_unique_name(method) {
                    return Callee::Resolved(id);
                }
                return Callee::Unresolved("external_module".into());
            }
        }
        // unique global qual "var.method"
        if let Some(id) = store.resolve_qual(callee_text) {
            return Callee::Resolved(id);
        }
        return Callee::Unresolved("dynamic_attribute".into());
    }

    // deeper paths: try qualified lookup then give up explicitly
    if let Some(id) = store.resolve_qual(callee_text) {
        return Callee::Resolved(id);
    }
    Callee::Unresolved("dynamic_attribute".into())
}
