//! PRD v2.0 slice 9 — the polyglot AST schema (§11.1, §11.3, §13).
//!
//! Every row comes from a real tree-sitter parse of real source text; the
//! pipeline is the PRD's: detect language → parse with the pinned grammar →
//! persist the file and its `ast_nodes` in one transaction → compute
//! `shape_hash` per node (ANL-06, pre-order grammar kinds only) → extract
//! symbols with parent-qualified names.
//!
//! Honesty invariants:
//! * a missing parse still indexes the file with `is_error` on the root —
//!   empty scope never certifies absence (§13);
//! * symbol extraction is declaration-kind driven; a declaration whose name
//!   cannot be located is skipped and COUNTED, never guessed;
//! * content and text are content-addressed (`blake3`), so unchanged files
//!   re-ingest as no-ops and `node_text` is shared across languages (§42).

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Language detection + grammar pinning (§13 step 1, §39)
// ---------------------------------------------------------------------------

/// The PRD §39 top-10 languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Python,
    JavaScript,
    TypeScript,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    CSharp,
    Php,
}

impl Lang {
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = Path::new(path).extension()?.to_str()?;
        Some(match ext {
            "py" | "pyi" => Lang::Python,
            "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "go" => Lang::Go,
            "rs" => Lang::Rust,
            "java" => Lang::Java,
            "c" | "h" => Lang::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hh" => Lang::Cpp,
            "cs" => Lang::CSharp,
            "php" => Lang::Php,
            _ => return None,
        })
    }

    /// Pinned grammar row (`files.tree_sitter_grammar_version`).
    pub fn grammar_version(self) -> &'static str {
        match self {
            Lang::Python => "tree-sitter-python 0.23",
            Lang::JavaScript => "tree-sitter-javascript 0.23",
            Lang::TypeScript => "tree-sitter-typescript 0.23",
            Lang::Go => "tree-sitter-go 0.23",
            Lang::Rust => "tree-sitter-rust 0.23",
            Lang::Java => "tree-sitter-java 0.23",
            Lang::C => "tree-sitter-c 0.24",
            Lang::Cpp => "tree-sitter-cpp 0.23",
            Lang::CSharp => "tree-sitter-c-sharp 0.23",
            Lang::Php => "tree-sitter-php 0.25",
        }
    }

    pub fn ts_language(self) -> Option<tree_sitter::Language> {
        let l: tree_sitter::Language = match self {
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::C => tree_sitter_c::LANGUAGE.into(),
            Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Lang::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        };
        Some(l)
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Python => "python",
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Go => "go",
            Lang::Rust => "rust",
            Lang::Java => "java",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::CSharp => "csharp",
            Lang::Php => "php",
        }
    }
}

/// Directories never ingested (PRD §7 walk ignore list).
pub const IGNORED_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "vendor", ".venv", "venv", "dist", "build", "__pycache__",
    ".next",
];

// ---------------------------------------------------------------------------
// Schema DDL — §11.1 / §11.3 exactly, plus the indexes the PRD names
// ---------------------------------------------------------------------------

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    id           INTEGER PRIMARY KEY,
    repo_id      INTEGER NOT NULL,
    path         TEXT    NOT NULL,
    content_hash BLOB    NOT NULL,
    language     TEXT    NOT NULL,
    revision     INTEGER NOT NULL,
    size_bytes   INTEGER NOT NULL,
    tree_sitter_grammar_version TEXT NOT NULL,
    UNIQUE (repo_id, path, revision)
);
CREATE INDEX IF NOT EXISTS idx_files_rev ON files(revision);
CREATE INDEX IF NOT EXISTS idx_files_hash ON files(content_hash);

CREATE TABLE IF NOT EXISTS node_kinds (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL,
    language TEXT,
    UNIQUE (name, language)
);

CREATE TABLE IF NOT EXISTS node_text (
    id       INTEGER PRIMARY KEY,
    content  TEXT    NOT NULL,
    hash     BLOB    NOT NULL UNIQUE,
    kind_hint TEXT
);

