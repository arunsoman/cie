//! PRD v2.0 slice 12 — framework detection (§11.9, §18) and
//! cross-language bindings (§11.8, §42).
//!
//! The `framework_adapters` table is the registry (one row per
//! language-framework-feature, §11.9 columns exactly); detection applies
//! real AST evidence over the cached sources and lands per-site rows in
//! `framework_roles` (the documented detection-output extension — the PRD
//! defines the registry, C4 §20 consumes its output).
//!
//! Framework attribution honesty: a framework NAME is attached only when
//! its own import/use evidence exists (`from fastapi import FastAPI`,
//! `use actix_web::...`, `github.com/gin-gonic/gin`); decorator/attribute
//! shape alone yields feature=route with framework `unattributed` at
//! confidence 0.5 — shape is never promoted to identity.
//!
//! Cross-language bindings follow the §11.8 three tiers exactly:
//! * `DECLARED` — the path exists in a parsed OpenAPI spec (confidence 1.0,
//!   acceptance #78);
//! * `URL_MATCHED` — a real client call's path literal matches a declared
//!   spec path (confidence 0.7, acceptance #79);
//! * `RUNTIME_OBSERVED` — reserved for the slice-20 runtime layer, never
//!   written by static detection.

use rusqlite::Connection;

use crate::schema::Lang;

// ---------------------------------------------------------------------------
// DDL — §11.9 registry exactly + §11.8 exactly + the detection-output table
// ---------------------------------------------------------------------------

pub const DDL_FW: &str = r#"
CREATE TABLE IF NOT EXISTS framework_adapters (
    id           INTEGER PRIMARY KEY,
    language     TEXT NOT NULL,
    framework    TEXT NOT NULL,
    feature      TEXT NOT NULL,
    ast_kind     TEXT NOT NULL,
    match_value  TEXT NOT NULL,
    confidence   REAL NOT NULL DEFAULT 1.0
);
CREATE INDEX IF NOT EXISTS idx_fw_lang ON framework_adapters(language, feature);

