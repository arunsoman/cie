//! PRD v2.0 slice 20 — runtime evidence (§9, §45) with the ExplorViz-style
//! structural/dynamic split (§44): the static store stays untouched, the
//! observed world lands in its own tables and is joined back by symbol.
//!
//! Inputs (OTLP/JSON and its flat span-array form): spans carry trace_id,
//! span_id, parent, service, operation, duration, status. Each span either
//! MAPS to a SymbolId (via the framework_roles route evidence and direct
//! name match) or is stored UNRESOLVED with its reason — acceptance #32
//! allows nothing in between.
//!
//! Derived, all evidence-backed:
//! * hot paths — hit counts per symbol (acceptance #34);
//! * flaky operations — symbols observed with both OK and ERROR;
//! * runtime drift — observed parent→child span pairs with NO static
//!   RESOLVED call edge between their symbols (acceptance #33): the
//!   runtime does something the static model cannot see, reported as such.
//!
//! The §45 table is exactly as written; the unresolved home is the
//! documented `runtime_unresolved` extension (the PRD's DDL pins
//! symbol_id NOT NULL, so unresolved spans need their own honest table).

use std::collections::BTreeMap;

use rusqlite::Connection;

use serde_json::Value;

pub const DDL_RT: &str = r#"
CREATE TABLE IF NOT EXISTS runtime_evidence (
    id           INTEGER PRIMARY KEY,
    symbol_id    INTEGER NOT NULL REFERENCES symbols(id),
    trace_id     BLOB NOT NULL,
    span_id      BLOB NOT NULL,
    parent_span  BLOB,
    duration_ns  INTEGER NOT NULL,
    status       TEXT NOT NULL,
    observed_ns  INTEGER NOT NULL,
    attrs        BLOB
);
CREATE INDEX IF NOT EXISTS idx_rt_symbol ON runtime_evidence(symbol_id, observed_ns);
CREATE INDEX IF NOT EXISTS idx_rt_trace  ON runtime_evidence(trace_id);

-- documented extension: acceptance #32's Unresolved arm needs a home and
-- the §45 DDL pins symbol_id NOT NULL — these spans land here, with reason.
CREATE TABLE IF NOT EXISTS runtime_unresolved (
    id           INTEGER PRIMARY KEY,
    trace_id     BLOB NOT NULL,
    span_id      BLOB NOT NULL,
    parent_span  BLOB,
    duration_ns  INTEGER NOT NULL,
    status       TEXT NOT NULL,
    observed_ns  INTEGER NOT NULL,
    service      TEXT,
    operation    TEXT,
    reason       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rt_unres_op ON runtime_unresolved(operation);

CREATE TABLE IF NOT EXISTS runtime_summary (
    symbol_id   INTEGER PRIMARY KEY REFERENCES symbols(id),
    hits        INTEGER NOT NULL,
    errors      INTEGER NOT NULL,
    total_ns    INTEGER NOT NULL,
    last_seen_ns INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS runtime_drift (
    id             INTEGER PRIMARY KEY,
    parent_symbol  INTEGER REFERENCES symbols(id),
    child_symbol   INTEGER REFERENCES symbols(id),
    parent_operation TEXT NOT NULL,
    child_operation  TEXT NOT NULL,
    kind           TEXT NOT NULL,   -- 'unmodeled-edge' | 'unmapped-operation'
    note           TEXT NOT NULL
);
"#;

pub fn create_rt_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_RT).map_err(|e| e.to_string())
}

#[derive(Debug, Default, serde::Serialize)]
pub struct RuntimeReport {
    pub spans: usize,
    pub mapped: usize,
    pub unresolved: usize,
    pub drift_edges: usize,
    pub flaky: usize,
}

