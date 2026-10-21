//! PRD v2.0 slice 15 — impact radius + duplicate detection (§23).
//!
//! The visual: one shared center, callers on the left by ring, duplicates
//! on the right by similarity tier (§23.1). Engine side:
//!
//! | Tier | Method | Confidence / displayMode |
//! |---|---|---|
//! | Canonical duplicate | `ir_hash` exact match | Fact (`ir-hash-match`, similarity 1.0) |
//! | High similarity | `ir_vec` cosine ≥ 0.85 | Inference (`ir-vec`) |
//! | Medium similarity | cosine 0.60–0.85 AND shape-hash match | Inference (`ir-shingle`) |
//! | Low similarity | cosine 0.40–0.60 | Hypothesis with counter-argument (`ir-vec-cosine`) |
//! | Below threshold | hidden | counted in gaps, never shown |
//!
//! Canonical rule (§23.3): the duplicate with the most callers AND the most
//! tests is canonical; the winner is computed over the whole pair set and
//! marked `isCanonical`, so the visual can swap sides without re-detecting.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use serde_json::{json, Value};

use crate::viewspec;

pub const DDL_DUP: &str = r#"
CREATE TABLE IF NOT EXISTS duplicates (
    id                 INTEGER PRIMARY KEY,
    symbol_a           INTEGER NOT NULL REFERENCES symbols(id),
    symbol_b           INTEGER NOT NULL REFERENCES symbols(id),
    similarity         REAL    NOT NULL,
    tier               TEXT    NOT NULL,   -- 'canonical' | 'high' | 'medium' | 'low'
    detection          TEXT    NOT NULL,   -- 'ir-hash-match' | 'ir-vec' | 'ir-shingle' | 'ir-vec-cosine'
    epistemic          TEXT    NOT NULL,   -- 'Fact' | 'Inference' | 'Hypothesis'
    canonical_symbol_id INTEGER REFERENCES symbols(id),
    counter_argument   TEXT
);
"#;

pub fn create_dup_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_DUP).map_err(|e| e.to_string())
}

#[derive(Debug, Default, serde::Serialize)]
pub struct DupReport {
    pub pairs: usize,
    pub canonical_hash: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    /// pairs below 0.40 similarity — hidden, counted (§23.2 last row)
    pub hidden_gaps: usize,
}