CREATE TABLE IF NOT EXISTS framework_roles (
    id            INTEGER PRIMARY KEY,
    file_id       INTEGER NOT NULL REFERENCES files(id),
    symbol_id     INTEGER REFERENCES symbols(id),
    feature       TEXT NOT NULL,
    framework     TEXT NOT NULL,
    evidence_text TEXT NOT NULL,
    confidence    REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fw_roles_file ON framework_roles(file_id);

CREATE TABLE IF NOT EXISTS cross_language_bindings (
    id                INTEGER PRIMARY KEY,
    source_symbol_id  INTEGER REFERENCES symbols(id),
    target_symbol_id  INTEGER REFERENCES symbols(id),
    protocol          TEXT NOT NULL,
    spec_file_id      INTEGER REFERENCES files(id),
    spec_pointer      TEXT,
    confidence        REAL NOT NULL,
    resolution        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_xlang_source ON cross_language_bindings(source_symbol_id);
CREATE INDEX IF NOT EXISTS idx_xlang_target ON cross_language_bindings(target_symbol_id);
"#;

pub fn create_fw_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_FW).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// §18 registry — one row per language-framework-feature (§11.9 columns)
// ---------------------------------------------------------------------------

pub const REGISTRY: &[(&str, &str, &str, &str, &str, f64)] = &[
    // Java — Spring Boot (§18 required)
    ("java", "spring-boot", "route", "method_declaration", "@GetMapping | @PostMapping | @PutMapping | @DeleteMapping | @RequestMapping", 1.0),
    ("java", "spring-boot", "service", "class_declaration", "@Service", 1.0),
    ("java", "spring-boot", "repository", "interface_declaration", "@Repository", 1.0),
    ("java", "spring-boot", "entity", "class_declaration", "@Entity", 1.0),
    ("java", "spring-boot", "config", "class_declaration", "@Configuration", 1.0),
    ("java", "spring-boot", "boot", "class_declaration", "@SpringBootApplication", 1.0),
    ("java", "spring-boot", "di-injected", "field_declaration", "@Autowired", 1.0),
    // Rust — Actix / Axum / Rocket
    ("rust", "actix-web", "route", "function_item", "#[get(..)] / #[post(..)] attribute", 1.0),
    ("rust", "axum", "route", "call_expression", ".route(path, get(handler))", 1.0),
    ("rust", "rocket", "route", "function_item", "#[get(..)] attribute + rocket import", 1.0),
    // Go — Gin / Echo / Chi
    ("go", "gin", "route", "call_expression", "r.GET(path, handler)", 1.0),
    ("go", "echo", "route", "call_expression", "e.GET(path, handler)", 1.0),
    ("go", "chi", "route", "call_expression", "r.Get(path, handler)", 1.0),
    // Python — Django / FastAPI / Flask
    ("python", "fastapi", "route", "function_definition", "@app.get(..) decorator + fastapi import", 1.0),
    ("python", "flask", "route", "function_definition", "@app.route(..) decorator + flask import", 1.0),
    ("python", "django", "route", "function_definition", "django import + view function", 1.0),
    // TypeScript — Express / NestJS / Next.js
    ("typescript", "express", "route", "call_expression", "app.get(path, handler)", 1.0),
    ("typescript", "nestjs", "route", "method_definition", "@Get(..) decorator", 1.0),
    ("javascript", "express", "route", "call_expression", "app.get(path, handler)", 1.0),
    // C# — ASP.NET Core
    ("csharp", "aspnetcore", "route", "method_declaration", "[HttpGet] attribute", 1.0),
    ("csharp", "aspnetcore", "route", "call_expression", "app.MapGet(path, handler)", 1.0),
];

/// Install the registry (idempotent; keyed by the §11.9 unique tuple).
pub fn install_registry(conn: &Connection) -> Result<usize, String> {
    let mut n = 0;
    for (lang, fw, feature, ast_kind, match_value, conf) in REGISTRY {
        conn.execute(
            "INSERT INTO framework_adapters(language, framework, feature, ast_kind, match_value, confidence)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6
             WHERE NOT EXISTS (
                SELECT 1 FROM framework_adapters
                WHERE language = ?1 AND framework = ?2 AND feature = ?3 AND ast_kind = ?4
             )",
            rusqlite::params![lang, fw, feature, ast_kind, match_value, conf],
        )
        .map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Detection over the cached sources
// ---------------------------------------------------------------------------

#[derive(Debug, Default, serde::Serialize)]
pub struct FwReport {
    pub roles: usize,
    pub declared_bindings: usize,
    pub url_matched_bindings: usize,
    pub specs_parsed: usize,
}

/// Detect framework roles + cross-language bindings over everything in the
/// store. Sources come from the slice-10 cache; specs (OpenAPI) come from
/// the ingested files whose text parses as OpenAPI.
pub fn detect(conn: &Connection) -> Result<FwReport, String> {
    let mut report = FwReport::default();
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
    for (file_id, path) in files {
        let Some(lang) = Lang::from_path(&path) else {
            // spec files are not code — try them as OpenAPI
            if is_spec_path(&path) {
                let compressed: Vec<u8> = match conn
                    .query_row(
                        "SELECT zstd_bytes FROM file_source_cache WHERE file_id = ?1",
                        rusqlite::params![file_id],
                        |r| r.get(0),
                    ) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let src = match zstd::bulk::decompress(&compressed, 16 << 20) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let text = String::from_utf8_lossy(&src).to_string();
                if let Some(paths) = extract_spec_paths(&text) {
                    report.specs_parsed += 1;
                    for p in paths {
                        conn.execute(
                            "INSERT INTO cross_language_bindings(protocol, spec_file_id, spec_pointer, confidence, resolution)
                             VALUES ('http', ?1, ?2, 1.0, 'DECLARED')",
                            rusqlite::params![file_id, p],
                        )
                        .map_err(|e| e.to_string())?;
                        report.declared_bindings += 1;
                    }
                }
            }
            continue;
        };
        let compressed: Vec<u8> = match conn
            .query_row(
                "SELECT zstd_bytes FROM file_source_cache WHERE file_id = ?1",
                rusqlite::params![file_id],
                |r| r.get(0),
            ) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let src = zstd::bulk::decompress(&compressed, 16 << 20).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&src).to_string();

        // 1. framework import/use attribution evidence
        let attributed = attribute_framework(lang, &text);
        // 2. role sites by language-specific walkers
        let sites = detect_roles(conn, file_id, lang, &text, &attributed, &mut report)?;

        // 3. URL_MATCHED bindings: client path literals vs declared specs
        let specs: Vec<(i64, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, spec_pointer FROM cross_language_bindings WHERE resolution='DECLARED'")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        if !specs.is_empty() {
            for (sym_id, literal) in client_path_literals(conn, file_id, lang, &text)? {
                for (spec_id, pointer) in &specs {
                    if literal == *pointer || literal == format!("/api{pointer}") {
                        conn.execute(
                            "INSERT INTO cross_language_bindings(source_symbol_id, protocol, spec_file_id, spec_pointer, confidence, resolution)
                             VALUES (?1, 'http', ?2, ?3, 0.7, 'URL_MATCHED')",
                            rusqlite::params![sym_id, spec_id, pointer],
                        )
                        .map_err(|e| e.to_string())?;
                        report.url_matched_bindings += 1;
                        break;
                    }
                }
            }
        }
        let _ = sites;
    }
    Ok(report)
}

fn is_spec_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".yaml") || p.ends_with(".yml") || p.ends_with(".json"))
        && (p.contains("openapi") || p.contains("swagger"))
}

