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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use rusqlite::{Connection, OpenFlags};

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
    Kotlin,
    Lua,
    Tsx,
}

impl Lang {
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = Path::new(path).extension()?.to_str()?;
        Some(match ext {
            "py" | "pyi" => Lang::Python,
            "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "ts" | "mts" | "cts" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "kt" | "kts" => Lang::Kotlin,
            "lua" => Lang::Lua,
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
            Lang::Kotlin => "tree-sitter-kotlin-ng 1.1",
            Lang::Lua => "tree-sitter-lua 0.5",
            Lang::Tsx => "tree-sitter-typescript TSX 0.23",
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
            Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Lang::Lua => tree_sitter_lua::LANGUAGE.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
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
            Lang::Kotlin => "kotlin",
            Lang::Lua => "lua",
            Lang::Tsx => "typescript",
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
CREATE INDEX IF NOT EXISTS idx_ast_span_no_file ON ast_nodes(start_byte, end_byte);
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

-- INCREMENTAL-ON (backlog item 6): build metadata — the tree fingerprint
-- the unchanged-tree short-circuit compares against (key 'tree_fingerprint').
CREATE TABLE IF NOT EXISTS store_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// Open (or create) a polyglot store and apply the §11 schema.
pub fn open_db(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    // Single-writer scratch store, but READ concurrently: the web opens this
    // exact file (node:sqlite, readOnly — polyglot-db.ts) while a rebuild may
    // run in another process. WAL is the only journal mode where a writer's
    // commit is never blocked by a live reader and vice versa (MVCC snapshot
    // isolation) — measured decisive: under OFF/DELETE a reader holding a read
    // transaction makes the writer's COMMIT fail with "database is locked"
    // (scripts/ds-bench, C2 probe); under WAL it commits in 0.07 ms. WAL also
    // returns 2.9× on point selects (11.4 ms vs 32.6 ms / 10k, bench B) and
    // crash-recovers on next open instead of corrupting (no rollback journal
    // under OFF). synchronous=NORMAL keeps the fsync out of the commit path
    // (WAL commits are app-crash-safe without it; an OS crash may lose the
    // last transactions — acceptable for a store that rebuilds
    // deterministically). Combined with the CLI's build-then-rename promotion
    // (Command::Polyglot) readers see old-complete or new-complete, never a
    // half-built DB. Cost: ~+13% on bulk writes, +36% file size incl. the WAL
    // file (bench B) — worth it by an order of magnitude.
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(|e| e.to_string())?;
    create_schema(&conn)?;
    Ok(conn)
}

/// The store-path convention every consumer shares: FNV-1a 64 over the
/// canonical root path, `$TMPDIR/cie-polyglot-<h:x>.db`. The CLI's Polyglot
/// command builds and promotes at exactly this name (main.rs), the web
/// reader re-derives it (polyglot-db.ts `fnv1a64Hex`), and the ingest
/// bridge (BRIDGE, backlog item 5) resolves it by default — one convention,
/// three consumers, one source of truth. Fails soft: an uncanonicalizable
/// path falls back to the literal path (same as the CLI's
/// `canonicalize().unwrap_or(path)`).
pub fn default_db_path(root: &Path) -> std::path::PathBuf {
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut h: u64 = 0xcbf29ce484222325;
    for b in canon.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    std::env::temp_dir().join(format!("cie-polyglot-{h:x}.db"))
}

/// One `file_source_cache` row: the blake3 `content_hash` the row was stored
/// under, the zstd-compressed source bytes, and the file's exact byte size
/// (the decompression bound — the cache is our own store, but the bound
/// keeps a corrupt row from allocating arbitrarily).
#[derive(Debug, Clone)]
pub struct SourceBlob {
    pub content_hash: Vec<u8>,
    pub zstd_bytes: Vec<u8>,
    pub size_bytes: usize,
}

/// Read the whole source cache, STRICTLY READ-ONLY. The bridge (cie-ingest)
/// calls this on the promoted store; a mid-rebuild `.building` file is never
/// at this path (build-then-rename, Command::Polyglot), so a successful open
/// always sees a complete, cleanly-checkpointed DB. Read-only keeps the
/// DS-EVAL discipline — the ingestion path may READ the polyglot universe,
/// never write it. Paths are normalized to forward slashes to match the
/// walk keys on every platform.
pub fn read_source_cache(db: &Path) -> Result<BTreeMap<String, SourceBlob>, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("{}: {e}", db.display()))?;
    let mut stmt = conn
        .prepare(
            "SELECT f.path, f.content_hash, f.size_bytes, c.zstd_bytes
             FROM files f JOIN file_source_cache c ON c.file_id = f.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (path, content_hash, size_bytes, zstd_bytes) = row.map_err(|e| e.to_string())?;
        if size_bytes < 0 {
            continue;
        }
        out.insert(
            path.replace('\\', "/"),
            SourceBlob {
                content_hash,
                zstd_bytes,
                size_bytes: size_bytes as usize,
            },
        );
    }
    Ok(out)
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
        Lang::TypeScript | Lang::Tsx => &[
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
        Lang::Kotlin => &[("function_declaration", "function"), ("class_declaration", "class"), ("object_declaration", "class")],
        Lang::Lua => &[("function_declaration", "function"), ("function_definition", "function")],
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
    /// INCREMENTAL-ON (backlog item 6): files whose rows were carried over
    /// from the previous store instead of re-parsed. Included in
    /// `files_ingested` (which keeps its meaning: files now in the store).
    pub files_reused: usize,
}

fn intern_kind(
    conn: &Connection,
    cache: &mut HashMap<(String, &'static str), i64>,
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
    cache: &mut HashMap<String, i64>,
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
///
/// One pass per FILE builds the shared pre-order kind stream S (kind +
/// \u{1} per node, children in source order — the exact byte sequence the
/// old per-node walk produced) and hashes each node's slice when its
/// subtree closes (a stack marker pops right there). DFS pre-order
/// contiguity makes a subtree's stream the contiguous run S[start..close],
/// so every shape_hash is one blake3 over contiguous bytes — bit-identical
/// to the old per-node result (same bytes hashed) — instead of re-walking
/// the subtree under every node, which cost O(subtree) node visits per node.
fn precompute_shape_hashes(root: tree_sitter::Node) -> HashMap<usize, Vec<u8>> {
    enum Ev<'t> {
        Node(tree_sitter::Node<'t>),
        /// pops exactly when pre-order node `usize`'s subtree is complete
        Close(usize),
    }
    let mut stream: Vec<u8> = Vec::new();
    let mut nid_by_idx: Vec<usize> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut out: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut stack: Vec<Ev> = vec![Ev::Node(root)];
    while let Some(ev) = stack.pop() {
        match ev {
            Ev::Node(node) => {
                nid_by_idx.push(node.id());
                starts.push(stream.len());
                stream.extend_from_slice(node.kind().as_bytes());
                stream.push(1u8); // the '\u{1}' separator
                // close marker below the children → it pops after the whole
                // subtree (children pushed in reverse so they visit in order)
                stack.push(Ev::Close(nid_by_idx.len() - 1));
                let mut cursor = node.walk();
                let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
                drop(cursor);
                for child in children.into_iter().rev() {
                    stack.push(Ev::Node(child));
                }
            }
            Ev::Close(i) => {
                let nid = nid_by_idx[i];
                let seg = &stream[starts[i]..stream.len()];
                out.insert(nid, blake3::hash(seg).as_bytes().to_vec());
            }
        }
    }
    out
}

/// Enclosing declaration chain for qualified symbol names — a persistent
/// (cons) list so the per-child copy that used to be a deep Vec<String>
/// clone is one Rc refcount bump. `newest` is the old `last`, `joined` the
/// old oldest-first "."-join, byte for byte.
#[derive(Clone)]
struct QualList(Option<Rc<QualCell>>);
struct QualCell {
    ast_id: i64,
    name: String,
    next: QualList,
}
impl QualList {
    fn empty() -> Self {
        QualList(None)
    }
    fn push(&self, ast_id: i64, name: String) -> QualList {
        QualList(Some(Rc::new(QualCell {
            ast_id,
            name,
            next: self.clone(),
        })))
    }
    fn newest(&self) -> Option<(i64, &str)> {
        self.0.as_ref().map(|c| (c.ast_id, c.name.as_str()))
    }
    fn joined(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = self;
        while let Some(c) = &cur.0 {
            parts.push(c.name.as_str());
            cur = &c.next;
        }
        parts.reverse();
        parts.join(".")
    }
    fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

struct WalkRow<'t> {
    parent: Option<i64>,
    child_index: usize,
    node: tree_sitter::Node<'t>,
    /// enclosing declaration chain — newest first, O(1) clone
    qualifier: QualList,
}

/// Ingest one file's parse into the schema. One transaction per file
/// (§13 step 3) — the direct path for single-file callers; the folder pass
/// uses `ingest_file_tx` under savepoints instead. Returns the file row id.
pub fn ingest_parsed_file(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    path: &str,
    lang: Lang,
    src: &str,
    stats: &mut IngestStats,
) -> Result<i64, String> {
    let mut kind_cache: HashMap<(String, &'static str), i64> = HashMap::new();
    let mut text_cache: HashMap<String, i64> = HashMap::new();
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let result = ingest_file_tx(conn, repo_id, revision, path, lang, src, stats, &mut kind_cache, &mut text_cache);

    match result {
        Ok((id, parse_errored)) => {
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

/// Ingest one file — parse + walk + writes — with NO transaction management
/// of its own. The direct path wraps it in BEGIN/COMMIT; the folder pass
/// wraps the whole loop in one transaction with a SAVEPOINT per file, so a
/// failing file still rolls back alone while the store fsyncs once, not
/// once per file. Returns (file row id, parse_errored).
fn ingest_file_tx(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    path: &str,
    lang: Lang,
    src: &str,
    stats: &mut IngestStats,
    kind_cache: &mut HashMap<(String, &'static str), i64>,
    text_cache: &mut HashMap<String, i64>,
) -> Result<(i64, bool), String> {
    let bytes = src.as_bytes();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.ts_language().ok_or("grammar unavailable")?)
        .map_err(|e| e.to_string())?;
    let tree = parser.parse(bytes, None).ok_or("parse aborted")?;
    let root = tree.root_node();
    let parse_errored = root.has_error();

    let file_id = ingest_walk(conn, repo_id, revision, path, lang, bytes, root, stats, kind_cache, text_cache)?;
    Ok((file_id, parse_errored))
}

/// Persist the coordinator's existing tree, without parsing source again.
pub fn ingest_tree(
    conn: &Connection, repo_id: i64, revision: i64, path: &str, lang: Lang,
    src: &str, tree: &tree_sitter::Tree, stats: &mut IngestStats,
) -> Result<i64, String> {
    let id = ingest_walk(conn, repo_id, revision, path, lang, src.as_bytes(), tree.root_node(), stats,
        &mut HashMap::new(), &mut HashMap::new())?;
    stats.files_ingested += 1;
    stats.files_parse_errors += usize::from(tree.root_node().has_error());
    Ok(id)
}

fn ingest_walk(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    path: &str,
    lang: Lang,
    bytes: &[u8],
    root: tree_sitter::Node,
    stats: &mut IngestStats,
    mut kind_cache: &mut HashMap<(String, &'static str), i64>,
    mut text_cache: &mut HashMap<String, i64>,
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

    let decls: HashMap<&'static str, &'static str> = decl_kinds(lang).iter().copied().collect();

    // every node's shape hash in one file-level pass (see fn docs) —
    // keyed by tree-sitter node id
    let shapes = precompute_shape_hashes(root);

    // Iterative pre-order walk (explicit stack; hostile nesting cannot
    // overflow). The qualifier chain rides on each stack row.
    let mut stack: Vec<WalkRow> = vec![WalkRow {
        parent: None,
        child_index: 0,
        node: root,
        qualifier: QualList::empty(),
    }];
    // symbol row id by declaration ast id (for parent_symbol_id + qual fix)
    let mut sym_ids: HashMap<i64, i64> = HashMap::new();

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
        let shape: &[u8] = &shapes[&node.id()];
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
                    let own_qual = row.qualifier.joined();
                    let qual = if own_qual.is_empty() {
                        name.to_string()
                    } else {
                        format!("{own_qual}.{name}")
                    };
                    let parent_sym = row
                        .qualifier
                        .newest()
                        .and_then(|(ast_id, _)| sym_ids.get(&ast_id).copied());
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
        let qualifier = match &decl_sym {
            Some((_, name)) => row.qualifier.push(ast_id, name.clone()),
            None => row.qualifier.clone(),
        };
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
///
/// The whole folder writes through ONE transaction: a per-file commit fsyncs
/// the store n times for data nobody reads until lowering starts. Per-file
/// atomicity is preserved with SAVEPOINTs — a failing file rolls back to its
/// own savepoint, everything before it stays committed (the exact outcome of
/// the old per-file commits), and the walk aborts with the same error.
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
    ingest_collected(conn, repo_id, revision, files, stats, None)
}

// ---------------------------------------------------------------------------
// INCREMENTAL-ON (backlog item 6) — the reuse tier
//
// The DS-EVAL build-then-rename contract rebuilds the store fresh each run;
// the honest-recomputation price is a full tree-sitter re-parse of every
// file. The reuse tier keeps the contract and drops the price: the PREVIOUS
// promoted store is ATTACHed as `prev`, and any walked file whose blake3
// equals the previous row's `content_hash` (same language, same pinned
// grammar version) has its `files`/`ast_nodes`/`symbols` rows copied verbatim
// — ids included — instead of re-parsed. Every derivation (source cache,
// UAST normalization, IR lowering, frameworks, C4, state machines, workflows,
// duplicates, runtime, viewspec) still recomputes each run from those rows,
// so a warm rebuild of an unchanged tree is row-for-row the full rebuild;
// a `--full` flag keeps the escape hatch. Reuse problems degrade to a full
// parse with a named note — never to a silently stale store.
// ---------------------------------------------------------------------------

/// The outcome of one reuse-tier pass. Invariant:
/// `files_reused + files_parsed == files_ingested` (the store population).
#[derive(Debug, Clone, Default)]
pub struct ReuseReport {
    /// files whose rows were copied from the previous store (hash-verified)
    pub files_reused: usize,
    /// files re-parsed this run (changed or new)
    pub files_parsed: usize,
    /// paths the previous store knew that the current walk no longer sees
    pub files_dropped: usize,
    /// why reuse did not engage (previous store unusable) — full parse ran
    pub note: Option<String>,
}

/// Walk + hash-match against the ATTACHed previous store, then ingest: the
/// unchanged subset is copied verbatim, the changed/new subset re-parses,
/// and every walked file that fails the hash/language/grammar match is
/// re-parsed too. `prev_db = None` is the plain cold path (`ingest_folder`).
pub fn ingest_folder_reusing(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    root: &Path,
    stats: &mut IngestStats,
    prev_db: Option<&Path>,
) -> Result<ReuseReport, String> {
    let Some(prev_db) = prev_db else {
        ingest_folder(conn, repo_id, revision, root, stats)?;
        return Ok(ReuseReport {
            files_reused: 0,
            files_parsed: stats.files_ingested,
            files_dropped: 0,
            note: None,
        });
    };
    // ATTACH the live promoted store. Only SELECTs ever touch it; the
    // build-then-rename promotion replaces the file afterwards and drops
    // stale sidecars (the ATTACH may create the same -wal/-shm coordination
    // files every WAL reader makes).
    let attach_sql = format!(
        "ATTACH DATABASE '{}' AS prev",
        prev_db.display().to_string().replace('\'', "''")
    );
    if let Err(e) = conn.execute_batch(&attach_sql) {
        let note = format!("previous store unusable ({e}) — full reparse");
        ingest_folder(conn, repo_id, revision, root, stats)?;
        return Ok(ReuseReport {
            files_reused: 0,
            files_parsed: stats.files_ingested,
            files_dropped: 0,
            note: Some(note),
        });
    }
    let result: Result<ReuseReport, String> = (|| {
        if let Err(e) = validate_prev_schema(conn) {
            ingest_folder(conn, repo_id, revision, root, stats)?;
            return Ok(ReuseReport {
                files_reused: 0,
                files_parsed: stats.files_ingested,
                files_dropped: 0,
                note: Some(format!(
                    "previous store schema not reusable ({e}) — full reparse"
                )),
            });
        }
        let mut files: Vec<(String, Lang, String)> = Vec::new();
        collect_files(root, root, &mut files, stats)?;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let plan = plan_reuse(conn, repo_id, files)?;
        let ReusePlan {
            reused_ids,
            parse,
            dropped,
        } = plan;
        let report = ReuseReport {
            files_reused: reused_ids.len(),
            files_parsed: parse.len(),
            files_dropped: dropped,
            note: None,
        };
        ingest_collected(conn, repo_id, revision, parse, stats, Some(&reused_ids))?;
        Ok(report)
    })();
    let _ = conn.execute_batch("DETACH DATABASE prev");
    let report = result?;
    stats.files_ingested += report.files_reused;
    stats.files_reused = report.files_reused;
    Ok(report)
}

/// The planned partition of one walk: carried-over prev file ids vs the
/// re-parse subset, plus the count of prev paths the walk no longer sees.
struct ReusePlan {
    reused_ids: Vec<i64>,
    parse: Vec<(String, Lang, String)>,
    dropped: usize,
}

/// Required prev-store columns — a store missing any of these predates the
/// reuse tier's expectations and falls back to a full parse, named.
fn validate_prev_schema(conn: &Connection) -> Result<(), String> {
    let required: &[(&str, &[&str])] = &[
        (
            "files",
            &[
                "id",
                "repo_id",
                "path",
                "content_hash",
                "language",
                "revision",
                "size_bytes",
                "tree_sitter_grammar_version",
            ],
        ),
        (
            "ast_nodes",
            &[
                "id",
                "file_id",
                "parent_id",
                "kind_id",
                "child_index",
                "start_byte",
                "end_byte",
                "start_row",
                "start_col",
                "end_row",
                "end_col",
                "is_named",
                "is_error",
                "is_missing",
                "text_id",
                "shape_hash",
            ],
        ),
        (
            "symbols",
            &[
                "id",
                "file_id",
                "ast_node_id",
                "name",
                "qual_name",
                "kind",
                "parent_symbol_id",
                "start_byte",
                "end_byte",
                "sig_end_byte",
                "subtree_hash",
            ],
        ),
        ("node_kinds", &["id", "name", "language"]),
        ("node_text", &["id", "content", "hash", "kind_hint"]),
    ];
    for (table, cols) in required {
        let mut stmt = conn
            .prepare(&format!("PRAGMA prev.table_info({table})"))
            .map_err(|e| e.to_string())?;
        let have: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for col in *cols {
            if !have.iter().any(|h| h == col) {
                return Err(format!("{table}.{col} missing"));
            }
        }
    }
    Ok(())
}

/// Latest-revision index of the previous store, then the per-file decision:
/// same path + same blake3 + same language + same pinned grammar → reuse;
/// anything else → re-parse. A path present before but absent from the walk
/// is counted dropped (its rows simply never get copied).
fn plan_reuse(
    conn: &Connection,
    repo_id: i64,
    files: Vec<(String, Lang, String)>,
) -> Result<ReusePlan, String> {
    // SQLite's bare-column-with-MAX rule returns the max-revision row's
    // columns, so this is the latest state per path.
    let mut stmt = conn
        .prepare(
            "SELECT path, id, content_hash, language, tree_sitter_grammar_version, MAX(revision)
             FROM prev.files WHERE repo_id = ?1 GROUP BY path",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([repo_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut prev_by_path: HashMap<String, (i64, Vec<u8>, String, String)> = HashMap::new();
    for row in rows {
        let (path, id, hash, language, gv) = row.map_err(|e| e.to_string())?;
        prev_by_path.insert(path, (id, hash, language, gv));
    }
    drop(stmt);

    let mut walked: HashSet<String> = HashSet::new();
    let mut plan = ReusePlan {
        reused_ids: Vec::new(),
        parse: Vec::new(),
        dropped: 0,
    };
    for (rel, lang, src) in files {
        walked.insert(rel.clone());
        let reusable = prev_by_path
            .get(&rel)
            .filter(|(_, hash, language, gv)| {
                language == lang.name() && gv == lang.grammar_version()
            })
            .map(|(_, hash, _, _)| file_hash(src.as_bytes()) == *hash)
            .unwrap_or(false);
        if reusable {
            plan.reused_ids.push(prev_by_path[&rel].0);
        } else {
            plan.parse.push((rel, lang, src));
        }
    }
    // dropped = previous paths the walk no longer sees — a walked-but-changed
    // path is a re-parse, not a drop
    plan.dropped = prev_by_path.keys().filter(|p| !walked.contains(*p)).count();
    Ok(plan)
}

/// Copy the reused files' rows verbatim — ids included — in FK-safe order
/// (shared intern rows → files → ast parents-before-children → symbols).
/// Kinds/texts use OR IGNORE: chunks overlap on shared intern rows by
/// design, and the ids come from one store, so same id == same row.
/// `file_source_cache` is NOT copied: `store_sources` recompresses every
/// file this run, deterministic over identical content.
fn copy_reused_rows(
    conn: &Connection,
    ids: &[i64],
    repo_id: i64,
    revision: i64,
    stats: &mut IngestStats,
) -> Result<(), String> {
    for chunk in ids.chunks(900) {
        let in_list = chunk
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        conn.execute(
            &format!(
                "INSERT OR IGNORE INTO node_kinds(id, name, language)
                 SELECT DISTINCT k.id, k.name, k.language
                 FROM prev.node_kinds k JOIN prev.ast_nodes a ON a.kind_id = k.id
                 WHERE a.file_id IN ({in_list})"
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            &format!(
                "INSERT OR IGNORE INTO node_text(id, content, hash, kind_hint)
                 SELECT DISTINCT t.id, t.content, t.hash, t.kind_hint
                 FROM prev.node_text t JOIN prev.ast_nodes a ON a.text_id = t.id
                 WHERE a.file_id IN ({in_list})"
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            &format!(
                "INSERT INTO files(id, repo_id, path, content_hash, language, revision,
                     size_bytes, tree_sitter_grammar_version)
                 SELECT id, ?1, path, content_hash, language, ?2, size_bytes,
                     tree_sitter_grammar_version
                 FROM prev.files WHERE id IN ({in_list})"
            ),
            rusqlite::params![repo_id, revision],
        )
        .map_err(|e| e.to_string())?;
        // the ingest walk inserts a parent before its children, so
        // parent_id < id on every row — ORDER BY id satisfies the self-FK
        // even under per-row immediate checking
        conn.execute(
            &format!(
                "INSERT INTO ast_nodes(id, file_id, parent_id, kind_id, child_index,
                     start_byte, end_byte, start_row, start_col, end_row, end_col,
                     is_named, is_error, is_missing, text_id, shape_hash)
                 SELECT id, file_id, parent_id, kind_id, child_index,
                     start_byte, end_byte, start_row, start_col, end_row, end_col,
                     is_named, is_error, is_missing, text_id, shape_hash
                 FROM prev.ast_nodes WHERE file_id IN ({in_list}) ORDER BY id"
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            &format!(
                "INSERT INTO symbols(id, file_id, ast_node_id, name, qual_name, kind,
                     parent_symbol_id, start_byte, end_byte, sig_end_byte, subtree_hash)
                 SELECT id, file_id, ast_node_id, name, qual_name, kind,
                     parent_symbol_id, start_byte, end_byte, sig_end_byte, subtree_hash
                 FROM prev.symbols WHERE file_id IN ({in_list}) ORDER BY id"
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        // honest stats: the carried rows count exactly as parsed rows would
        let (nodes, syms, errs): (i64, i64, i64) = conn
            .query_row(
                &format!(
                    "SELECT
                       (SELECT COUNT(*) FROM prev.ast_nodes WHERE file_id IN ({in_list})),
                       (SELECT COUNT(*) FROM prev.symbols WHERE file_id IN ({in_list})),
                       (SELECT COUNT(DISTINCT file_id) FROM prev.ast_nodes
                          WHERE file_id IN ({in_list}) AND (is_error = 1 OR is_missing = 1))"
                ),
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        stats.ast_nodes += nodes as usize;
        stats.symbols += syms as usize;
        stats.files_parse_errors += errs as usize;
        stats.files_reused += chunk.len();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// INCREMENTAL-ON — the unchanged-tree short-circuit
//
// The per-file reuse tier already makes warm rebuilds parity-identical, but
// at scale the pipeline is DERIVATION-bound (IR lowering, frameworks, C4,
// state machines, workflows, duplicates all recompute over 1M+ carried
// rows), so skipping only the parse is perf-neutral. When the tree is
// UNCHANGED and the engine is the same, every one of those derivations
// recomputes to exactly the rows the previous store already holds (the
// warm==full gate above proves it row-for-row) — so the honest maximum is
// to not rebuild at all: hash the tree (files + dirs + walk ignores +
// engine stamp), compare with the previous store's `store_meta` row, and
// hand back the previous store as-is. Any change — file content, a new or
// gone path, a directory, the ignored-dir list, a grammar pin, the engine
// version — changes the fingerprint and forces the rebuild. `--full`
// bypasses the check entirely; a runtime (--otel) build never takes it.
// ---------------------------------------------------------------------------

/// The engine half of the fingerprint: the crate version plus every input
/// the derivations depend on that is not file content. Bump the
/// cie-polyglot version when derivation logic changes — the stamp is what
/// keeps a changed engine from serving stale derived rows.
fn engine_stamp() -> String {
    const EXTS: &str = "py,pyi,js,jsx,mjs,cjs,ts,go,rs,java,c,cpp,cs,php";
    let grammars: Vec<&str> = [
        Lang::Python,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Go,
        Lang::Rust,
        Lang::Java,
        Lang::C,
        Lang::Cpp,
        Lang::CSharp,
        Lang::Php,
    ]
    .iter()
    .map(|l| l.grammar_version())
    .collect();
    format!(
        "cie-polyglot v{} | grammars [{}] | exts [{}] | ignored [{}]",
        env!("CARGO_PKG_VERSION"),
        grammars.join(","),
        EXTS,
        IGNORED_DIRS.join(",")
    )
}

/// A tree fingerprint: how many supported files the walk saw, and the
/// blake3 over (every supported file's path + size + content hash, every
/// visited directory, the unsupported-file count, the engine stamp).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFingerprint {
    pub files: usize,
    pub hash: String,
}

/// Walk the tree exactly like `collect_files` (same ignore list, same
/// unsupported rule) but streaming — nothing is held in memory — and hash
/// the structure into one blake3. Deterministic for a deterministic tree.
pub fn hash_tree(root: &Path) -> Result<TreeFingerprint, String> {
    let mut files: Vec<(String, usize, Vec<u8>)> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    fn visit(
        dir: &Path,
        rel: &str,
        files: &mut Vec<(String, usize, Vec<u8>)>,
        dirs: &mut Vec<String>,
        skipped: &mut usize,
    ) -> Result<(), String> {
        dirs.push(rel.to_string());
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if p.is_dir() {
                if IGNORED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                visit(&p, &child_rel, files, dirs, skipped)?;
            } else if Lang::from_path(&child_rel).is_some() {
                let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let hash = blake3::hash(&bytes).as_bytes().to_vec();
                files.push((child_rel, bytes.len(), hash));
            } else {
                *skipped += 1;
            }
        }
        Ok(())
    }
    visit(root, "", &mut files, &mut dirs, &mut skipped)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    dirs.sort();
    let mut buf: Vec<u8> = Vec::new();
    for (path, size, hash) in &files {
        buf.extend_from_slice(format!("F\x00{}\x00{}\x00", path, size).as_bytes());
        buf.extend_from_slice(&hash[..]);
        buf.push(0);
    }
    for d in &dirs {
        buf.extend_from_slice(format!("D\x00{}\x00", d).as_bytes());
    }
    buf.extend_from_slice(format!("S\x00{}\x00", skipped).as_bytes());
    buf.extend_from_slice(format!("E\x00{}\x00", engine_stamp()).as_bytes());
    Ok(TreeFingerprint {
        files: files.len(),
        hash: hex(&blake3::hash(&buf).as_bytes()[..]),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Read the previous store's tree fingerprint. `None` on anything unusual —
/// missing file, unreadable file, store predating `store_meta` — which the
/// caller treats as "no short-circuit, rebuild normally".
pub fn read_tree_fingerprint(db: &Path) -> Option<TreeFingerprint> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let raw: String = conn
        .query_row(
            "SELECT value FROM store_meta WHERE key = 'tree_fingerprint'",
            [],
            |r| r.get(0),
        )
        .ok()?;
    let (files, hash) = raw.split_once('|')?;
    let files: usize = files.strip_prefix("files:")?.parse().ok()?;
    let hash = hash.strip_prefix("hash:")?.to_string();
    Some(TreeFingerprint { files, hash })
}

/// Stamp the just-built store with the tree's fingerprint (best-effort: a
/// missing stamp only disables the short-circuit — the safe direction).
pub(crate) fn write_tree_fingerprint(conn: &Connection, root: &Path) -> Result<(), String> {
    let fp = hash_tree(root)?;
    conn.execute(
        "INSERT OR REPLACE INTO store_meta(key, value) VALUES ('tree_fingerprint', ?1)",
        rusqlite::params![format!("files:{}|hash:{}", fp.files, fp.hash)],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Read-only handle on a promoted store — the short-circuit serves
/// `--viewspec` bundles from the previous store without rebuilding.
pub fn open_read_only(db: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("{}: {e}", db.display()))
}

/// Ingest a pre-collected (path, lang, source) list under one transaction
/// with per-file savepoints. The intern caches are shared across the folder:
/// node_kinds and node_text are folder-global tables, so this only memoizes
/// lookups an earlier file already answered — the first-encounter order that
/// assigns every id is unchanged. `reused_ids` (INCREMENTAL-ON) carries the
/// previous store's file ids to copy verbatim BEFORE any parsing, so the
/// parse tier's interned ids land strictly above every copied id.
fn ingest_collected(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    files: Vec<(String, Lang, String)>,
    stats: &mut IngestStats,
    reused_ids: Option<&[i64]>,
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut kind_cache: HashMap<(String, &'static str), i64> = HashMap::new();
    let mut text_cache: HashMap<String, i64> = HashMap::new();
    if let Some(ids) = reused_ids {
        copy_reused_rows(&tx, ids, repo_id, revision, stats)?;
    }
    for (i, (rel, lang, src)) in files.into_iter().enumerate() {
        let sp = format!("f{i}");
        tx.execute_batch(&format!("SAVEPOINT {sp}"))
            .map_err(|e| e.to_string())?;
        match ingest_file_tx(
            &tx,
            repo_id,
            revision,
            &rel,
            lang,
            &src,
            stats,
            &mut kind_cache,
            &mut text_cache,
        ) {
            Ok((_, parse_errored)) => {
                tx.execute_batch(&format!("RELEASE SAVEPOINT {sp}"))
                    .map_err(|e| e.to_string())?;
                stats.files_ingested += 1;
                if parse_errored {
                    stats.files_parse_errors += 1;
                }
            }
            Err(e) => {
                tx.execute_batch(&format!("ROLLBACK TO SAVEPOINT {sp}"))
                    .map_err(|err| err.to_string())?;
                tx.execute_batch(&format!("RELEASE SAVEPOINT {sp}"))
                    .map_err(|err| err.to_string())?;
                // files before the failure stay committed (the old per-file
                // commit outcome); the failed file is fully undone
                tx.commit().map_err(|err| err.to_string())?;
                return Err(e);
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())
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

    /// The old per-node implementation, kept verbatim as the brute-force
    /// reference the stream version must match bit for bit.
    fn shape_hash_brute(root: tree_sitter::Node) -> Vec<u8> {
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

    #[test]
    fn shape_stream_matches_per_node_brute_force_on_every_node() {
        // deep nesting, literals, comments, multi-language: every node of
        // every tree must get exactly the brute-force hash
        let sources: &[(Lang, &str)] = &[
            (
                Lang::Python,
                "class Cart:\n    def add(self, xs):\n        total = 0  # sum\n        for x in xs:\n            if x > 0 and x != 3:\n                total += x\n            else:\n                total = total - 1\n        return {'t': total, 'n': None}\n",
            ),
            (
                Lang::Rust,
                "fn go(xs: Vec<i64>) -> i64 {\n    let mut t = 0;\n    for x in &xs {\n        if x > &2 { t += *x; } else { t -= 1; }\n    }\n    match t { 0 => 1, _ => t * 2 }\n}\n",
            ),
            (
                Lang::TypeScript,
                "class Svc {\n  run(items: string[]): number {\n    let n = 0;\n    for (const it of items) {\n      if (it.length > 2) n += it.length;\n    }\n    return items.map(x => x + n).length;\n  }\n}\n",
            ),
        ];
        for (lang, src) in sources {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.ts_language().expect("grammar"))
                .expect("set lang");
            let tree = parser.parse(src, None).expect("parse");
            let root = tree.root_node();
            assert!(!root.has_error(), "test source must parse clean");
            let by_nid = precompute_shape_hashes(root);
            let mut stack = vec![root];
            let mut checked = 0usize;
            while let Some(n) = stack.pop() {
                assert_eq!(
                    by_nid[&n.id()],
                    shape_hash_brute(n),
                    "shape mismatch at kind {}",
                    n.kind()
                );
                let mut c = n.walk();
                let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
                drop(c);
                stack.extend(ch);
                checked += 1;
            }
            assert!(checked > 50, "corpus too small to be meaningful");
        }
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
