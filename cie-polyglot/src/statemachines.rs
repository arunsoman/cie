//! PRD v2.0 slice 13 — state machine detection (§19), three tiers with the
//! PRD's epistemic encoding:
//!
//! | Tier | Signal | Confidence | Visual law |
//! |---|---|---|---|
//! | T1 Explicit | `createMachine({...})` (XState), Spring `StateMachineBuilder`, `@Workflow` (Temporal), Step Functions YAML | Fact (solid) | states/transitions extracted verbatim |
//! | T2 Structural | enum + `switch`/`match` arms assigning the switched variable | Inference (dashed); incomplete → Fog (hatched) with reason | "4 of 6 enum members appear in switch arms" |
//! | T3 Behavioral | call-graph ordering across components + conditional branches | Hypothesis (dotted) WITH counter-argument | static order, runtime may differ |
//!
//! The `state_machines` table is the documented output extension (the PRD
//! defines detection, not storage); every row cites its evidence node and,
//! for non-Fact tiers, carries the mandated counter-argument/fog-reason.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::schema::Lang;

pub const DDL_SM: &str = r#"
CREATE TABLE IF NOT EXISTS state_machines (
    id               INTEGER PRIMARY KEY,
    source_symbol_id INTEGER REFERENCES symbols(id),
    name             TEXT    NOT NULL,
    tier             TEXT    NOT NULL,      -- 'T1' | 'T2' | 'T3'
    epistemic        TEXT    NOT NULL,      -- 'Fact' | 'Inference' | 'Fog' | 'Hypothesis'
    states           TEXT    NOT NULL,      -- JSON array
    transitions      TEXT    NOT NULL,      -- JSON array of {from,to,event?,guard?}
    coverage         TEXT,                  -- e.g. "2 of 3 enum members appear in switch arms"
    counter_argument TEXT,
    fog_reason       TEXT,
    evidence_node_id INTEGER REFERENCES ast_nodes(id)
);
"#;

pub fn create_sm_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_SM).map_err(|e| e.to_string())
}

#[derive(Debug, Default, serde::Serialize)]
pub struct SmReport {
    pub t1: usize,
    pub t2: usize,
    pub t2_fog: usize,
    pub t3: usize,
}

/// Detect state machines across every ingested file.
pub fn detect(conn: &Connection) -> Result<SmReport, String> {
    let mut report = SmReport::default();
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
        let Some(lang) = Lang::from_path(&path) else { continue };
        let compressed: Vec<u8> = match conn.query_row(
            "SELECT zstd_bytes FROM file_source_cache WHERE file_id = ?1",
            rusqlite::params![file_id],
            |r| r.get(0),
        ) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let src = zstd::bulk::decompress(&compressed, 16 << 20).map_err(|e| e.to_string())?;
        detect_t1(conn, file_id, lang, &src, &mut report)?;
        detect_t2(conn, file_id, lang, &src, &mut report)?;
    }
    detect_t3(conn, &mut report)?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// T1 — explicit definitions (Fact)
// ---------------------------------------------------------------------------

fn detect_t1(conn: &Connection, file_id: i64, lang: Lang, src: &[u8], report: &mut SmReport) -> Result<(), String> {
    match lang {
        Lang::JavaScript | Lang::TypeScript => detect_t1_xstate(conn, file_id, lang, src, report),
        Lang::Java => detect_t1_marker(conn, file_id, lang, src, "StateMachineBuilder", report),
        _ => Ok(()),
    }
}

/// XState `createMachine({ id, states: { <name>: { on: { <event>: { target } } } } })`
/// — states and transitions extracted VERBATIM from the literal (§19.2 T1).
fn detect_t1_xstate(conn: &Connection, file_id: i64, lang: Lang, src: &[u8], report: &mut SmReport) -> Result<(), String> {
    let Some(tree) = parse(lang, src) else { return Ok(()) };
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" {
            let fn_text = n
                .child_by_field_name("function")
                .and_then(|f| f.utf8_text(src).ok())
                .unwrap_or("");
            if fn_text == "createMachine" {
                let args = n.child_by_field_name("arguments");
                if let Some(args) = args {
                    let (states, transitions) = extract_xstate_literal(args, src);
                    if !states.is_empty() {
                        write_sm(
                            conn,
                            None,
                            "createMachine",
                            "T1",
                            "Fact",
                            &states,
                            &transitions,
                            None,
                            None,
                            None,
                            node_row(conn, file_id, (n.start_byte(), n.end_byte())),
                        )?;
                        report.t1 += 1;
                    }
                }
            }
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        stack.extend(ch);
    }
    Ok(())
}

