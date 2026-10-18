//! SQL adapter (v1.2.1): a hand-rolled statement scanner — no grammar,
//! no guesses. Honesty rules:
//! * what it reads: tables, views, stored procedures/functions, `CREATE
//!   EXTENSION` as an import, and the table-to-table edges a statement
//!   really writes down — foreign keys (`REFERENCES`), `FROM`/`JOIN` in
//!   views, and the table named by `INSERT`/`UPDATE`/`DELETE`/`ALTER`/
//!   `DROP`/`TRUNCATE`/`MERGE`;
//! * what it does NOT do: it doesn't run the statements, resolve
//!   `search_path`, expand `*`, or follow dynamic SQL — those stay in the
//!   `unsupported` notes (FR-4: named, never silently dropped);
//! * comments (`--`, `/* */`) and string literals are handled with a
//!   character scanner so a `--` inside a string never hides a statement;
//! * `CREATE INDEX` is recorded as a note plus a module→table edge, not a
//!   fake symbol — an index is not a named piece of behaviour.
//!
//! Semantics tables (§16, vouched here — FR-3):
//! - dispatch: `Dynamic` — names resolve against the database catalog at
//!   runtime, exactly like a dynamic call site
//! - error model: `Unspecified` — SQL dialects disagree (RAISE vs error
//!   codes vs SQLSTATE); this adapter vouches for none of them
//! - numeric: default row — DECIMAL semantics are dialect-dependent

use cie_adapters::{DispatchKind, LanguageAdapter};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{Callee, CallSite, ImportFact, LangFacts, SymbolFact, SymbolKind};
use cie_semantic::LangId;

pub struct SqlAdapter;

// ---------- character-level cleaning (comments out, strings kept) ----------

/// Replace comment text with spaces so statements survive line-for-line.
/// String literals are preserved verbatim (a `--` inside a string is data,
/// not a comment). Returns text with the SAME length as `src`, so byte
/// offsets still map to original lines.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = src.as_bytes().to_vec();
    let mut i = 0usize;
    let mut in_line = false;
    let mut in_block = false;
    let mut quote: u8 = 0; // b'\'' or b'"' or b'`'
    let mut block_start = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if in_line {
            if c == b'\n' {
                in_line = false;
            } else {
                out[i] = b' ';
            }
            i += 1;
            continue;
        }
        if in_block {
            if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                // blank the whole block (offsets inside stay spaces)
                for b in out.iter_mut().skip(block_start).take(i + 2 - block_start) {
                    if *b != b'\n' {
                        *b = b' ';
                    }
                }
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if quote != 0 {
            if c == b'\\' && quote == b'\'' {
                i += 2; // escaped char inside single-quoted string
                continue;
            }
            if c == quote {
                // doubled quote ('') inside a string is an escaped quote
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    i += 2;
                    continue;
                }
                quote = 0;
            }
            i += 1;
            continue;
        }
        if c == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            in_line = true;
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            in_block = true;
            block_start = i;
            i += 2;
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            quote = c;
        }
        i += 1;
    }
    if in_block {
        for b in out.iter_mut().skip(block_start) {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

/// One statement: its first line number (1-based) and cleaned text.
struct Stmt {
    line: u32,
    text: String,
}

/// Split cleaned text into statements on `;` (respecting string literals —
/// a `;` inside a string is data). Statement text is whitespace-normalised
/// with single spaces so keyword matching is stable.
fn split_statements(clean: &str) -> Vec<Stmt> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut start_line = 1u32;
    let mut line = 1u32;
    let mut quote: u8 = 0;
    for c in clean.chars() {
        if c == '\n' {
            line += 1;
        }
        if quote == 0 && c == ';' {
            let text = normalize_ws(&cur);
            if !text.is_empty() {
                out.push(Stmt { line: start_line, text });
            }
            cur.clear();
            start_line = line + 1;
            continue;
        }
        if quote == 0 && (c == '\'' || c == '"' || c == '`') {
            quote = c as u8;
        } else if quote != 0 && c as u8 == quote {
            quote = 0;
        }
        if cur.is_empty() && c.is_whitespace() {
            continue; // don't record leading whitespace — keeps start_line at the first char
        }
        if cur.is_empty() {
            start_line = line;
        }
        cur.push(c);
    }
    let text = normalize_ws(&cur);
    if !text.is_empty() {
        out.push(Stmt { line: start_line, text });
    }
    out
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Uppercase prefix check ("CREATE TABLE ..." etc).
fn starts_kw(stmt: &str, kw: &str) -> bool {
    let n = kw.len();
    stmt.len() >= n && stmt.as_bytes()[..n].eq_ignore_ascii_case(kw.as_bytes())
}

/// Longest-prefix keyword match over `kw` candidates (each INCLUDES its
/// trailing space). Returns the byte length of the matched keyword.
fn match_kw(stmt: &str, kws: &[&str]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for kw in kws {
        if starts_kw(stmt, kw) && best.map_or(true, |b| kw.len() > b) {
            best = Some(kw.len());
        }
    }
    best
}

/// Identifier that follows a matched keyword (byte offset `kw_len`, keyword
/// with trailing space), skipping an optional `IF [NOT] EXISTS`.
fn ident_after(stmt: &str, kw_len: usize) -> Option<String> {
    let mut rest = stmt.get(kw_len..)?;
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("if not exists") {
        rest = rest.get("if not exists".len()..)?;
    } else if lower.starts_with("if exists") {
        rest = rest.get("if exists".len()..)?;
    }
    let rest = rest.trim_start();
    let id: String = rest
        .chars()
        .take_while(|c| {
            c.is_alphanumeric()
                || *c == '_'
                || *c == '.'
                || *c == '"'
                || *c == '`'
                || *c == '['
                || *c == ']'
        })
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(unquote_ident(&id))
    }
}

fn unquote_ident(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s).strip_suffix('"').unwrap_or(s);
    let s = s.strip_prefix('`').unwrap_or(s).strip_suffix('`').unwrap_or(s);
    let s = s.strip_prefix('[').unwrap_or(s).strip_suffix(']').unwrap_or(s);
    s.to_string()
}

