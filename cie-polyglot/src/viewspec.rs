//! PRD v2.0 §25 ViewSpec producer — engine-side emission for workflows and
//! state machines. The visual law is the PRD's:
//!
//! | displayMode | stroke | required disclosure |
//! |---|---|---|
//! | Fact | solid | evidence only |
//! | Inference | dashed | `claimId` |
//! | Hypothesis | dotted | `claimId` + `counterArgument` |
//! | Fog | hatched | `claimId` + `fogReason` |
//!
//! These are the same invariants the TS-side `gate()` (§26) enforces before
//! any engine renders — the producer refuses to emit a spec that would fail
//! the gate, so the two layers cannot drift.

use std::collections::BTreeMap;

use rusqlite::Connection;

use serde_json::{json, Value};

/// Produce the ViewSpec for every state machine + workflow in the store.
/// Returns one spec per artifact plus a combined overview, keyed by name.
pub fn produce(conn: &Connection) -> Result<BTreeMap<String, Value>, String> {
    let mut specs = BTreeMap::new();
    produce_state_machines(conn, &mut specs)?;
    produce_workflows(conn, &mut specs)?;
    Ok(specs)
}

fn machine_claims(conn: &Connection) -> Result<BTreeMap<i64, i64>, String> {
    // stable per-store claim numbering: C-SM-<n> in id order
    let mut stmt = conn
        .prepare("SELECT id FROM state_machines ORDER BY id")
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids.into_iter().enumerate().map(|(i, id)| (id, (i + 1) as i64)).collect())
}