fn extract_xstate_literal(args: tree_sitter::Node, src: &[u8]) -> (Vec<String>, Vec<serde_json::Value>) {
    let mut states = Vec::new();
    let mut transitions = Vec::new();
    // the machine literal is the OBJECT argument of the call
    let machine_obj = args
        .children(&mut args.walk())
        .find(|c| c.kind() == "object");
    let Some(machine_obj) = machine_obj else { return (states, transitions) };
    // states: { <name>: {... on: { <event>: { target: 'x' } } } }
    if let Some(obj) = find_property_value(machine_obj, "states", src) {
        // named object pairs
        for pair in obj.children(&mut obj.walk()) {
            if pair.kind() != "pair" {
                continue;
            }
            let Some(key) = pair.child_by_field_name("key").and_then(|k| k.utf8_text(src).ok()) else { continue };
            let Some(val) = pair.child_by_field_name("value") else { continue };
            states.push(key.to_string());
            // on: { <event>: { target } }
            if let Some(on) = find_property_value(val, "on", src) {
                for ep in on.children(&mut on.walk()) {
                    if ep.kind() != "pair" {
                        continue;
                    }
                    let event = ep.child_by_field_name("key").and_then(|k| k.utf8_text(src).ok()).unwrap_or("");
                    let ev = ep.child_by_field_name("value");
                    // target may be a direct string or nested { target: 'x' }
                    let mut targets: Vec<String> = Vec::new();
                    if let Some(ev) = ev {
                        if let Some(t) = find_property_value(ev, "target", src) {
                            if let Some(s) = string_text(t, src) {
                                targets.push(s);
                            }
                        } else if let Some(s) = string_text(ev, src) {
                            targets.push(s);
                        }
                    }
                    for t in targets {
                        transitions.push(serde_json::json!({
                            "from": key, "to": t, "event": event
                        }));
                    }
                }
            }
        }
    }
    (states, transitions)
}

fn find_property_value<'t>(node: tree_sitter::Node<'t>, key: &str, src: &'t [u8]) -> Option<tree_sitter::Node<'t>> {
    for pair in node.children(&mut node.walk()) {
        if pair.kind() != "pair" {
            continue;
        }
        let k = pair.child_by_field_name("key").and_then(|k| k.utf8_text(src).ok());
        if k == Some(key) {
            return pair.child_by_field_name("value");
        }
    }
    None
}

fn string_text<'t>(node: tree_sitter::Node<'t>, src: &'t [u8]) -> Option<String> {
    node.utf8_text(src)
        .ok()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string())
        .filter(|s| !s.is_empty())
}

/// Spring `StateMachineBuilder` / Temporal `@Workflow` — T1 detected with
/// its evidence; states extracted only when a verbatim literal exists
/// (builder-style configuration is method chains, honestly recorded empty
/// with the evidence text preserved).
fn detect_t1_marker(conn: &Connection, file_id: i64, lang: Lang, src: &[u8], marker: &str, report: &mut SmReport) -> Result<(), String> {
    let Some(tree) = parse(lang, src) else { return Ok(()) };
    let text = String::from_utf8_lossy(src);
    if !text.contains(marker) && !text.contains("@Workflow") {
        return Ok(());
    }
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let t = n.utf8_text(src).unwrap_or("");
        if (t.contains(marker) || n.kind() == "marker_annotation" && t.contains("Workflow"))
            && n.kind() != "program"
        {
            write_sm(
                conn,
                symbol_at(conn, file_id, (n.start_byte(), n.end_byte())),
                marker,
                "T1",
                "Fact",
                &[],
                &[],
                Some(format!("explicit definition detected via {marker}; state list not literal-extractable in v1")),
                None,
                None,
                node_row(conn, file_id, (n.start_byte(), n.end_byte())),
            )?;
            report.t1 += 1;
            break;
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        stack.extend(ch);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// T2 — enum + switch/match arms assigning the switched variable (Inference;
// incomplete coverage → Fog with the "N of M" reason)
// ---------------------------------------------------------------------------

/// Enum member names per language (verified grammar kinds).
fn enum_decls(lang: Lang, root: tree_sitter::Node, src: &[u8]) -> Vec<(String, Vec<String>, (usize, usize))> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match lang {
            Lang::Rust if n.kind() == "enum_item" => {
                let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()).unwrap_or("");
                let mut members = Vec::new();
                if let Some(body) = n.child_by_field_name("body") {
                    let mut bc = body.walk();
                    for v in body.children(&mut bc) {
                        if v.kind() == "enum_variant" {
                            if let Some(vn) = v.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()) {
                                members.push(vn.to_string());
                            }
                        }
                    }
                }
                out.push((name.to_string(), members, (n.start_byte(), n.end_byte())));
            }
            Lang::Java if n.kind() == "enum_declaration" => {
                let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()).unwrap_or("");
                let mut members = Vec::new();
                if let Some(body) = n.child_by_field_name("body") {
                    let mut bc = body.walk();
                    for v in body.children(&mut bc) {
                        if v.kind() == "enum_constant" {
                            if let Some(vn) = v.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()) {
                                members.push(vn.to_string());
                            }
                        }
                    }
                }
                out.push((name.to_string(), members, (n.start_byte(), n.end_byte())));
            }
            Lang::CSharp if n.kind() == "enum_declaration" => {
                let name = n.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()).unwrap_or("");
                let mut members = Vec::new();
                if let Some(body) = n.child_by_field_name("body") {
                    let mut bc = body.walk();
                    for v in body.children(&mut bc) {
                        if v.kind() == "enum_member_declaration" {
                            if let Some(vn) = v.child_by_field_name("name").and_then(|x| x.utf8_text(src).ok()) {
                                members.push(vn.to_string());
                            }
                        }
                    }
                }
                out.push((name.to_string(), members, (n.start_byte(), n.end_byte())));
            }
            _ => {}
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        stack.extend(ch);
    }
    out
}

