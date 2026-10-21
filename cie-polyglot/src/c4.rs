//! PRD v2.0 slice 12b — C4 container and component detection (§20),
//! emitting the §11.7 concept DAG tables (concept_nodes / concept_edges /
//! concept_node_symbols).
//!
//! The correction that eliminated the wall of file paths (§22.1) is
//! structural here: files NEVER become ConceptNodes — nodes are the
//! project (Domain), one Container per manifest root, components from
//! framework annotations (Fact) / naming suffixes (Inference) / DDD
//! directory segments (Inference), and classes link under their container
//! or component. Files appear only as evidence in `provenance` and in
//! `symbols.file_id`.
//!
//! Every edge is traceable (§22.3 closed reason set):
//! `PackageContainment | ClusterCohesion | FcaSubsumption | CallOwnership`,
//! and relationships carry the §20.3 derivation (`uses`, `depends-on`,
//! `calls` with DECLARED=Fact / URL_MATCHED=Inference).

use std::collections::BTreeMap;

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// DDL — §11.7 exactly + the relationship table (documented extension)
// ---------------------------------------------------------------------------

pub const DDL_C4: &str = r#"
CREATE TABLE IF NOT EXISTS concept_nodes (
    id            INTEGER PRIMARY KEY,
    label         TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    revision      INTEGER NOT NULL,
    confidence    REAL    NOT NULL,
    provenance    TEXT    NOT NULL,
    primary_edge  TEXT    NOT NULL,
    semantic_hash BLOB    NOT NULL
);

CREATE TABLE IF NOT EXISTS concept_edges (
    parent_id     INTEGER NOT NULL REFERENCES concept_nodes(id),
    child_id      INTEGER NOT NULL REFERENCES concept_nodes(id),
    reason        TEXT    NOT NULL,
    is_primary    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (parent_id, child_id, reason)
);

CREATE TABLE IF NOT EXISTS concept_node_symbols (
    concept_id    INTEGER NOT NULL REFERENCES concept_nodes(id),
    symbol_id     INTEGER NOT NULL REFERENCES symbols(id),
    PRIMARY KEY (concept_id, symbol_id)
);

-- §20.3 relationships (from-node → to-node), the arrows of the C4 view.
-- Documented extension: the PRD defines the derivation but not the table.
CREATE TABLE IF NOT EXISTS c4_relationships (
    from_node   INTEGER NOT NULL REFERENCES concept_nodes(id),
    to_node     INTEGER NOT NULL REFERENCES concept_nodes(id),
    kind        TEXT NOT NULL,
    resolution  TEXT NOT NULL,
    confidence  REAL NOT NULL,
    PRIMARY KEY (from_node, to_node, kind)
);
"#;

pub fn create_c4_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_C4).map_err(|e| e.to_string())
}

/// Manifests that root a container (§20.1 source 1).
pub const MANIFESTS: &[(&str, &str)] = &[
    ("pom.xml", "maven"),
    ("build.gradle", "gradle"),
    ("Cargo.toml", "cargo"),
    ("go.mod", "go"),
    ("package.json", "npm"),
    ("pyproject.toml", "python"),
];

/// Naming suffix groups (§20.2 signal 2, Inference).
pub const SUFFIX_GROUPS: &[(&str, &str, &str)] = &[
    ("Repository", "Persistence", "Service"),
    ("Controller", "Interface", "Service"),
    ("Handler", "Interface", "Service"),
    ("Service", "Service", "Service"),
];

/// DDD directory segments (§20.2 signal 4, Inference).
pub const DDD_DIRS: &[&str] = &["domain", "application", "infrastructure"];

#[derive(Debug, Default, serde::Serialize)]
pub struct C4Report {
    pub project: usize,
    pub containers: usize,
    pub components: usize,
    pub classes_linked: usize,
    pub relationships: usize,
}

struct NodeOut {
    label: String,
    kind: &'static str,
    confidence: f64,
    provenance: String,
    symbols: Vec<i64>,
}

fn semantic_hash(label: &str, kind: &str, provenance: &str) -> Vec<u8> {
    blake3::hash(format!("{kind}\u{1}{label}\u{1}{provenance}").as_bytes())
        .as_bytes()
        .to_vec()
}

