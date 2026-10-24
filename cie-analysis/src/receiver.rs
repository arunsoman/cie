//! Receiver-type resolution for call sites (behavioural-evidence redesign,
//! effects + dispatch layers).
//!
//! Turns the binding evidence adapters record (declared field/param/local
//! types, assigned right-hand sides, `self.X` attributes, type supers) into
//! a receiver classification for each call:
//!
//! * [`ReceiverClass::Db`] — the receiver's identity IS a database API,
//!   reached through the resolution ladder: declared binding → type-resolved
//!   API identity → imported-module identity → factory-derived handle.
//! * [`ReceiverClass::InMemory`] — the receiver is a known in-memory type
//!   (String/bytes/Matcher/List/…): a DB-shaped method name on it is a
//!   memory operation, never a database effect.
//! * [`ReceiverClass::Unknown`] — nothing in the tree types the receiver;
//!   claims are demoted to typed lexical hints rather than guessed.
//!
//! The classification is generic library knowledge (the same class of table
//! as the builtin spellings), never an application allowlist.

use cie_adapters::effects::{db_factory_rhs, is_db_module, is_in_memory_type};
use cie_core::types::SymbolId;
use cie_semantic::facts::{BindingFact, LangFacts, SymbolStore, TypeDeclFact};

/// How strongly (and through which rung) a receiver resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverClass {
    /// The receiver's identity IS a database API (ladder rung recorded).
    Db {
        identity: String,
        basis: &'static str,
    },
    /// A known in-memory type: DB-shaped method names are memory operations.
    InMemory {
        identity: String,
    },
    /// An in-tree type symbol (declaration resolvable, dispatch virtual).
    InTree {
        identity: String,
        symbol: SymbolId,
    },
    /// A statically-typed external surface (ResponseEntity, Logger, os.path…).
    ExternalType {
        identity: String,
    },
    /// Nothing in the tree types the receiver — claims demote to hints.
    Unknown,
}

impl ReceiverClass {
    pub fn is_db(&self) -> bool {
        matches!(self, ReceiverClass::Db { .. })
    }
    /// Short honest description for hint notes / records.
    pub fn describe(&self) -> String {
        match self {
            ReceiverClass::Db { identity, basis } => format!("{identity} ({basis})"),
            ReceiverClass::InMemory { identity } => format!("in-memory type {identity}"),
            ReceiverClass::InTree { identity, .. } => format!("in-tree type {identity}"),
            ReceiverClass::ExternalType { identity } => format!("external type {identity}"),
            ReceiverClass::Unknown => "no resolved type identity".to_string(),
        }
    }
}

/// supers text that names a database API (`extends JpaRepository<...>`).
fn is_db_supers(supers: &str) -> bool {
    let s = supers.to_ascii_lowercase();
    s.contains("repository")
        || s.contains("jdbctemplate")
        || s.contains("entitymanager")
        || s.contains("mongorepository")
        || s.contains("crudrepository")
        || s.contains("sqlsession")
}

fn local_type(store:&SymbolStore,facts:&LangFacts,text:&str)->Option<SymbolId>{
    let qualified=text.split('<').next().unwrap_or(text).trim();
    let simple=qualified.rsplit('.').next().unwrap_or(qualified);
    let candidates:Vec<_>=store.symbols.iter().filter(|s|s.kind.is_type_like()&&s.name==simple).collect();
    let same_file:Vec<_>=candidates.iter().filter(|s|store.files.path(s.loc.file)==facts.path).collect();
    if same_file.len()==1{return Some(same_file[0].id);}
    let imports:Vec<_>=facts.imports.iter().filter(|i|i.module.rsplit('.').next()==Some(simple)).map(|i|i.module.as_str()).collect();
    let imported:Vec<_>=candidates.iter().filter(|s|imports.iter().any(|q|store.files.path(s.loc.file).ends_with(&format!("{}.java",q.replace('.',"/"))))).collect();
    if imported.len()==1{return Some(imported[0].id);}
    if !imports.is_empty(){return None;}
    if qualified.contains('.') {
        return candidates.iter().find(|s|s.qual_name==qualified || store.files.path(s.loc.file).ends_with(&format!("{}.java",qualified.replace('.',"/")))).map(|s|s.id);
    }
    if candidates.len()==1{Some(candidates[0].id)}else{None}
}
fn known_db_type(name:&str)->bool{
    ["java.sql.","javax.sql.","jakarta.persistence.","javax.persistence.","org.springframework.jdbc.","org.springframework.data.","org.hibernate."].iter().any(|p|name.starts_with(p))
}

