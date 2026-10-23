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
//! Two exclusions precede every tier, both removing *containment geometry*,
//! not duplicated behavior:
//!
//! 1. the kind rule: a pair where one side is a container kind (`class`,
//!    `trait`) and the other a member kind (`function`, `method`) is never a
//!    duplicate. The member is *defined inside* the container, so its op-bag
//!    is a subset of the container's by construction — the similarity is
//!    containment geometry (a class whose only content is one function even
//!    shares that function's exact `ir_hash`, which used to surface as a
//!    canonical Fact pair). Same-kind pairs keep comparing: two functions,
//!    function × method, two classes, class × trait — including across
//!    files, where an identical body in the same kind is a genuine duplicate.
//! 2. the span rule: a pair whose byte spans nest — one symbol's span
//!    strictly contains the other's, in the same file — is never a duplicate
//!    either, whatever the kinds. The inner text is part of the outer text,
//!    so the outer's op-bag contains the inner's by construction: a nested
//!    function inside its encloser, a nested class inside its outer class.
//!    Identical spans are not containment (neither side owns the other's
//!    text), and different files never nest (an out-of-line method body is
//!    genuinely different text from its class's header).
//!
//! Excluded pairs are counted (`skipped_kind_mismatch`, `skipped_nested`),
//! never shown and never counted as hidden — they are not duplicates at any
//! tier. The two buckets are disjoint: a pair the kind rule already removes
//! (a class × its own method) is counted there only, never twice.
//!
//! Canonical rule (§23.3): the duplicate with the most callers AND the most
//! tests is canonical; the winner is computed over the whole pair set and
//! marked `isCanonical`, so the visual can swap sides without re-detecting.

use std::collections::{BTreeMap, BTreeSet, HashMap};

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
CREATE TABLE IF NOT EXISTS duplicate_gaps (
    symbol_id    INTEGER PRIMARY KEY REFERENCES symbols(id),
    hidden_count INTEGER NOT NULL
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
    /// container × member pairs excluded before any tier: a class/trait and
    /// a function/method defined inside it are containment, not duplication
    pub skipped_kind_mismatch: usize,
    /// same-file span-containment pairs excluded before any tier: when one
    /// symbol's byte span strictly contains another's (in the same file),
    /// the outer symbol's op-bag contains the inner's by construction —
    /// containment geometry, not duplicated behavior. Pairs already caught
    /// by the kind rule are counted there only, never twice.
    pub skipped_nested: usize,
}

// ---------------------------------------------------------------------------
// Detection engine — threshold all-pairs cosine over a dense op-bag space
// ---------------------------------------------------------------------------

/// The low tier floor (§23.2): pairs below this similarity are hidden pairs.
/// They are counted per symbol in `duplicate_gaps`, never materialized —
/// the brute-force reference wrote one row per hidden pair (O(n²) rows for
/// output that is only ever read as per-symbol counts).
const LOW_FLOOR: f64 = 0.40;

/// Slack for the scan-abort bound: the remainder mass is a floating-point
/// upper estimate, so the abort is padded by far more than any summation
/// error (~1 ulp) — an abort can only ever be a few dims LATER than the
/// exact bound, never earlier. It can never cause a missed pair.
const ABORT_SLACK: f64 = 1e-9;

/// One symbol's op-bag in the dense shared vocabulary. Dims are the globally
/// sorted op names, so ascending dim id == ascending string key — the exact
/// iteration order the brute-force `BTreeMap` cosine used. Combined with
/// norms being `sqrt` of the same sums (correctly rounded ⇒ precomputing is
/// bit-identical to recomputing per pair), every similarity this module
/// reports is bit-identical to a full brute-force pass.
struct Bag {
    dims: Vec<u32>,
    vals: Vec<u32>,
    norm: f64,
}

impl Bag {
    fn build(bag: &BTreeMap<String, u32>, dim_of: &HashMap<&str, u32>) -> Bag {
        // BTreeMap iteration is ascending by string key; dim ids were assigned
        // in sorted-name order, so the vec comes out sorted by dim already.
        let pairs: Vec<(u32, u32)> = bag
            .iter()
            .map(|(k, v)| (dim_of[k.as_str()], *v))
            .collect();
        let mut sum_sq = 0f64;
        let mut dims = Vec::with_capacity(pairs.len());
        let mut vals = Vec::with_capacity(pairs.len());
        for (d, v) in pairs {
            let x = v as f64;
            sum_sq += x * x;
            dims.push(d);
            vals.push(v);
        }
        Bag { dims, vals, norm: sum_sq.sqrt() }
    }

    /// Exact cosine, op-for-op the brute-force computation. Both dim arrays
    /// are ascending, so a two-pointer merge visits the shared dims in the
    /// same ascending (== string) order the old per-dim `binary_search` did:
    /// the hit sequence, every term `(a_d as f64) * (b_d as f64)` and the
    /// accumulation order are identical, so the dot is bit-identical to the
    /// brute-force `BTreeMap` cosine — but O(dims_a + dims_b) sequential
    /// instead of O(dims_a · log dims_b) with a random probe per dim.
    fn cosine(&self, other: &Bag) -> f64 {
        if self.norm == 0.0 || other.norm == 0.0 {
            return 0.0;
        }
        let mut dot = 0f64;
        let (mut i, mut j) = (0usize, 0usize);
        while i < self.dims.len() && j < other.dims.len() {
            match self.dims[i].cmp(&other.dims[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    dot += (self.vals[i] as f64) * (other.vals[j] as f64);
                    i += 1;
                    j += 1;
                }
            }
        }
        dot / (self.norm * other.norm)
    }
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

/// The §23.2 tier ladder, as one fixed table: a row's `(tier, detection,
/// epistemic, counter_argument)` is fully determined by its outcome, so the
/// bulk INSERT carries a u8 outcome per row and expands it to the four text
/// columns with `CASE` literals inside the prepared statement — the texts
/// live in the statement, not in per-row binds (rusqlite binds every text
/// with SQLITE_TRANSIENT: a malloc + memcpy per column per row, ~4 copies ×
/// 12.6M rows of pure overhead for strings that never change).
const COMBOS: [(&str, &str, &str, Option<&str>); 4] = [
    ("canonical", "ir-hash-match", "Fact", None),
    ("high", "ir-vec", "Inference", None),
    ("medium", "ir-shingle", "Inference", None),
    (
        "low",
        "ir-vec-cosine",
        "Hypothesis",
        Some("op-bag cosine only; control flow and data flow not compared — needs IR-level diff to confirm"),
    ),
];
const C_CANONICAL: u8 = 0;
const C_HIGH: u8 = 1;
const C_MEDIUM: u8 = 2;
const C_LOW: u8 = 3;

// ---- symbol kinds and the container × member exclusion (§23 preamble) ----

const K_OTHER: u8 = 0; // future kinds pair freely
const K_CLASS: u8 = 1; // containers: their members are defined inside them
const K_TRAIT: u8 = 2;
const K_FUNCTION: u8 = 3; // members: functions and methods
const K_METHOD: u8 = 4;

fn kind_code(kind: &str) -> u8 {
    match kind {
        "class" => K_CLASS,
        "trait" => K_TRAIT,
        "function" => K_FUNCTION,
        "method" => K_METHOD,
        _ => K_OTHER,
    }
}

#[inline]
fn is_container(k: u8) -> bool {
    k == K_CLASS || k == K_TRAIT
}

#[inline]
fn is_member(k: u8) -> bool {
    k == K_FUNCTION || k == K_METHOD
}

/// True when this pair must never be compared: one side is a class/trait,
/// the other a function/method — the member lives inside the container, so
/// the op-bag similarity is containment, not duplication.
#[inline]
fn kind_excluded(a: u8, b: u8) -> bool {
    (is_container(a) && is_member(b)) || (is_member(a) && is_container(b))
}

/// Byte spans for the ir_meta id set, aligned with the `ids` order:
/// `(file_id, start_byte, end_byte)`. A symbol whose row is missing
/// (impossible for ir_meta ids — start/end are NOT NULL — but defensive)
/// gets the `(0, 0, 0)` sentinel: file ids start at 1, so it can never nest
/// against a real span, and two sentinels are identical spans, which are
/// not strict containment either.
fn load_spans(conn: &Connection, ids: &[i64]) -> Result<Vec<(i64, i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT id, file_id, start_byte, end_byte FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ),
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<BTreeMap<i64, (i64, i64, i64)>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    Ok(ids
        .iter()
        .map(|s| rows.get(s).copied().unwrap_or((0, 0, 0)))
        .collect())
}