/// switch/match statements per language: (subject text, [(case_label, assigned_to_text)], span).
fn switch_stmts(lang: Lang, root: tree_sitter::Node, src: &[u8]) -> Vec<(String, Vec<(String, String)>, (usize, usize))> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let is_switch = matches!(
            (lang, n.kind()),
            (Lang::Java, "switch_expression")
                | (Lang::Java, "switch_statement")
                | (Lang::CSharp, "switch_statement")
                | (Lang::Rust, "match_expression")
        );
        if is_switch {
            let subject = match lang {
                Lang::Rust => n.child_by_field_name("value").and_then(|x| x.utf8_text(src).ok()),
                _ => n
                    .child_by_field_name("condition")
                    .and_then(|x| x.utf8_text(src).ok())
                    .or_else(|| {
                        // java switch_expression has no condition field — the
                        // subject is the first identifier child
                        n.children(&mut n.walk())
                            .find(|c| c.kind() == "identifier")
                            .and_then(|c| c.utf8_text(src).ok())
                    }),
            }
            .unwrap_or("")
            .trim_matches(|c| c == '(' || c == ')')
            .trim()
            .to_string();
            // walk arms: case labels + assignments whose target == subject
            let mut arms: Vec<(String, String)> = Vec::new();
            let mut astack = vec![n];
            while let Some(a) = astack.pop() {
                if a.id() != n.id() && matches!((lang, a.kind()), (Lang::Rust, "match_expression")) {
                    continue; // nested match handled separately
                }
                if lang == Lang::Rust {
                    if a.kind() == "match_arm" {
                        // pattern text; block/expression may assign `subject = ...`
                        let pat = a.child_by_field_name("pattern").and_then(|x| x.utf8_text(src).ok()).unwrap_or("");
                        let bare = pat.rsplit("::").next().unwrap_or(pat).trim().to_string();
                        let body = a.child_by_field_name("value");
                        if let Some(body) = body {
                            let bt = body.utf8_text(src).unwrap_or("");
                            // assignment `s = OrderState::Paid` or tail expr
                            if let Some(target) = assignment_target(bt, &subject) {
                                arms.push((bare, target));
                            }
                        }
                    }
                } else {
                    if a.kind() == "switch_block_statement_group" || a.kind() == "switch_section" {
                        let at = a.utf8_text(src).unwrap_or("");
                        let labels: Vec<String> = a
                            .children(&mut a.walk())
                            .filter(|c| c.kind() == "switch_label" || c.kind() == "case_switch_label")
                            .map(|c| {
                                c.utf8_text(src)
                                    .unwrap_or("")
                                    .trim_start_matches("case")
                                    .trim()
                                    .trim_end_matches(':')
                                    .to_string()
                            })
                            .collect();
                        if let Some(target) = assignment_target(at, &subject) {
                            let label = labels.first().cloned().unwrap_or_default();
                            arms.push((label, target));
                        }
                    }
                }
                let mut ac = a.walk();
                let ch: Vec<tree_sitter::Node> = a.children(&mut ac).collect();
                drop(ac);
                astack.extend(ch);
            }
            if !arms.is_empty() {
                out.push((subject, arms, (n.start_byte(), n.end_byte())));
            }
        }
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node> = n.children(&mut c).collect();
        drop(c);
        stack.extend(ch);
    }
    out
}