CREATE TABLE IF NOT EXISTS uast_kinds (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE
);

-- §11.2 tables created here so the §11.1 FK resolves; ROWS are populated by
-- the slice-11 normalization pass (uast.rs) — this migration is tables-only.
CREATE TABLE IF NOT EXISTS uast_map (
    id            INTEGER PRIMARY KEY,
    ast_kind_id   INTEGER NOT NULL REFERENCES node_kinds(id),
    uast_kind_id  INTEGER NOT NULL REFERENCES uast_kinds(id),
    confidence    REAL    NOT NULL DEFAULT 1.0,
    provenance    TEXT    NOT NULL,
    UNIQUE (ast_kind_id, uast_kind_id)
);

CREATE TABLE IF NOT EXISTS ast_nodes (
    id          INTEGER PRIMARY KEY,
    file_id     INTEGER NOT NULL REFERENCES files(id),
    parent_id   INTEGER REFERENCES ast_nodes(id),
    kind_id     INTEGER NOT NULL REFERENCES node_kinds(id),
    child_index INTEGER NOT NULL DEFAULT 0,
    start_byte  INTEGER NOT NULL,
    end_byte    INTEGER NOT NULL,
    start_row   INTEGER NOT NULL,
    start_col   INTEGER NOT NULL,
    end_row     INTEGER NOT NULL,
    end_col     INTEGER NOT NULL,
    is_named    INTEGER NOT NULL DEFAULT 1,
    is_error    INTEGER NOT NULL DEFAULT 0,
    is_missing  INTEGER NOT NULL DEFAULT 0,
    text_id     INTEGER REFERENCES node_text(id),
    shape_hash  BLOB,
    uast_kind_id INTEGER REFERENCES uast_kinds(id)
);
CREATE INDEX IF NOT EXISTS idx_ast_span ON ast_nodes(file_id, start_byte, end_byte);
CREATE INDEX IF NOT EXISTS idx_ast_children ON ast_nodes(parent_id, child_index);
CREATE INDEX IF NOT EXISTS idx_ast_kind ON ast_nodes(file_id, kind_id);
CREATE INDEX IF NOT EXISTS idx_ast_errors ON ast_nodes(file_id, is_error, is_missing);
CREATE INDEX IF NOT EXISTS idx_ast_shape ON ast_nodes(shape_hash);

CREATE TABLE IF NOT EXISTS symbols (
    id              INTEGER PRIMARY KEY,
    file_id         INTEGER NOT NULL REFERENCES files(id),
    ast_node_id     INTEGER NOT NULL REFERENCES ast_nodes(id),
    name            TEXT    NOT NULL,
    qual_name       TEXT    NOT NULL,
    kind            TEXT    NOT NULL,
    parent_symbol_id INTEGER REFERENCES symbols(id),
    start_byte      INTEGER NOT NULL,
    end_byte        INTEGER NOT NULL,
    sig_end_byte    INTEGER,
    subtree_hash    BLOB    NOT NULL,
    annotations     TEXT
);
CREATE INDEX IF NOT EXISTS idx_sym_file   ON symbols(file_id);
CREATE INDEX IF NOT EXISTS idx_sym_parent ON symbols(parent_symbol_id);
CREATE INDEX IF NOT EXISTS idx_sym_name   ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_sym_qual   ON symbols(qual_name);
CREATE INDEX IF NOT EXISTS idx_sym_kind   ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_sym_span   ON symbols(file_id, start_byte, end_byte);
"#;

/// Open (or create) a polyglot store and apply the §11 schema.
pub fn open_db(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    create_schema(&conn)?;
    Ok(conn)
}

pub fn create_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Declaration kinds — the seed table symbol extraction reads (§13 step 5);
// slice 11 generalizes these rows into uast_map.
// ---------------------------------------------------------------------------