/// True when one span strictly contains the other in the same file: the
/// inner symbol's text is part of the outer's, so the outer's op-bag is a
/// superset of the inner's by construction. Identical spans are NOT
/// containment (neither side owns the other's text).
#[inline]
fn span_nested(a: (i64, i64, i64), b: (i64, i64, i64)) -> bool {
    if a.0 != b.0 {
        return false;
    }
    let (sa, ea) = (a.1, a.2);
    let (sb, eb) = (b.1, b.2);
    (sa <= sb && eb <= ea && (sa < sb || eb < ea))
        || (sb <= sa && ea <= eb && (sb < sa || ea < eb))
}

/// Symbol kinds for the ir_meta id set, aligned with the `ids` order.
fn load_kinds(conn: &Connection, ids: &[i64]) -> Result<Vec<u8>, String> {
    let mut stmt = conn
        .prepare("SELECT id, kind FROM symbols")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<BTreeMap<i64, String>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    let kinds = rows;
    Ok(ids
        .iter()
        .map(|s| kinds.get(s).map(|k| kind_code(k)).unwrap_or(K_OTHER))
        .collect())
}

/// Rows per multi-row INSERT. 512 × 4 params = 2048 bound parameters, well
/// under SQLite's 32766 default limit; the VDBE amortizes one step/reset
/// cycle over 512 rows instead of 1. The VALUES text is built once.
const CHUNK_ROWS: usize = 512;

/// The column order of every INSERT into `duplicates`.
const DUP_COLS: &str = "symbol_a, symbol_b, similarity, tier, detection, epistemic, canonical_symbol_id, counter_argument";

/// One VALUES tuple per row: four numbered bound params (`a, b, similarity,
/// outcome` — outcome is the u8 combo code) plus four `CASE` literals that
/// expand that row's outcome into the tier texts; the canonical winner is a
/// constant for the whole run, so it is embedded as a literal (or NULL when
/// absent). Every stored byte is identical to binding the texts directly —
/// same storage classes, same values, same NULLs.
fn dup_values_sql(rows: usize, winner: Option<i64>) -> String {
    // the CASE bodies come straight from the COMBOS ladder — one source of
    // truth for what each outcome code means
    let mut tier_body = String::new();
    let mut det_body = String::new();
    let mut epi_body = String::new();
    for (i, (tier, detection, epistemic, _)) in COMBOS.iter().enumerate() {
        tier_body.push_str(&format!(" WHEN {i} THEN '{tier}'"));
        det_body.push_str(&format!(" WHEN {i} THEN '{detection}'"));
        epi_body.push_str(&format!(" WHEN {i} THEN '{epistemic}'"));
    }
    let low = COMBOS[3];
    let counter_body = match low.3 {
        Some(c) => format!(" WHEN 3 THEN '{c}' ELSE NULL END"),
        None => String::new(),
    };
    let winner_sql = match winner {
        Some(w) => w.to_string(),
        None => "NULL".to_string(),
    };
    let mut sql = String::with_capacity(rows * 520 + 320);
    sql.push_str("INSERT INTO duplicates(");
    sql.push_str(DUP_COLS);
    sql.push_str(") VALUES ");
    for i in 0..rows {
        if i > 0 {
            sql.push(',');
        }
        // params ?{4i+1}..?{4i+4}: a, b, similarity, outcome
        let a = 4 * i + 1;
        let b = 4 * i + 2;
        let sim = 4 * i + 3;
        let c = 4 * i + 4;
        sql.push_str(&format!(
            "(?{a}, ?{b}, ?{sim}, CASE ?{c}{tier_body} ELSE '{}' END, CASE ?{c}{det_body} ELSE '{}' END, CASE ?{c}{epi_body} ELSE '{}' END, {winner_sql}, CASE ?{c}{counter_body})",
            low.0, low.1, low.2
        ));
    }
    sql
}

/// Restores the connection's `PRAGMA foreign_keys` to its original value on
/// every drop path (success, `?` error, unwind). The duplicates output is
/// the only place this guard is used: the referenced ids were all read from
/// `symbols`/`ir_meta` inside this same call, and upstream FKs already
/// validated them when they were written, so the per-row parent probes are
/// pure redundancy during the bulk insert.
struct FkGuard<'a> {
    conn: &'a Connection,
    was_on: bool,
    armed: bool,
}

