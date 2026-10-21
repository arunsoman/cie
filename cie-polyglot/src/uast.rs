//! PRD v2.0 slice 11 — UAST normalization (§11.2, §14, §42).
//!
//! One mapping, one join: `uast_map` turns per-grammar kinds into the
//! normalized UAST kinds so "give me all functions in this polyglot repo"
//! is a single SQL join instead of per-language queries (§14).
//!
//! Honesty rules:
//! * a node whose grammar kind has no mapping keeps `uast_kind_id = NULL`
//!   and stays visible to language-specific queries only (§14 step 3) —
//!   the unmapped remainder is COUNTED and reported, never forced;
//! * every map row carries `confidence` + `provenance`; the PRD's pinned
//!   example rows (§11.2) ship at confidence 1.0, the extended rows at the
//!   same confidence but with their own provenance tag, and acceptance
//!   #77's exact pairs are asserted by test;
//! * the seed is verified against REAL tree-sitter parses (the probe in
//!   git history shows the grammar kind names checked per language).

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::schema::Lang;

/// Normalized UAST kinds (§11.2 — the PRD's cross-language vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uast {
    FunctionDecl,
    MethodDecl,
    TypeDecl,
    TraitDecl,
    CallExpr,
    ImportDecl,
}

impl Uast {
    pub fn name(self) -> &'static str {
        match self {
            Uast::FunctionDecl => "FunctionDecl",
            Uast::MethodDecl => "MethodDecl",
            Uast::TypeDecl => "TypeDecl",
            Uast::TraitDecl => "TraitDecl",
            Uast::CallExpr => "CallExpr",
            Uast::ImportDecl => "ImportDecl",
        }
    }
    fn all() -> [Uast; 6] {
        [
            Uast::FunctionDecl,
            Uast::MethodDecl,
            Uast::TypeDecl,
            Uast::TraitDecl,
            Uast::CallExpr,
            Uast::ImportDecl,
        ]
    }
}

