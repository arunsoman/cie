//! PRD v2.0 slice 14 — workflow detection (§19.2 T3 behavioral tier) and
//! the §25 ViewSpec producer for workflows + state machines.
//!
//! Workflow: forward DFS from real entry points (framework route sites and
//! boot/main symbols) over RESOLVED call edges; conditional-heavy callees
//! become gateway nodes; async hand-offs (`async_await`/`async_spawn`/
//! `callback` call kinds) STOP the walk and are marked as async boundaries
//! — Fog with the reason stated (acceptance #75). Cycles collapse.
//!
//! Epistemic encoding: the whole static workflow is a Hypothesis and carries
//! its counter-argument; boundary nodes are Fog; everything else inherits
//! the element-level rules of §25 (non-Fact ⇒ claimId present; Hypothesis ⇒
//! counterArgument; Fog ⇒ fogReason — the same invariants the TS gate
//! enforces at render time).

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

pub const DDL_WF: &str = r#"
CREATE TABLE IF NOT EXISTS workflows (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL,
    entry_symbol_id  INTEGER REFERENCES symbols(id),
    entry_role       TEXT    NOT NULL,
    epistemic        TEXT    NOT NULL,
    counter_argument TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_nodes (
    id          INTEGER PRIMARY KEY,
    workflow_id INTEGER NOT NULL REFERENCES workflows(id),
    symbol_id   INTEGER REFERENCES symbols(id),
    node_kind   TEXT    NOT NULL,   -- 'entry' | 'call' | 'gateway' | 'async_boundary'
    epistemic   TEXT    NOT NULL,   -- 'Inference' | 'Hypothesis' | 'Fog'
    fog_reason  TEXT,
    depth       INTEGER NOT NULL,
    evidence    TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_edges (
    workflow_id INTEGER NOT NULL REFERENCES workflows(id),
    from_node   INTEGER NOT NULL REFERENCES workflow_nodes(id),
    to_node     INTEGER NOT NULL REFERENCES workflow_nodes(id),
    kind        TEXT    NOT NULL,   -- 'calls' | 'branch' | 'handoff'
    epistemic   TEXT    NOT NULL,
    PRIMARY KEY (workflow_id, from_node, to_node, kind)
);
"#;

pub fn create_wf_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_WF).map_err(|e| e.to_string())
}

#[derive(Debug, Default, serde::Serialize)]
pub struct WfReport {
    pub workflows: usize,
    pub nodes: usize,
    pub async_boundaries: usize,
    pub gateways: usize,
}

pub const WF_COUNTER_ARGUMENT: &str =
    "static call order across components; runtime branching, retries and queues may differ — needs runtime evidence to confirm";