/// Ingest OTel spans (OTLP/JSON `resourceSpans` or a flat array of span
/// objects) and join them to the static store.
pub fn ingest(conn: &Connection, payload: &str) -> Result<RuntimeReport, String> {
    let spans = parse_spans(payload)?;
    let mut report = RuntimeReport::default();
    let routes = route_index(conn)?;
    let names = name_index(conn)?;

    for sp in &spans {
        report.spans += 1;
        let matched = match_symbol(conn, &routes, &names, &sp.service, &sp.operation)?;
        match matched {
            Some((sid, how)) => {
                // the operation rides in attrs so drift joins can name both
                // sides without a denormalized column on the §45 table
                let mut attrs = sp.attrs_map.clone();
                attrs.insert("cie.operation".into(), sp.operation.clone());
                conn.execute(
                    "INSERT INTO runtime_evidence(symbol_id, trace_id, span_id, parent_span, duration_ns, status, observed_ns, attrs)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        sid,
                        sp.trace_id,
                        sp.span_id,
                        sp.parent_span,
                        sp.duration_ns,
                        sp.status,
                        sp.observed_ns,
                        serde_json::to_vec(&attrs).ok()
                    ],
                )
                .map_err(|e| e.to_string())?;
                report.mapped += 1;
                let _ = how;
            }
            None => {
                conn.execute(
                    "INSERT INTO runtime_unresolved(trace_id, span_id, parent_span, duration_ns, status, observed_ns, service, operation, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        sp.trace_id,
                        sp.span_id,
                        sp.parent_span,
                        sp.duration_ns,
                        sp.status,
                        sp.observed_ns,
                        sp.service,
                        sp.operation,
                        sp.unresolved_reason.clone().unwrap_or_else(|| {
                            "no symbol matches this service+operation (no route evidence, no name match)".to_string()
                        })
                    ],
                )
                .map_err(|e| e.to_string())?;
                report.unresolved += 1;
            }
        }
    }

    summarize(conn)?;
    report.drift_edges = detect_drift(conn)?;
    report.flaky = count_flaky(conn)?;
    Ok(report)
}

struct RawSpan {
    trace_id: Vec<u8>,
    span_id: Vec<u8>,
    parent_span: Option<Vec<u8>>,
    duration_ns: i64,
    status: String,
    observed_ns: i64,
    service: String,
    operation: String,
    attrs_map: BTreeMap<String, String>,
    unresolved_reason: Option<String>,
}