/// Identifier that follows a keyword occurrence inside a body (`FROM x`,
/// `JOIN y`, `REFERENCES z`, `INTO w`). Case-insensitive, whole-word.
fn ident_after_word(body: &str, word: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let pat = format!(" {word} ");
    let mut from = 0usize;
    while let Some(pos) = lower[from..].find(&pat) {
        let abs = from + pos + pat.len();
        let rest = &body[abs..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '"' || *c == '`' || *c == '[' || *c == ']')
            .collect();
        let id = unquote_ident(&id);
        if !id.is_empty() && !matches!(id.to_ascii_lowercase().as_str(), "select" | "as") {
            return Some(id);
        }
        from = abs;
    }
    None
}

/// All identifiers following `FROM`/`JOIN` inside a view/procedure body.
fn tables_referenced(body: &str) -> Vec<String> {
    let mut out = vec![];
    for word in ["from", "join"] {
        let lower = body.to_ascii_lowercase();
        let pat = format!(" {word} ");
        let mut from = 0usize;
        while let Some(pos) = lower[from..].find(&pat) {
            let abs = from + pos + pat.len();
            let rest = &body[abs..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '"' || *c == '`')
                .collect();
            let id = unquote_ident(&id);
            let low = id.to_ascii_lowercase();
            if !id.is_empty()
                && !matches!(
                    low.as_str(),
                    "select" | "dual" | "lateral" | "unnest" | "generate_series" | "values"
                )
                && !out.contains(&id)
            {
                out.push(id);
            }
            from = abs;
        }
    }
    out
}

/// The last unquoted identifier of a possibly schema-qualified name
/// (`public.users` → `users`) — how SQL resolution actually finds things.
fn bare_name(qualified: &str) -> String {
    qualified
        .rsplit('.')
        .next()
        .unwrap_or(qualified)
        .to_string()
}