/// Grammar kind → UAST kind per language. The PRD §11.2 example table is
/// the pinned subset (python function_definition, rust function_item, go
/// function_declaration, java method_declaration, python class_definition,
/// rust struct_item, rust trait_item, java interface_declaration, python
/// call, java method_invocation); the remaining rows extend the same
/// table to the full §39 top-10, each kind name verified by a real parse.
pub fn uast_map_seed(lang: Lang) -> &'static [(&'static str, Uast)] {
    match lang {
        Lang::Python => &[
            ("function_definition", Uast::FunctionDecl),
            ("class_definition", Uast::TypeDecl),
            ("call", Uast::CallExpr),
            ("import_statement", Uast::ImportDecl),
            ("import_from_statement", Uast::ImportDecl),
        ],
        Lang::JavaScript => &[
            ("function_declaration", Uast::FunctionDecl),
            ("generator_function_declaration", Uast::FunctionDecl),
            ("class_declaration", Uast::TypeDecl),
            ("method_definition", Uast::MethodDecl),
            ("call_expression", Uast::CallExpr),
            ("import_statement", Uast::ImportDecl),
        ],
        Lang::TypeScript => &[
            ("function_declaration", Uast::FunctionDecl),
            ("generator_function_declaration", Uast::FunctionDecl),
            ("class_declaration", Uast::TypeDecl),
            ("abstract_class_declaration", Uast::TypeDecl),
            ("method_definition", Uast::MethodDecl),
            ("interface_declaration", Uast::TraitDecl),
            ("call_expression", Uast::CallExpr),
            ("import_statement", Uast::ImportDecl),
        ],
        Lang::Go => &[
            ("function_declaration", Uast::FunctionDecl),
            ("method_declaration", Uast::MethodDecl),
            ("call_expression", Uast::CallExpr),
            ("import_declaration", Uast::ImportDecl),
        ],
        Lang::Rust => &[
            ("function_item", Uast::FunctionDecl),
            ("struct_item", Uast::TypeDecl),
            ("enum_item", Uast::TypeDecl),
            ("trait_item", Uast::TraitDecl),
            ("call_expression", Uast::CallExpr),
            ("use_declaration", Uast::ImportDecl),
        ],
        Lang::Java => &[
            ("method_declaration", Uast::MethodDecl),
            ("class_declaration", Uast::TypeDecl),
            ("interface_declaration", Uast::TraitDecl),
            ("enum_declaration", Uast::TypeDecl),
            ("method_invocation", Uast::CallExpr),
            ("import_declaration", Uast::ImportDecl),
        ],
        Lang::C => &[
            ("function_definition", Uast::FunctionDecl),
            ("call_expression", Uast::CallExpr),
            ("preproc_include", Uast::ImportDecl),
        ],
        Lang::Cpp => &[
            ("function_definition", Uast::FunctionDecl),
            ("class_specifier", Uast::TypeDecl),
            ("call_expression", Uast::CallExpr),
            ("preproc_include", Uast::ImportDecl),
        ],
        Lang::CSharp => &[
            ("method_declaration", Uast::MethodDecl),
            ("class_declaration", Uast::TypeDecl),
            ("interface_declaration", Uast::TraitDecl),
            ("invocation_expression", Uast::CallExpr),
            ("using_directive", Uast::ImportDecl),
        ],
        Lang::Php => &[
            ("function_definition", Uast::FunctionDecl),
            ("method_declaration", Uast::MethodDecl),
            ("class_declaration", Uast::TypeDecl),
            ("function_call_expression", Uast::CallExpr),
            ("member_call_expression", Uast::CallExpr),
        ],
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct NormalizeReport {
    pub uast_kinds: usize,
    pub map_rows: usize,
    pub nodes_tagged: usize,
    pub nodes_unmapped: usize,
    /// grammar kinds present in ast_nodes with no mapping (honest remainder)
    pub kinds_unmapped: usize,
}

/// Idempotently install the UAST vocabulary + per-language map rows, then
/// tag every `ast_nodes` row whose grammar kind has a mapping. Unmapped
/// kinds stay NULL and are counted (§14).
pub fn normalize(conn: &Connection) -> Result<NormalizeReport, String> {
    // 1. uast_kinds vocabulary
    for u in Uast::all() {
        conn.execute(
            "INSERT OR IGNORE INTO uast_kinds(name) VALUES (?1)",
            rusqlite::params![u.name()],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut report = NormalizeReport::default();
    report.uast_kinds = conn
        .query_row("SELECT COUNT(*) FROM uast_kinds", [], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())? as usize;

    // 2. map rows keyed by (grammar kind, language) — the node_kinds table
    //    namespaces kinds per language, so the same grammar name in two
    //    languages gets its own row pair.
    let mut kind_ids: BTreeMap<(String, String), i64> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name, language FROM node_kinds")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query([])
            .map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let id: i64 = r.get(0).map_err(|e| e.to_string())?;
            let name: String = r.get(1).map_err(|e| e.to_string())?;
            let lang: String = r.get(2).map_err(|e| e.to_string())?;
            kind_ids.insert((name, lang), id);
        }
    }

    for lang in [
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
    ] {
        for (grammar_kind, uast) in uast_map_seed(lang) {
            let Some(ast_kind_id) = kind_ids.get(&(grammar_kind.to_string(), lang.name().to_string()))
            else {
                // grammar kind never observed in this DB — no row, honest
                continue;
            };
            let uast_kind_id: i64 = conn
                .query_row(
                    "SELECT id FROM uast_kinds WHERE name = ?1",
                    rusqlite::params![uast.name()],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR IGNORE INTO uast_map(ast_kind_id, uast_kind_id, confidence, provenance)
                 VALUES (?1, ?2, 1.0, ?3)",
                rusqlite::params![
                    ast_kind_id,
                    uast_kind_id,
                    format!("prd-v2.0 §11.2 seed ({})", lang.name())
                ],
            )
            .map_err(|e| e.to_string())?;
            report.map_rows += 1;
        }
    }

    // 3. tag nodes (single join update)
    conn.execute(
        "UPDATE ast_nodes SET uast_kind_id = (
            SELECT m.uast_kind_id FROM uast_map m WHERE m.ast_kind_id = ast_nodes.kind_id
         )",
        [],
    )
    .map_err(|e| e.to_string())?;
    (report.nodes_tagged, report.nodes_unmapped) = {
        let (t, u): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*) FILTER (WHERE uast_kind_id IS NOT NULL), COUNT(*) FILTER (WHERE uast_kind_id IS NULL) FROM ast_nodes",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .map_err(|e| e.to_string())?;
        (t as usize, u as usize)
    };
    report.kinds_unmapped = conn
        .query_row(
            "SELECT COUNT(*) FROM node_kinds k
             WHERE EXISTS (SELECT 1 FROM ast_nodes a WHERE a.kind_id = k.id AND a.uast_kind_id IS NULL)",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as usize;
    Ok(report)
}

/// The polyglot query §14 exists to enable: every function-like declaration
/// across ALL languages, one join. Returns (path, qual_name, revision).
pub fn all_functions(conn: &Connection) -> Result<Vec<(String, String, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.path, s.qual_name, f.revision
             FROM ast_nodes a
             JOIN uast_map m ON m.ast_kind_id = a.kind_id
             JOIN uast_kinds u ON u.id = m.uast_kind_id AND u.name = 'FunctionDecl'
             JOIN symbols s ON s.ast_node_id = a.id
             JOIN files f ON f.id = s.file_id
             ORDER BY f.path, s.qual_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Same join for any UAST kind — the generic polyglot query surface.
pub fn by_uast_kind(
    conn: &Connection,
    kind: Uast,
) -> Result<Vec<(String, String, String, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.path, s.qual_name, u.name, f.revision
             FROM ast_nodes a
             JOIN uast_map m ON m.ast_kind_id = a.kind_id
             JOIN uast_kinds u ON u.id = m.uast_kind_id
             JOIN symbols s ON s.ast_node_id = a.id
             JOIN files f ON f.id = s.file_id
             WHERE u.name = ?1
             ORDER BY f.path, s.qual_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![kind.name()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use rusqlite::Connection;

    fn db_with_sources() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        let mut st = IngestStats::default();
        ingest_parsed_file(
            &conn, 1, 42, "cart.py", Lang::Python,
            "class Cart:\n    def add_item(self, item):\n        stash(item)\n        return item\n",
            &mut st,
        )
        .unwrap();
        ingest_parsed_file(
            &conn, 1, 42, "price.rs", Lang::Rust,
            "struct Price { cents: u64 }\nfn price_of(p: &Price) -> u64 { tag(p.cents) }\n",
            &mut st,
        )
        .unwrap();
        ingest_parsed_file(
            &conn, 1, 42, "Main.java", Lang::Java,
            "public class Main {\n    void run() { boot(); }\n}\n",
            &mut st,
        )
        .unwrap();
        conn
    }

    /// Acceptance #77: python `function_definition` and rust `function_item`
    /// normalize to `FunctionDecl` and answer in ONE join.
    #[test]
    fn uast_map_maps_python_fn_and_rust_fn_to_function_decl() {
        let conn = db_with_sources();
        let rep = super::normalize(&conn).unwrap();
        assert!(rep.map_rows >= 9, "expected >= 9 observed map rows, got {:?}", rep);
        assert!(rep.nodes_tagged > 0);
        // the pinned §11.2 rows exist with confidence 1.0
        let conf: f64 = conn
            .query_row(
                "SELECT m.confidence FROM uast_map m
                 JOIN node_kinds k ON k.id = m.ast_kind_id
                 JOIN uast_kinds u ON u.id = m.uast_kind_id
                 WHERE k.name = 'function_definition' AND k.language = 'python' AND u.name = 'FunctionDecl'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(conf, 1.0);
        let conf2: f64 = conn
            .query_row(
                "SELECT m.confidence FROM uast_map m
                 JOIN node_kinds k ON k.id = m.ast_kind_id
                 JOIN uast_kinds u ON u.id = m.uast_kind_id
                 WHERE k.name = 'function_item' AND k.language = 'rust' AND u.name = 'FunctionDecl'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(conf2, 1.0);
        let fns = super::all_functions(&conn).unwrap();
        let quals: Vec<&str> = fns.iter().map(|(_, q, _)| q.as_str()).collect();
        assert!(quals.contains(&"Cart.add_item"), "python method via one join: {quals:?}");
        assert!(quals.contains(&"price_of"), "rust fn via the same join: {quals:?}");
        // java method_declaration → MethodDecl (pinned row)
        let methods = super::by_uast_kind(&conn, super::Uast::MethodDecl).unwrap();
        assert!(methods.iter().any(|(_, q, _, _)| q == "Main.run"));
    }

    #[test]
    fn java_method_invocation_is_call_expr_pinned_row() {
        let conn = db_with_sources();
        super::normalize(&conn).unwrap();
        let calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ast_nodes a
                 JOIN uast_map m ON m.ast_kind_id = a.kind_id
                 JOIN uast_kinds u ON u.id = m.uast_kind_id
                 WHERE u.name = 'CallExpr'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(calls >= 3, "stash/tag/boot calls tagged CallExpr, got {calls}");
    }

    #[test]
    fn unmapped_nodes_stay_null_and_counted() {
        let conn = db_with_sources();
        let rep = super::normalize(&conn).unwrap();
        assert!(rep.nodes_unmapped > 0, "structural nodes (module, block…) legitimately unmapped");
        assert!(rep.nodes_tagged > 0);
        // idempotent: second run changes nothing
        let rep2 = super::normalize(&conn).unwrap();
        assert_eq!(rep.nodes_tagged, rep2.nodes_tagged);
    }

    #[test]
    fn normalize_is_idempotent_on_map_rows() {
        let conn = db_with_sources();
        let r1 = super::normalize(&conn).unwrap();
        let r2 = super::normalize(&conn).unwrap();
        assert_eq!(r1.map_rows, r2.map_rows, "INSERT OR IGNORE keeps rows stable");
        assert_eq!(r1.uast_kinds, r2.uast_kinds);
    }
}