impl<'a> FkGuard<'a> {
    fn off(conn: &'a Connection) -> Result<FkGuard<'a>, String> {
        let was_on: bool = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .map(|v| v != 0)
            .map_err(|e| e.to_string())?;
        if was_on {
            // legal only outside a transaction — detect() toggles before
            // its own txn opens and restores after it commits
            conn.pragma_update(None, "foreign_keys", false)
                .map_err(|e| e.to_string())?;
        }
        Ok(FkGuard { conn, was_on, armed: true })
    }
}

impl Drop for FkGuard<'_> {
    fn drop(&mut self) {
        if self.armed && self.was_on {
            let _ = self.conn.pragma_update(None, "foreign_keys", true);
        }
        self.armed = false;
    }
}

/// Detect duplicate pairs. Same rows, same tiers, same canonical winner and
/// the same per-symbol hidden counts as the brute-force reference
/// (`detect_reference` in tests) — the speed comes from three structural
/// changes, none of them parallelism:
///
///   1. exact-hash grouping (O(n)): identical IRs are canonical duplicates
///      with no cosine at all;
///   2. an inverted index over the dense op-bag space with a Cauchy–Schwarz
///      remainder bound: for any pair with cosine ≥ LOW_FLOOR, the smaller
///      index's own scan provably reaches a shared dim before its suffix
///      mass drops below LOW_FLOOR² (any pair reachable only through the
///      tail is bounded under the floor), so sub-threshold pairs are never
///      enumerated;
///   3. one bulk transaction for the whole output instead of one auto-commit
///      INSERT per pair; the §23.3 canonical winner ("computed over the
///      whole pair set") is computed up front and written with each row,
///      replacing the old post-hoc full-table UPDATE per family member
///      (every pair got a row — hidden included — so the brute-force
///      family was always the complete graph and the family walk always
///      selected the same global argmax);
///   4. rows stream into 512-row multi-row INSERTs whose tier texts are
///      `CASE` literals inside the prepared statement — no 12.6M-row staging
///      Vec, one step/reset per 512 rows instead of per row, and no
///      SQLITE_TRANSIENT copy for the four text columns (rusqlite copies
///      every bound text; these texts never vary, so they live in the
///      statement and each row binds only a, b, similarity, outcome);
///   5. the per-row foreign-key parent probes are suspended for the bulk
///      insert (`FkGuard` restores the original setting on every path) —
///      three probes × 12.6M rows is ~8.6s of redundant work: every
///      referenced id was read from `symbols`/`ir_meta` in this same call
///      and those tables' own foreign keys already enforced it at write
///      time. Table contents are unaffected either way.
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
    let n = meta.len();
    if n < 2 {
        // no pairs, no gaps, no winner — the brute-force pass wrote no rows either
        return Ok(report);
    }

    // -- dense op-bag space: one shared vocabulary, norms computed once ------
    let mut names: Vec<&str> = meta
        .values()
        .flat_map(|(_, bag)| bag.keys().map(|k| k.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();
    let dim_of: HashMap<&str, u32> = names
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, i as u32))
        .collect();
    let ids: Vec<i64> = meta.keys().copied().collect();
    let bags: Vec<Bag> = ids
        .iter()
        .map(|s| Bag::build(&meta[s].1, &dim_of))
        .collect();

    // shape-hash group ids, aligned with bag positions: equal subtree_hash
    // bytes ⟺ equal id (missing ids — impossible for ir_meta symbols, but
    // the old `shape.get(&a) == shape.get(&b)` treated None == None as equal
    // — all share one group, preserving that exactly). Replaces a per-pair
    // BTreeMap lookup + byte compare with one array compare.
    let mut shape_gid: Vec<u32> = Vec::with_capacity(n);
    {
        let mut intern: HashMap<&[u8], u32> = HashMap::with_capacity(n);
        let mut next_id = 0u32;
        let mut missing = None::<u32>;
        for id in &ids {
            match shape.get(id) {
                Some(h) => {
                    let e = intern.entry(h.as_slice()).or_insert_with(|| {
                        next_id += 1;
                        next_id - 1
                    });
                    shape_gid.push(*e);
                }
                None => {
                    let e = missing.get_or_insert_with(|| {
                        next_id += 1;
                        next_id - 1
                    });
                    shape_gid.push(*e);
                }
            }
        }
    }

    let mut shown_deg = vec![0i64; n]; // partners at or above the floor

    // -- 0. container × member exclusion bookkeeping ---------------------------
    // A class/trait and a function/method are never duplicates (the member
    // is defined inside the container — the op-bag similarity is pure
    // containment). Excluded pairs leave BOTH the shown set and the hidden
    // residual; the per-symbol skipped count is exact arithmetic, not a
    // walk: every container pairs with every member exactly once.
    let kind_of = load_kinds(conn, &ids)?;
    let nc = kind_of.iter().filter(|k| is_container(**k)).count() as i64;
    let nm = kind_of.iter().filter(|k| is_member(**k)).count() as i64;
    let skipped_deg = |i: usize| -> i64 {
        let k = kind_of[i];
        if is_container(k) {
            nm
        } else if is_member(k) {
            nc
        } else {
            0
        }
    };

    // -- 0b. same-file span-containment exclusion bookkeeping ----------------
    // When one symbol's byte span strictly contains another's (same file),
    // the outer's op-bag contains the inner's by construction — the same
    // containment geometry the kind rule removes for container × member,
    // now for any kinds: a nested function inside its encloser, a nested
    // class inside its outer class. The per-symbol skip count is computed
    // exactly from the spans, not from the scan (staged pairs are visited
    // one-sided, so the scan cannot count symmetric degrees): per file,
    // sort by (start asc, end desc); only later-starting symbols can sit
    // inside a span, and the inner loop stops at the first start ≥ this
    // span's end. Pairs the kind rule already excluded are counted there
    // only — the buckets stay disjoint.
    let spans = load_spans(conn, &ids)?;
    let mut nested_only_deg = vec![0i64; n];
    {
        let mut by_file: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (i, sp) in spans.iter().enumerate() {
            by_file.entry(sp.0).or_default().push(i);
        }
        for positions in by_file.values() {
            let mut order: Vec<usize> = positions.clone();
            // start asc, end desc: a later position with an earlier-or-equal
            // start is either inside this span or not comparable to it
            order.sort_unstable_by(|&x, &y| {
                spans[x].1.cmp(&spans[y].1).then(spans[y].2.cmp(&spans[x].2))
            });
            for (ii, &x) in order.iter().enumerate() {
                let end_x = spans[x].2;
                for &y in order[ii + 1..].iter() {
                    if spans[y].1 >= end_x {
                        break; // starts ascend: nothing further can be inside x
                    }
                    if span_nested(spans[x], spans[y])
                        && !kind_excluded(kind_of[x], kind_of[y])
                    {
                        nested_only_deg[x] += 1;
                        nested_only_deg[y] += 1;
                    }
                }
            }
        }
    }
    let skipped_nested: i64 = nested_only_deg.iter().sum::<i64>() / 2;

    // -- 1. exact-hash grouping: canonical Fact rows, no cosine --------------
    // BTreeMap keyed by hash bytes: iteration order (and therefore the row
    // emission order) is identical to the previous pass. hash_gid gives the
    // walk an O(1) "same ir_hash" test instead of a 32-byte memcmp.
    let mut groups: BTreeMap<&[u8], Vec<u32>> = BTreeMap::new();
    for (i, s) in ids.iter().enumerate() {
        groups.entry(meta[s].0.as_slice()).or_default().push(i as u32);
    }
    let mut hash_gid: Vec<u32> = vec![0; n];
    for (gi, members) in groups.values().enumerate() {
        for &m in members {
            hash_gid[m as usize] = gi as u32;
        }
    }

    // -- 2. inverted index + weight-descending scan with remainder abort -----
    // postings[d] lists the vectors containing dim d (built in ascending
    // index order, so the candidate stream is deterministic).
    let mut postings: Vec<Vec<u32>> = vec![Vec::new(); names.len()];
    for (i, bag) in bags.iter().enumerate() {
        if bag.norm == 0.0 {
            continue; // empty bag: cosine 0 with everything, shares no dim
        }
        for &d in &bag.dims {
            postings[d as usize].push(i as u32);
        }
    }
    let mut candidates: Vec<(u32, u32)> = Vec::new();
    {
        // generation stamps replace the bool + clear-everything loop: a
        // partner touched in a's scan has stamp == gen, and moving to the
        // next `a` invalidates every stamp at once (no second pass over
        // touched_list just to unmark).
        let mut stamp = vec![u32::MAX; n];
        let mut gen: u32 = 0;
        let mut touched_list: Vec<u32> = Vec::new();
        for a in 0..n {
            let bag = &bags[a];
            if bag.norm == 0.0 {
                continue;
            }
            touched_list.clear();
            let this_gen = gen;
            gen += 1;
            // dims of `a` in descending weight order (ties by dim for determinism)
            let mut order: Vec<usize> = (0..bag.dims.len()).collect();
            order.sort_unstable_by(|&x, &y| bag.vals[y].cmp(&bag.vals[x]));
            // suffix squared mass in unit space, backward pass: suf[k] is an
            // upper bound on the contribution any unscanned tail can still make
            let d = bag.dims.len();
            let mut suf = vec![0f64; d + 1];
            for k in (0..d).rev() {
                let uw = bag.vals[k] as f64 / bag.norm;
                suf[k] = suf[k + 1] + uw * uw;
            }
            for &k in &order {
                let dim = bag.dims[k] as usize;
                for &bi in &postings[dim] {
                    let b = bi as usize;
                    if b == a
                        || stamp[b] == this_gen
                        || hash_gid[a] == hash_gid[b]
                        || kind_excluded(kind_of[a], kind_of[b])
                        || span_nested(spans[a], spans[b])
                    {
                        continue; // self / staged this round / canonical row / containment
                    }
                    stamp[b] = this_gen;
                    touched_list.push(bi);
                }
                if suf[k + 1] < LOW_FLOOR * LOW_FLOOR - ABORT_SLACK {
                    break;
                }
            }
            for &bi in &touched_list {
                if (a as u32) < bi {
                    // the bound is per-scan: a's own scan reaches every
                    // ≥-floor partner, so staging from the smaller index
                    // yields each unordered pair exactly once
                    candidates.push((a as u32, bi));
                }
            }
        }
    }

    // -- 3. §23.3 winner up front, then ONE bulk transaction ------------------
    // The winner is a pure read of calls/symbols/files (tables the inserts
    // never touch), so computing it before the write phase is order-neutral.
    let winner = canonical_winner(conn, &ids)?;
    // Bulk-load discipline: skip the per-row parent probes while the
    // 12.6M-row output is written. Every referenced id was read out of
    // `symbols`/`ir_meta` earlier in this very call, and those tables' own
    // foreign keys already validated them at write time — re-probing three
    // parents per row is ~8.6s of redundant B-tree searches. The guard
    // restores the connection's original setting on every path (success,
    // error, panic) and the pragma is only ever toggled outside a txn.
    let _fk_guard = FkGuard::off(conn)?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    {
        // 512-row multi-row INSERT with the tier texts as CASE literals:
        // one step/reset cycle per 512 rows and zero per-row text binds
        // (params are just a, b, similarity, outcome — the texts live in
        // the prepared statement). Rows stream straight from the loops —
        // no 12.6M-row staging Vec in between.
        let full_sql = dup_values_sql(CHUNK_ROWS, winner);
        let mut full = tx.prepare(&full_sql).map_err(|e| e.to_string())?;
        let mut pending: usize = 0;
        // the rows of the not-yet-full chunk, replayed into an exact-size
        // tail statement at the end
        let mut buf: Vec<(i64, i64, f64, u8)> = Vec::with_capacity(CHUNK_ROWS);

        // canonical rows first — the grouping order above, exactly the
        // order the previous engine staged them in
        for members in groups.values() {
            for i in 0..members.len() {
                for j in i + 1..members.len() {
                    let (x, y) = (members[i] as usize, members[j] as usize);
                    // containment of either kind is not a canonical
                    // duplicate: a class × its own function (kind rule), or
                    // a nested function/class × its encloser (span rule —
                    // single-member containers even share the exact hash)
                    if kind_excluded(kind_of[x], kind_of[y])
                        || span_nested(spans[x], spans[y])
                    {
                        continue;
                    }
                    let (av, bv) = (ids[x], ids[y]);
                    feed_row(
                        &mut full,
                        &mut pending,
                        &mut buf,
                        av.min(bv),
                        bv.max(av),
                        1.0,
                        C_CANONICAL,
                    )?;
                    shown_deg[x] += 1;
                    shown_deg[y] += 1;
                    report.canonical_hash += 1;
                }
            }
        }

        // -- 4. exact verification + the §23.2 tier ladder (reference-identical) --
        for (ai, bi) in candidates {
            let (x, y) = (ai as usize, bi as usize);
            let sim = bags[x].cosine(&bags[y]);
            if sim < LOW_FLOOR {
                continue; // sub-floor candidate: counted only in the gap residual
            }
            let combo: u8;
            if sim >= 0.85 {
                combo = C_HIGH;
                report.high += 1;
            } else if sim >= 0.60 && shape_gid[x] == shape_gid[y] {
                combo = C_MEDIUM;
                report.medium += 1;
            } else {
                combo = C_LOW;
                report.low += 1;
            }
            let (av, bv) = (ids[x], ids[y]);
            feed_row(
                &mut full,
                &mut pending,
                &mut buf,
                av.min(bv),
                bv.max(av),
                sim,
                combo,
            )?;
            shown_deg[x] += 1;
            shown_deg[y] += 1;
        }

        // tail chunk: an exact-size statement for the last pending rows —
        // the same rows, the same order, so ids stay sequential
        if pending > 0 {
            let tail_sql = dup_values_sql(pending, winner);
            let mut tail = tx.prepare(&tail_sql).map_err(|e| e.to_string())?;
            for (i, &(a, b, sim, combo)) in buf.iter().enumerate() {
                let base = 4 * i;
                tail.raw_bind_parameter(base + 1, a).map_err(|e| e.to_string())?;
                tail.raw_bind_parameter(base + 2, b).map_err(|e| e.to_string())?;
                tail.raw_bind_parameter(base + 3, sim).map_err(|e| e.to_string())?;
                tail.raw_bind_parameter(base + 4, combo as i64).map_err(|e| e.to_string())?;
            }
            tail.raw_execute().map_err(|e| e.to_string())?;
        }
    }
    {
        // hidden pairs are counted, not materialized: one row per symbol
        let mut gap = tx
            .prepare("INSERT OR REPLACE INTO duplicate_gaps(symbol_id, hidden_count) VALUES (?1, ?2)")
            .map_err(|e| e.to_string())?;
        for (i, &s) in ids.iter().enumerate() {
            let hidden =
                n as i64 - 1 - shown_deg[i] - skipped_deg(i) - nested_only_deg[i];
            gap.execute(rusqlite::params![s, hidden])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    drop(_fk_guard); // restore FK enforcement before the caller sees the conn

    // Built once, AFTER the bulk insert above (never in `create_dup_schema`,
    // which runs before any row exists) — a one-pass index build over the
    // finished table, not per-row maintenance during a multi-million-row
    // INSERT. symbol_a/symbol_b back the impact view's
    // "WHERE symbol_a = ? OR symbol_b = ?" lookup (SQLite OR-optimizes into
    // two indexed rowid scans + a merge — without them it's a full scan per
    // center, and `produce_bundle` queries up to 50 centers); tier backs
    // the bundle's per-tier center-selection query; detection/epistemic back
    // the web's duplicates filters (polyglot-db.ts) — indexes MUST exist at
    // build time so the web read path stays purely readOnly (DS-EVAL §6.3:
    // the old lazy `ensureIndexes()` wrote on the read path and its
    // read-write open blocked a concurrent writer under any journal mode).
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_dup_symbol_a ON duplicates(symbol_a);
         CREATE INDEX IF NOT EXISTS idx_dup_symbol_b ON duplicates(symbol_b);
         CREATE INDEX IF NOT EXISTS idx_dup_tier ON duplicates(tier);
         CREATE INDEX IF NOT EXISTS idx_dup_detection ON duplicates(detection);
         CREATE INDEX IF NOT EXISTS idx_dup_epistemic ON duplicates(epistemic);",
    )
    .map_err(|e| e.to_string())?;

    // every fed row landed in exactly one counted bucket; sub-floor
    // candidates were never fed; containment pairs were never candidates at
    // all (kind-excluded counted in skipped_kind_mismatch, span-nested in
    // skipped_nested — disjoint buckets — neither hidden)
    report.pairs = report.canonical_hash + report.high + report.medium + report.low;
    report.skipped_kind_mismatch = (nc * nm) as usize;
    report.skipped_nested = skipped_nested as usize;
    report.hidden_gaps = n * (n - 1) / 2
        - report.skipped_kind_mismatch
        - report.skipped_nested
        - report.pairs;
    Ok(report)
}