fn produce_state_machines(conn: &Connection, specs: &mut BTreeMap<String, Value>) -> Result<(), String> {
    let claims = machine_claims(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.name, m.tier, m.epistemic, m.states, m.transitions, m.coverage, m.counter_argument, m.fog_reason
             FROM state_machines m ORDER BY m.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    for (id, name, tier, epistemic, states, transitions, coverage, counter, fog) in rows {
        let states: Vec<String> = serde_json::from_str(&states).map_err(|e| e.to_string())?;
        let transitions: Vec<Value> = serde_json::from_str(&transitions).map_err(|e| e.to_string())?;
        let claim = format!("C-SM-{}", claims.get(&id).copied().unwrap_or(id));
        let mut elements: Vec<Value> = Vec::new();
        for s in &states {
            let mut el = json!({
                "id": format!("{name}:{s}"),
                "elementKind": "state",
                "displayMode": epistemic,
                "label": s,
                "evidence": { "kind": "state-machine", "tier": tier, "machine": name },
                "fields": { "machine": name, "tier": tier },
            });
            if epistemic != "Fact" {
                el["claimId"] = json!(claim);
            }
            if let Some(c) = &counter {
                el["counterArgument"] = json!(c);
            }
            if let Some(f) = fog.as_ref().or(coverage.as_ref()) {
                el["fogReason"] = json!(f);
            }
            elements.push(el);
        }
        let mut edges: Vec<Value> = Vec::new();
        for t in &transitions {
            edges.push(json!({
                "source": format!("{name}:{}", t["from"]),
                "target": format!("{name}:{}", t["to"]),
                "edgeKind": "transition",
                "displayMode": epistemic,
                "evidence": { "kind": "transition", "event": t["event"] },
            }));
        }
        let mut footer = json!({
            "epistemic": epistemic,
            "tier": tier,
        });
        if let Some(c) = &counter {
            footer["counterArgument"] = json!(c);
        }
        if let Some(f) = &fog {
            footer["fogReason"] = json!(f);
        }
        let spec = json!({
            "elements": elements,
            "edges": edges,
            "footer": footer,
            "gaps": coverage.as_ref().map(|c| json!([{ "reason": c }])).unwrap_or(json!([])),
            "scope": { "artifact": "state-machine", "tier": tier },
            "layoutHint": { "strategy": "cie-state-machine" },
        });
        validate(&spec)?;
        specs.insert(format!("state-machine:{name}"), spec);
    }
    Ok(())
}

fn produce_workflows(conn: &Connection, specs: &mut BTreeMap<String, Value>) -> Result<(), String> {
    // one combined workflow spec (all workflows share the Hypothesis posture)
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.workflow_id, n.node_kind, n.epistemic, n.fog_reason, n.depth, n.evidence,
                    s.qual_name, w.name
             FROM workflow_nodes n
             LEFT JOIN symbols s ON s.id = n.symbol_id
             JOIN workflows w ON w.id = n.workflow_id
             ORDER BY n.workflow_id, n.depth",
        )
        .map_err(|e| e.to_string())?;
    let nodes = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    if nodes.is_empty() {
        return Ok(());
    }
    let mut elements: Vec<Value> = Vec::new();
    let mut id_by_row: BTreeMap<i64, String> = BTreeMap::new();
    for (row, wf, kind, _epi, fog, depth, evidence, qname, wname) in &nodes {
        let label = qname.clone().unwrap_or_else(|| kind.to_string());
        let el_id = format!("wf{wf}:{label}:{row}");
        id_by_row.insert(*row, el_id.clone());
        // node-level display mode: Fog boundaries are Fog; the static chain
        // is Inference; the workflow's own posture is Hypothesis on the entry
        let display = if kind == "async_boundary" {
            "Fog"
        } else if *depth == 0 {
            "Hypothesis"
        } else {
            "Inference"
        };
        let mut el = json!({
            "id": el_id,
            "elementKind": "symbol",
            "displayMode": display,
            "label": label,
            "evidence": { "kind": "workflow-edge", "text": evidence },
            "fields": { "workflow": wname, "nodeKind": kind, "depth": depth },
        });
        if display != "Fact" {
            el["claimId"] = json!(format!("C-WF-{wf}"));
        }
        if display == "Hypothesis" {
            el["counterArgument"] = json!(WF_COUNTER_PLACEHOLDER);
        }
        if let Some(f) = fog {
            el["fogReason"] = json!(f);
        }
        elements.push(el);
    }
    let mut edges: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT from_node, to_node, kind FROM workflow_edges")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (from, to, kind) in rows {
            let (Some(f), Some(t)) = (id_by_row.get(&from), id_by_row.get(&to)) else { continue };
            edges.push(json!({
                "source": f,
                "target": t,
                "edgeKind": kind,
                "displayMode": if kind == "handoff" { "Fog" } else { "Inference" },
                "evidence": { "kind": "call-edge" },
            }));
        }
    }
    let spec = json!({
        "elements": elements,
        "edges": edges,
        "footer": {
            "epistemic": "Hypothesis",
            "counterArgument": WF_COUNTER_PLACEHOLDER,
        },
        "gaps": [],
        "scope": { "artifact": "workflow" },
        "layoutHint": { "strategy": "dagre" },
    });
    validate(&spec)?;
    specs.insert("workflows".to_string(), spec);
    Ok(())
}

const WF_COUNTER_PLACEHOLDER: &str =
    "static call order across components; runtime branching, retries and queues may differ — needs runtime evidence to confirm";