/// Build the C4 hierarchy over an ingested + lowered store. `root` is the
/// ingested folder (for DDD dir checks); `revision` stamps the nodes.
pub fn detect_c4(conn: &Connection, root: &std::path::Path, revision: i64) -> Result<C4Report, String> {
    let mut report = C4Report::default();
    let files: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, path FROM files ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    // ---- project node (Domain) -------------------------------------------
    let project_label = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let project = insert_node(
        conn,
        &NodeOut {
            label: project_label.clone(),
            kind: "Domain",
            confidence: 1.0,
            provenance: "c4 §20.1: project root".into(),
            symbols: vec![],
        },
        revision,
    )?;
    report.project = 1;

    // ---- containers: one per manifest root (§20.1) ------------------------
    // file scope per container (dir prefix)
    let mut containers: Vec<(i64, String)> = Vec::new(); // (node id, dir prefix)
    let mut file_container: BTreeMap<i64, i64> = BTreeMap::new();
    {
        let mut manifest_dirs: Vec<(String, String)> = Vec::new(); // (dir, tool)
        for (id, path) in &files {
            for (m, tool) in MANIFESTS {
                if path == *m || path.ends_with(&format!("/{m}")) {
                    let dir = path
                        .rsplit_once('/')
                        .map(|(d, _)| d.to_string())
                        .unwrap_or_default();
                    manifest_dirs.push((dir, tool.to_string()));
                }
            }
        }
        manifest_dirs.sort();
        manifest_dirs.dedup();
        for (dir, tool) in &manifest_dirs {
            let label = if dir.is_empty() {
                project_label.clone()
            } else {
                dir.rsplit('/').next().unwrap_or(dir).to_string()
            };
            let cid = insert_node(
                conn,
                &NodeOut {
                    label: label.clone(),
                    kind: "Container",
                    confidence: 1.0,
                    provenance: format!("manifest:{} ({tool})", if dir.is_empty() { "./".into() } else { format!("{dir}/") }),
                    symbols: vec![],
                },
                revision,
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO concept_edges(parent_id, child_id, reason, is_primary) VALUES (?1, ?2, 'PackageContainment', 1)",
                rusqlite::params![project, cid],
            )
            .map_err(|e| e.to_string())?;
            containers.push((cid, dir.clone()));
            report.containers += 1;
        }

        // scope files into containers (longest-prefix wins)
        for (id, path) in &files {
            let mut best: Option<(usize, i64)> = None;
            for (cid, dir) in &containers {
                let d = dir.as_str();
                let scope = if d.is_empty() {
                    1
                } else if path.starts_with(&format!("{d}/")) {
                    d.len()
                } else {
                    continue;
                };
                if best.map(|(l, _)| scope > l).unwrap_or(true) {
                    best = Some((scope, *cid));
                }
            }
            if let Some((_, cid)) = best {
                file_container.insert(*id, cid);
            }
        }
    }

    // ---- components (§20.2) ------------------------------------------------
    //  a. framework annotation sites (Fact when confidence 1.0)
    {
        let mut stmt = conn
            .prepare(
                "SELECT r.file_id, r.symbol_id, r.feature, r.framework, r.evidence_text, r.confidence
                 FROM framework_roles r ORDER BY r.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, f64>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (file_id, symbol_id, feature, framework, evidence, conf) in rows {
            let Some(cid) = file_container.get(&file_id).copied() else { continue };
            let kind = match feature.as_str() {
                "route" | "service" => "Service",
                "repository" | "entity" => "Entity",
                _ => "Service",
            };
            // label from the symbol name when the role site is a decl
            let label = symbol_id
                .and_then(|sid| {
                    conn.query_row("SELECT name FROM symbols WHERE id = ?1", rusqlite::params![sid], |r| {
                        r.get::<_, String>(0)
                    })
                    .ok()
                })
                .unwrap_or_else(|| format!("{feature} ({framework})"));
            let node = insert_node(
                conn,
                &NodeOut {
                    label,
                    kind,
                    confidence: conf,
                    provenance: format!("framework:{framework} — {evidence}"),
                    symbols: symbol_id.into_iter().collect(),
                },
                revision,
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO concept_edges(parent_id, child_id, reason, is_primary) VALUES (?1, ?2, 'PackageContainment', 1)",
                rusqlite::params![cid, node],
            )
            .map_err(|e| e.to_string())?;
            report.components += 1;
        }
    }

    //  b. naming suffix groups (Inference) — per container
    for (cid, dir) in &containers {
        let mut stmt = conn
            .prepare(
                "SELECT id, name FROM symbols
                 WHERE file_id IN (SELECT id FROM files WHERE ?1 = '' OR path LIKE ?2 || '/%')
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let like = format!("{dir}");
        let rows = stmt
            .query_map(rusqlite::params![dir, like], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut groups: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for (sid, name) in rows {
            for (suffix, group, _kind) in SUFFIX_GROUPS {
                if name.ends_with(suffix) {
                    // symbol-level annotation nodes (a) already exist for
                    // framework-attributed decls; the suffix group is the
                    // cohesion view — attach the symbol to BOTH, never fake
                    // one from the other
                    groups.entry(group.to_string()).or_default().push(sid);
                }
            }
        }
        for (group, syms) in groups {
            let node = insert_node(
                conn,
                &NodeOut {
                    label: group.clone(),
                    kind: "Service",
                    confidence: 0.7,
                    provenance: "naming-suffix (§20.2 signal 2, Inference)".into(),
                    symbols: syms,
                },
                revision,
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO concept_edges(parent_id, child_id, reason, is_primary) VALUES (?1, ?2, 'ClusterCohesion', 0)",
                rusqlite::params![cid, node],
            )
            .map_err(|e| e.to_string())?;
            report.components += 1;
        }

        //  c. DDD directory segments (Inference)
        for seg in DDD_DIRS {
            let seg_path = if dir.is_empty() {
                format!("{seg}/")
            } else {
                format!("{dir}/{seg}/")
            };
            let n_files: i64 = {
                let mut stmt = conn
                    .prepare("SELECT COUNT(*) FROM files WHERE path LIKE ?1")
                    .map_err(|e| e.to_string())?;
                stmt.query_row(rusqlite::params![format!("{seg_path}%")], |r| r.get(0))
                    .unwrap_or(0)
            };
            if n_files > 0 {
                let node = insert_node(
                    conn,
                    &NodeOut {
                        label: seg.to_string(),
                        kind: "Service",
                        confidence: 0.7,
                        provenance: format!("ddd-dir:{seg_path} (§20.2 signal 4, Inference)"),
                        symbols: vec![],
                    },
                    revision,
                )?;
                conn.execute(
                    "INSERT OR IGNORE INTO concept_edges(parent_id, child_id, reason, is_primary) VALUES (?1, ?2, 'PackageContainment', 0)",
                    rusqlite::params![cid, node],
                )
                .map_err(|e| e.to_string())?;
                report.components += 1;
            }
        }
    }

    // ---- classes link under their container/component (§22.1) -------------
    // TypeDecl symbols with no node yet link directly to the container —
    // classes link to C4 components; files never appear.
    {
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.name, s.file_id FROM symbols s
                 JOIN ast_nodes a ON a.id = s.ast_node_id
                 JOIN uast_map m ON m.ast_kind_id = a.kind_id
                 JOIN uast_kinds u ON u.id = m.uast_kind_id
                 WHERE u.name IN ('TypeDecl', 'TraitDecl')",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        for (sid, name, file_id) in rows {
            // skip symbols already carried by a node
            let carried: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM concept_node_symbols WHERE symbol_id = ?1",
                    rusqlite::params![sid],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if carried > 0 {
                continue;
            }
            let Some(cid) = file_container.get(&file_id).copied() else { continue };
            let node = insert_node(
                conn,
                &NodeOut {
                    label: name,
                    kind: "Entity",
                    confidence: 1.0,
                    provenance: "class node (§22.1: classes link to components, files are evidence)".into(),
                    symbols: vec![sid],
                },
                revision,
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO concept_edges(parent_id, child_id, reason, is_primary) VALUES (?1, ?2, 'ClusterCohesion', 1)",
                rusqlite::params![cid, node],
            )
            .map_err(|e| e.to_string())?;
            report.classes_linked += 1;
        }
    }

    // ---- relationships (§20.3) --------------------------------------------
    // cross-component calls → uses; imports → depends-on (v1: calls, via the
    // calls table joined through component membership)
    {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT cs.symbol_id, c.callee_symbol_id
                 FROM calls c
                 JOIN concept_node_symbols cs ON cs.symbol_id = c.caller_symbol_id
                 WHERE c.callee_symbol_id IS NOT NULL AND c.resolution = 'RESOLVED'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        // map symbol → its concept node; symbols with no dedicated node
        // (bare functions) resolve to their file's container node, so a
        // cross-container call is still visible (never dropped silently)
        let node_of_sym: BTreeMap<i64, i64> = {
            let mut stmt = conn
                .prepare("SELECT concept_id, symbol_id FROM concept_node_symbols")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            drop(stmt);
            let mut m: BTreeMap<i64, i64> = rows.into_iter().collect();
            // container fallback for un-carried symbols
            let mut fstmt = conn
                .prepare("SELECT id, file_id FROM symbols")
                .map_err(|e| e.to_string())?;
            let srows = fstmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            drop(fstmt);
            for (sid, fid) in srows {
                if let Some(cid) = file_container.get(&fid) {
                    m.entry(sid).or_insert(*cid);
                }
            }
            m
        };
        for (caller_sym, callee_sym) in rows {
            let (Some(from), Some(to)) = (node_of_sym.get(&caller_sym), node_of_sym.get(&callee_sym))
            else {
                continue;
            };
            if from == to {
                continue;
            }
            conn.execute(
                "INSERT OR IGNORE INTO c4_relationships(from_node, to_node, kind, resolution, confidence)
                 VALUES (?1, ?2, 'uses', 'static', 1.0)",
                rusqlite::params![from, to],
            )
            .map_err(|e| e.to_string())?;
            report.relationships += 1;
        }
        // DECLARED / URL_MATCHED bindings → calls (Fact / Inference)
        let mut stmt = conn
            .prepare(
                "SELECT b.resolution, b.source_symbol_id FROM cross_language_bindings b WHERE b.source_symbol_id IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        for (resolution, src_sym) in rows {
            let Some(from) = node_of_sym.get(&src_sym).copied() else { continue };
            let (conf, res) = if resolution == "DECLARED" {
                (1.0, "DECLARED")
            } else {
                (0.7, "URL_MATCHED")
            };
            conn.execute(
                "INSERT OR IGNORE INTO c4_relationships(from_node, to_node, kind, resolution, confidence)
                 VALUES (?1, ?2, 'calls', ?3, ?4)",
                rusqlite::params![from, project, res, conf],
            )
            .map_err(|e| e.to_string())?;
            report.relationships += 1;
        }
    }

    Ok(report)
}

fn insert_node(conn: &Connection, n: &NodeOut, revision: i64) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO concept_nodes(label, kind, revision, confidence, provenance, primary_edge, semantic_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            n.label,
            n.kind,
            revision,
            n.confidence,
            n.provenance,
            match n.kind {
                "Domain" => "PackageContainment",
                "Container" => "PackageContainment",
                _ => "ClusterCohesion",
            },
            semantic_hash(&n.label, n.kind, &n.provenance)
        ],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    for s in &n.symbols {
        conn.execute(
            "INSERT OR IGNORE INTO concept_node_symbols(concept_id, symbol_id) VALUES (?1, ?2)",
            rusqlite::params![id, s],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::{create_fw_schema, detect, install_registry};
    use crate::ir::{cache_source_bytes, create_ir_schema, lower_all};
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_ir_schema(&conn).unwrap();
        create_fw_schema(&conn).unwrap();
        create_c4_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn containers_from_manifests_project_domain_root() {
        let conn = setup();
        let mut st = IngestStats::default();
        // two manifest roots → two containers under one project Domain
        let f1 = ingest_parsed_file(&conn, 1, 41, "checkout-api/lib.py", Lang::Python, "def total(c):\n    return 1\n", &mut st).unwrap();
        let _ = f1;
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'checkout-api/pyproject.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"pyproject").as_bytes().to_vec()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'pricing-engine/Cargo.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"cargo").as_bytes().to_vec()],
        )
        .unwrap();
        let rep = detect_c4(&conn, std::path::Path::new("/tmp/demo"), 41).unwrap();
        assert_eq!(rep.containers, 2);
        assert_eq!(rep.project, 1);
        let kinds: Vec<(String, String)> = {
            let mut stmt = conn.prepare("SELECT kind, label FROM concept_nodes ORDER BY kind").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(kinds.iter().any(|(k, l)| k == "Domain" && l == "demo"), "{kinds:?}");
        assert!(kinds.iter().any(|(k, l)| k == "Container" && l == "checkout-api"), "{kinds:?}");
        assert!(kinds.iter().any(|(k, l)| k == "Container" && l == "pricing-engine"), "{kinds:?}");
        // hierarchy edge from project → container with the §22.3 reason
        let reasons: Vec<String> = {
            let mut stmt = conn.prepare("SELECT DISTINCT reason FROM concept_edges").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert!(reasons.iter().all(|r| matches!(r.as_str(), "PackageContainment" | "ClusterCohesion" | "FcaSubsumption" | "CallOwnership")), "{reasons:?}");
    }

    #[test]
    fn no_file_path_ever_becomes_a_concept_node() {
        // acceptance #28 guard, enforced at the C4 construction site
        let conn = setup();
        let mut st = IngestStats::default();
        ingest_parsed_file(&conn, 1, 41, "svc/cart_page.py", Lang::Python, "def cart_page():\n    return 1\n", &mut st).unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'pyproject.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"pyproject").as_bytes().to_vec()],
        )
        .unwrap();
        detect_c4(&conn, std::path::Path::new("/tmp/demo"), 41).unwrap();
        let bad: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM concept_nodes WHERE label LIKE '%.py' OR label LIKE '%.rs' OR label LIKE '%.ts' OR label LIKE '%.go' OR label LIKE '%.java'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad, 0, "files are evidence metadata, never nodes");
    }

    #[test]
    fn framework_annotation_component_is_fact_and_cites_evidence() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(\"/items\")\ndef list_items():\n    return []\n";
        let f = ingest_parsed_file(&conn, 1, 41, "svc/main.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'pyproject.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"pyproject").as_bytes().to_vec()],
        )
        .unwrap();
        install_registry(&conn).unwrap();
        detect(&conn).unwrap();
        let rep = detect_c4(&conn, std::path::Path::new("/tmp/demo"), 41).unwrap();
        assert!(rep.components >= 1);
        let (label, conf, prov): (String, f64, String) = conn
            .query_row(
                "SELECT label, confidence, provenance FROM concept_nodes WHERE kind='Service' AND provenance LIKE 'framework:%'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(label, "list_items");
        assert_eq!(conf, 1.0, "framework annotation is Fact");
        assert!(prov.contains("fastapi"), "provenance cites the evidence: {prov}");
    }

    #[test]
    fn naming_suffix_group_is_inference() {
        let conn = setup();
        let mut st = IngestStats::default();
        let rs = "struct OrderRepository;\nimpl OrderRepository {\n    fn load(&self) -> u32 { 1 }\n}\n";
        let f = ingest_parsed_file(&conn, 1, 41, "svc/repo.rs", Lang::Rust, rs, &mut st).unwrap();
        cache_source_bytes(&conn, f, rs).unwrap();
        lower_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'Cargo.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"cargo").as_bytes().to_vec()],
        )
        .unwrap();
        let rep = detect_c4(&conn, std::path::Path::new("/tmp/demo"), 41).unwrap();
        let (label, conf): (String, f64) = conn
            .query_row(
                "SELECT label, confidence FROM concept_nodes WHERE provenance LIKE 'naming-suffix%'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(label, "Persistence");
        assert_eq!(conf, 0.7, "naming suffix is Inference");
        let syms: i64 = conn
            .query_row("SELECT COUNT(*) FROM concept_node_symbols WHERE symbol_id = (SELECT id FROM symbols WHERE name='OrderRepository')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(syms, 1);
        assert!(rep.classes_linked >= 0);
    }

    #[test]
    fn cross_component_call_becomes_uses_relationship() {
        let conn = setup();
        let mut st = IngestStats::default();
        // caller in container A (Cargo.toml), callee struct in same file —
        // both become class nodes under ONE container (single manifest);
        // to get two containers, put the callee under another manifest root
        let f1 = ingest_parsed_file(&conn, 1, 41, "api/main.py", Lang::Python, "class Api:\n    def handle(self):\n        return store_save()\n", &mut st).unwrap();
        cache_source_bytes(&conn, f1, "class Api:\n    def handle(self):\n        return store_save()\n").unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'api/pyproject.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"pyproject").as_bytes().to_vec()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'store/Cargo.toml', ?1, 'toml', 41, 10, 'none')",
            rusqlite::params![blake3::hash(b"cargo").as_bytes().to_vec()],
        )
        .unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "store/lib.py", Lang::Python, "def store_save():\n    return 1\n", &mut st).unwrap();
        cache_source_bytes(&conn, f2, "def store_save():\n    return 1\n").unwrap();
        lower_all(&conn).unwrap();
        let rep = detect_c4(&conn, std::path::Path::new("/tmp/demo"), 41).unwrap();
        // the name-resolved call crosses the two containers' class nodes
        assert!(rep.relationships >= 1, "cross-component uses edge exists: {rep:?}");
        let kinds: Vec<String> = {
            let mut stmt = conn.prepare("SELECT DISTINCT kind FROM c4_relationships").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert!(kinds.contains(&"uses".to_string()));
    }
}
