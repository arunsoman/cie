//! PRD v2.0 slice 15 — impact radius + duplicate detection (§23).
//!
//! The visual: one shared center, callers on the left by ring, duplicates
//! on the right by similarity tier (§23.1). Engine side:
//!
//! | Tier | Method | Confidence / displayMode |
//! |---|---|---|
//! | Canonical duplicate | `ir_hash` exact match | Fact (`ir-hash-match`, similarity 1.0) |
//! | High fragment overlap | jscpd line coverage ≥ 0.85 | Inference (`token-clone`) |
//! | Medium fragment overlap | coverage 0.60–0.85 | Inference (`token-clone`) |
//! | Low fragment overlap | coverage below 0.60 | Inference (`token-clone`) |
//! Candidate discovery is sparse; absence is not proof of uniqueness.
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
CREATE TABLE IF NOT EXISTS duplicate_clusters (
    cluster_id INTEGER NOT NULL,
    symbol_id INTEGER NOT NULL REFERENCES symbols(id),
    ordinal INTEGER NOT NULL,
    PRIMARY KEY (cluster_id, symbol_id)
);
CREATE TABLE IF NOT EXISTS duplicate_gaps (
    symbol_id    INTEGER PRIMARY KEY REFERENCES symbols(id),
    hidden_count INTEGER NOT NULL
);
"#;

pub fn create_dup_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_DUP).map_err(|e| e.to_string())
}