/// find `<subject> = <value>` inside text; segments may carry the leading
/// `case <X>:` label of a switch group, which is stripped first (honest
/// scanner: only exact subject match, never a fuzzy one)
fn assignment_target(text: &str, subject: &str) -> Option<String> {
    for segment in text.split([';', '\n']) {
        let mut s = segment.trim();
        if s.starts_with("case ") {
            if let Some((_, after)) = s.split_once(':') {
                s = after.trim();
            }
        }
        if let Some((lhs, rhs)) = s.split_once('=') {
            let lhs = lhs.trim();
            if lhs == subject && !rhs.trim().is_empty() {
                let rhs = rhs.trim().trim_end_matches(|c| c == ';' || c == ',').trim();
                let bare = rhs.rsplit("::").next().unwrap_or(rhs).trim();
                return Some(bare.to_string());
            }
        }
    }
    None
}

fn detect_t2(conn: &Connection, file_id: i64, lang: Lang, src: &[u8], report: &mut SmReport) -> Result<(), String> {
    let Some(tree) = parse(lang, src) else { return Ok(()) };
    let enums = enum_decls(lang, tree.root_node(), src);
    if enums.is_empty() {
        return Ok(());
    }
    let switches = switch_stmts(lang, tree.root_node(), src);
    for (ename, members, espan) in enums {
        for (subject, arms, sspan) in &switches {
            // the switch must be over a variable of (or named after) the enum
            let near = subject.to_lowercase().contains(&ename.to_lowercase())
                || ename.to_lowercase().contains(&subject.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_').to_lowercase());
            if !near {
                continue;
            }
            // "members appear in switch arms" = the state is HANDLED — a case
            // label / match pattern. Being a transition TARGET means it is
            // assigned to, not that its behaviour is modeled.
            let covered: Vec<String> = members
                .iter()
                .filter(|m| arms.iter().any(|(label, _)| label == *m))
                .cloned()
                .collect();
            let total = members.len();
            let found = covered.len();
            if found == 0 {
                continue;
            }
            let (epistemic, fog, coverage) = if found == total {
                ("Inference", None, None)
            } else {
                (
                    "Fog",
                    Some(format!(
                        "{found} of {total} enum members appear in switch arms — the uncovered states' behavior is unmodeled"
                    )),
                    Some(format!("{found} of {total} enum members appear in switch arms")),
                )
            };
            let transitions: Vec<serde_json::Value> = arms
                .iter()
                .map(|(label, target)| {
                    serde_json::json!({ "from": label, "to": target, "event": format!("assign {subject}") })
                })
                .collect();
            write_sm(
                conn,
                symbol_at(conn, file_id, *sspan),
                &format!("{ename} ({subject})"),
                "T2",
                epistemic,
                &members,
                &transitions,
                coverage,
                Some("structural: switch/match arms over the enum-typed variable; guard conditions may route around these transitions".to_string()),
                fog,
                node_row(conn, file_id, espan),
            )?;
            if epistemic == "Fog" {
                report.t2_fog += 1;
            } else {
                report.t2 += 1;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// T3 — behavioral (Hypothesis, always with its counter-argument)
// ---------------------------------------------------------------------------

/// A symbol whose CFG shows ≥2 conditional branches AND ≥2 resolved
/// cross-file calls behaves like a state machine over its effects — a
/// Hypothesis, labelled as such.
fn detect_t3(conn: &Connection, report: &mut SmReport) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.name, s.file_id,
                    (SELECT COUNT(*) FROM ir_instructions i JOIN ir_blocks b ON b.id = i.block_id WHERE b.symbol_id = s.id AND i.op = 'Cmp'),
                    (SELECT COUNT(*) FROM calls c WHERE c.caller_symbol_id = s.id AND c.resolution = 'RESOLVED')
             FROM symbols s",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (sid, name, file_id, n_cmp, n_calls) in rows {
        if n_cmp < 2 || n_calls < 2 {
            continue;
        }
        // cross-file: any callee whose file differs
        let cross: i64 = {
            let mut q = conn
                .prepare(
                    "SELECT COUNT(*) FROM calls c
                     JOIN symbols cf ON cf.id = c.callee_symbol_id
                     WHERE c.caller_symbol_id = ?1 AND cf.file_id != ?2",
                )
                .map_err(|e| e.to_string())?;
            q.query_row(rusqlite::params![sid, file_id], |r| r.get(0))
                .unwrap_or(0)
        };
        if cross < 2 {
            continue;
        }
        write_sm(
            conn,
            Some(sid),
            &name,
            "T3",
            "Hypothesis",
            &[],
            &[],
            None,
            Some("static call order across components; runtime branching, retries and queues may differ — needs runtime evidence to confirm".to_string()),
            None,
            None,
        )?;
        report.t3 += 1;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn parse(lang: Lang, src: &[u8]) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang.ts_language()?).ok()?;
    parser.parse(src, None)
}

fn node_row(conn: &Connection, file_id: i64, span: (usize, usize)) -> Option<i64> {
    conn.query_row(
        "SELECT a.id FROM ast_nodes a WHERE a.file_id = ?1 AND a.start_byte = ?2 AND a.end_byte = ?3 LIMIT 1",
        rusqlite::params![file_id, span.0 as i64, span.1 as i64],
        |r| r.get(0),
    )
    .ok()
}

fn symbol_at(conn: &Connection, file_id: i64, span: (usize, usize)) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM symbols WHERE file_id = ?1 AND start_byte <= ?2 AND end_byte >= ?3 ORDER BY (end_byte - start_byte) LIMIT 1",
        rusqlite::params![file_id, span.0 as i64, span.1 as i64],
        |r| r.get(0),
    )
    .ok()
}

#[allow(clippy::too_many_arguments)]
fn write_sm(
    conn: &Connection,
    source_symbol_id: Option<i64>,
    name: &str,
    tier: &str,
    epistemic: &str,
    states: &[String],
    transitions: &[serde_json::Value],
    coverage: Option<String>,
    counter_argument: Option<String>,
    fog_reason: Option<String>,
    evidence_node_id: Option<i64>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO state_machines(source_symbol_id, name, tier, epistemic, states, transitions, coverage, counter_argument, fog_reason, evidence_node_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            source_symbol_id,
            name,
            tier,
            epistemic,
            serde_json::to_string(states).map_err(|e| e.to_string())?,
            serde_json::to_string(transitions).map_err(|e| e.to_string())?,
            coverage,
            counter_argument,
            fog_reason,
            evidence_node_id
        ],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Non-Fact rows must carry their mandated disclosure; Facts must not be
/// diluted with one. Enforcement points for the honesty rules.
pub fn audit(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id, epistemic, counter_argument, fog_reason FROM state_machines")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (id, epi, ca, fr) in rows {
        match epi.as_str() {
            "Hypothesis" if ca.is_none() => {
                return Err(format!("state machine {id}: Hypothesis without counter-argument (PRD §25)"))
            }
            "Fog" if fr.is_none() => return Err(format!("state machine {id}: Fog without reason (PRD §25)")),
            "Fact" if ca.is_some() || fr.is_some() => {
                return Err(format!("state machine {id}: Fact carries no disclosures by construction"))
            }
            _ => {}
        }
    }
    Ok(())
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
        create_sm_schema(&conn).unwrap();
        conn
    }

    /// Acceptance #71: XState createMachine extracted as Fact, verbatim.
    #[test]
    fn xstate_create_machine_is_fact_with_verbatim_states() {
        let conn = setup();
        let mut st = IngestStats::default();
        let src = "export const machine = createMachine({\n  id: 'order',\n  states: {\n    draft: { on: { PAY: { target: 'paid' } } },\n    paid: { on: { SHIP: 'shipped' } },\n    shipped: {}\n  }\n});\n";
        let f = ingest_parsed_file(&conn, 1, 41, "order.ts", Lang::TypeScript, src, &mut st).unwrap();
        cache_source_bytes(&conn, f, src).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.t1, 1);
        let (epi, states, transitions): (String, String, String) = conn
            .query_row("SELECT epistemic, states, transitions FROM state_machines WHERE name='createMachine'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(epi, "Fact");
        let s: Vec<String> = serde_json::from_str(&states).unwrap();
        assert_eq!(s, vec!["draft", "paid", "shipped"]);
        let t: Vec<serde_json::Value> = serde_json::from_str(&transitions).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0]["from"], "draft");
        assert_eq!(t[0]["to"], "paid");
        assert_eq!(t[0]["event"], "PAY");
    }

    /// Acceptance #72: Spring StateMachineBuilder extracted as Fact.
    #[test]
    fn spring_state_machine_builder_is_fact() {
        let conn = setup();
        let mut st = IngestStats::default();
        let src = "import org.springframework.statemachine.builder.StateMachineBuilder;\npublic class OrderConfig {\n    void build(StateMachineBuilder b) { b.configureConfiguration(); }\n}\n";
        let f = ingest_parsed_file(&conn, 1, 41, "OrderConfig.java", Lang::Java, src, &mut st).unwrap();
        cache_source_bytes(&conn, f, src).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.t1, 1);
        let epi: String = conn
            .query_row("SELECT epistemic FROM state_machines WHERE tier='T1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(epi, "Fact");
    }

    /// Acceptance #73: enum + switch extracted as Inference; partial
    /// coverage becomes Fog with the "N of M" reason.
    #[test]
    fn enum_switch_is_inference_and_partial_is_fog() {
        let conn = setup();
        let mut st = IngestStats::default();
        // 3 members, 2 covered → Fog with reason
        let src = "enum OrderState { DRAFT, PAID, SHIPPED }\npublic class Fulfilment {\n    void advance(OrderState state) {\n        switch (state) {\n            case DRAFT: state = PAID; break;\n            case PAID: state = SHIPPED; break;\n        }\n    }\n}\n";
        let f = ingest_parsed_file(&conn, 1, 41, "F.java", Lang::Java, src, &mut st).unwrap();
        cache_source_bytes(&conn, f, src).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.t2_fog, 1, "2 of 3 covered → Fog");
        let (epi, fog, cov): (String, Option<String>, Option<String>) = conn
            .query_row("SELECT epistemic, fog_reason, coverage FROM state_machines WHERE tier='T2'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(epi, "Fog");
        assert!(fog.unwrap().contains("2 of 3"));
        assert!(cov.unwrap().contains("2 of 3"));
        audit(&conn).unwrap();

        // complete coverage → Inference, no fog
        let conn2 = setup();
        let mut st2 = IngestStats::default();
        let full = "enum OrderState { DRAFT, PAID }\npublic class Fulfilment {\n    void advance(OrderState state) {\n        switch (state) {\n            case DRAFT: state = PAID; break;\n            case PAID: state = DRAFT; break;\n        }\n    }\n}\n";
        let f2 = ingest_parsed_file(&conn2, 1, 41, "F.java", Lang::Java, full, &mut st2).unwrap();
        cache_source_bytes(&conn2, f2, full).unwrap();
        let rep2 = detect(&conn2).unwrap();
        assert_eq!(rep2.t2, 1);
        let epi2: String = conn2
            .query_row("SELECT epistemic FROM state_machines WHERE tier='T2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(epi2, "Inference");
    }

    /// Acceptance #74: call-graph workflow extracted as Hypothesis WITH its
    /// counter-argument (enforced by audit()).
    #[test]
    fn behavioral_t3_is_hypothesis_with_counter_argument() {
        let conn = setup();
        let mut st = IngestStats::default();
        let a = "def step_one(x):\n    if x > 1:\n        return helper_a(x)\n    if x < 0:\n        return helper_b(x)\n    return None\n";
        let b = "def helper_a(x):\n    return x + 1\n\ndef helper_b(x):\n    return x - 1\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.t3, 1, "step_one: 2 cmps + 2 cross-file resolved calls");
        let (epi, ca): (String, Option<String>) = conn
            .query_row("SELECT epistemic, counter_argument FROM state_machines WHERE tier='T3'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(epi, "Hypothesis");
        assert!(ca.is_some(), "counter-argument is mandatory (§19.3)");
        audit(&conn).unwrap();
    }

    #[test]
    fn audit_enforces_disclosure_rules() {
        let conn = setup();
        // a Hypothesis row without a counter-argument must FAIL the audit
        conn.execute(
            "INSERT INTO state_machines(source_symbol_id, name, tier, epistemic, states, transitions)
             VALUES (NULL, 'bad', 'T3', 'Hypothesis', '[]', '[]')",
            [],
        )
        .unwrap();
        assert!(audit(&conn).is_err(), "hypothesis without counter-argument is a violation");
    }
}