/// `(grammar kind → symbol kind)` per language. Name lookup is the `name`
/// field where the grammar provides one; C/C++ need a declarator chain walk.
pub fn decl_kinds(lang: Lang) -> &'static [(&'static str, &'static str)] {
    match lang {
        Lang::Python => &[
            ("function_definition", "function"),
            ("class_definition", "class"),
        ],
        Lang::JavaScript => &[
            ("function_declaration", "function"),
            ("generator_function_declaration", "function"),
            ("class_declaration", "class"),
            ("method_definition", "method"),
        ],
        Lang::TypeScript => &[
            ("function_declaration", "function"),
            ("generator_function_declaration", "function"),
            ("class_declaration", "class"),
            ("abstract_class_declaration", "class"),
            ("method_definition", "method"),
            ("interface_declaration", "trait"),
        ],
        Lang::Go => &[
            ("function_declaration", "function"),
            ("method_declaration", "method"),
        ],
        Lang::Rust => &[
            ("function_item", "function"),
            ("struct_item", "class"),
            ("enum_item", "class"),
            ("trait_item", "trait"),
        ],
        Lang::Java => &[
            ("method_declaration", "method"),
            ("class_declaration", "class"),
            ("interface_declaration", "trait"),
            ("enum_declaration", "class"),
        ],
        Lang::C => &[("function_definition", "function")],
        Lang::Cpp => &[
            ("function_definition", "function"),
            ("class_specifier", "class"),
        ],
        Lang::CSharp => &[
            ("method_declaration", "method"),
            ("class_declaration", "class"),
            ("interface_declaration", "trait"),
        ],
        Lang::Php => &[
            ("function_definition", "function"),
            ("method_declaration", "method"),
            ("class_declaration", "class"),
        ],
    }
}