/// OpenAPI paths: JSON via serde_json; YAML via the top-level two-space
/// `  /path:` convention (no yaml dependency; unmatched shapes yield an
/// empty set, never a guess).
pub fn extract_spec_paths(text: &str) -> Option<Vec<String>> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let paths = v.get("paths")?.as_object()?;
        return Some(paths.keys().cloned().collect());
    }
    // YAML: the paths block runs from `paths:` until the next top-level key
    // (column-0 text); entries are the indented `/path:` keys at any depth.
    let mut in_paths = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("paths:") {
            in_paths = true;
            continue;
        }
        if !in_paths {
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            // next top-level key — the paths block ended
            in_paths = false;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('/') {
            let entry = trimmed.split(':').next().unwrap_or("").trim().to_string();
            if !entry.is_empty() {
                out.push(entry);
            }
        }
    }
    Some(out)
}

/// Framework identity only from its own import/use evidence.
fn attribute_framework(lang: Lang, text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut push = |fw: &str| out.push((fw.to_string(), lang.name().to_string()));
    match lang {
        Lang::Python => {
            if text.contains("from fastapi") || text.contains("import fastapi") {
                push("fastapi");
            }
            if text.contains("from flask") || text.contains("import flask") {
                push("flask");
            }
            if text.contains("import django") || text.contains("from django") {
                push("django");
            }
        }
        Lang::Rust => {
            if text.contains("actix_web") {
                push("actix-web");
            }
            if text.contains("use axum") || text.contains("axum::") {
                push("axum");
            }
            if text.contains("use rocket") || text.contains("rocket::") {
                push("rocket");
            }
        }
        Lang::Go => {
            if text.contains("github.com/gin-gonic/gin") {
                push("gin");
            }
            if text.contains("github.com/labstack/echo") {
                push("echo");
            }
            if text.contains("github.com/go-chi/chi") {
                push("chi");
            }
        }
        Lang::JavaScript | Lang::TypeScript => {
            if text.contains("require('express')")
                || text.contains("require(\"express\")")
                || text.contains("from 'express'")
                || text.contains("from \"express\"")
            {
                push("express");
            }
            if text.contains("@nestjs/") {
                push("nestjs");
            }
        }
        Lang::Java => {
            if text.contains("org.springframework") {
                push("spring-boot");
            }
        }
        Lang::CSharp => {
            if text.contains("Microsoft.AspNetCore") {
                push("aspnetcore");
            }
        }
        _ => {}
    }
    out
}

fn role_id(conn: &Connection, file_id: i64, span: (usize, usize)) -> Option<i64> {
    conn.query_row(
        "SELECT s.id FROM symbols s WHERE s.file_id = ?1 AND s.start_byte = ?2 AND s.end_byte = ?3",
        rusqlite::params![file_id, span.0 as i64, span.1 as i64],
        |r| r.get(0),
    )
    .ok()
}