/// The producer-side mirror of the TS gate (§26): refuses to emit a spec
/// that the gate would reject. Same rules, same refusals.
pub fn validate(spec: &Value) -> Result<(), String> {
    let els = spec
        .get("elements")
        .and_then(|e| e.as_array())
        .ok_or("spec has no elements")?;
    for el in els {
        let id = el.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let display = el.get("displayMode").and_then(|v| v.as_str()).unwrap_or("Fact");
        if el.get("evidence").is_none() {
            return Err(format!("ELEMENT_WITHOUT_EVIDENCE: {id}"));
        }
        if display != "Fact" && el.get("claimId").is_none() {
            return Err(format!("NON_FACT_WITHOUT_CLAIM: {id}"));
        }
        if display == "Hypothesis" && el.get("counterArgument").is_none() {
            return Err(format!("HYPOTHESIS_WITHOUT_COUNTER: {id}"));
        }
        if display == "Fog" && el.get("fogReason").is_none() {
            return Err(format!("FOG_WITHOUT_REASON: {id}"));
        }
    }
    if let Some(edges) = spec.get("edges").and_then(|e| e.as_array()) {
        for e in edges {
            if e.get("evidence").is_none() {
                return Err("EDGE_WITHOUT_EVIDENCE".to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::{create_fw_schema, detect as fw_detect, install_registry};
    use crate::ir::{cache_source_bytes, create_ir_schema, lower_all};
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use crate::statemachines::{create_sm_schema, detect as sm_detect};
    use crate::workflows::{create_wf_schema, detect as wf_detect};
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
    fn states_and_workflow_specs_pass_the_gate_rules() {
        let conn = setup();
        let mut st = IngestStats::default();
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
        let ts = "export const m = createMachine({\n  states: { draft: { on: { PAY: 'paid' } }, paid: {} }\n});\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "svc.py", Lang::Python, py, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "m.ts", Lang::TypeScript, ts, &mut st).unwrap();
        cache_source_bytes(&conn, f1, py).unwrap();
        cache_source_bytes(&conn, f2, ts).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();
        sm_detect(&conn).unwrap();
        wf_detect(&conn).unwrap();

        let specs = produce(&conn).unwrap();
        assert!(specs.keys().any(|k| k.starts_with("state-machine:")), "{:?}", specs.keys());
        assert!(specs.contains_key("workflows"));
        for (name, spec) in &specs {
            validate(spec).unwrap_or_else(|e| panic!("spec {name} must pass the gate mirror: {e}"));
        }
        // the XState spec is Fact (T1) — no claimIds on its state elements
        let sm = specs.values().find(|s| s["scope"]["artifact"] == "state-machine").unwrap();
        for el in sm["elements"].as_array().unwrap() {
            assert_eq!(el["displayMode"], "Fact");
            assert!(el.get("claimId").is_none(), "Fact carries no claim disclosures");
        }
        // the workflow spec: entry is Hypothesis with counter-argument; the
        // async boundary is Fog with its reason
        let wf = &specs["workflows"];
        let entry = wf["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["fields"]["depth"] == 0)
            .unwrap();
        assert_eq!(entry["displayMode"], "Hypothesis");
        assert!(entry.get("counterArgument").is_some());
        let fog = wf["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["displayMode"] == "Fog")
            .expect("async boundary present as Fog");
        assert!(fog.get("fogReason").is_some());
    }

    #[test]
    fn validate_refuses_unevidenced_and_undisclosed() {
        let no_evidence = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x" } ], "edges": [] });
        assert_eq!(validate(&no_evidence), Err("ELEMENT_WITHOUT_EVIDENCE: x".to_string()));
        let no_claim = json!({ "elements": [ { "id": "x", "displayMode": "Inference", "label": "x", "evidence": {} } ], "edges": [] });
        assert_eq!(validate(&no_claim), Err("NON_FACT_WITHOUT_CLAIM: x".to_string()));
        let no_counter = json!({ "elements": [ { "id": "x", "displayMode": "Hypothesis", "label": "x", "evidence": {}, "claimId": "C-1" } ], "edges": [] });
        assert_eq!(validate(&no_counter), Err("HYPOTHESIS_WITHOUT_COUNTER: x".to_string()));
        let no_fog = json!({ "elements": [ { "id": "x", "displayMode": "Fog", "label": "x", "evidence": {}, "claimId": "C-1" } ], "edges": [] });
        assert_eq!(validate(&no_fog), Err("FOG_WITHOUT_REASON: x".to_string()));
        let good = json!({ "elements": [ { "id": "x", "displayMode": "Fog", "label": "x", "evidence": {}, "claimId": "C-1", "fogReason": "why" } ], "edges": [ { "source": "x", "target": "x", "evidence": {} } ] });
        assert!(validate(&good).is_ok());
    }
}