/// Classify a declared type text through the identity tables + in-tree supers.
fn classify_type(type_text: &str, facts: &LangFacts, store: &SymbolStore) -> ReceiverClass {
    let t = type_text.trim();
    if t.is_empty() {
        return ReceiverClass::Unknown;
    }
    if let Some(symbol)=local_type(store,facts,t){
        let identity=store.get(symbol).map(|s|s.qual_name.clone()).unwrap_or_else(||t.to_string());
        return ReceiverClass::InTree{identity,symbol};
    }
    if known_db_type(t) || facts.imports.iter().any(|i|i.module.rsplit('.').next()==Some(t)&&known_db_type(&i.module)) {
        return ReceiverClass::Db {
            identity: t.to_string(),
            basis: "type-resolved-api-identity",
        };
    }
    if is_in_memory_type(t) {
        return ReceiverClass::InMemory {
            identity: t.to_string(),
        };
    }
    // in-tree type whose own supers name a DB API, or any in-tree type
    let simple = t.rsplit('.').next().unwrap_or(t);
    let simple = simple.split('<').next().unwrap_or(simple).trim();
    if let Some(decl) = facts
        .type_decls
        .iter()
        .find(|d: &&TypeDeclFact| d.name == simple)
    {
        if is_db_supers(&decl.supers) {
            return ReceiverClass::Db {
                identity: t.to_string(),
                basis: "type-resolved-api-identity",
            };
        }
    }
    if t.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
        return ReceiverClass::ExternalType {
            identity: t.to_string(),
        };
    }
    ReceiverClass::Unknown
}

/// Ancestor class chain of a symbol (bounded), nearest first.
fn class_chain(store: &SymbolStore, sym: SymbolId) -> Vec<SymbolId> {
    let mut out = vec![];
    let mut cur = store.get(sym).and_then(|s| s.parent);
    let mut depth = 0;
    while let Some(p) = cur {
        if depth >= 8 {
            break;
        }
        out.push(p);
        cur = store.get(p).and_then(|s| s.parent);
        depth += 1;
    }
    out
}

fn binding_for<'a>(
    facts: &'a LangFacts,
    store: &SymbolStore,
    sym: SymbolId,
    name: &str,
) -> Option<&'a BindingFact> {
    // locals/params of the enclosing function shadow class fields.
    if let Some(b) = facts
        .bindings
        .iter()
        .find(|b| b.owner == sym && b.name == name)
    {
        return Some(b);
    }
    for class in class_chain(store, sym) {
        if let Some(b) = facts
            .bindings
            .iter()
            .find(|b| b.owner == class && b.name == name)
        {
            return Some(b);
        }
    }
    None
}

/// Interpret one binding into a receiver class (rhs chains recurse ≤ 2 hops).
fn interpret_binding(b: &BindingFact, facts: &LangFacts, store: &SymbolStore, depth: u32) -> ReceiverClass {
    if let Some(t) = &b.type_text {
        return classify_type(t, facts, store);
    }
    if let Some(rhs) = &b.rhs_text {
        if db_factory_rhs(rhs) {
            return ReceiverClass::Db {
                identity: format!("db-handle (factory: {rhs})"),
                basis: "factory-derived-db-handle",
            };
        }
        if depth > 0 {
            // `cursor = self.conn.cursor()` — the cursor is a DB handle when
            // its factory's own receiver resolves to a DB connection.
            if let Some(inner) = rhs.split('.').next() {
                let inner = inner.trim();
                if !inner.is_empty() {
                    if let Some(ib) = facts
                        .bindings
                        .iter()
                        .find(|b| b.name == inner)
                    {
                        let cls = interpret_binding(ib, facts, store, depth - 1);
                        if cls.is_db() {
                            return ReceiverClass::Db {
                                identity: format!("db-handle (factory: {rhs})"),
                                basis: "factory-derived-db-handle",
                            };
                        }
                    } else if is_db_module(inner) {
                        return ReceiverClass::Db {
                            identity: format!("db-handle (factory: {rhs})"),
                            basis: "imported-module-identity",
                        };
                    }
                }
            }
        }
    }
    ReceiverClass::Unknown
}

