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
//!
//! Evidence is the byte-span ref the TS contract requires (`{file, start,
//! end}` — viz-adapter/src/types.ts `EvidenceRef`, enforced by
//! `gate.ts` on every element AND edge). Semantic provenance (machine tier,
//! detection kind, …) lives in `fields`, never in place of the span. The
//! local `validate()` mirrors the FULL TS clause set — duplicate ids, span
//! evidence, claim/counter/fog disclosures, bodyLines/alsoEvidence
//! well-formedness, the 2 000-element cap — so a spec that passes here
//! passes `gate()` there.

use std::collections::BTreeMap;

use rusqlite::Connection;

use serde_json::{json, Value};

/// Produce the ViewSpec for every state machine + workflow in the store.
/// Returns one spec per artifact plus a combined overview, keyed by name.
pub fn produce(conn: &Connection) -> Result<BTreeMap<String, Value>, String> {
    let mut specs = BTreeMap::new();
    produce_state_machines(conn, &mut specs)?;
    produce_workflows(conn, &mut specs)?;
    produce_c4(conn, &mut specs)?;
    Ok(specs)
}

/// The bundle: every produced spec in one JSON document — the seam the web
/// pipeline consumes (one engine invocation, one parse on the reader side).
/// Impact+duplicate specs are produced per duplicate-family center
/// (non-hidden rows), capped at 50; the cap is counted, never silent.
///
/// Each producer family (and each impact center) is independent: one
/// family's error — a gate failure or anything else — is recorded in
/// `skipped` and the rest of the bundle still ships. A single bad spec
/// used to blank the whole document (empty stdout, exit 1); that is worse
/// than honestly naming what couldn't be drawn.
pub fn produce_bundle(conn: &Connection) -> Result<Value, String> {
    let mut specs = BTreeMap::new();
    let mut skipped: Vec<Value> = Vec::new();

    for (family, result) in [
        ("state-machines", produce_state_machines(conn, &mut specs)),
        ("workflows", produce_workflows(conn, &mut specs)),
        ("c4", produce_c4(conn, &mut specs)),
    ] {
        if let Err(reason) = result {
            skipped.push(json!({ "family": family, "reason": reason }));
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT symbol_a FROM duplicates WHERE tier != 'hidden'
             UNION
             SELECT DISTINCT symbol_b FROM duplicates WHERE tier != 'hidden'
             ORDER BY 1
             LIMIT 51",
        )
        .map_err(|e| e.to_string())?;
    let centers: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    let truncated = centers.len() > 50;
    for center in centers.into_iter().take(50) {
        match crate::duplicates::impact_duplicate_spec(conn, center) {
            Ok(spec) => {
                specs.insert(format!("impact:{center}"), spec);
            }
            Err(reason) => skipped.push(json!({ "family": format!("impact:{center}"), "reason": reason })),
        }
    }
    Ok(json!({
        "kind": "cie-polyglot-viewspec-bundle",
        "spec-count": specs.len(),
        "impact-centers-truncated": truncated,
        "skipped": skipped,
        "specs": Value::Object(specs.into_iter().collect()),
    }))
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
            "SELECT m.id, m.name, m.tier, m.epistemic, m.states, m.transitions, m.coverage, m.counter_argument, m.fog_reason,
                    m.source_symbol_id, m.evidence_node_id
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
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<i64>>(10)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    for (id, name, tier, epistemic, states, transitions, coverage, counter, fog, sym, node) in rows {
        // the byte-span anchor: the machine's symbol, else its evidence AST
        // node. A machine with NEITHER has no locatable source — it is not
        // shown (an unanchorable claim would fail the gate's evidence rule,
        // and fabricating a file would be worse).
        let Some(span) = sym
            .and_then(|s| symbol_span(conn, s))
            .or_else(|| node.and_then(|n| ast_node_span(conn, n)))
        else {
            continue;
        };
        let states: Vec<String> = serde_json::from_str(&states).map_err(|e| e.to_string())?;
        let transitions: Vec<Value> = serde_json::from_str(&transitions).map_err(|e| e.to_string())?;
        let claim = format!("C-SM-{}", claims.get(&id).copied().unwrap_or(id));
        let salience = json!({
            "factors": [format!("tier:{tier}")],
            "weight": match epistemic.as_str() {
                "Fact" => 1.0,
                "Inference" => 0.7,
                "Hypothesis" => 0.4,
                _ => 0.3,
            },
            "whyShown": format!("state machine '{name}' detected at {}", span.0),
        });
        let mut elements: Vec<Value> = Vec::new();
        for s in &states {
            let mut el = json!({
                "id": format!("{name}:{s}"),
                "elementKind": "state",
                "displayMode": epistemic,
                "label": s,
                "evidence": span_ev(&span),
                "salience": salience,
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
                "evidence": span_ev(&span),
                "fields": { "event": t["event"] },
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
                    s.qual_name, w.name, s.start_byte, s.end_byte, f.path
             FROM workflow_nodes n
             LEFT JOIN symbols s ON s.id = n.symbol_id
             LEFT JOIN files f ON f.id = s.file_id
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
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<i64>>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    if nodes.is_empty() {
        return Ok(());
    }
    // entry-span fallback: symbol-less nodes (async boundaries observed
    // from the walk) cite the entry symbol's span of their own workflow —
    // the hand-off was observed FROM there, so the citation is honest.
    let entry_span: BTreeMap<i64, Span> = nodes
        .iter()
        .filter(|(_, _, kind, _, _, depth, _, _, _, start, end, path)| {
            *depth == 0 && kind != "async_boundary" && start.is_some() && end.is_some() && path.is_some()
        })
        .map(|(_, wf, _, _, _, _, _, _, _, start, end, path)| {
            (*wf, (path.clone().unwrap(), start.unwrap(), end.unwrap()))
        })
        .collect();
    let mut elements: Vec<Value> = Vec::new();
    let mut id_by_row: BTreeMap<i64, String> = BTreeMap::new();
    let mut span_by_row: BTreeMap<i64, Span> = BTreeMap::new();
    for (row, wf, kind, _epi, fog, depth, _evidence, qname, wname, start, end, path) in &nodes {
        let label = qname.clone().unwrap_or_else(|| kind.to_string());
        let span = match (start, end, path) {
            (Some(s), Some(e), Some(p)) => Some((p.clone(), *s, *e)),
            _ => entry_span.get(wf).cloned(),
        };
        let Some(span) = span else { continue };
        let el_id = format!("wf{wf}:{label}:{row}");
        id_by_row.insert(*row, el_id.clone());
        span_by_row.insert(*row, span.clone());
        // node-level display mode: Fog boundaries are Fog; the static chain
        // is Inference; the workflow's own posture is Hypothesis on the entry
        let display = if kind == "async_boundary" {
            "Fog"
        } else if *depth == 0 {
            "Hypothesis"
        } else {
            "Inference"
        };
        let salience = json!({
            "factors": [format!("nodeKind:{kind}"), format!("depth:{depth}")],
            "weight": match display {
                "Inference" => 0.7,
                "Hypothesis" => 0.4,
                _ => 0.3,
            },
            "whyShown": format!("workflow '{}' step at depth {depth}", wname),
        });
        let mut el = json!({
            "id": el_id,
            "elementKind": "symbol",
            "displayMode": display,
            "label": label,
            "evidence": span_ev(&span),
            "salience": salience,
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
            // the edge's evidence is the caller's span — the resolved call
            // site that justifies the arrow lives inside the source node
            let evidence = span_by_row.get(&from).map(span_ev).unwrap_or(json!(null));
            edges.push(json!({
                "source": f,
                "target": t,
                "edgeKind": kind,
                "displayMode": if kind == "handoff" { "Fog" } else { "Inference" },
                "evidence": evidence,
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

// ---------------------------------------------------------------------------
// C4 (§20) — the concept DAG as a ViewSpec
// ---------------------------------------------------------------------------

/// C4 spec from the §11.7 concept DAG tables + §20.3 relationships.
/// displayMode follows confidence: 1.0 = Fact, below = Inference with its
/// per-node claimId. Evidence is a real byte span per node: the symbol a
/// component was detected at, the manifest file a container roots at, the
/// first file under a DDD segment, the store's first file for the project
/// root. Edges carry the §22.3 closed reason set as edgeKind and cite the
/// SOURCE node's span (the call site lives in the source component).
/// Over-cap stores truncate class nodes salience-first — counted, never
/// silent; edges into dropped nodes are dropped with them.
fn produce_c4(conn: &Connection, specs: &mut BTreeMap<String, Value>) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.label, n.kind, n.confidence, n.provenance
             FROM concept_nodes n ORDER BY n.id",
        )
        .map_err(|e| e.to_string())?;
    let nodes = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    if nodes.is_empty() {
        return Ok(());
    }

    // --- per-node byte-span evidence ------------------------------------
    // symbol carrier → span; else resolve from the provenance shape
    let span_of_node = |node_id: i64, provenance: &str| -> Option<Span> {
        let sym: Option<i64> = conn
            .query_row(
                "SELECT symbol_id FROM concept_node_symbols WHERE concept_id = ?1 AND symbol_id IS NOT NULL ORDER BY symbol_id LIMIT 1",
                rusqlite::params![node_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(s) = sym {
            return symbol_span(conn, s);
        }
        // containers: "manifest:<dir> (<tool>)" → the manifest file itself
        if let Some(rest) = provenance.strip_prefix("manifest:") {
            let dir = rest.split(" (").next().unwrap_or("").trim();
            for (m, _) in crate::c4::MANIFESTS {
                let candidate = if dir.is_empty() || dir == "./" {
                    m.to_string()
                } else {
                    format!("{dir}/{m}")
                };
                if let Ok((path, size)) = conn.query_row(
                    "SELECT path, size_bytes FROM files WHERE path = ?1 ORDER BY id LIMIT 1",
                    rusqlite::params![candidate],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                ) {
                    return Some((path, 0, size));
                }
            }
            return None;
        }
        // DDD segments: "ddd-dir:<seg_path>" → the first file under it
        if let Some(seg) = provenance.strip_prefix("ddd-dir:") {
            if let Ok((path, size)) = conn.query_row(
                "SELECT path, size_bytes FROM files WHERE path LIKE ?1 ORDER BY id LIMIT 1",
                rusqlite::params![format!("{seg}%")],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            ) {
                return Some((path, 0, size));
            }
            return None;
        }
        // project root: the store's own first file is the evidence the
        // project exists (§20.1: the root is where the walk started)
        conn.query_row(
            "SELECT path, size_bytes FROM files ORDER BY id LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok()
        .map(|(path, size)| (path, 0, size))
    };

    // build candidate elements with a salience rank for the cap
    struct C4El {
        node_id: i64,
        rank: u8,
        el: Value,
        span: Span,
    }
    let mut candidates: Vec<C4El> = Vec::new();
    for (id, label, kind, confidence, provenance) in &nodes {
        let Some(span) = span_of_node(*id, provenance) else { continue };
        let fact = *confidence >= 1.0;
        let display = if fact { "Fact" } else { "Inference" };
        let rank: u8 = if provenance.starts_with("class node") {
            2 // classes are the leaves — first to yield under the cap
        } else if kind == "Container" || kind == "Domain" {
            0 // the view's skeleton — last to yield
        } else {
            1 // components
        };
        let mut el = json!({
            "id": format!("c4:{id}"),
            "elementKind": if kind == "Container" { "container" } else if kind == "Domain" { "domain" } else { "component" },
            "displayMode": display,
            "label": label,
            "evidence": span_ev(&span),
            "salience": {
                "factors": [format!("kind:{kind}")],
                "weight": confidence,
                "whyShown": provenance,
            },
            "fields": { "c4Kind": kind, "confidence": confidence, "provenance": provenance },
        });
        if !fact {
            el["claimId"] = json!(format!("C-C4-{id}"));
        }
        candidates.push(C4El { node_id: *id, rank, el, span });
    }
    if candidates.is_empty() {
        return Ok(());
    }

    // VIZ-I-06: salience-bounded truncation, counted not silent
    candidates.sort_by(|a, b| a.rank.cmp(&b.rank).then(b.el["salience"]["weight"].as_f64().unwrap_or(0.0).total_cmp(&a.el["salience"]["weight"].as_f64().unwrap_or(0.0))).then(a.node_id.cmp(&b.node_id)));
    let mut gaps: Vec<Value> = Vec::new();
    let mut hidden_classes = 0usize;
    let mut kept: Vec<C4El> = Vec::new();
    let mut seen_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for c in candidates {
        if kept.len() >= ELEMENT_CAP {
            hidden_classes += 1;
            continue;
        }
        if !seen_ids.insert(c.el["id"].as_str().unwrap_or_default().to_string()) {
            hidden_classes += 1; // duplicate label-id guard — never two of one node
            continue;
        }
        kept.push(c);
    }
    if hidden_classes > 0 {
        gaps.push(json!({ "reason": format!("{hidden_classes} concept node(s) below the salience line stay hidden, counted not shown") }));
    }
    let span_by_node: BTreeMap<i64, Span> = kept.iter().map(|c| (c.node_id, c.span.clone())).collect();
    let elements: Vec<Value> = kept.into_iter().map(|c| c.el).collect();

    // containment / cohesion edges (§11.7, reasons from the §22.3 closed set)
    let mut edges: Vec<Value> = Vec::new();
    let mut push_edge = |from: i64, to: i64, kind: &str, display: &str, fields: Value| {
        let (Some(sf), Some(st)) = (span_by_node.get(&from), span_by_node.get(&to)) else {
            return; // an endpoint truncated with its node — never a ghost arrow
        };
        edges.push(json!({
            "source": format!("c4:{from}"),
            "target": format!("c4:{to}"),
            "edgeKind": kind,
            "displayMode": display,
            "evidence": span_ev(sf),
            "fields": fields,
        }));
    };
    {
        let mut stmt = conn
            .prepare("SELECT parent_id, child_id, reason, is_primary FROM concept_edges")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (parent, child, reason, is_primary) in rows {
            // containment from manifest roots is direct structural evidence
            // (Fact); cohesion/subsumption groupings are inferred
            let display = match reason.as_str() {
                "PackageContainment" | "CallOwnership" => "Fact",
                _ => "Inference",
            };
            push_edge(parent, child, &reason, display, json!({ "isPrimary": is_primary == 1 }));
        }
    }
    // §20.3 relationships — uses (resolved calls) / calls (bindings)
    {
        let mut stmt = conn
            .prepare("SELECT from_node, to_node, kind, resolution, confidence FROM c4_relationships")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, f64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (from, to, kind, resolution, confidence) in rows {
            let display = if confidence >= 1.0 { "Fact" } else { "Inference" };
            push_edge(
                from,
                to,
                &kind,
                display,
                json!({ "resolution": resolution, "confidence": confidence }),
            );
        }
    }

    let spec = json!({
        "elements": elements,
        "edges": edges,
        "footer": {
            "epistemic": "Fact",
            "note": "containers from manifest roots (§20.1); components from framework annotations, naming suffixes and DDD segments (§20.2); relationships per §20.3 — every edge reason in the §22.3 closed set",
        },
        "gaps": gaps,
        "scope": { "artifact": "c4" },
        "layoutHint": { "strategy": "dagre" },
    });
    validate(&spec)?;
    specs.insert("c4".to_string(), spec);
    Ok(())
}

// ---------------------------------------------------------------------------
// Byte-span evidence helpers — the TS EvidenceRef contract
// ---------------------------------------------------------------------------

/// A resolvable source location: (file path, start byte, end byte).
pub(crate) type Span = (String, i64, i64);

/// The file + byte span a symbol occupies (symbols.start/end_byte are NOT
/// NULL, so a symbol id always resolves while its file row exists).
pub(crate) fn symbol_span(conn: &Connection, symbol_id: i64) -> Option<Span> {
    conn.query_row(
        "SELECT f.path, s.start_byte, s.end_byte
         FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.id = ?1",
        rusqlite::params![symbol_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .ok()
}

/// The file + byte span of an AST node (fallback anchor when a detection
/// carries `evidence_node_id` instead of a symbol).
pub(crate) fn ast_node_span(conn: &Connection, node_id: i64) -> Option<Span> {
    conn.query_row(
        "SELECT f.path, a.start_byte, a.end_byte
         FROM ast_nodes a JOIN files f ON f.id = a.file_id
         WHERE a.id = ?1",
        rusqlite::params![node_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .ok()
}

/// Build the wire-shape EvidenceRef from a span — exactly `{file, start,
/// end}`, nothing else (viz-adapter/src/types.ts).
pub(crate) fn span_ev(span: &Span) -> Value {
    json!({ "file": span.0, "start": span.1, "end": span.2 })
}

// ---------------------------------------------------------------------------
// The gate mirror — FULL TS clause set (viz-adapter/src/gate.ts + cie-viz)
// ---------------------------------------------------------------------------

/// PRD VIZ-I-06: salience-bounded cap of 2 000 elements per view.
pub const ELEMENT_CAP: usize = 2_000;

/// RFC-0024 `isWellFormedRef` mirror: non-empty file, finite non-negative
/// start, end >= start.
fn well_formed_ref(r: Option<&Value>) -> bool {
    let Some(obj) = r.and_then(|r| r.as_object()) else { return false };
    let Some(file) = obj.get("file").and_then(|f| f.as_str()) else { return false };
    if file.is_empty() {
        return false;
    }
    let (Some(start), Some(end)) = (
        obj.get("start").and_then(|s| s.as_f64()),
        obj.get("end").and_then(|e| e.as_f64()),
    ) else {
        return false;
    };
    start.is_finite() && end.is_finite() && start >= 0.0 && end >= start
}

/// The producer-side mirror of the TS gate (§26): refuses to emit a spec
/// that the gate would reject — same clauses, same stable codes. Lists ALL
/// violations (not just the first) so a producer fixes a spec in one pass.
pub fn validate(spec: &Value) -> Result<(), String> {
    let mut v: Vec<String> = Vec::new();
    let els = spec
        .get("elements")
        .and_then(|e| e.as_array())
        .ok_or("spec has no elements")?;

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for el in els {
        let id = el.get("id").and_then(|x| x.as_str()).unwrap_or("?").to_string();
        if !seen.insert(id.clone()) {
            v.push(format!("DUPLICATE_ELEMENT_ID: {id}"));
        }
        if !well_formed_ref(el.get("evidence")) {
            v.push(format!("ELEMENT_WITHOUT_EVIDENCE: {id}"));
        }
        let display = el.get("displayMode").and_then(|x| x.as_str()).unwrap_or("Fact");
        if display != "Fact" && el.get("claimId").is_none() {
            v.push(format!("NON_FACT_WITHOUT_CLAIM: {id}"));
        }
        if display == "Hypothesis" && el.get("counterArgument").is_none() {
            v.push(format!("HYPOTHESIS_WITHOUT_COUNTER: {id}"));
        }
        if display == "Fog" && el.get("fogReason").is_none() {
            v.push(format!("FOG_WITHOUT_REASON: {id}"));
        }
        // RFC-0024 patch 3: the two additive checks over the open fields bag
        if let Some(fields) = el.get("fields") {
            if let Some(lines) = fields.get("bodyLines") {
                let ok = lines
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .all(|l| l.get("evidence").map(|r| well_formed_ref(Some(r))).unwrap_or(false))
                    })
                    .unwrap_or(false);
                if !ok {
                    v.push(format!("BODY_LINE_WITHOUT_EVIDENCE: {id}"));
                }
            }
            if let Some(also) = fields.get("alsoEvidence") {
                let ok = also
                    .as_array()
                    .map(|a| a.iter().all(|r| well_formed_ref(Some(r))))
                    .unwrap_or(false);
                if !ok {
                    v.push(format!("ALSO_EVIDENCE_MALFORMED: {id}"));
                }
            }
        }
    }
    if els.len() > ELEMENT_CAP {
        v.push(format!("ELEMENT_CAP_EXCEEDED: {} elements", els.len()));
    }
    if let Some(edges) = spec.get("edges").and_then(|e| e.as_array()) {
        for e in edges {
            if !well_formed_ref(e.get("evidence")) {
                v.push("EDGE_WITHOUT_EVIDENCE".to_string());
            }
        }
    }
    if v.is_empty() {
        Ok(())
    } else {
        Err(v.join("; "))
    }
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
        crate::c4::create_c4_schema(&conn).unwrap();
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
        // evidence parity with the TS contract: EVERY element and edge in
        // every spec cites a real {file, start, end} byte span
        for (name, spec) in &specs {
            for el in spec["elements"].as_array().unwrap() {
                let ev = &el["evidence"];
                assert!(ev["file"].as_str().map(|f| !f.is_empty()).unwrap_or(false), "{name}: {:?}", el["id"]);
                assert!(ev["start"].is_i64() && ev["end"].is_i64(), "{name}: {:?}", el["id"]);
            }
            for e in spec["edges"].as_array().unwrap() {
                let ev = &e["evidence"];
                assert!(ev["file"].as_str().map(|f| !f.is_empty()).unwrap_or(false), "{name} edge lacks file");
            }
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
    fn bundle_wraps_every_spec_and_counts_honestly() {
        let conn = setup();
        let ts = "export const m = createMachine({\n  states: { draft: { on: { PAY: 'paid' } }, paid: {} }\n});\n";
        let f = ingest_parsed_file(&conn, 1, 41, "m.ts", Lang::TypeScript, ts, &mut IngestStats::default()).unwrap();
        cache_source_bytes(&conn, f, ts).unwrap();
        sm_detect(&conn).unwrap();
        crate::duplicates::create_dup_schema(&conn).unwrap();

        let bundle = produce_bundle(&conn).unwrap();
        assert_eq!(bundle["kind"], "cie-polyglot-viewspec-bundle");
        let specs = bundle["specs"].as_object().unwrap();
        assert!(specs.keys().any(|k| k.starts_with("state-machine:")), "{:?}", specs.keys());
        assert_eq!(bundle["spec-count"], specs.len());
        // no duplicate rows in this fixture → no impact specs, cap not hit —
        // counted honestly either way
        assert_eq!(bundle["impact-centers-truncated"], false);
        assert!(!specs.keys().any(|k| k.starts_with("impact:")));
        for (name, spec) in specs {
            validate(spec).unwrap_or_else(|e| panic!("bundled spec {name} must pass the gate mirror: {e}"));
        }
    }

    #[test]
    fn c4_spec_cites_manifest_and_symbol_spans() {
        let conn = setup();
        let py = "\
from fastapi import FastAPI
app = FastAPI()

@app.get(\"/orders\")
def list_orders():
    return load_orders()
";
        let f = ingest_parsed_file(&conn, 1, 41, "svc.py", Lang::Python, py, &mut IngestStats::default()).unwrap();
        cache_source_bytes(&conn, f, py).unwrap();
        lower_all(&conn).unwrap();
        install_registry(&conn).unwrap();
        fw_detect(&conn).unwrap();
        // a root manifest → one container rooted at it (§20.1)
        conn.execute(
            "INSERT INTO files(id, repo_id, path, content_hash, language, revision, size_bytes, tree_sitter_grammar_version)
             VALUES (100, 1, 'pyproject.toml', x'00', 'toml', 41, 20, 'toml')",
            [],
        )
        .unwrap();
        crate::c4::create_c4_schema(&conn).unwrap();
        crate::c4::detect_c4(&conn, std::path::Path::new("."), 41).unwrap();

        let specs = produce(&conn).unwrap();
        let c4 = specs.get("c4").expect("c4 spec produced");
        validate(c4).unwrap_or_else(|e| panic!("c4 must pass the gate mirror: {e}"));
        let els = c4["elements"].as_array().unwrap();
        assert!(els.len() >= 3, "domain + container + component expected: {els:?}");
        // the container cites the manifest file itself as byte-span evidence
        let container = els
            .iter()
            .find(|e| e["fields"]["c4Kind"] == "Container")
            .expect("container element");
        assert_eq!(container["evidence"]["file"], "pyproject.toml");
        assert_eq!(container["displayMode"], "Fact");
        // the framework-annotation component cites the symbol's file span
        let component = els
            .iter()
            .find(|e| e["fields"]["c4Kind"] == "Service")
            .expect("framework component element");
        assert_eq!(component["evidence"]["file"], "svc.py");
        assert!(component["evidence"]["start"].is_i64());
        // containment edges use the closed reason set and carry span evidence
        for e in c4["edges"].as_array().unwrap() {
            assert!(e["evidence"]["file"].as_str().map(|f| !f.is_empty()).unwrap_or(false));
            assert!(e["edgeKind"].as_str().map(|k| !k.is_empty()).unwrap_or(false));
        }
    }

    #[test]
    fn validate_refuses_unevidenced_and_undisclosed() {
        let span = json!({ "file": "a.py", "start": 0, "end": 9 });
        // no evidence at all / empty-file / non-span evidence → same code
        let no_evidence = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x" } ], "edges": [] });
        assert!(validate(&no_evidence).unwrap_err().contains("ELEMENT_WITHOUT_EVIDENCE: x"));
        let empty_file = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x", "evidence": { "file": "", "start": 0, "end": 1 } } ], "edges": [] });
        assert!(validate(&empty_file).unwrap_err().contains("ELEMENT_WITHOUT_EVIDENCE: x"));
        let semantic_only = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x", "evidence": { "kind": "symbol" } } ], "edges": [] });
        assert!(validate(&semantic_only).unwrap_err().contains("ELEMENT_WITHOUT_EVIDENCE: x"));
        let no_claim = json!({ "elements": [ { "id": "x", "displayMode": "Inference", "label": "x", "evidence": span } ], "edges": [] });
        assert!(validate(&no_claim).unwrap_err().contains("NON_FACT_WITHOUT_CLAIM: x"));
        let no_counter = json!({ "elements": [ { "id": "x", "displayMode": "Hypothesis", "label": "x", "evidence": span, "claimId": "C-1" } ], "edges": [] });
        assert!(validate(&no_counter).unwrap_err().contains("HYPOTHESIS_WITHOUT_COUNTER: x"));
        let no_fog = json!({ "elements": [ { "id": "x", "displayMode": "Fog", "label": "x", "evidence": span, "claimId": "C-1" } ], "edges": [] });
        assert!(validate(&no_fog).unwrap_err().contains("FOG_WITHOUT_REASON: x"));
        // edges need span evidence too
        let edge_no_ev = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x", "evidence": span } ], "edges": [ { "source": "x", "target": "x", "evidence": {} } ] });
        assert!(validate(&edge_no_ev).unwrap_err().contains("EDGE_WITHOUT_EVIDENCE"));
        // RFC-0024 patch 3 mirrors
        let bad_body = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x", "evidence": span, "fields": { "bodyLines": [ { "text": "no ref" } ] } } ], "edges": [] });
        assert!(validate(&bad_body).unwrap_err().contains("BODY_LINE_WITHOUT_EVIDENCE: x"));
        let bad_also = json!({ "elements": [ { "id": "x", "displayMode": "Fact", "label": "x", "evidence": span, "fields": { "alsoEvidence": [ { "start": 5 } ] } } ], "edges": [] });
        assert!(validate(&bad_also).unwrap_err().contains("ALSO_EVIDENCE_MALFORMED: x"));
        // duplicate ids are refused (provenance-audit prerequisite)
        let dup_ids = json!({ "elements": [
            { "id": "x", "displayMode": "Fact", "label": "x", "evidence": span },
            { "id": "x", "displayMode": "Fact", "label": "x-again", "evidence": span }
        ], "edges": [] });
        assert!(validate(&dup_ids).unwrap_err().contains("DUPLICATE_ELEMENT_ID: x"));
        // over-cap spec is refused
        let capped = json!({ "elements": (0..ELEMENT_CAP + 1).map(|i| json!({ "id": format!("e{i}"), "displayMode": "Fact", "label": "x", "evidence": span })).collect::<Vec<_>>(), "edges": [] });
        assert!(validate(&capped).unwrap_err().contains("ELEMENT_CAP_EXCEEDED"));
        // fully attested: span evidence + disclosures + well-formed RFC-0024 bags
        let good = json!({ "elements": [ { "id": "x", "displayMode": "Fog", "label": "x", "evidence": span, "claimId": "C-1", "fogReason": "why", "fields": { "bodyLines": [ { "text": "holds the ledger", "evidence": span } ], "alsoEvidence": [ span ] } } ], "edges": [ { "source": "x", "target": "x", "evidence": span } ] });
        assert!(validate(&good).is_ok());
    }
}