/// OTLP/JSON: resourceSpans[] → scopeSpans[] → spans[]; or a flat array of
/// spans. Fields per the OTLP JSON mapping (hex ids, string nano times).
fn parse_spans(payload: &str) -> Result<Vec<RawSpan>, String> {
    let v: Value = serde_json::from_str(payload).map_err(|e| format!("payload is not JSON: {e}"))?;
    let mut out = Vec::new();
    let empty = Vec::new();
    let span_nodes: Vec<&Value> = if let Some(rs) = v.get("resourceSpans").and_then(|x| x.as_array()) {
        rs.iter()
            .flat_map(|r| r.get("scopeSpans").and_then(|x| x.as_array()).unwrap_or(&empty))
            .flat_map(|s| s.get("spans").and_then(|x| x.as_array()).unwrap_or(&empty))
            .collect()
    } else if let Some(arr) = v.as_array() {
        arr.iter().collect()
    } else {
        return Err("payload is neither OTLP/JSON nor a span array".into());
    };

    for sp in span_nodes {
        let operation = sp
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or("span without a name")?
            .to_string();
        let trace_id = hex_or(sp.get("traceId").and_then(|x| x.as_str()).unwrap_or(""));
        let span_id = hex_or(sp.get("spanId").and_then(|x| x.as_str()).unwrap_or(""));
        let parent_span = sp
            .get("parentSpanId")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty() && *s != "0000000000000000")
            .map(hex_or);
        let start = sp
            .get("startTimeUnixNano")
            .and_then(nano)
            .unwrap_or(0);
        let end = sp
            .get("endTimeUnixNano")
            .and_then(nano)
            .unwrap_or(start);
        let duration_ns = (end - start).max(0);
        let status = sp
            .get("status")
            .map(|s| {
                let code = s.get("code").cloned().unwrap_or(Value::Null);
                match &code {
                    Value::Number(n) if n.as_i64() == Some(1) => "OK".to_string(),
                    Value::Number(n) if n.as_i64() == Some(2) => "ERROR".to_string(),
                    Value::String(s2) if s2 == "ERROR" => "ERROR".to_string(),
                    Value::String(s2) if s2 == "OK" => "OK".to_string(),
                    _ => "UNSET".to_string(),
                }
            })
            .unwrap_or_else(|| "UNSET".to_string());
        // attributes: OTel key/value pairs or a flat object
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        if let Some(attrs) = sp.get("attributes").and_then(|x| x.as_array()) {
            for a in attrs {
                if let (Some(k), Some(v)) = (a.get("key").and_then(|x| x.as_str()), a.get("value")) {
                    let vs = v
                        .get("stringValue")
                        .or_else(|| v.get("intValue"))
                        .or_else(|| v.get("boolValue"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    map.insert(k.to_string(), vs.to_string());
                }
            }
        }
        let service = map
            .get("service.name")
            .cloned()
            .unwrap_or_default();
        out.push(RawSpan {
            trace_id,
            span_id,
            parent_span,
            duration_ns,
            status,
            observed_ns: start,
            service,
            operation,
            attrs_map: map,
            unresolved_reason: None,
        });
    }
    Ok(out)
}

fn hex_or(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn nano(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_i128().and_then(|x| i64::try_from(x).ok())),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// route evidence index: (path fragment → symbol id) from framework_roles.
fn route_index(conn: &Connection) -> Result<Vec<(String, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT symbol_id, evidence_text FROM framework_roles WHERE symbol_id IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    let mut out = Vec::new();
    for (sid, ev) in rows {
        // quoted path literals in the evidence text are the route shape
        for seg in ev.split('"') {
            if seg.starts_with('/') && seg.len() > 1 {
                out.push((seg.trim_end_matches('/').to_string(), sid));
            }
        }
    }
    Ok(out)
}

fn name_index(conn: &Connection) -> Result<BTreeMap<String, i64>, String> {
    let mut stmt = conn.prepare("SELECT id, name FROM symbols").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(0)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().collect())
}

fn match_symbol(
    conn: &Connection,
    routes: &[(String, i64)],
    names: &BTreeMap<String, i64>,
    _service: &str,
    operation: &str,
) -> Result<Option<(i64, String)>, String> {
    // 1. route path match: "GET /orders" contains a declared route path
    for (path, sid) in routes {
        if operation.contains(path.as_str()) {
            let still: i64 = conn
                .query_row("SELECT COUNT(*) FROM symbols WHERE id = ?1", rusqlite::params![sid], |r| r.get(0))
                .unwrap_or(0);
            if still > 0 {
                return Ok(Some((*sid, format!("route evidence '{path}'"))));
            }
        }
    }
    // 2. direct name match on the operation's last segment
    let base = operation
        .rsplit(['/', '.', ':', ' '])
        .next()
        .unwrap_or(operation);
    if let Some(sid) = names.get(base) {
        return Ok(Some((*sid, "operation name match".into())));
    }
    Ok(None)
}

/// Per-symbol aggregates (hit counts, errors, total time, last seen).
fn summarize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "DELETE FROM runtime_summary;
         INSERT INTO runtime_summary(symbol_id, hits, errors, total_ns, last_seen_ns)
         SELECT symbol_id,
                COUNT(*),
                SUM(CASE WHEN status = 'ERROR' THEN 1 ELSE 0 END),
                SUM(duration_ns),
                MAX(observed_ns)
         FROM runtime_evidence
         GROUP BY symbol_id;",
    )
    .map_err(|e| e.to_string())
}