/// Resolve a receiver expression to its classification.
pub fn resolve_receiver(
    store: &SymbolStore,
    facts: &LangFacts,
    sym: SymbolId,
    receiver_expr: &str,
) -> ReceiverClass {
    let expr = receiver_expr.trim();
    if expr.is_empty() {
        return ReceiverClass::Unknown;
    }
    // `this.x` / `self.x` → the attribute name against the class scope.
    let attr = expr
        .trim_start_matches("this.")
        .trim_start_matches("self.")
        .trim();
    if attr != expr {
        if let Some(b) = binding_for(facts, store, sym, attr) {
            return interpret_binding(b, facts, store, 2);
        }
        return ReceiverClass::Unknown;
    }
    // plain name: binding lookup first
    if !expr.contains('.') {
        if let Some(b) = binding_for(facts, store, sym, expr) {
            return interpret_binding(b, facts, store, 2);
        }
        // an imported module receiver (`jaydebeapi.connect(...)`, `os.path.join`)
        if facts.imports.iter().any(|i| i.module == expr) {
            if is_db_module(expr) {
                return ReceiverClass::Db {
                    identity: expr.to_string(),
                    basis: "imported-module-identity",
                };
            }
            return ReceiverClass::ExternalType {
                identity: expr.to_string(),
            };
        }
        // an in-tree type name (`ResponseEntity.ok(...)` style statics resolve
        // through the symbol table when the type is declared in the tree)
        if let Some(sym) = local_type(store,facts,expr) {
            if let Some(sf) = store.get(sym) {
                if matches!(sf.kind, cie_semantic::facts::SymbolKind::Class | cie_semantic::facts::SymbolKind::Interface | cie_semantic::facts::SymbolKind::Enum | cie_semantic::facts::SymbolKind::Struct) {
                    return ReceiverClass::InTree {
                        identity: sf.qual_name.clone(),
                        symbol: sym,
                    };
                }
            }
        }
        // Uppercase-leading heads are type expressions (static calls); a
        // known in-memory/DB type name still classifies without a binding.
        if expr
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            return classify_type(expr, facts, store);
        }
        return ReceiverClass::Unknown;
    }
    // dotted receiver (`System.out`, `a.b.c`): classify by the head type name
    let head = expr.split('.').next().unwrap_or(expr);
    if !head.is_empty()
        && head
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
    {
        return classify_type(head, facts, store);
    }
    ReceiverClass::Unknown
}

/// Resolve local declarations before interpreting API effects. Inheritance
/// evidence belongs to the declaring file, not the caller's file.
pub fn resolve_receiver_project(store:&SymbolStore,facts:&LangFacts,all:&[LangFacts],sym:SymbolId,expr:&str)->ReceiverClass{
    let result=resolve_receiver(store,facts,sym,expr);
    if let ReceiverClass::InTree{ref identity,symbol}=result {
        if let Some(owner)=all.iter().find(|f|store.get(symbol).map(|s|store.files.path(s.loc.file)==f.path).unwrap_or(false)){
            if let Some(decl)=owner.type_decls.iter().find(|d|d.symbol==symbol){
                let names:Vec<_>=decl.supers.split(|c:char|!c.is_alphanumeric()&&c!='.'&&c!='_').filter(|s|!s.is_empty()).collect();
                if names.iter().any(|name|known_db_type(name)||owner.imports.iter().any(|i|i.module.rsplit('.').next()==Some(*name)&&known_db_type(&i.module))){
                    return ReceiverClass::Db{identity:identity.clone(),basis:"type-resolved-api-identity"};
                }
            }
        }
    }
    result
}