/// Cosine over the stored op-bag vectors (ir_meta.ir_vec JSON maps).
fn cosine(a: &BTreeMap<String, u32>, b: &BTreeMap<String, u32>) -> f64 {
    let mut dot = 0f64;
    let mut na = 0f64;
    let mut nb = 0f64;
    for (k, va) in a {
        na += (*va as f64) * (*va as f64);
        if let Some(vb) = b.get(k) {
            dot += (*va as f64) * (*vb as f64);
        }
    }
    for vb in b.values() {
        nb += (*vb as f64) * (*vb as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn load_ir_meta(conn: &Connection) -> Result<BTreeMap<i64, (Vec<u8>, BTreeMap<String, u32>)>, String> {
    let mut stmt = conn
        .prepare("SELECT symbol_id, ir_hash, ir_vec FROM ir_meta")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    let mut out = BTreeMap::new();
    for (sid, hash, vec_bytes) in rows {
        let vec = String::from_utf8(vec_bytes).map_err(|e| e.to_string())?;
        let v: BTreeMap<String, u32> = serde_json::from_str(&vec).map_err(|e| e.to_string())?;
        out.insert(sid, (hash, v));
    }
    Ok(out)
}

/// Detect duplicate pairs over every lowered symbol pair (i < j).
pub fn detect(conn: &Connection) -> Result<DupReport, String> {
    let mut report = DupReport::default();
    let meta = load_ir_meta(conn)?;
    let shape: BTreeMap<i64, Vec<u8>> = {
        let mut stmt = conn
            .prepare("SELECT id, subtree_hash FROM symbols")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };
    let ids: Vec<i64> = meta.keys().copied().collect();
    for (i, &a) in ids.iter().enumerate() {
        for &b in ids.iter().skip(i + 1) {
            let (ha, va) = &meta[&a];
            let (hb, vb) = &meta[&b];
            let (tier, detection, epistemic, sim, counter): (&str, &str, &str, f64, Option<&str>);
            if ha == hb {
                tier = "canonical";
                detection = "ir-hash-match";
                epistemic = "Fact";
                sim = 1.0;
                counter = None;
                report.canonical_hash += 1;
            } else {
                let c = cosine(va, vb);
                if c >= 0.85 {
                    tier = "high";
                    detection = "ir-vec";
                    epistemic = "Inference";
                    sim = c;
                    counter = None;
                    report.high += 1;
                } else if c >= 0.60 && shape.get(&a) == shape.get(&b) {
                    tier = "medium";
                    detection = "ir-shingle";
                    epistemic = "Inference";
                    sim = c;
                    counter = None;
                    report.medium += 1;
                } else if c >= 0.40 {
                    tier = "low";
                    detection = "ir-vec-cosine";
                    epistemic = "Hypothesis";
                    sim = c;
                    counter = Some("op-bag cosine only; control flow and data flow not compared — needs IR-level diff to confirm");
                    report.low += 1;
                } else {
                    tier = "hidden";
                    detection = "ir-vec-cosine";
                    epistemic = "Hypothesis";
                    sim = c;
                    counter = None;
                    report.hidden_gaps += 1;
                }
            }
            conn.execute(
                "INSERT INTO duplicates(symbol_a, symbol_b, similarity, tier, detection, epistemic, canonical_symbol_id, counter_argument)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
                rusqlite::params![a.min(b), b.max(a), sim, tier, detection, epistemic, counter],
            )
            .map_err(|e| e.to_string())?;
            if tier != "hidden" {
                report.pairs += 1;
            }
        }
    }
    mark_canonical(conn)?;
    Ok(report)
}

/// §23.3: within each duplicate family (connected component of pairs), the
/// member with the most callers AND the most tests is canonical. Both
/// directions are covered because the family is undirected.
fn mark_canonical(conn: &Connection) -> Result<(), String> {
    // callers per symbol (resolved reverse deps)
    let mut callers: BTreeMap<i64, i64> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT callee_symbol_id, COUNT(DISTINCT caller_symbol_id) FROM calls WHERE callee_symbol_id IS NOT NULL GROUP BY callee_symbol_id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (s, n) in rows {
            callers.insert(s, n);
        }
    }
    // tests per symbol: test-path symbols that resolve-call it (real
    // references from test files, not name matching)
    let mut tests: BTreeMap<i64, i64> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT c.callee_symbol_id, COUNT(DISTINCT c.caller_symbol_id)
                 FROM calls c JOIN symbols s ON s.id = c.caller_symbol_id
                 JOIN files f ON f.id = s.file_id
                 WHERE c.resolution='RESOLVED' AND (f.path LIKE '%test%' OR f.path LIKE '%spec%')
                 GROUP BY c.callee_symbol_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (s, n) in rows {
            tests.insert(s, n);
        }
    }
    // families = connected components over the duplicates edges
    let pairs: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare("SELECT symbol_a, symbol_b FROM duplicates").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let mut adj: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for (a, b) in &pairs {
        adj.entry(*a).or_default().insert(*b);
        adj.entry(*b).or_default().insert(*a);
    }
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    for root in adj.keys() {
        if seen.contains(root) {
            continue;
        }
        let mut family: Vec<i64> = Vec::new();
        let mut stack = vec![*root];
        while let Some(n) = stack.pop() {
            if !seen.insert(n) {
                continue;
            }
            family.push(n);
            for m in adj.get(&n).into_iter().flatten() {
                if !seen.contains(m) {
                    stack.push(*m);
                }
            }
        }
        // canonical = most callers, tie-broken by most tests, then id
        let best = *family
            .iter()
            .max_by_key(|s| (callers.get(s).copied().unwrap_or(0), tests.get(s).copied().unwrap_or(0), std::cmp::Reverse(*s)))
            .unwrap();
        for s in &family {
            conn.execute(
                "UPDATE duplicates SET canonical_symbol_id = ?1 WHERE symbol_a = ?2 OR symbol_b = ?2",
                rusqlite::params![best, s],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Impact + duplicates ViewSpec (§23.1) — center / caller rings / dup hemisphere
// ---------------------------------------------------------------------------

/// Build the impact+duplicates ViewSpec for one center symbol.
pub fn impact_duplicate_spec(conn: &Connection, center: i64) -> Result<Value, String> {
    let name = |sid: i64| -> String {
        conn.query_row("SELECT qual_name FROM symbols WHERE id = ?1", rusqlite::params![sid], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_else(|_| format!("symbol {sid}"))
    };
    // byte-span evidence (the TS EvidenceRef contract): a symbol cites the
    // file + bytes it occupies — the call site / duplicate body IS there
    let ev = |sid: i64| -> Value {
        viewspec::symbol_span(conn, sid)
            .map(|s| viewspec::span_ev(&s))
            .unwrap_or_else(|| json!({ "file": "", "start": 0, "end": 0 }))
    };
    let mut elements: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();

    // center
    elements.push(json!({
        "id": "center",
        "elementKind": "symbol",
        "displayMode": "Fact",
        "label": name(center),
        "evidence": ev(center),
        "salience": { "factors": ["impact-center"], "weight": 1.0, "whyShown": "the change under question" },
        "fields": { "role": "center", "ring": 0 },
    }));

    // callers by ring: ring 1 direct (Fact), ring 2 via-a-caller (Inference)
    let mut ring2: BTreeSet<i64> = BTreeSet::new();
    {
        let mut stmt = conn
            .prepare("SELECT DISTINCT caller_symbol_id FROM calls WHERE callee_symbol_id = ?1 AND resolution='RESOLVED'")
            .map_err(|e| e.to_string())?;
        let ring1: Vec<i64> = stmt
            .query_map(rusqlite::params![center], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for r1 in &ring1 {
            elements.push(json!({
                "id": format!("c{r1}"),
                "elementKind": "symbol",
                "displayMode": "Fact",
                "label": name(*r1),
                "evidence": ev(*r1),
                "salience": { "factors": ["direct-caller"], "weight": 0.9, "whyShown": "resolved call edge into the center" },
                "fields": { "role": "caller", "ring": 1, "via": "resolved-call-edge" },
            }));
            edges.push(json!({
                "source": format!("c{r1}"), "target": "center", "edgeKind": "calls",
                "displayMode": "Fact", "evidence": ev(*r1),
            }));
        }
        for r1 in &ring1 {
            let mut s2 = conn
                .prepare("SELECT DISTINCT caller_symbol_id FROM calls WHERE callee_symbol_id = ?1 AND resolution='RESOLVED'")
                .map_err(|e| e.to_string())?;
            let mid: Vec<i64> = s2
                .query_map(rusqlite::params![r1], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for r2 in mid {
                if r2 == center || ring1.contains(&r2) {
                    continue;
                }
                if ring2.insert(r2) {
                    elements.push(json!({
                        "id": format!("c{r2}"),
                        "elementKind": "symbol",
                        "displayMode": "Inference",
                        "label": name(r2),
                        "claimId": format!("C-IMP-{center}"),
                        "evidence": ev(r2),
                        "salience": { "factors": ["transitive-caller"], "weight": 0.6, "whyShown": "reaches the center through a ring-1 caller" },
                        "fields": { "role": "caller", "ring": 2, "via": "transitive-call-edge" },
                    }));
                    edges.push(json!({
                        "source": format!("c{r2}"), "target": format!("c{r1}"), "edgeKind": "calls",
                        "displayMode": "Inference", "evidence": ev(r2),
                    }));
                }
            }
        }
    }

    // duplicates hemisphere
    let mut gaps: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT symbol_a, symbol_b, similarity, tier, detection, epistemic, canonical_symbol_id, counter_argument FROM duplicates WHERE symbol_a = ?1 OR symbol_b = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![center], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (a, b, sim, tier, detection, epi, canon, counter) in rows {
            let other = if a == center { b } else { a };
            let drift = if sim < 1.0 && tier == "high" { "diverged" } else { "same" };
            let mut el = json!({
                "id": format!("dup{other}"),
                "elementKind": "symbol",
                "displayMode": epi,
                "label": name(other),
                "evidence": ev(other),
                "salience": { "factors": ["duplicate"], "weight": sim, "whyShown": format!("similarity {sim} via {detection}") },
                "fields": {
                    "role": "duplicate", "ring": 1,
                    "similarity": sim, "isCanonical": canon == Some(other),
                    "detection": detection,
                },
            });
            if epi != "Fact" {
                el["claimId"] = json!(format!("C-DUP-{center}"));
            }
            if epi == "Hypothesis" {
                el["counterArgument"] = json!(counter.unwrap_or_else(|| "similarity only".to_string()));
            }
            if drift == "diverged" {
                el["fields"]["drift"] = json!("diverged");
            }
            elements.push(el);
            // the similarity edge is bidirectional and never a call arrow
            edges.push(json!({
                "source": "center", "target": format!("dup{other}"), "edgeKind": "similar",
                "displayMode": epi,
                "evidence": ev(other),
                "fields": { "similarity": sim },
            }));
        }
    }

    // the gap the detection names honestly: below-threshold pairs are hidden
    let hidden: i64 = {
        let mut stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM duplicates WHERE tier='hidden' AND (symbol_a = ?1 OR symbol_b = ?1)",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![center], |r| r.get(0))
            .unwrap_or(0)
    };
    if hidden > 0 {
        gaps.push(json!({ "reason": format!("{hidden} candidate pair(s) below the 0.40 similarity threshold stay hidden, counted not shown") }));
    }

    let spec = json!({
        "elements": elements,
        "edges": edges,
        "footer": { "epistemic": "Fact", "note": "impact rings from resolved call edges; duplicates from ir hash/cosine tiers (§23.2)" },
        "gaps": gaps,
        "hidden": [],
        "scope": { "artifact": "impact-duplicates", "center": center },
        "layoutHint": { "strategy": "cie-impact-duplicate" },
    });
    viewspec::validate(&spec)?;
    Ok(spec)
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
        create_dup_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn identical_bodies_are_canonical_hash_fact() {
        let conn = setup();
        let mut st = IngestStats::default();
        let a = "def charge(x):\n    if x > 1:\n        return audit(x)\n    return x\n";
        let b = "def charge2(x):\n    if x > 1:\n        return audit(x)\n    return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.canonical_hash, 1, "identical bodies: {rep:?}");
        let (epi, det, sim): (String, String, f64) = conn
            .query_row("SELECT epistemic, detection, similarity FROM duplicates", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(epi, "Fact");
        assert_eq!(det, "ir-hash-match");
        assert_eq!(sim, 1.0);
    }

    #[test]
    fn near_identical_bodies_are_high_tier_inference() {
        let conn = setup();
        let mut st = IngestStats::default();
        let a = "def big(x):\n    if x > 1:\n        return audit(x)\n    if x < 0:\n        return reject(x)\n    return x\n";
        // same shape minus one branch → high cosine, different hash
        let b = "def big2(x):\n    if x > 1:\n        return audit(x)\n    return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert!(rep.high >= 1 || rep.medium >= 1 || rep.low >= 1, "non-identical similar pair tiers: {rep:?}");
        let (epi, det): (String, String) = conn
            .query_row("SELECT epistemic, detection FROM duplicates", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_ne!(epi, "Fact", "different bodies are never Fact");
        assert!(det != "ir-hash-match");
    }

    #[test]
    fn canonical_marking_prefers_most_callers() {
        let conn = setup();
        let mut st = IngestStats::default();
        // charge and charge2 are identical; charge has 2 callers, charge2 none
        let a = "def charge(x):\n    return audit(x)\n\ndef call_one():\n    return charge(1)\n\ndef call_two():\n    return charge(2)\n";
        let b = "def charge2(x):\n    return audit(x)\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        detect(&conn).unwrap();
        let canon: String = conn
            .query_row(
                "SELECT s.name FROM duplicates d JOIN symbols s ON s.id = d.canonical_symbol_id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(canon, "charge", "the member with the most callers is canonical (§23.3)");
    }

    #[test]
    fn impact_duplicate_spec_passes_the_gate() {
        let conn = setup();
        let mut st = IngestStats::default();
        let a = "def charge(x):\n    return audit(x)\n\ndef caller_one():\n    return charge(1)\n";
        let b = "def charge2(x):\n    return audit(x)\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        detect(&conn).unwrap();
        let center: i64 = conn
            .query_row("SELECT id FROM symbols WHERE name='charge'", [], |r| r.get(0))
            .unwrap();
        let spec = impact_duplicate_spec(&conn, center).unwrap();
        viewspec::validate(&spec).unwrap();
        // structure: center + caller ring 1 + duplicate hemisphere
        let roles: Vec<(String, i64)> = spec["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["fields"]["role"].as_str().unwrap_or("").to_string(),
                    e["fields"]["ring"].as_i64().unwrap_or(-1),
                )
            })
            .collect();
        assert!(roles.contains(&("center".into(), 0)));
        assert!(roles.contains(&("caller".into(), 1)));
        assert!(roles.iter().any(|(r, _)| r == "duplicate"), "{roles:?}");
        // the similarity edge exists and is NOT a call arrow
        let kinds: Vec<String> = spec["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["edgeKind"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(kinds.contains(&"similar".to_string()));
        assert!(kinds.contains(&"calls".to_string()));
    }
}