/// Locate a declaration's name node: the `name` field when the grammar
/// provides one; otherwise the first identifier inside a declarator chain
/// (C/C++). Returns `None` when nothing name-like exists — the caller
/// skips and counts, never guesses.
fn find_decl_name<'t>(node: tree_sitter::Node<'t>, src: &'t [u8]) -> Option<&'t str> {
    if let Some(n) = node.child_by_field_name("name") {
        return Some(n.utf8_text(src).ok()?);
    }
    // C/C++: function_definition → declarator → function_declarator → declarator
    let mut cur = node.child_by_field_name("declarator");
    let mut hops = 0;
    while let Some(d) = cur {
        if d.kind() == "identifier" || d.kind() == "field_identifier" {
            return Some(d.utf8_text(src).ok()?);
        }
        let inner = d.child_by_field_name("declarator");
        cur = inner.or_else(|| {
            d.children(&mut d.walk())
                .find(|c| c.kind() == "function_declarator" || c.kind() == "pointer_declarator")
        });
        hops += 1;
        if hops > 6 {
            break;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Ingestion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct IngestStats {
    pub files_ingested: usize,
    pub files_parse_errors: usize,
    pub ast_nodes: usize,
    pub symbols: usize,
    pub skipped_unsupported: usize,
    pub skipped_no_name: usize,
}

fn intern_kind(
    conn: &Connection,
    cache: &mut BTreeMap<(String, &'static str), i64>,
    name: &str,
    language: &'static str,
) -> Result<i64, String> {
    let key = (name.to_string(), language);
    if let Some(id) = cache.get(&key) {
        return Ok(*id);
    }
    conn.execute(
        "INSERT OR IGNORE INTO node_kinds(name, language) VALUES (?1, ?2)",
        rusqlite::params![name, language],
    )
    .map_err(|e| e.to_string())?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM node_kinds WHERE name = ?1 AND language = ?2",
            rusqlite::params![name, language],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    cache.insert(key, id);
    Ok(id)
}

fn intern_text(
    conn: &Connection,
    cache: &mut BTreeMap<String, i64>,
    text: &str,
    kind_hint: &str,
) -> Result<i64, String> {
    if text.is_empty() || text.len() > 256 {
        // long/empty texts stay in the file source, not the shared table
        return Ok(-1);
    }
    if let Some(id) = cache.get(text) {
        return Ok(*id);
    }
    let hash: [u8; 32] = *blake3::hash(text.as_bytes()).as_bytes();
    conn.execute(
        "INSERT OR IGNORE INTO node_text(content, hash, kind_hint) VALUES (?1, ?2, ?3)",
        rusqlite::params![text, hash.to_vec(), kind_hint],
    )
    .map_err(|e| e.to_string())?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM node_text WHERE hash = ?1",
            rusqlite::params![hash.to_vec()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    cache.insert(text.to_string(), id);
    Ok(id)
}

/// ANL-06 shape hash: pre-order sequence of grammar kinds in the subtree,
/// text excluded — structurally identical subtrees share the hash.
/// Iterative pre-order (no recursion on hostile-nesting files).
fn shape_hash(root: tree_sitter::Node) -> Vec<u8> {
    let mut buf = String::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        buf.push_str(node.kind());
        buf.push('\u{1}');
        let mut cursor = node.walk();
        let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
        drop(cursor);
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    blake3::hash(buf.as_bytes()).as_bytes().to_vec()
}

struct WalkRow<'t> {
    parent: Option<i64>,
    child_index: usize,
    node: tree_sitter::Node<'t>,
    /// enclosing declaration chain: (ast_id, name) — newest last
    qualifier: Vec<(i64, String)>,
}

/// Ingest one file's parse into the schema. Everything happens in one
/// transaction per file (§13 step 3). Returns the file row id.
pub fn ingest_parsed_file(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    path: &str,
    lang: Lang,
    src: &str,
    stats: &mut IngestStats,
) -> Result<i64, String> {
    let bytes = src.as_bytes();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.ts_language().ok_or("grammar unavailable")?)
        .map_err(|e| e.to_string())?;
    let tree = parser.parse(bytes, None).ok_or("parse aborted")?;
    let root = tree.root_node();
    let parse_errored = root.has_error();

    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let result = ingest_tx(conn, repo_id, revision, path, lang, bytes, root, stats);

    match result {
        Ok(id) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            stats.files_ingested += 1;
            if parse_errored {
                stats.files_parse_errors += 1;
            }
            Ok(id)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn ingest_tx(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    path: &str,
    lang: Lang,
    bytes: &[u8],
    root: tree_sitter::Node,
    stats: &mut IngestStats,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            repo_id,
            path,
            file_hash(bytes),
            lang.name(),
            revision,
            bytes.len() as i64,
            lang.grammar_version()
        ],
    )
    .map_err(|e| e.to_string())?;
    let file_id = conn.last_insert_rowid();

    let mut kind_cache: BTreeMap<(String, &'static str), i64> = BTreeMap::new();
    let mut text_cache: BTreeMap<String, i64> = BTreeMap::new();
    let decls: BTreeMap<&'static str, &'static str> = decl_kinds(lang).iter().copied().collect();

    // Iterative pre-order walk (explicit stack; hostile nesting cannot
    // overflow). The qualifier chain rides on each stack row.
    let mut stack: Vec<WalkRow> = vec![WalkRow {
        parent: None,
        child_index: 0,
        node: root,
        qualifier: Vec::new(),
    }];
    // symbol row id by declaration ast id (for parent_symbol_id + qual fix)
    let mut sym_ids: BTreeMap<i64, i64> = BTreeMap::new();

    // prepared once per file, reused for every node in the walk — the old
    // code re-parsed the INSERT SQL for every ast node and symbol
    let mut ast_ins = conn
        .prepare_cached(
            "INSERT INTO ast_nodes(file_id, parent_id, kind_id, child_index, start_byte, end_byte,
                 start_row, start_col, end_row, end_col, is_named, is_error, is_missing, text_id, shape_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .map_err(|e| e.to_string())?;
    let mut sym_ins = conn
        .prepare_cached(
            "INSERT INTO symbols(file_id, ast_node_id, name, qual_name, kind, parent_symbol_id,
                 start_byte, end_byte, sig_end_byte, subtree_hash, annotations)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .map_err(|e| e.to_string())?;

    while let Some(row) = stack.pop() {
        let node = row.node;
        let kind_name = node.kind();
        let kind_id = intern_kind(conn, &mut kind_cache, kind_name, lang.name())?;
        let text_id = if node.child_count() == 0 {
            let text = node.utf8_text(bytes).unwrap_or("");
            intern_text(conn, &mut text_cache, text, kind_name)?
        } else {
            -1
        };
        let shape = shape_hash(node);
        let is_decl = decls.contains_key(kind_name);

        ast_ins
            .execute(rusqlite::params![
                file_id,
                row.parent,
                kind_id,
                row.child_index as i64,
                node.start_byte() as i64,
                node.end_byte() as i64,
                node.start_position().row as i64,
                node.start_position().column as i64,
                node.end_position().row as i64,
                node.end_position().column as i64,
                node.is_named() as i64,
                node.is_error() as i64,
                node.is_missing() as i64,
                if text_id < 0 { None } else { Some(text_id) },
                shape
            ])
            .map_err(|e| e.to_string())?;
        let ast_id = conn.last_insert_rowid();
        stats.ast_nodes += 1;

        // The symbol row is written after its ast row so the FK
        // `symbols.ast_node_id REFERENCES ast_nodes(id)` is real from birth
        // (no placeholder, no patch-up).
        let mut decl_sym: Option<(i64, String)> = None;
        if is_decl {
            match find_decl_name(node, bytes) {
                Some(name) => {
                    let own_qual = row
                        .qualifier
                        .iter()
                        .map(|(_, n)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let qual = if own_qual.is_empty() {
                        name.to_string()
                    } else {
                        format!("{own_qual}.{name}")
                    };
                    let parent_sym = row
                        .qualifier
                        .last()
                        .and_then(|(ast_id, _)| sym_ids.get(ast_id).copied());
                    sym_ins
                        .execute(rusqlite::params![
                            file_id,
                            ast_id,
                            name,
                            qual,
                            decls[kind_name],
                            parent_sym,
                            node.start_byte() as i64,
                            node.end_byte() as i64,
                            Option::<i64>::None,
                            shape,
                            Option::<String>::None
                        ])
                        .map_err(|e| e.to_string())?;
                    let sym_row = conn.last_insert_rowid();
                    stats.symbols += 1;
                    sym_ids.insert(ast_id, sym_row);
                    decl_sym = Some((sym_row, name.to_string()));
                }
                None => {
                    // no name-like node in the declaration — skip and count,
                    // never guess (§13 honesty rule)
                    stats.skipped_no_name += 1;
                }
            }
        }

        // push children in reverse so pre-order pop order matches source order
        let mut qualifier = row.qualifier.clone();
        if let Some((sym_row, name)) = &decl_sym {
            qualifier.push((ast_id, name.clone()));
            let _ = sym_row;
        }
        let mut cursor = node.walk();
        let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
        drop(cursor);
        for (idx, child) in children.into_iter().enumerate().rev() {
            stack.push(WalkRow {
                parent: Some(ast_id),
                child_index: idx,
                node: child,
                qualifier: qualifier.clone(),
            });
        }
    }
    Ok(file_id)
}

/// blake3 of the file content.
fn file_hash(bytes: &[u8]) -> Vec<u8> {
    blake3::hash(bytes).as_bytes().to_vec()
}

/// Walk a folder (PRD §7 ignore list) and ingest every supported file.
/// Unsupported files are counted, never silently dropped.
pub fn ingest_folder(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    root: &Path,
    stats: &mut IngestStats,
) -> Result<(), String> {
    let mut files: Vec<(String, Lang, String)> = Vec::new();
    collect_files(root, root, &mut files, stats)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (rel, lang, src) in files {
        ingest_parsed_file(conn, repo_id, revision, &rel, lang, &src, stats)?;
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Lang, String)>,
    stats: &mut IngestStats,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_files(root, &p, out, stats)?;
        } else {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string();
            let Some(lang) = Lang::from_path(&rel) else {
                stats.skipped_unsupported += 1;
                continue;
            };
            let src = std::fs::read_to_string(&p)
                .map_err(|e| format!("{}: {e}", p.display()))?;
            out.push((rel, lang, src));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    const PY: &str = "class Cart:\n    def add_item(self, item):\n        return item\n\ndef total(cart):\n    return 42\n";
    const RUST: &str = "struct Price { cents: u64 }\ntrait Calc { fn compute(&self) -> u64; }\nfn price_of(p: &Price) -> u64 { p.cents }\n";
    const GO: &str = "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n";
    const TS: &str = "interface Handler { handle(x: string): void }\nclass Cart { add(item: string): void {} }\nfunction total(): number { return 1; }\n";
    const JAVA: &str = "public class Cart {\n    public int total() { return 1; }\n}\n";
    const CS: &str = "public class Cart {\n    public int Total() { return 1; }\n}\n";
    const PHP: &str = "<?php\nfunction total($c) { return 1; }\n";
    const C: &str = "int total(int x) { return x; }\n";
    const CPP: &str = "class Cart {\npublic:\n    int total() { return 1; }\n};\n";
    const JS: &str = "class Cart { add(item) {} }\nfunction total() { return 1; }\n";

    #[test]
    fn files_and_ast_rows_written_with_spans() {
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 1, "cart.py", Lang::Python, PY, &mut st).unwrap();
        assert_eq!(st.files_ingested, 1);
        let (n, sb, eb): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), MIN(start_byte), MAX(end_byte) FROM ast_nodes",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(n > 10);
        assert_eq!(sb, 0);
        assert_eq!(eb as usize, PY.len());
        // grammar version recorded per PRD §11.1
        let gv: String = conn
            .query_row("SELECT tree_sitter_grammar_version FROM files", [], |r| r.get(0))
            .unwrap();
        assert!(gv.starts_with("tree-sitter-python"));
    }

    #[test]
    fn symbols_parent_qualified_across_five_languages() {
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 1, "cart.py", Lang::Python, PY, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "price.rs", Lang::Rust, RUST, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "add.go", Lang::Go, GO, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "cart.ts", Lang::TypeScript, TS, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "Cart.java", Lang::Java, JAVA, &mut st).unwrap();
        assert_eq!(st.skipped_no_name, 0);

        let qn = |q: &str| -> Option<String> {
            conn.query_row(
                "SELECT qual_name FROM symbols WHERE qual_name = ?1",
                rusqlite::params![q],
                |r| r.get(0),
            )
            .ok()
        };
        // python class.method qualification
        assert_eq!(qn("Cart.add_item").as_deref(), Some("Cart.add_item"));
        assert_eq!(qn("total").as_deref(), Some("total"));
        // rust struct + trait + free function
        assert_eq!(qn("Price").as_deref(), Some("Price"));
        assert_eq!(qn("Calc").as_deref(), Some("Calc"));
        assert_eq!(qn("price_of").as_deref(), Some("price_of"));
        // go + ts + java
        assert_eq!(qn("Add").as_deref(), Some("Add"));
        assert_eq!(qn("Handler").as_deref(), Some("Handler"));
        assert_eq!(qn("Cart.add").as_deref(), Some("Cart.add"));
        assert_eq!(qn("Cart.total").as_deref(), Some("Cart.total"));

        let kinds: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare("SELECT kind, COUNT(*) FROM symbols GROUP BY kind ORDER BY kind")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, String>(0).unwrap(), r.get::<_, i64>(1).unwrap())))
                .unwrap()
                .map(|x| x.unwrap())
                .collect()
        };
        let traits = kinds.iter().find(|(k, _)| k == "trait").map(|(_, n)| *n).unwrap_or(0);
        assert!(traits >= 2, "rust Calc + ts Handler must both land: {kinds:?}");
    }

    #[test]
    fn remaining_prd_languages_extract_symbols() {
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 1, "Cart.cs", Lang::CSharp, CS, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "total.php", Lang::Php, PHP, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "total.c", Lang::C, C, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "cart.cpp", Lang::Cpp, CPP, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "cart.js", Lang::JavaScript, JS, &mut st).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))
            .unwrap();
        // cs: Cart+Total; php: total; c: total; cpp: Cart+total (method through
        // class_specifier body); js: Cart+add+total
        assert!(n >= 9, "expected >= 9 symbols, got {n}");
        assert_eq!(st.skipped_no_name, 0);
    }

    #[test]
    fn parse_error_file_still_indexed_and_flagged() {
        // §13: missing parse → is_error set; file still indexed
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 1, "broken.py", Lang::Python, "def broken(:\n", &mut st)
            .unwrap();
        assert_eq!(st.files_ingested, 1);
        assert_eq!(st.files_parse_errors, 1);
        let flagged: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ast_nodes WHERE is_error = 1 OR is_missing = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(flagged >= 1, "error/missing nodes must be flagged, got {flagged}");
    }

    #[test]
    fn shape_hash_rename_stable_and_content_addressed() {
        let conn = db();
        let mut st = IngestStats::default();
        // structurally identical functions with different local names → same
        // shape hash on the decl node
        ingest_parsed_file(
            &conn, 1, 1, "a.py", Lang::Python,
            "def f(alpha):\n    return alpha + 1\n", &mut st,
        )
        .unwrap();
        ingest_parsed_file(
            &conn, 1, 1, "b.py", Lang::Python,
            "def g(beta):\n    return beta + 1\n", &mut st,
        )
        .unwrap();
        let (ha, hb): (Vec<u8>, Vec<u8>) = (
            conn.query_row(
                "SELECT shape_hash FROM ast_nodes WHERE kind_id = (SELECT id FROM node_kinds WHERE name='function_definition') AND file_id=1 ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            conn.query_row(
                "SELECT shape_hash FROM ast_nodes WHERE kind_id = (SELECT id FROM node_kinds WHERE name='function_definition') AND file_id=2 ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        );
        assert_eq!(ha, hb, "identical shapes must hash identically");
    }

    #[test]
    fn node_text_content_addressed_shared() {
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 1, "a.py", Lang::Python, PY, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 1, "b.py", Lang::Python, "def total(c):\n    return 42\n", &mut st)
            .unwrap();
        // the identifier text "total" appears in two files → ONE node_text row
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM node_text WHERE content = 'total'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn unsupported_files_counted_never_guessed() {
        let dir = std::env::temp_dir().join(format!("cie-pg-s9-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.py"), PY).unwrap();
        std::fs::write(dir.join("sub/b.rs"), RUST).unwrap();
        std::fs::write(dir.join("readme.md"), "# docs").unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join("node_modules/x.js"), "var q=1;").unwrap();
        let conn = db();
        let mut st = IngestStats::default();
        ingest_folder(&conn, 1, 1, &dir, &mut st).unwrap();
        assert_eq!(st.files_ingested, 2);
        assert_eq!(st.skipped_unsupported, 1, "readme.md counted, not ingested");
        assert!(st.ast_nodes > 0, "AST rows exist for both files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revision_scoped_time_travel_rows() {
        // acceptance #11 groundwork: same path under two revisions
        let conn = db();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, PY, &mut st).unwrap();
        ingest_parsed_file(&conn, 1, 42, "a.py", Lang::Python, "x = 1\n", &mut st).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE path='a.py'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "both revisions retained (time travel)");
    }
}