/// Bind one duplicates row into the current 512-row INSERT chunk; execute
/// the statement when the chunk fills. `buf` remembers the chunk's rows so
/// the final partial chunk can be replayed into an exact-size statement.
#[inline]
fn feed_row(
    ins: &mut rusqlite::Statement<'_>,
    pending: &mut usize,
    buf: &mut Vec<(i64, i64, f64, u8)>,
    a: i64,
    b: i64,
    sim: f64,
    combo: u8,
) -> Result<(), String> {
    let base = *pending * 4;
    ins.raw_bind_parameter(base + 1, a).map_err(|e| e.to_string())?;
    ins.raw_bind_parameter(base + 2, b).map_err(|e| e.to_string())?;
    ins.raw_bind_parameter(base + 3, sim).map_err(|e| e.to_string())?;
    ins.raw_bind_parameter(base + 4, combo as i64).map_err(|e| e.to_string())?;
    buf.push((a, b, sim, combo));
    *pending += 1;
    if *pending == CHUNK_ROWS {
        ins.raw_execute().map_err(|e| e.to_string())?;
        *pending = 0;
        buf.clear();
    }
    Ok(())
}

/// §23.3 canonical winner: the member with the most callers, tie-broken by
/// most tests, then by id — computed over the whole pair set (the brute-force
/// families were complete graphs, so the per-family walk always reduced to
/// this global argmax).
fn canonical_winner(conn: &Connection, ids: &[i64]) -> Result<Option<i64>, String> {
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
        for (s, cnt) in rows {
            callers.insert(s, cnt);
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
        for (s, cnt) in rows {
            tests.insert(s, cnt);
        }
    }
    Ok(ids
        .iter()
        .copied()
        .max_by_key(|s| {
            (
                callers.get(s).copied().unwrap_or(0),
                tests.get(s).copied().unwrap_or(0),
                std::cmp::Reverse(*s),
            )
        }))
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

    // the gap the detection names honestly: below-threshold partners are
    // hidden, counted (per symbol) not shown — never materialized as rows
    let hidden: i64 = {
        let mut stmt = conn
            .prepare("SELECT hidden_count FROM duplicate_gaps WHERE symbol_id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(rusqlite::params![center], |r| r.get(0))
            .unwrap_or(0)
    };
    if hidden > 0 {
        gaps.push(json!({ "reason": format!("{hidden} candidate pair(s) below the 0.40 similarity threshold stay hidden, counted not shown") }));
    }

    // VIZ-I-06: salience-bounded truncation, counted not silent — same
    // convention as the C4 producer. A hub symbol's duplicate hemisphere
    // can run into the thousands (dense near-duplicate codebases); rather
    // than let that blow the whole spec past the gate's ELEMENT_CAP, the
    // center is always kept, callers next, and duplicate partners yield
    // first — lowest similarity dropped first, counted in `gaps`.
    if elements.len() > viewspec::ELEMENT_CAP {
        let rank = |el: &Value| -> u8 {
            match el["fields"]["role"].as_str().unwrap_or("") {
                "center" => 0,
                "caller" => {
                    if el["fields"]["ring"].as_i64() == Some(1) {
                        1
                    } else {
                        2
                    }
                }
                _ => 3, // duplicate
            }
        };
        let weight = |el: &Value| -> f64 { el["salience"]["weight"].as_f64().unwrap_or(0.0) };
        let mut order: Vec<usize> = (0..elements.len()).collect();
        order.sort_by(|&a, &b| {
            rank(&elements[a])
                .cmp(&rank(&elements[b]))
                .then(weight(&elements[b]).total_cmp(&weight(&elements[a])))
        });
        let keep: std::collections::BTreeSet<usize> =
            order.into_iter().take(viewspec::ELEMENT_CAP).collect();
        let hidden_count = elements.len() - keep.len();
        let kept_ids: std::collections::BTreeSet<String> = elements
            .iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, el)| el["id"].as_str().unwrap_or("").to_string())
            .collect();
        elements = elements
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, el)| el)
            .collect();
        edges.retain(|e| {
            let s = e["source"].as_str().unwrap_or("");
            let t = e["target"].as_str().unwrap_or("");
            kept_ids.contains(s) && kept_ids.contains(t)
        });
        gaps.push(json!({
            "reason": format!(
                "{hidden_count} ring/duplicate node(s) above the {}-element display cap stay hidden by salience rank, counted not shown",
                viewspec::ELEMENT_CAP
            )
        }));
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
    fn class_and_its_function_are_not_duplicates() {
        let conn = setup();
        let mut st = IngestStats::default();
        // one class whose only content is one function — identical op-bags,
        // so the pre-rule engine paired them as a canonical Fact pair at
        // similarity 1.0. Containment is where the function is DEFINED,
        // not a duplicate.
        let a = "class Foo:\n    def bar(self):\n        if x > 1:\n            return audit(x)\n        return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.pairs, 0, "no pair between a class and its own function: {rep:?}");
        assert_eq!(rep.skipped_kind_mismatch, 1, "the class × function pair is counted as excluded: {rep:?}");
        assert_eq!(rep.hidden_gaps, 0, "containment is not a hidden duplicate either: {rep:?}");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM duplicates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "no duplicates row is materialized");
        // the per-symbol gap is zero too: the partner was excluded, not hidden
        let gaps: i64 = conn
            .query_row("SELECT COALESCE(SUM(hidden_count), 0) FROM duplicate_gaps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gaps, 0);
    }

    #[test]
    fn duplicated_classes_and_methods_survive_the_kind_rule() {
        let conn = setup();
        let mut st = IngestStats::default();
        // the same class-with-one-method duplicated across two files: every
        // class × function combination is excluded (containment), but the
        // REAL duplicates — the two classes and the two functions — remain
        // canonical Facts.
        let a = "class Foo:\n    def bar(self):\n        if x > 1:\n            return audit(x)\n        return x\n";
        let b = "class Foo2:\n    def bar2(self):\n        if x > 1:\n            return audit(x)\n        return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.skipped_kind_mismatch, 4, "every class × function combination: {rep:?}");
        assert_eq!(rep.canonical_hash, 2, "class × class and function × function survive: {rep:?}");
        assert_eq!(rep.hidden_gaps, 0);
        // both surviving rows are same-kind pairs
        let stmt = "SELECT sa.kind, sb.kind FROM duplicates d \
                    JOIN symbols sa ON sa.id = d.symbol_a \
                    JOIN symbols sb ON sb.id = d.symbol_b";
        let kinds: Vec<(String, String)> = conn
            .prepare(stmt)
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(kinds.len(), 2);
        for (ka, kb) in &kinds {
            assert_eq!(ka, kb, "only same-kind pairs survive the rule");
        }
    }

    #[test]
    fn nested_function_is_not_a_duplicate_of_its_encloser() {
        let conn = setup();
        let mut st = IngestStats::default();
        // one encloser whose only content is one nested function: same kind
        // on both sides, so the KIND rule never fires — the SPAN rule does
        // (the inner text is part of the outer's, so the outer's op-bag
        // contains the inner's by construction).
        let a = "def outer(x):\n    def inner(y):\n        if y > 1:\n            return audit(y)\n        return y\n    return inner(x)\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.pairs, 0, "no pair between a function and its own nested function: {rep:?}");
        assert_eq!(rep.skipped_nested, 1, "outer × inner is the excluded pair: {rep:?}");
        assert_eq!(rep.skipped_kind_mismatch, 0, "same kind — the kind rule never fires: {rep:?}");
        assert_eq!(rep.hidden_gaps, 0, "containment is not a hidden duplicate either: {rep:?}");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM duplicates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "no duplicates row is materialized");
        let gaps: i64 = conn
            .query_row("SELECT COALESCE(SUM(hidden_count), 0) FROM duplicate_gaps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gaps, 0, "the excluded partner is not hidden either");
    }

    #[test]
    fn nested_class_is_not_a_duplicate_of_its_outer_class() {
        let conn = setup();
        let mut st = IngestStats::default();
        // a nested class inside its outer class: Outer × Inner is
        // container × container (same kind), so only the SPAN rule removes
        // it; the class × method pairs stay in the KIND rule's bucket. The
        // buckets must not double-count the same geometry.
        let a = "class Outer:\n    class Inner:\n        def leaf(self, x):\n            if x > 1:\n                return audit(x)\n            return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.pairs, 0, "Outer × Inner is containment: {rep:?}");
        assert_eq!(rep.skipped_nested, 1, "Outer × Inner in the span bucket: {rep:?}");
        assert_eq!(rep.skipped_kind_mismatch, 2, "Outer × leaf, Inner × leaf in the kind bucket: {rep:?}");
        assert_eq!(rep.hidden_gaps, 0, "C(3,2)=3 pairs = 1 span + 2 kind + 0 shown: {rep:?}");
    }

    #[test]
    fn sibling_functions_still_compare_inside_their_container() {
        let conn = setup();
        let mut st = IngestStats::default();
        // two sibling defs inside one class: their spans are disjoint, so
        // the span rule leaves them alone — the identical bodies remain a
        // canonical Fact. The class × each-member pairs belong to the KIND
        // bucket only (the span rule must not swallow them too).
        let a = "class C:\n    def one(self, x):\n        if x > 1:\n            return audit(x)\n        return x\n    def two(self, x):\n        if x > 1:\n            return audit(x)\n        return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.skipped_nested, 0, "siblings are disjoint spans: {rep:?}");
        assert_eq!(rep.skipped_kind_mismatch, 2, "C × one, C × two: {rep:?}");
        assert_eq!(rep.canonical_hash, 1, "one × two are genuine duplicates: {rep:?}");
        assert_eq!(rep.hidden_gaps, 0, "C(3,2)=3 pairs = 2 kind + 1 shown: {rep:?}");
    }

    #[test]
    fn nesting_never_crosses_files() {
        let conn = setup();
        let mut st = IngestStats::default();
        // the same encloser-with-nested-function shape in two files: within
        // each file outer × inner is span-excluded, but ACROSS files the two
        // enclosers (and the two nested fns) are genuine duplicates — the
        // span rule is strictly same-file, like the kind rule's cross-file
        // sibling exception.
        let a = "def outer(x):\n    def inner(y):\n        if y > 1:\n            return audit(y)\n        return y\n    return inner(x)\n";
        let b = "def outer2(x):\n    def inner2(y):\n        if y > 1:\n            return audit(y)\n        return y\n    return inner2(x)\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        let f2 = ingest_parsed_file(&conn, 1, 41, "b.py", Lang::Python, b, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        cache_source_bytes(&conn, f2, b).unwrap();
        lower_all(&conn).unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(rep.skipped_nested, 2, "one encloser × inner pair per file: {rep:?}");
        assert_eq!(rep.canonical_hash, 2, "outer×outer2 and inner×inner2 across files: {rep:?}");
    }

    #[test]
    fn exclusion_buckets_are_disjoint_and_complete() {
        let conn = setup();
        let mut st = IngestStats::default();
        // one file with all three geometries: the kind rule (class × its
        // method), the span rule (nested fn inside its encloser), and
        // genuine duplicates (identical flat bodies). Every unordered pair
        // of the 6 symbols must land in exactly ONE bucket — kind, nested,
        // shown, or hidden — so the identity below is exact.
        let a = "def holder(x):\n    def nested(y):\n        return bump(y)\n    return nested(x)\n\nclass C:\n    def m(self, x):\n        if x > 1:\n            return audit(x)\n        return x\n\ndef flat1(x):\n    if x > 1:\n        return audit(x)\n    return x\n\ndef flat2(x):\n    if x > 1:\n        return audit(x)\n    return x\n";
        let f1 = ingest_parsed_file(&conn, 1, 41, "a.py", Lang::Python, a, &mut st).unwrap();
        cache_source_bytes(&conn, f1, a).unwrap();
        lower_all(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))
            .unwrap();
        let rep = detect(&conn).unwrap();
        assert_eq!(n, 6, "holder, nested, C, m, flat1, flat2");
        // the kind rule is unconditional about geometry: C (a class) × every
        // function/method in the ir_meta set — m, holder, nested, flat1,
        // flat2 — is 5 excluded pairs, wherever those functions sit
        assert_eq!(rep.skipped_kind_mismatch, 5, "C × every member kind: {rep:?}");
        assert_eq!(rep.skipped_nested, 1, "holder × nested: {rep:?}");
        assert!(rep.canonical_hash >= 1, "the identical flat bodies survive: {rep:?}");
        assert_eq!(
            rep.hidden_gaps as i64,
            n * (n - 1) / 2 - rep.skipped_kind_mismatch as i64 - rep.skipped_nested as i64 - rep.pairs as i64,
            "every pair lands in exactly one bucket: {rep:?}"
        );
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

    // -----------------------------------------------------------------------
    // The brute-force reference: the pre-index engine, kept as the
    // reference implementation of the SAME rule set (tiers, hidden
    // residual, and the container × member exclusion) so the indexed
    // engine can be proven equal to it (same rows, same gaps, same
    // canonical winner). Nothing in prod may call any of this.
    // -----------------------------------------------------------------------
    mod brute {
        use std::collections::{BTreeMap, BTreeSet};

        use rusqlite::Connection;

        use super::super::DupReport;

        pub fn cosine(a: &BTreeMap<String, u32>, b: &BTreeMap<String, u32>) -> f64 {
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

        pub fn detect(conn: &Connection) -> Result<DupReport, String> {
            let mut report = DupReport::default();
            let meta = super::super::load_ir_meta(conn)?;
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
            let kind_of = super::super::load_kinds(conn, &ids).unwrap();
            let spans = super::super::load_spans(conn, &ids).unwrap();
            for (i, &a) in ids.iter().enumerate() {
                for (j, &b) in ids.iter().enumerate().skip(i + 1) {
                    // container × member: containment, not duplication —
                    // excluded before any tier, never materialized
                    if super::super::kind_excluded(kind_of[i], kind_of[j]) {
                        report.skipped_kind_mismatch += 1;
                        continue;
                    }
                    // same-file span containment: the same geometry, any kinds
                    if super::super::span_nested(spans[i], spans[j]) {
                        report.skipped_nested += 1;
                        continue;
                    }
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

        fn mark_canonical(conn: &Connection) -> Result<(), String> {
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
    }

    /// tiny deterministic generator (no rand dependency; same stream every run)
    struct Lcg(u64);
    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) % n as u64) as usize
        }
    }

    const STMTS: &[&str] = &[
        "    if x > %K:\n        audit(u, x)\n",
        "    else:\n        skip(u)\n",
        "    while x > 0:\n        x -= step(u)\n",
        "    for i in range(%K):\n        total += bump(u, i)\n",
        "    try:\n        save(u, x)\n    except ValueError:\n        roll(u)\n",
        "    data = transform(u, x)\n",
        "    if data == x:\n        mark(u)\n",
        "    return finish(u, x)\n",
        "    x = a + b\n    y = x * 2\n    z = y - a\n",
        "    return z\n",
    ];

    /// A deterministic corpus with exact duplicates, near duplicates, weakly
    /// related bodies and unrelated arithmetic bodies — all four tiers and
    /// the hidden residual all show up.
    fn generated_sources(m: usize) -> Vec<(String, String)> {
        let mut rng = Lcg(0xC1E5_0001);
        let mut bodies: Vec<String> = Vec::new();
        for i in 0..m {
            let body = if i >= 6 && i % 7 == 0 {
                bodies[i - 6].clone() // exact duplicate of an earlier body
            } else if i % 11 == 3 {
                // near-empty body: its op bag barely overlaps anything else,
                // so its pairs fall under the floor (the hidden residual)
                "    pass\n".to_string()
            } else if i >= 1 && i % 5 == 0 {
                // near duplicate: previous body with one statement swapped
                let mut lines: Vec<String> = bodies[i - 1].lines().map(|l| l.to_string()).collect();
                let at = 1 + rng.below(lines.len().saturating_sub(1).max(1));
                let st = STMTS[rng.below(STMTS.len())].replace("%K", &(rng.below(9) + 1).to_string());
                if at < lines.len() {
                    lines[at] = st.trim_end().to_string();
                }
                lines.join("\n")
            } else {
                let len = 3 + rng.below(5);
                let mut s = String::new();
                for _ in 0..len {
                    s.push_str(STMTS[rng.below(STMTS.len())].replace("%K", &(rng.below(9) + 1).to_string()).as_str());
                }
                s
            };
            bodies.push(body);
        }
        let mut files: Vec<(String, String)> = Vec::new();
        for chunk in 0..3 {
            let mut src = String::new();
            let mut idx = chunk;
            while idx < m {
                src.push_str(&format!("def fn_{}(u, x):\n", idx));
                src.push_str(bodies[idx].trim_end());
                src.push_str("\n\n");
                idx += 3;
            }
            files.push((format!("gen_{chunk}.py"), src));
        }
        // one fixed file with real nesting: encloser × nested function and
        // outer class × nested class are span-containment pairs the rule
        // must exclude in BOTH engines — the parity check below covers the
        // span rule too, and the corpus-exercise assertion pins skipped_nested ≥ 1
        files.push((
            "gen_nested.py".to_string(),
            "def outer_gen(u, x):\n    def inner_gen(u, x):\n        if x > 3:\n            audit(u, x)\n        return x\n    return inner_gen(u, x)\n\nclass GenOuter:\n    class GenInner:\n        def leaf_gen(self, u, x):\n            if x > 1:\n                audit(u, x)\n            return x\n".to_string(),
        ));
        files
    }

    fn build_conn(files: &[(String, String)]) -> Connection {
        let conn = setup();
        let mut st = IngestStats::default();
        for (name, src) in files {
            let fid = ingest_parsed_file(&conn, 1, 41, name, Lang::Python, src, &mut st).unwrap();
            cache_source_bytes(&conn, fid, src).unwrap();
        }
        lower_all(&conn).unwrap();
        conn
    }

    type RowDump = Vec<(i64, i64, f64, String, String, String, Option<i64>, Option<String>)>;

    fn dump_rows(conn: &Connection) -> RowDump {
        let mut stmt = conn
            .prepare(
                "SELECT symbol_a, symbol_b, similarity, tier, detection, epistemic, canonical_symbol_id, counter_argument
                 FROM duplicates WHERE tier != 'hidden' ORDER BY symbol_a, symbol_b, tier",
            )
            .unwrap();
        stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    }

    #[test]
    fn dense_cosine_is_bit_identical_to_brute_force() {
        let mut rng = Lcg(0xB17_5EED);
        for _ in 0..500 {
            let mut a: BTreeMap<String, u32> = BTreeMap::new();
            let mut b: BTreeMap<String, u32> = BTreeMap::new();
            for _ in 0..(1 + rng.below(30)) {
                if rng.below(4) != 0 {
                    a.insert(format!("op{}", rng.below(40)), 1 + rng.below(20) as u32);
                }
                if rng.below(4) != 0 {
                    b.insert(format!("op{}", rng.below(40)), 1 + rng.below(20) as u32);
                }
            }
            let old = brute::cosine(&a, &b);
            let mut names: Vec<&str> = a
                .keys()
                .map(|k| k.as_str())
                .chain(b.keys().map(|k| k.as_str()))
                .collect();
            names.sort_unstable();
            names.dedup();
            let dim_of: HashMap<&str, u32> =
                names.iter().enumerate().map(|(i, k)| (*k, i as u32)).collect();
            let ba = Bag::build(&a, &dim_of);
            let bb = Bag::build(&b, &dim_of);
            assert_eq!(ba.cosine(&bb), old, "bags {a:?} vs {b:?}");
        }
    }

    #[test]
    fn indexed_engine_matches_brute_force_reference() {
        let files = generated_sources(90);
        let conn_new = build_conn(&files);
        let conn_ref = build_conn(&files);

        let rep_new = detect(&conn_new).unwrap();
        let rep_ref = brute::detect(&conn_ref).unwrap();

        // same totals
        assert_eq!(rep_new.pairs, rep_ref.pairs, "shown pairs");
        assert_eq!(rep_new.canonical_hash, rep_ref.canonical_hash, "canonical");
        assert_eq!(rep_new.high, rep_ref.high);
        assert_eq!(rep_new.medium, rep_ref.medium);
        assert_eq!(rep_new.low, rep_ref.low);
        assert_eq!(rep_new.hidden_gaps, rep_ref.hidden_gaps);
        assert_eq!(
            rep_new.skipped_kind_mismatch, rep_ref.skipped_kind_mismatch,
            "container × member exclusions"
        );
        assert_eq!(
            rep_new.skipped_nested, rep_ref.skipped_nested,
            "same-file span-containment exclusions"
        );

        // identical rows: similarity bits, tiers, detection, winner, counter
        let rows_new = dump_rows(&conn_new);
        let rows_ref = dump_rows(&conn_ref);
        assert_eq!(rows_new.len(), rows_ref.len(), "row count");
        for (i, (rn, rr)) in rows_new.iter().zip(rows_ref.iter()).enumerate() {
            assert_eq!(rn, rr, "first differing row at sorted index {i}");
        }

        // hidden pairs: reference materializes rows, engine counts per symbol —
        // the per-symbol counts must agree exactly (zero rows carry no
        // information on either side, so both maps are filtered to nonzeros:
        // the reference only ever writes a row when the count is ≥ 1)
        let ref_hidden: BTreeMap<i64, i64> = {
            let mut stmt = conn_ref
                .prepare(
                    "SELECT s, COUNT(*) FROM (
                         SELECT symbol_a AS s FROM duplicates WHERE tier='hidden'
                         UNION ALL
                         SELECT symbol_b AS s FROM duplicates WHERE tier='hidden'
                     ) GROUP BY s",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .collect::<Result<BTreeMap<_, _>, _>>()
                .unwrap()
        };
        let new_hidden: BTreeMap<i64, i64> = {
            let mut stmt = conn_new
                .prepare("SELECT symbol_id, hidden_count FROM duplicate_gaps WHERE hidden_count > 0")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .collect::<Result<BTreeMap<_, _>, _>>()
                .unwrap()
        };
        assert_eq!(new_hidden, ref_hidden, "per-symbol hidden counts");

        // the corpus actually exercises the machinery
        assert!(rep_ref.canonical_hash >= 1, "exact duplicates present");
        assert!(rep_ref.hidden_gaps >= 1, "sub-floor residual present");
        assert!(rep_ref.skipped_nested >= 1, "span-containment exclusions present");
    }
}