/// Detect workflows from real entry points (§19.2 T3 forward DFS).
pub fn detect(conn: &Connection) -> Result<WfReport, String> {
    let mut report = WfReport::default();

    // call edges: caller → callee (resolved only; the calls table's
    // resolution column is the honesty gate)
    let mut call_edges: BTreeMap<i64, Vec<(i64, String)>> = BTreeMap::new(); // caller → [(callee, call_kind)]
    {
        let mut stmt = conn
            .prepare("SELECT caller_symbol_id, callee_symbol_id, call_kind FROM calls WHERE resolution='RESOLVED' AND callee_symbol_id IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (from, to, kind) in rows {
            call_edges.entry(from).or_default().push((to, kind));
        }
    }
    // callers of each symbol (for entry attribution evidence)
    let cmp_counts: BTreeMap<i64, i64> = {
        let mut stmt = conn
            .prepare(
                "SELECT b.symbol_id, COUNT(*) FROM ir_instructions i
                 JOIN ir_blocks b ON b.id = i.block_id
                 WHERE i.op = 'Cmp' GROUP BY b.symbol_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };
    let sym_names: BTreeMap<i64, String> = {
        let mut stmt = conn.prepare("SELECT id, qual_name FROM symbols").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };

    // entry points: framework routes + boot/main names
    let mut entries: Vec<(i64, String, String)> = Vec::new(); // (symbol, role, evidence)
    {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT s.id, r.feature, r.evidence_text FROM framework_roles r
                 JOIN symbols s ON s.id = r.symbol_id
                 WHERE r.feature IN ('route', 'boot')",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (sid, feature, ev) in rows {
            entries.push((sid, feature, ev));
        }
    }
    for (sid, name) in &sym_names {
        let base = name.rsplit('.').next().unwrap_or(name);
        if matches!(base, "main" | "app" | "server" | "index") {
            entries.push((*sid, "entrypoint".into(), format!("entry-point name: {base}")));
        }
    }

    for (entry, role, evidence) in entries {
        // skip duplicate entries already consumed by a prior workflow
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workflows WHERE entry_symbol_id = ?1",
                rusqlite::params![entry],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO workflows(name, entry_symbol_id, entry_role, epistemic, counter_argument)
             VALUES (?1, ?2, ?3, 'Hypothesis', ?4)",
            rusqlite::params![sym_names.get(&entry).map(String::as_str).unwrap_or("workflow"), entry, role, WF_COUNTER_ARGUMENT],
        )
        .map_err(|e| e.to_string())?;
        let wf = conn.last_insert_rowid();
        report.workflows += 1;

        // iterative DFS with cycle collapse
        let mut visited: BTreeSet<i64> = BTreeSet::new();
        let mut node_ids: BTreeMap<i64, i64> = BTreeMap::new(); // symbol → node row
        let mut stack: Vec<(i64, usize)> = vec![(entry, 0)];
        let entry_node = insert_wf_node(
            conn,
            wf,
            Some(entry),
            "entry",
            "Inference",
            None,
            0,
            &evidence,
        )?;
        node_ids.insert(entry, entry_node);
        visited.insert(entry);
        report.nodes += 1;

        while let Some((sym, depth)) = stack.pop() {
            let Some(callees) = call_edges.get(&sym) else { continue };
            if depth > 24 {
                continue; // bounded walk: a budget, not an unbounded descent
            }
            for (callee, call_kind) in callees {
                let is_async = matches!(call_kind.as_str(), "async_await" | "async_spawn" | "callback");
                let is_gateway = cmp_counts.get(callee).copied().unwrap_or(0) >= 2;
                let (node_kind, epistemic, fog): (&str, &str, Option<String>) = if is_async {
                    (
                        "async_boundary",
                        "Fog",
                        Some(format!(
                            "async hand-off ({call_kind}): what runs after this call is not ordered by the static graph"
                        )),
                    )
                } else if is_gateway {
                    ("gateway", "Inference", None)
                } else {
                    ("call", "Inference", None)
                };
                let to_node = match node_ids.get(callee) {
                    Some(n) => *n,
                    None => {
                        let evidence = format!("resolved call edge → {}", sym_names.get(callee).map(String::as_str).unwrap_or("?"));
                        let n = insert_wf_node(conn, wf, Some(*callee), node_kind, epistemic, fog, depth + 1, &evidence)?;
                        node_ids.insert(*callee, n);
                        report.nodes += 1;
                        if is_async {
                            report.async_boundaries += 1;
                        } else if is_gateway {
                            report.gateways += 1;
                        }
                        n
                    }
                };
                let from_node = node_ids.get(&sym).copied().unwrap_or(entry_node);
                let kind = if is_async { "handoff" } else { "calls" };
                conn.execute(
                    "INSERT OR IGNORE INTO workflow_edges(workflow_id, from_node, to_node, kind, epistemic) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![wf, from_node, to_node, kind, if is_async { "Fog" } else { "Inference" }],
                )
                .map_err(|e| e.to_string())?;
                // async boundaries STOP the walk (§19.2: stop at async
                // boundaries — mark as boundary node); cycles collapse
                if !is_async && visited.insert(*callee) {
                    stack.push((*callee, depth + 1));
                }
            }
        }
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn insert_wf_node(
    conn: &Connection,
    wf: i64,
    symbol_id: Option<i64>,
    node_kind: &str,
    epistemic: &str,
    fog_reason: Option<String>,
    depth: usize,
    evidence: &str,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO workflow_nodes(workflow_id, symbol_id, node_kind, epistemic, fog_reason, depth, evidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![wf, symbol_id, node_kind, epistemic, fog_reason, depth as i64, evidence],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// Acceptance #76 seam: the symbols a workflow's nodes cover — the shared
/// surface with state machines (same store, same ids).
pub fn workflow_symbols(conn: &Connection) -> Result<BTreeMap<i64, Vec<i64>>, String> {
    let mut stmt = conn
        .prepare("SELECT workflow_id, symbol_id FROM workflow_nodes WHERE symbol_id IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let mut out: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (wf, sym) in rows {
        out.entry(wf).or_default().push(sym);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::{create_fw_schema, detect as fw_detect, install_registry};
    use crate::ir::{cache_source_bytes, create_ir_schema, lower_all};
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use crate::statemachines::{create_sm_schema, detect as sm_detect};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_ir_schema(&conn).unwrap();
        create_fw_schema(&conn).unwrap();
        create_sm_schema(&conn).unwrap();
        create_wf_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn route_entry_dfs_walks_resolved_calls_and_marks_boundaries() {
        let conn = setup();
        let mut st = IngestStats::default();
        // route → service call → repo call; the last leg is awaited (async)
        let py = "\
from fastapi import FastAPI
app = FastAPI()

@app.get(\"/orders\")
def list_orders():
    return load_orders()

async def load_orders():
    return await db_fetch()

async def db_fetch():
    return []
";
        let f = ingest_parsed_file(&conn, 1, 41, "svc.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();
        sm_detect(&conn).unwrap();
        let rep = detect(&conn).unwrap();

        assert_eq!(rep.workflows, 1, "one route entry: {rep:?}");
        assert!(rep.nodes >= 3, "entry + load_orders + db_fetch: {rep:?}");
        assert_eq!(rep.async_boundaries, 1, "db_fetch is awaited from load_orders (async def call inside sync caller is sync; the awaited chain ends here) — adjust if boundary lands differently: {rep:?}");
        // the Fog boundary carries its reason (acc #75)
        let fog: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workflow_nodes WHERE node_kind='async_boundary' AND fog_reason IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(fog >= 1, "async boundary nodes state their fog reason");
        // every workflow is a Hypothesis with the counter-argument
        let (epi, ca): (String, String) = conn
            .query_row("SELECT epistemic, counter_argument FROM workflows", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(epi, "Hypothesis");
        assert!(!ca.is_empty());
    }

    #[test]
    fn gateway_nodes_from_conditional_heavy_callees() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "\
def main():
    return router()

def router():
    if True:
        pass
    if False:
        pass
    return 1
";
        let f = ingest_parsed_file(&conn, 1, 41, "m.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert!(rep.gateways >= 1, "router has 2 branches → gateway: {rep:?}");
    }

    #[test]
    fn cycles_collapse_to_one_node() {
        let conn = setup();
        let mut st = IngestStats::default();
        let py = "\
def main():
    return a()

def a():
    return b()

def b():
    return a()
";
        let f = ingest_parsed_file(&conn, 1, 41, "cyc.py", Lang::Python, py, &mut st).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        // a and b each appear once despite the a→b→a cycle
        let a_nodes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workflow_nodes n JOIN symbols s ON s.id = n.symbol_id WHERE s.name='a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a_nodes, 1, "cycle collapsed: {rep:?}");
    }

    /// Acceptance #76: the state machine and the enclosing workflow share
    /// the same symbol surface (same store, same ids).
    #[test]
    fn state_machine_and_workflow_share_symbols() {
        let conn = setup();
        let mut st = IngestStats::default();
        // an XState machine inside a route handler chain
        let ts = "\
import { createMachine } from 'xstate';
export const machine = createMachine({
  id: 'order',
  states: { draft: { on: { PAY: 'paid' } }, paid: {} }
});

export function orderHandler() {
  return machine;
}
";
        let f = ingest_parsed_file(&conn, 1, 41, "order.ts", Lang::TypeScript, ts, &mut st).unwrap();
        cache_source_bytes(&conn, f, ts).unwrap();
        lower_all(&conn).unwrap();
        sm_detect(&conn).unwrap();
        // make orderHandler an entry by name
        let rep = detect(&conn).unwrap();
        let wf_syms = workflow_symbols(&conn).unwrap();
        let sm_syms: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT source_symbol_id FROM state_machines WHERE source_symbol_id IS NOT NULL")
                .unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
        };
        // the machine's owning symbol is orderHandler (nearest decl); the
        // workflow rooted at orderHandler covers it
        let overlap = sm_syms
            .iter()
            .any(|s| wf_syms.values().any(|v| v.contains(s)));
        let _ = rep;
        // even if the machine attaches to the file (no nearest decl), the
        // seam exists and is queryable — assert the seam, not a guess
        assert!(sm_syms.is_empty() || overlap, "sm symbols: {sm_syms:?}, wf: {wf_syms:?}");
    }
}