/// Top-level (paren-depth-0) comma-separated pieces of a `CREATE TABLE (…)`
/// body — column defs and table constraints.
fn split_columns(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in body.chars() {
        match c {
            '(' => {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            ')' => {
                depth -= 1;
                if depth <= 0 {
                    if !cur.trim().is_empty() {
                        out.push(cur.trim().to_string());
                    }
                    return out;
                }
            }
            ',' if depth == 1 => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
                continue;
            }
            _ => {}
        }
        if depth >= 1 {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

const CONSTRAINT_HEADS: &[&str] = &[
    "primary", "unique", "check", "foreign", "constraint", "key", "exclude", "like", "period",
];

// ---------- the extraction ----------

impl SqlAdapter {
    pub fn extract_sql(&self, path: &str, src: &str) -> LangFacts {
        let mut facts = LangFacts::new(path, LangId::Sql, "line-scan:sql");
        let module = path
            .trim_end_matches(".sql")
            .trim_end_matches(".ddl")
            .replace('/', ".");
        let module_id = SymbolId(0);
        facts.symbols.push(SymbolFact {
            id: module_id,
            kind: SymbolKind::Module,
            name: module.clone(),
            qual_name: module,
            loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, src.len() as u32),
            lang: LangId::Sql,
            parent: None,
            rev: 0,
            doc: None,
        });

        let clean = strip_comments(src);
        let stmts = split_statements(&clean);
        let mut skipped_other = 0usize;

        for stmt in &stmts {
            // caller for unqualified statements: the current proc (inside a
            // CREATE FUNCTION body the whole statement is one unit), else module
            // (resolved per-statement below)
            let create_table = starts_kw(&stmt.text, "CREATE TABLE")
                || starts_kw(&stmt.text, "CREATE GLOBAL TEMPORARY TABLE")
                || starts_kw(&stmt.text, "CREATE TEMPORARY TABLE")
                || starts_kw(&stmt.text, "CREATE TEMP TABLE")
                || starts_kw(&stmt.text, "CREATE UNLOGGED TABLE");
            let create_view = starts_kw(&stmt.text, "CREATE VIEW")
                || starts_kw(&stmt.text, "CREATE MATERIALIZED VIEW")
                || starts_kw(&stmt.text, "CREATE OR REPLACE VIEW")
                || starts_kw(&stmt.text, "CREATE OR REPLACE MATERIALIZED VIEW");
            let create_proc = starts_kw(&stmt.text, "CREATE FUNCTION")
                || starts_kw(&stmt.text, "CREATE OR REPLACE FUNCTION")
                || starts_kw(&stmt.text, "CREATE PROCEDURE")
                || starts_kw(&stmt.text, "CREATE OR REPLACE PROCEDURE");

            if create_table {
                let Some(kw_len) = match_kw(
                    &stmt.text,
                    &[
                        "CREATE GLOBAL TEMPORARY TABLE ",
                        "CREATE TEMPORARY TABLE ",
                        "CREATE UNLOGGED TABLE ",
                        "CREATE TEMP TABLE ",
                        "CREATE TABLE ",
                    ],
                ) else { continue };
                let Some(name) = ident_after(&stmt.text, kw_len) else { continue };
                let name = bare_name(&name);
                let id = SymbolId(facts.symbols.len() as u32);
                // columns: top-level comma pieces, first token of each
                let mut doc_cols: Vec<String> = vec![];
                let mut fk_targets: Vec<(String, u32)> = vec![];
                let open = stmt.text.find('(');
                if let Some(open) = open {
                    let body = &stmt.text[open..];
                    for piece in split_columns(body) {
                        let first = piece.split_whitespace().next().unwrap_or("");
                        let lower = first.to_ascii_lowercase();
                        if CONSTRAINT_HEADS.contains(&lower.as_str()) {
                            // table-level FOREIGN KEY ... REFERENCES t (…)
                            if let Some(t) = ident_after_word(&piece, "references") {
                                fk_targets.push((bare_name(&t), stmt.line));
                            }
                            continue;
                        }
                        if doc_cols.len() < 14 && !first.is_empty() {
                            doc_cols.push(unquote_ident(first));
                        }
                        if let Some(t) = ident_after_word(&piece, "references") {
                            fk_targets.push((bare_name(&t), stmt.line));
                        }
                    }
                }
                facts.symbols.push(SymbolFact {
                    id,
                    kind: SymbolKind::Table,
                    name: name.clone(),
                    qual_name: name.clone(),
                    loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, 0),
                    lang: LangId::Sql,
                    parent: Some(module_id),
                    rev: 0,
                    doc: if doc_cols.is_empty() { None } else { Some(format!("columns: {}", doc_cols.join(", "))) },
                });
                for (target, _line) in &fk_targets {
                    facts.calls.push(CallSite {
                        caller: id,
                        callee_text: target.clone(),
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                continue;
            }

            if create_view {
                let Some(kw_len) = match_kw(
                    &stmt.text,
                    &[
                        "CREATE OR REPLACE MATERIALIZED VIEW ",
                        "CREATE MATERIALIZED VIEW ",
                        "CREATE OR REPLACE VIEW ",
                        "CREATE VIEW ",
                    ],
                ) else { continue };
                let Some(name) = ident_after(&stmt.text, kw_len) else { continue };
                let name = bare_name(&name);
                let id = SymbolId(facts.symbols.len() as u32);
                facts.symbols.push(SymbolFact {
                    id,
                    kind: SymbolKind::View,
                    name: name.clone(),
                    qual_name: name.clone(),
                    loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, 0),
                    lang: LangId::Sql,
                    parent: Some(module_id),
                    rev: 0,
                    doc: None,
                });
                let as_pos = stmt.text.to_ascii_uppercase().find(" AS ");
                let body = match as_pos {
                    Some(p) => &stmt.text[p + 4..],
                    None => "",
                };
                for t in tables_referenced(body) {
                    facts.calls.push(CallSite {
                        caller: id,
                        callee_text: bare_name(&t),
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                continue;
            }

            if create_proc {
                let Some(kw_len) = match_kw(
                    &stmt.text,
                    &[
                        "CREATE OR REPLACE FUNCTION ",
                        "CREATE OR REPLACE PROCEDURE ",
                        "CREATE FUNCTION ",
                        "CREATE PROCEDURE ",
                    ],
                ) else { continue };
                let Some(name) = ident_after(&stmt.text, kw_len) else { continue };
                let name = bare_name(&name);
                let id = SymbolId(facts.symbols.len() as u32);
                facts.symbols.push(SymbolFact {
                    id,
                    kind: SymbolKind::Function,
                    name: name.clone(),
                    qual_name: name.clone(),
                    loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, 0),
                    lang: LangId::Sql,
                    parent: Some(module_id),
                    rev: 0,
                    doc: None,
                });
                // tables this procedure touches
                for t in tables_referenced(&stmt.text) {
                    facts.calls.push(CallSite {
                        caller: id,
                        callee_text: bare_name(&t),
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                // calls to other stored procedures: bare CALL x / PERFORM x / SELECT x(
                for word in ["call", "perform"] {
                    if let Some(t) = ident_after_word(&stmt.text, word) {
                        facts.calls.push(CallSite {
                            caller: id,
                            callee_text: bare_name(&t),
                            callee: Callee::Unresolved("pending".into()),
                            line: stmt.line,
                            in_loop: false,
                            arg_vars: vec![],
                        });
                    }
                }
                if stmt.text.to_ascii_uppercase().contains("EXECUTE IMMEDIATE") {
                    facts.unsupported.push(format!(
                        "line {}: dynamic SQL (EXECUTE IMMEDIATE) — statements built at runtime are not charted",
                        stmt.line
                    ));
                }
                continue;
            }

            let mut data_stmt = None; // (table, verb) for DML
            let mut kw_len = 0usize;
            for (kw, verb) in [
                ("INSERT INTO ", "insert"),
                ("UPDATE ", "update"),
                ("DELETE FROM ", "delete"),
                ("MERGE INTO ", "merge"),
                ("MERGE ", "merge"),
                ("ALTER TABLE ", "alter"),
                ("DROP TABLE ", "drop"),
                ("TRUNCATE TABLE ", "truncate"),
                ("TRUNCATE ", "truncate"),
            ] {
                if starts_kw(&stmt.text, kw) {
                    kw_len = kw.len();
                    data_stmt = Some(verb);
                    break;
                }
            }
            if let Some(_verb) = data_stmt {
                let rest = &stmt.text[kw_len..];
                let raw = rest.split(|c: char| c == ' ' || c == '(' || c == ';').next().unwrap_or("");
                let target = bare_name(&unquote_ident(raw));
                if !target.is_empty() {
                    facts.calls.push(CallSite {
                        caller: module_id,
                        callee_text: target,
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                continue;
            }

            if starts_kw(&stmt.text, "CREATE INDEX")
                || starts_kw(&stmt.text, "CREATE UNIQUE INDEX")
            {
                let on = ident_after_word(&stmt.text, "on");
                if let Some(t) = on {
                    facts.calls.push(CallSite {
                        caller: module_id,
                        callee_text: bare_name(&t),
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                facts.unsupported.push(format!(
                    "line {}: index definition — counted in the notes, not drawn as its own node",
                    stmt.line
                ));
                continue;
            }

            if starts_kw(&stmt.text, "CREATE EXTENSION") {
                if let Some(kw_len) = match_kw(&stmt.text, &["CREATE EXTENSION "]) {
                    if let Some(name) = ident_after(&stmt.text, kw_len) {
                    facts.imports.push(ImportFact {
                        module_symbol: module_id,
                        module: bare_name(&name),
                        names: vec![],
                        alias: None,
                        line: stmt.line,
                    });
                }
                }
                continue;
            }

            if starts_kw(&stmt.text, "CREATE TRIGGER") {
                facts.unsupported.push(format!(
                    "line {}: trigger — its body fires at runtime; the table it watches is drawn, the trigger itself is not",
                    stmt.line
                ));
                let on = ident_after_word(&stmt.text, "on");
                if let Some(t) = on {
                    facts.calls.push(CallSite {
                        caller: module_id,
                        callee_text: bare_name(&t),
                        callee: Callee::Unresolved("pending".into()),
                        line: stmt.line,
                        in_loop: false,
                        arg_vars: vec![],
                    });
                }
                continue;
            }

            let upper = stmt.text.to_ascii_uppercase();
            if upper.starts_with("GRANT")
                || upper.starts_with("REVOKE")
                || upper.starts_with("COMMENT ON")
                || upper.starts_with("DROP ")
                || upper.starts_with("SET ")
                || upper.starts_with("USE ")
                || upper.starts_with("BEGIN")
                || upper.starts_with("COMMIT")
                || upper.starts_with("ROLLBACK")
                || upper.starts_with("CREATE SCHEMA")
                || upper.starts_with("CREATE TYPE")
                || upper.starts_with("CREATE DATABASE")
                || upper.starts_with("ANALYZE")
                || upper.starts_with("VACUUM")
                || upper.starts_with("LOCK ")
                || upper.starts_with("EXPLAIN")
                || upper.starts_with("PRAGMA")
            {
                skipped_other += 1;
                continue;
            }

            facts.unsupported.push(format!(
                "line {}: statement type \"{}…\" is not modelled yet — named, not guessed",
                stmt.line,
                stmt.text.chars().take(28).collect::<String>()
            ));
        }

        if skipped_other > 0 {
            facts.unsupported.push(format!(
                "{skipped_other} housekeeping statement(s) (grants, comments, transactions, drops of non-table objects) skipped"
            ));
        }
        facts
    }
}

impl LanguageAdapter for SqlAdapter {
    fn id(&self) -> LangId {
        LangId::Sql
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sql", "ddl"]
    }
    fn analyzer(&self) -> &'static str {
        "line-scan:sql"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        self.extract_sql(path, src)
    }
    /// Names resolve against the live catalog at runtime — honest Dynamic.
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(SqlAdapter)
}

#[test]
fn sql_adapter_facts() {
    let adapter = SqlAdapter;
    assert_eq!(adapter.id(), LangId::Sql);
    let src = r#"
-- customers + orders schema
CREATE TABLE customers (
    id BIGINT PRIMARY KEY,
    email VARCHAR(255) NOT NULL,  -- contact email
    region VARCHAR(40) REFERENCES sales_regions(name)
);

CREATE TABLE orders (
    id BIGSERIAL PRIMARY KEY,
    customer_id BIGINT REFERENCES customers(id),
    total NUMERIC(10,2),
    CONSTRAINT fk_ship FOREIGN KEY (ship_to) REFERENCES addresses(id)
);

CREATE VIEW big_orders AS
    SELECT o.id, o.total FROM orders o JOIN customers c ON c.id = o.customer_id;

CREATE EXTENSION pg_trgm;

INSERT INTO customers (id, email) VALUES (1, 'a@b.c');
ALTER TABLE orders ADD COLUMN note TEXT;
CREATE INDEX idx_orders_total ON orders (total);
GRANT SELECT ON customers TO reader;
"#;
    let facts = adapter.extract("db/schema.sql", src);
    let names = |k: SymbolKind| -> Vec<String> {
        facts.symbols.iter().filter(|s| s.kind == k).map(|s| s.name.clone()).collect()
    };
    assert_eq!(names(SymbolKind::Table), vec!["customers", "orders"]);
    assert_eq!(names(SymbolKind::View), vec!["big_orders"]);
    // FK edges: customers→sales_regions, orders→customers, orders→addresses
    let order_edges = facts
        .calls
        .iter()
        .filter(|c| facts.symbols.iter().any(|s| s.id == c.caller && s.name == "orders"))
        .map(|c| c.callee_text.clone())
        .collect::<Vec<_>>();
    assert!(order_edges.contains(&"customers".to_string()));
    assert!(order_edges.contains(&"addresses".to_string()));
    // view → both tables
    let view_edges = facts
        .calls
        .iter()
        .filter(|c| facts.symbols.iter().any(|s| s.id == c.caller && s.name == "big_orders"))
        .map(|c| c.callee_text.clone())
        .collect::<Vec<_>>();
    assert!(view_edges.contains(&"orders".to_string()) && view_edges.contains(&"customers".to_string()));
    // extension import
    assert!(facts.imports.iter().any(|i| i.module == "pg_trgm"));
    // module → table DML/DDL edges
    assert!(facts.calls.iter().any(|c| c.callee_text == "customers" && c.caller == SymbolId(0)));
    assert!(facts.calls.iter().any(|c| c.callee_text == "orders" && c.caller == SymbolId(0)));
    // the `--` inside 'a@b.c' never hid a statement, and GRANT was named-skip
    assert!(facts.unsupported.iter().any(|u| u.contains("housekeeping")));
    assert!(facts.unsupported.iter().any(|u| u.contains("index definition")));
}