/// Runtime drift (§9): observed span pairs whose symbols have NO static
/// RESOLVED call edge — the runtime does something the static model cannot
/// see. Evidence: the two span operations and their symbols.
fn detect_drift(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM runtime_drift", []).map_err(|e| e.to_string())?;
    // (parent_span_id → symbol) index, then walk child spans
    let parent_sym: BTreeMap<Vec<u8>, i64> = {
        let mut stmt = conn
            .prepare("SELECT span_id, symbol_id FROM runtime_evidence")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };
    let op_names: BTreeMap<Vec<u8>, String> = evidence_ops(conn)?;
    let mut n = 0usize;
    let children: Vec<(Vec<u8>, Option<Vec<u8>>, i64, Option<Vec<u8>>)> = {
        let mut stmt = conn
            .prepare("SELECT span_id, parent_span, symbol_id, attrs FROM runtime_evidence")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Option<Vec<u8>>>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<Vec<u8>>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    for (span_id, parent, child_sym, attrs) in &children {
        let Some(p) = parent else { continue };
        let Some(psym) = parent_sym.get(p) else { continue };
        let child_op = operation_of(attrs).unwrap_or_default();
        let parent_op = op_names.get(p).cloned().unwrap_or_default();
        if psym == child_sym {
            continue;
        }
        let static_edge: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE caller_symbol_id = ?1 AND callee_symbol_id = ?2 AND resolution='RESOLVED'",
                rusqlite::params![psym, child_sym],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if static_edge == 0 {
            conn.execute(
                "INSERT INTO runtime_drift(parent_symbol, child_symbol, parent_operation, child_operation, kind, note)
                 VALUES (?1, ?2, ?3, ?4, 'unmodeled-edge', 'observed parent→child spans have no static resolved call edge — the runtime reaches it another way')",
                rusqlite::params![psym, child_sym, parent_op, child_op],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

/// Operation names per span: stored in attrs at ingest time as
/// `cie.operation` (see ingest — attrs include it).
fn evidence_ops(conn: &Connection) -> Result<BTreeMap<Vec<u8>, String>, String> {
    let mut out = BTreeMap::new();
    let mut stmt = conn
        .prepare("SELECT span_id, attrs FROM runtime_evidence")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Option<Vec<u8>>>(1)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, attrs) in rows {
        if let Some(a) = attrs {
            if let Ok(m) = serde_json::from_slice::<BTreeMap<String, String>>(&a) {
                if let Some(op) = m.get("cie.operation") {
                    out.insert(id, op.clone());
                }
            }
        }
    }
    Ok(out)
}

fn operation_of(attrs: &Option<Vec<u8>>) -> Option<String> {
    let a = attrs.as_ref()?;
    let m: BTreeMap<String, String> = serde_json::from_slice(a).ok()?;
    m.get("cie.operation").cloned()
}

fn count_flaky(conn: &Connection) -> Result<usize, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT symbol_id FROM runtime_evidence
                GROUP BY symbol_id
                HAVING COUNT(DISTINCT status) > 1
             )",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(n as usize)
}

/// Hot paths (acceptance #34): top-k symbols by observed hit count.
pub fn hot_paths(conn: &Connection, k: usize) -> Result<Vec<(String, i64, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.qual_name, r.hits, r.errors FROM runtime_summary r
             JOIN symbols s ON s.id = r.symbol_id
             ORDER BY r.hits DESC, s.qual_name LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![k as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::{create_fw_schema, detect as fw_detect, install_registry};
    use crate::ir::{cache_source_bytes, create_ir_schema, lower_all};
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_ir_schema(&conn).unwrap();
        create_fw_schema(&conn).unwrap();
        create_rt_schema(&conn).unwrap();
        conn
    }

    /// Acceptance #32: a span maps to a SymbolId (via route evidence) or is
    /// explicitly Unresolved with a reason — nothing in between.
    #[test]
    fn spans_map_to_symbols_or_unresolved_with_reason() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(\"/orders\")\ndef list_orders():\n    return []\n";
        let f = ingest_parsed_file(&conn, 1, 41, "svc.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();

        let payload = r#"{
          "resourceSpans": [{
            "scopeSpans": [{
              "spans": [
                { "name": "GET /orders", "traceId": "aa", "spanId": "01",
                  "startTimeUnixNano": "1000", "endTimeUnixNano": "3000",
                  "attributes": [{ "key": "service.name", "value": { "stringValue": "orders" } }] },
                { "name": "totally_unknown_op", "traceId": "aa", "spanId": "02",
                  "parentSpanId": "01",
                  "startTimeUnixNano": "2000", "endTimeUnixNano": "2500",
                  "attributes": [{ "key": "service.name", "value": { "stringValue": "orders" } }] }
              ]
            }]
          }]
        }"#;
        let rep = ingest(&conn, payload).unwrap();
        assert_eq!(rep.spans, 2);
        assert_eq!(rep.mapped, 1, "GET /orders maps via route evidence");
        assert_eq!(rep.unresolved, 1);
        let reason: String = conn
            .query_row("SELECT reason FROM runtime_unresolved", [], |r| r.get(0))
            .unwrap();
        assert!(reason.contains("no symbol matches"), "unresolved states its reason: {reason}");
        // mapped span carries symbol + duration
        let (sid, dur): (i64, i64) = conn
            .query_row("SELECT symbol_id, duration_ns FROM runtime_evidence", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let sym: String = conn
            .query_row("SELECT name FROM symbols WHERE id = ?1", rusqlite::params![sid], |r| r.get(0))
            .unwrap();
        assert_eq!(sym, "list_orders");
        assert_eq!(dur, 2000);
    }

    /// Acceptance #33: observed parent→child with no static edge = drift.
    #[test]
    fn observed_edge_without_static_edge_is_drift() {
        let conn = setup();
        let mut st = IngestStats::default();
        // two symbols; runtime shows parent→child but static has no call
        let py = "def parent_op():\n    return 1\n\ndef child_op():\n    return 2\n";
        let f = ingest_parsed_file(&conn, 1, 41, "x.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();

        // embed the operations into the spans' attrs so the join works
        let payload = r#"{
          "resourceSpans": [{
            "scopeSpans": [{
              "spans": [
                { "name": "parent_op", "traceId": "bb", "spanId": "11",
                  "startTimeUnixNano": "10", "endTimeUnixNano": "20",
                  "attributes": [{ "key": "service.name", "value": { "stringValue": "x" } }] },
                { "name": "child_op", "traceId": "bb", "spanId": "12",
                  "parentSpanId": "11",
                  "startTimeUnixNano": "15", "endTimeUnixNano": "18",
                  "attributes": [{ "key": "service.name", "value": { "stringValue": "x" } }] }
              ]
            }]
          }]
        }"#;
        let rep = ingest(&conn, payload).unwrap();
        assert_eq!(rep.mapped, 2);
        assert!(rep.drift_edges >= 1, "no static edge between them: {rep:?}");
        let (kind, note): (String, String) = conn
            .query_row("SELECT kind, note FROM runtime_drift", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(kind, "unmodeled-edge");
        assert!(note.contains("no static resolved call edge"));
    }

    /// Acceptance #34: hot paths ranked by span frequency.
    #[test]
    fn hot_paths_ranked_by_hits() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(\"/hot\")\ndef hot():\n    return 1\n\n@app.get(\"/cold\")\ndef cold():\n    return 2\n";
        let f = ingest_parsed_file(&conn, 1, 41, "h.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();

        let mut spans = String::from(r#"{"resourceSpans":[{"scopeSpans":[{"spans":["#);
        let mut items = Vec::new();
        for i in 0..5 {
            items.push(format!(
                r#"{{ "name": "GET /hot", "traceId": "cc", "spanId": "{:02x}", "startTimeUnixNano": "10", "endTimeUnixNano": "20" }}"#,
                i + 1
            ));
        }
        items.push(
            r#"{ "name": "GET /cold", "traceId": "cc", "spanId": "99", "startTimeUnixNano": "10", "endTimeUnixNano": "20" }"#.to_string(),
        );
        spans.push_str(&items.join(","));
        spans.push_str("]}]}]}");
        ingest(&conn, &spans).unwrap();
        let hot = hot_paths(&conn, 2).unwrap();
        assert_eq!(hot[0].0, "hot");
        assert_eq!(hot[0].1, 5);
        assert_eq!(hot[1].0, "cold");
        assert_eq!(hot[1].1, 1);
    }

    #[test]
    fn flaky_operations_counted() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(\"/flaky\")\ndef flaky():\n    return 1\n";
        let f = ingest_parsed_file(&conn, 1, 41, "fk.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();
        let payload = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[
            { "name": "GET /flaky", "traceId": "dd", "spanId": "21", "startTimeUnixNano": "10", "endTimeUnixNano": "12",
              "status": { "code": 1 } },
            { "name": "GET /flaky", "traceId": "de", "spanId": "22", "startTimeUnixNano": "20", "endTimeUnixNano": "25",
              "status": { "code": 2 } }
        ]}]}]}"#;
        let rep = ingest(&conn, payload).unwrap();
        assert_eq!(rep.flaky, 1, "same symbol observed OK and ERROR");
        let (hits, errors): (i64, i64) = conn
            .query_row("SELECT hits, errors FROM runtime_summary", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((hits, errors), (2, 1));
    }
}