fn write_role(
    conn: &Connection,
    report: &mut FwReport,
    file_id: i64,
    span: (usize, usize),
    feature: &str,
    framework: &str,
    evidence: &str,
    confidence: f64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO framework_roles(file_id, symbol_id, feature, framework, evidence_text, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            file_id,
            role_id(conn, file_id, span),
            feature,
            framework,
            evidence.chars().take(160).collect::<String>(),
            confidence
        ],
    )
    .map_err(|e| e.to_string())?;
    report.roles += 1;
    Ok(())
}

/// Per-language role walkers. Every emitted row cites its evidence text.
fn detect_roles(
    conn: &Connection,
    file_id: i64,
    lang: Lang,
    text: &str,
    attributed: &[(String, String)],
    report: &mut FwReport,
) -> Result<Vec<(String, String)>, String> {
    let fw_of = |names: &[&str]| -> String {
        for (fw, _) in attributed {
            for want in names {
                if fw == want {
                    return fw.clone();
                }
            }
        }
        "unattributed".to_string()
    };
    let conf_of = |fw: &str| if fw == "unattributed" { 0.5 } else { 1.0 };
    let mut sites: Vec<(String, String)> = Vec::new();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.ts_language().ok_or("grammar unavailable")?)
        .map_err(|e| e.to_string())?;
    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return Ok(sites);
    };
    let src = text.as_bytes();

    // Pre-order walk collecting the per-language patterns.
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let k = n.kind();
        match lang {
            Lang::Python => {
                if k == "decorated_definition" {
                    // decorator text + the decorated function
                    let mut dc = n.walk();
                    let decor: Vec<String> = n
                        .children(&mut dc)
                        .filter(|c| c.kind() == "decorator")
                        .map(|c| c.utf8_text(src).unwrap_or("").to_string())
                        .collect();
                    let def = n.children(&mut dc).find(|c| c.kind() == "function_definition");
                    drop(dc);
                    if let Some(def) = def {
                        for d in &decor {
                            let (feature, names): (&str, &[&str]) =
                                if d.contains(".get(") || d.contains(".post(") || d.contains(".route(") || d.contains(".put(") || d.contains(".delete(") {
                                    ("route", &["fastapi", "flask"])
                                } else if d.contains("api_view") || d.contains("django") {
                                    ("route", &["django"])
                                } else if d.contains("service") {
                                    ("service", &[])
                                } else {
                                    continue;
                                };
                            let fw = fw_of(names);
                            write_role(conn, report, file_id, (def.start_byte(), def.end_byte()), feature, &fw, d, conf_of(&fw))?;
                            sites.push((feature.to_string(), fw));
                        }
                    }
                }
            }
            Lang::Java => {
                if k == "annotation" || k == "marker_annotation" {
                    let a = n.utf8_text(src).unwrap_or("");
                    // the annotated declaration is the nearest named ancestor
                    let mut anc = n.parent();
                    while let Some(p) = anc {
                        if matches!(p.kind(), "class_declaration" | "interface_declaration" | "method_declaration" | "field_declaration") {
                            let feature = if a.contains("Mapping") {
                                "route"
                            } else if a.contains("@Service") {
                                "service"
                            } else if a.contains("@Repository") {
                                "repository"
                            } else if a.contains("@Entity") {
                                "entity"
                            } else if a.contains("@Configuration") {
                                "config"
                            } else if a.contains("@SpringBootApplication") {
                                "boot"
                            } else if a.contains("@Autowired") {
                                "di-injected"
                            } else {
                                break;
                            };
                            let fw = fw_of(&["spring-boot"]);
                            write_role(conn, report, file_id, (p.start_byte(), p.end_byte()), feature, &fw, a, conf_of(&fw))?;
                            sites.push((feature.to_string(), fw));
                            break;
                        }
                        anc = p.parent();
                    }
                }
            }
            Lang::Rust => {
                if k == "attribute_item" {
                    let a = n.utf8_text(src).unwrap_or("");
                    if a.starts_with("#[get(") || a.starts_with("#[post(") || a.starts_with("#[put(") || a.starts_with("#[delete(") {
                        // the attribute PRECEDES the function — it is the
                        // next named sibling, not a parent
                        let mut target = n.next_named_sibling();
                        while let Some(p) = target {
                            if p.kind() == "function_item" {
                                let fw = fw_of(&["actix-web", "rocket"]);
                                write_role(conn, report, file_id, (p.start_byte(), p.end_byte()), "route", &fw, a, conf_of(&fw))?;
                                sites.push(("route".into(), fw));
                                break;
                            }
                            target = p.next_named_sibling();
                        }
                    }
                }
                if k == "call_expression" {
                    let t = n.utf8_text(src).unwrap_or("");
                    if t.contains(".route(") && (t.contains("get(") || t.contains("post(")) {
                        let fw = fw_of(&["axum"]);
                        write_role(conn, report, file_id, (n.start_byte(), n.end_byte()), "route", &fw, t, conf_of(&fw))?;
                        sites.push(("route".into(), fw));
                    }
                }
            }
            Lang::Go => {
                if k == "call_expression" {
                    let t = n.utf8_text(src).unwrap_or("");
                    let upper = t.contains(".GET(") || t.contains(".POST(") || t.contains(".PUT(") || t.contains(".DELETE(");
                    let lower = t.contains(".Get(") || t.contains(".Post(");
                    if upper || lower {
                        let names: &[&str] = if upper { &["gin", "echo"] } else { &["chi"] };
                        let fw = fw_of(names);
                        write_role(conn, report, file_id, (n.start_byte(), n.end_byte()), "route", &fw, t, conf_of(&fw))?;
                        sites.push(("route".into(), fw));
                    }
                }
            }
            Lang::JavaScript | Lang::TypeScript => {
                if k == "call_expression" {
                    let t = n.utf8_text(src).unwrap_or("");
                    if t.contains(".get(") || t.contains(".post(") || t.contains(".use(") || t.contains(".put(") || t.contains(".delete(") {
                        if t.contains("app.") || t.contains("router.") {
                            let fw = fw_of(&["express"]);
                            write_role(conn, report, file_id, (n.start_byte(), n.end_byte()), "route", &fw, t, conf_of(&fw))?;
                            sites.push(("route".into(), fw));
                        }
                    }
                }
                if k == "decorator" {
                    let a = n.utf8_text(src).unwrap_or("");
                    if a.starts_with("@Get") || a.starts_with("@Post") || a.starts_with("@Controller") {
                        let fw = fw_of(&["nestjs"]);
                        write_role(conn, report, file_id, (n.start_byte(), n.end_byte()), "route", &fw, a, conf_of(&fw))?;
                        sites.push(("route".into(), fw));
                    }
                }
            }
            Lang::CSharp => {
                if k == "attribute_list" {
                    let a = n.utf8_text(src).unwrap_or("");
                    if a.contains("HttpGet") || a.contains("HttpPost") || a.contains("Route") {
                        let fw = fw_of(&["aspnetcore"]);
                        let mut anc = n.parent();
                        while let Some(p) = anc {
                            if p.kind() == "method_declaration" {
                                write_role(conn, report, file_id, (p.start_byte(), p.end_byte()), "route", &fw, a, conf_of(&fw))?;
                                sites.push(("route".into(), fw));
                                break;
                            }
                            anc = p.parent();
                        }
                    }
                }
                if k == "invocation_expression" {
                    let t = n.utf8_text(src).unwrap_or("");
                    if t.contains("MapGet(") || t.contains("MapPost(") {
                        let fw = fw_of(&["aspnetcore"]);
                        write_role(conn, report, file_id, (n.start_byte(), n.end_byte()), "route", &fw, t, conf_of(&fw))?;
                        sites.push(("route".into(), fw));
                    }
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        for ch in ch.into_iter().rev() {
            stack.push(ch);
        }
    }
    Ok(sites)
}

/// Client path literals: the FIRST string argument of real HTTP client
/// calls, with the enclosing symbol as the binding source. Returns
/// (enclosing symbol id, path literal).
fn client_path_literals(
    conn: &Connection,
    file_id: i64,
    lang: Lang,
    text: &str,
) -> Result<Vec<(i64, String)>, String> {
    let mut out = Vec::new();
    let client_markers: &[&str] = match lang {
        Lang::Python => &["requests.", "httpx.", "session.get(", "session.post("],
        Lang::JavaScript | Lang::TypeScript => &["fetch(", "axios."],
        Lang::Go => &["http.Get(", "http.Post("],
        Lang::Rust => &["reqwest::", ".get(\"http"],
        _ => return Ok(out),
    };
    let has_client = client_markers.iter().any(|m| text.contains(m));
    if !has_client {
        return Ok(out);
    }
    // enclosing symbol per string-literal span, from the slice-9 symbols
    let syms: Vec<(i64, usize, usize)> = {
        let mut stmt = conn
            .prepare("SELECT id, start_byte, end_byte FROM symbols WHERE file_id = ?1 ORDER BY start_byte")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![file_id], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, i64>(2)? as usize))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let enclosing = |pos: usize| -> Option<i64> {
        syms.iter()
            .filter(|(_, s, e)| pos >= *s && pos <= *e)
            .map(|(id, _, _)| *id)
            .next_back()
    };
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.ts_language().ok_or("grammar unavailable")?)
        .map_err(|e| e.to_string())?;
    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return Ok(out);
    };
    let src = text.as_bytes();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let k = n.kind();
        let is_client_call = {
            let t = n.utf8_text(src).unwrap_or("");
            client_markers.iter().any(|m| t.starts_with(m) || (t.contains(m) && k.contains("call")))
        };
        if is_client_call {
            // first string literal in the call subtree
            let mut lit_stack = vec![n];
            let mut found: Option<tree_sitter::Node> = None;
            while let Some(x) = lit_stack.pop() {
                if x.kind().contains("string") && x.id() != n.id() {
                    found = Some(x);
                    break;
                }
                let mut xc = x.walk();
                let ch: Vec<tree_sitter::Node> = x.children(&mut xc).collect();
                drop(xc);
                lit_stack.extend(ch);
            }
            if let Some(lit) = found {
                let raw = lit.utf8_text(src).unwrap_or("");
                let path = raw.trim_matches(|c| c == '"' || c == '\'');
                if path.starts_with('/') {
                    if let Some(sym) = enclosing(n.start_byte()) {
                        out.push((sym, path.to_string()));
                    }
                }
            }
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        for ch in ch.into_iter().rev() {
            stack.push(ch);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{cache_source_bytes, create_ir_schema, lower_all};
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_ir_schema(&conn).unwrap();
        create_fw_schema(&conn).unwrap();
        conn
    }

    /// Acceptance #80: routes detected in Spring Boot, FastAPI, and Actix.
    #[test]
    fn routes_detected_in_spring_fastapi_actix() {
        let conn = setup();
        let mut st = IngestStats::default();
        let java = "import org.springframework.web.bind.annotation.*;\n@RestController\npublic class CartController {\n    @GetMapping(\"/cart\")\n    public int getCart() { return 1; }\n}\n";
        let py = "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(\"/items\")\ndef list_items():\n    return []\n";
        let rs = "use actix_web::{get, App, HttpServer, Responder};\n\n#[get(\"/health\")]\nasync fn health() -> impl Responder {\n    \"ok\"\n}\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "Cart.java", Lang::Java, java, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "main.py", Lang::Python, py, &mut st).unwrap();
        let f3 = ingest_parsed_file(&conn, 1, 41, "main.rs", Lang::Rust, rs, &mut st).unwrap();
        cache_source_bytes(&conn, f1, java).unwrap();
        cache_source_bytes(&conn, f2, py).unwrap();
        cache_source_bytes(&conn, f3, rs).unwrap();
        install_registry(&conn).unwrap();
        let rep = detect(&conn).unwrap();

        let roles: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT framework, feature FROM framework_roles WHERE feature='route'")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(roles.iter().any(|(f, _)| f == "spring-boot"), "{roles:?}");
        assert!(roles.iter().any(|(f, _)| f == "fastapi"), "{roles:?}");
        assert!(roles.iter().any(|(f, _)| f == "actix-web"), "{roles:?}");
        assert_eq!(rep.roles, 3);
    }

    #[test]
    fn unattributed_decorator_shape_is_half_confidence() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "app = get_app()\n\n@app.get(\"/items\")\ndef list_items():\n    return []\n";
        let f = ingest_parsed_file(&conn, 1, 41, "u.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        install_registry(&conn).unwrap();
        detect(&conn).unwrap();
        let (fw, conf): (String, f64) = conn
            .query_row(
                "SELECT framework, confidence FROM framework_roles WHERE feature='route'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fw, "unattributed", "no import evidence, no identity");
        assert_eq!(conf, 0.5);
    }

    #[test]
    fn java_spring_features_beyond_routes() {
        let conn = setup();
        let mut st = IngestStats::default();
        let java = "import org.springframework.stereotype.Service;\n@Service\npublic class CheckoutService {\n    public int total() { return 1; }\n}\n";
        let f = ingest_parsed_file(&conn, 1, 41, "S.java", Lang::Java, java, &mut st).unwrap();
        cache_source_bytes(&conn, f, java).unwrap();
        install_registry(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        let feats: Vec<String> = {
            let mut stmt = conn.prepare("SELECT DISTINCT feature FROM framework_roles").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert!(feats.contains(&"service".to_string()), "{feats:?}");
        assert!(rep.roles >= 1);
    }

    /// Acceptance #78: DECLARED binding from an OpenAPI spec has confidence 1.0.
    #[test]
    fn openapi_spec_yields_declared_bindings_confidence_one() {
        let conn = setup();
        let spec = "openapi: 3.0.0\npaths:\n  /pets:\n    get:\n      summary: list\n  /pets/{id}:\n    get:\n      summary: one\n";
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'spec/openapi.yaml', ?1, 'yaml', 41, ?2, 'none')",
            rusqlite::params![blake3::hash(spec.as_bytes()).as_bytes().to_vec(), spec.len() as i64],
        )
        .unwrap();
        let fid = conn.last_insert_rowid();
        cache_source_bytes(&conn, fid, spec).unwrap();
        install_registry(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.specs_parsed, 1);
        assert_eq!(rep.declared_bindings, 2, "both spec paths declared");
        let (conf, res): (f64, String) = conn
            .query_row(
                "SELECT confidence, resolution FROM cross_language_bindings WHERE spec_pointer='/pets'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(conf, 1.0);
        assert_eq!(res, "DECLARED");
    }

    /// Acceptance #79: URL_MATCHED binding has confidence 0.7.
    #[test]
    fn client_call_matching_spec_path_is_url_matched() {
        let conn = setup();
        let mut st = IngestStats::default();
        let spec = "openapi: 3.0.0\npaths:\n  /pets:\n    get:\n      summary: list\n";
        conn.execute(
            "INSERT INTO files(repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (1, 'openapi.yaml', ?1, 'yaml', 41, ?2, 'none')",
            rusqlite::params![blake3::hash(spec.as_bytes()).as_bytes().to_vec(), spec.len() as i64],
        )
        .unwrap();
        let fid = conn.last_insert_rowid();
        cache_source_bytes(&conn, fid, spec).unwrap();

        let py = "import requests\n\ndef load_pets():\n    return requests.get('/pets')\n";
        let f2 = ingest_parsed_file(&conn, 1, 41, "client.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f2, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.declared_bindings, 1);
        assert_eq!(rep.url_matched_bindings, 1);
        let (conf, src): (f64, Option<i64>) = conn
            .query_row(
                "SELECT confidence, source_symbol_id FROM cross_language_bindings WHERE resolution='URL_MATCHED'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(conf, 0.7, "URL_MATCHED is exactly 0.7 per acceptance #79");
        assert!(src.is_some(), "the client call site is the binding source");
    }

    #[test]
    fn registry_rows_land_with_119_columns() {
        let conn = setup();
        let n = install_registry(&conn).unwrap();
        assert_eq!(n, REGISTRY.len());
        let (lang, fw, feature, ast_kind, match_value, conf): (String, String, String, String, String, f64) = conn
            .query_row(
                "SELECT language, framework, feature, ast_kind, match_value, confidence FROM framework_adapters WHERE language='python' AND framework='fastapi'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!((lang.as_str(), fw.as_str(), feature.as_str()), ("python", "fastapi", "route"));
        assert!(conf == 1.0);
        assert!(!ast_kind.is_empty() && !match_value.is_empty());
    }
}