/// Persist connected components of canonical duplicate edges. Similarity
/// inferences never enter these exact-IR-hash families. Memory is O(symbols),
/// and pairs are streamed from SQLite rather than collected in the browser.
pub fn cluster_canonical(conn: &Connection) -> Result<usize,String> {
    fn find(parent: &mut std::collections::BTreeMap<i64,i64>, x:i64)->i64 {
        let mut root=x;
        while parent[&root]!=root {root=parent[&root];}
        let mut cur=x;
        while parent[&cur]!=root {let next=parent[&cur];parent.insert(cur,root);cur=next;}
        root
    }
    let mut parent=std::collections::BTreeMap::new();
    let mut sizes=std::collections::BTreeMap::new();
    {
        let mut q=conn.prepare("SELECT symbol_a,symbol_b FROM duplicates WHERE tier='canonical' ORDER BY id").map_err(|e|e.to_string())?;
        let rows=q.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())?;
        for row in rows {
            let (a,b)=row.map_err(|e|e.to_string())?;
            parent.entry(a).or_insert(a);parent.entry(b).or_insert(b);
            sizes.entry(a).or_insert(1usize);sizes.entry(b).or_insert(1usize);
            let mut a=find(&mut parent,a);let mut b=find(&mut parent,b);
            if a!=b {if sizes[&a]<sizes[&b]{std::mem::swap(&mut a,&mut b);}parent.insert(b,a);let count=sizes[&b];*sizes.get_mut(&a).unwrap()+=count;}
        }
    }
    let keys:Vec<_>=parent.keys().copied().collect();
    let mut groups:std::collections::BTreeMap<i64,Vec<i64>>=std::collections::BTreeMap::new();
    for id in keys {groups.entry(find(&mut parent,id)).or_default().push(id);}
    let tx=conn.unchecked_transaction().map_err(|e|e.to_string())?;
    tx.execute("DELETE FROM duplicate_clusters",[]).map_err(|e|e.to_string())?;
    {
        let mut insert=tx.prepare("INSERT INTO duplicate_clusters VALUES (?1,?2,?3)").map_err(|e|e.to_string())?;
        for members in groups.values() {
            let cluster=members[0];
            for (ordinal,id) in members.iter().enumerate(){insert.execute(rusqlite::params![cluster,id,ordinal as i64]).map_err(|e|e.to_string())?;}
        }
    }
    tx.commit().map_err(|e|e.to_string())?;
    Ok(groups.len())
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
    ("high", "token-clone", "Inference", Some("jscpd token fragment overlap; semantic equivalence and IR-hash equality are not established")),
    ("medium", "token-clone", "Inference", Some("jscpd token fragment overlap; semantic equivalence and IR-hash equality are not established")),
    (
        "low",
        "token-clone",
        "Inference",
        Some("jscpd token fragment overlap; control-flow and data-flow equivalence are not established"),
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

/// Detect duplicate pairs using `jscpd-rs`'s native clone detector (a
/// token-based copy/paste detector — OXC-fast tokenization for JS/TS, a
/// generic tokenizer for every other format, including java/kotlin/python)
/// in place of the previous op-bag cosine-similarity engine.
///
/// Each symbol's own extracted source text (by its byte span) becomes one
/// jscpd "file" — `source_id` is the symbol id itself, so a `CloneMatch`
/// between two "files" directly names the two duplicate symbols, with no
/// line-range-to-symbol lookup needed. The container/member and same-file
/// span-containment exclusions (§23 preamble) still apply: jscpd would
/// otherwise happily report a class's body as a "clone" of its own single
/// method, since the method's text is a literal substring of the class's.
///
/// jscpd has no continuous similarity score — a match either clears its
/// `min_lines`/`min_tokens` floor and gets reported, or it doesn't exist.
/// The old 0.85/0.60/0.40 tier cut points are kept, but now bucket on how
/// much of the SMALLER symbol's own line count the matched fragment covers
/// (even whole-body token overlap remains inference unless stored IR hashes match). Because jscpd only
/// ever reports matches that already cleared its own floor, there is no
/// honest "hidden below floor" residual the way the old exhaustive cosine
/// scan had — `hidden_gaps` is always 0 here.
pub fn detect(conn: &Connection) -> Result<DupReport, String> {
    let mut report = DupReport::default();

    struct SymInfo {
        file_id: i64,
        start: i64,
        end: i64,
        kind: u8,
    }
    let mut syms: BTreeMap<i64, SymInfo> = BTreeMap::new();
    let mut languages: BTreeMap<i64, String> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.file_id, s.start_byte, s.end_byte, s.kind, f.language
                 FROM symbols s JOIN files f ON f.id = s.file_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, file_id, start, end, kind, lang) = row.map_err(|e| e.to_string())?;
            syms.insert(
                id,
                SymInfo { file_id, start, end, kind: kind_code(&kind) },
            );
            languages.insert(id, lang);
        }
    }
    if syms.len() < 2 {
        return Ok(report);
    }

    // Canonical claims require actual stored IR-hash equality, independently
    // of a token detector's line coverage ratio.
    let mut hashes: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    {
        let mut query = conn.prepare("SELECT symbol_id,ir_hash FROM ir_meta").map_err(|e|e.to_string())?;
        let rows = query.query_map([], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;
        for row in rows { let (id,hash)=row.map_err(|e|e.to_string())?;if hash.len()==32 {hashes.insert(id,hash);} }
    }

    // Decompress every file's cached source text once, keyed by file_id —
    // symbols sharing a file reuse the same decompressed text.
    let mut file_text: BTreeMap<i64, String> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.size_bytes, c.zstd_bytes
                 FROM files f JOIN file_source_cache c ON c.file_id = f.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Vec<u8>>(2)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (file_id, size_bytes, zstd_bytes) = row.map_err(|e| e.to_string())?;
            if size_bytes < 0 {
                continue;
            }
            let Ok(bytes) = zstd::bulk::decompress(&zstd_bytes, size_bytes as usize) else {
                continue;
            };
            if let Ok(text) = String::from_utf8(bytes) {
                file_text.insert(file_id, text);
            }
        }
    }

    // One jscpd "file" per symbol. line_count is the tiering denominator —
    // jscpd reports byte/line ranges per match, not a similarity score.
    let mut jscpd_files: Vec<jscpd_rs::SourceFile> = Vec::with_capacity(syms.len());
    let mut line_count: BTreeMap<i64, usize> = BTreeMap::new();
    for (&id, info) in &syms {
        let Some(text) = file_text.get(&info.file_id) else { continue };
        let bytes = text.as_bytes();
        let (start, end) = (info.start as usize, info.end as usize);
        if end > bytes.len() || start >= end {
            continue;
        }
        let body = String::from_utf8_lossy(&bytes[start..end]).into_owned();
        line_count.insert(id, body.lines().count().max(1));
        jscpd_files.push(jscpd_rs::SourceFile {
            source_id: id.to_string(),
            format: languages.get(&id).cloned().unwrap_or_default(),
            content: body,
        });
    }
    if jscpd_files.len() < 2 {
        return Ok(report);
    }

    let mut options = jscpd_rs::get_default_options();
    options.reporters.clear();
    options.silent = true;
    // Every symbol is already its own "file" (no file-level floor needed
    // the way a whole-repo scan would) — these are jscpd's lowest floors
    // that still keep single-token noise (e.g. a bare `return;`) out.
    options.min_lines = 1;
    options.min_tokens = 15;
    options.max_lines = 1_000_000;
    options.max_size_bytes = 64 * 1024 * 1024;
    let result = jscpd_rs::detect_source_files(jscpd_files, &options);

    // §23.3 winner, computed over every symbol that appears in any match —
    // the old engine computed it "over the whole pair set" too.
    let winner_ids: Vec<i64> = {
        let mut set = BTreeSet::new();
        for clone in &result.clones {
            if let (Ok(a), Ok(b)) = (
                clone.duplication_a.source_id.parse::<i64>(),
                clone.duplication_b.source_id.parse::<i64>(),
            ) {
                set.insert(a);
                set.insert(b);
            }
        }
        set.into_iter().collect()
    };
    let winner = canonical_winner(conn, &winner_ids)?;

    // Bulk-load discipline, same as before: every referenced id was just
    // read from `symbols` in this call, so the per-row FK parent probes are
    // pure redundancy during the insert.
    let _fk_guard = FkGuard::off(conn)?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut seen_pairs: BTreeSet<(i64, i64)> = BTreeSet::new();
    {
        let full_sql = dup_values_sql(CHUNK_ROWS, winner);
        let mut full = tx.prepare(&full_sql).map_err(|e| e.to_string())?;
        let mut pending: usize = 0;
        let mut buf: Vec<(i64, i64, f64, u8)> = Vec::with_capacity(CHUNK_ROWS);

        for clone in &result.clones {
            let (Ok(a_id), Ok(b_id)) = (
                clone.duplication_a.source_id.parse::<i64>(),
                clone.duplication_b.source_id.parse::<i64>(),
            ) else {
                continue;
            };
            if a_id == b_id {
                continue;
            }
            let (Some(sa), Some(sb)) = (syms.get(&a_id), syms.get(&b_id)) else {
                continue;
            };
            if kind_excluded(sa.kind, sb.kind) {
                report.skipped_kind_mismatch += 1;
                continue;
            }
            if span_nested((sa.file_id, sa.start, sa.end), (sb.file_id, sb.start, sb.end)) {
                report.skipped_nested += 1;
                continue;
            }
            let (lo, hi) = (a_id.min(b_id), a_id.max(b_id));
            if !seen_pairs.insert((lo, hi)) {
                continue; // jscpd may report more than one overlapping fragment per pair
            }
            let matched_lines = (clone.duplication_a.end.line + 1)
                .saturating_sub(clone.duplication_a.start.line)
                .max(1);
            let shorter = line_count
                .get(&a_id)
                .copied()
                .unwrap_or(1)
                .min(line_count.get(&b_id).copied().unwrap_or(1));
            let ratio = (matched_lines as f64 / shorter as f64).min(1.0);
            let combo = if hashes.get(&lo).zip(hashes.get(&hi)).map(|(a,b)|a==b).unwrap_or(false) {
                report.canonical_hash += 1;
                C_CANONICAL
            } else if ratio >= 0.85 {
                report.high += 1;
                C_HIGH
            } else if ratio >= 0.60 {
                report.medium += 1;
                C_MEDIUM
            } else {
                report.low += 1;
                C_LOW
            };
            feed_row(&mut full, &mut pending, &mut buf, lo, hi, if combo == C_CANONICAL { 1.0 } else { ratio }, combo)?;
        }

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
        // jscpd is a sparse detector: it never claims to have exhaustively
        // compared every pair the way the old cosine scan did, so there is
        // no honest "hidden below floor" count to report per symbol.
        let mut gap = tx
            .prepare("INSERT OR REPLACE INTO duplicate_gaps(symbol_id, hidden_count) VALUES (?1, 0)")
            .map_err(|e| e.to_string())?;
        for &id in syms.keys() {
            gap.execute(rusqlite::params![id]).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    drop(_fk_guard); // restore FK enforcement before the caller sees the conn

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_dup_symbol_a ON duplicates(symbol_a);
         CREATE INDEX IF NOT EXISTS idx_dup_symbol_b ON duplicates(symbol_b);
         CREATE INDEX IF NOT EXISTS idx_dup_tier ON duplicates(tier);
         CREATE INDEX IF NOT EXISTS idx_dup_detection ON duplicates(detection);
         CREATE INDEX IF NOT EXISTS idx_dup_epistemic ON duplicates(epistemic);",
    )
    .map_err(|e| e.to_string())?;

    report.pairs = report.canonical_hash + report.high + report.medium + report.low;
    report.hidden_gaps = 0;
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
    fn token_overlap_with_unequal_ir_hashes_is_never_a_fact() {
        let conn=setup();let mut stats=IngestStats::default();
        let a="def charge(x):\n    if x > 1:\n        return audit(x)\n    return x\n";
        let b=a.replace("charge", "charge2");
        let f1=ingest_parsed_file(&conn,1,41,"a.py",Lang::Python,a,&mut stats).unwrap();
        let f2=ingest_parsed_file(&conn,1,41,"b.py",Lang::Python,&b,&mut stats).unwrap();
        cache_source_bytes(&conn,f1,a).unwrap();cache_source_bytes(&conn,f2,&b).unwrap();lower_all(&conn).unwrap();
        conn.execute("UPDATE ir_meta SET ir_hash=?1 WHERE symbol_id IN (SELECT id FROM symbols WHERE file_id=?2)",rusqlite::params![vec![7u8;32],f2]).unwrap();
        let report=detect(&conn).unwrap();assert!(report.pairs>0);assert_eq!(report.canonical_hash,0);
        let (kind,mode):(String,String)=conn.query_row("SELECT detection,epistemic FROM duplicates",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(kind,"token-clone");assert_eq!(mode,"Inference");
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
