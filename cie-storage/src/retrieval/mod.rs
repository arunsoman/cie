//! Revision-pinned retrieval. All physical access stays behind this module.
mod compatibility;
mod contract;
mod outputs;
mod query;
pub mod tools;
pub use contract::*;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Declared continuation-cursor lifetime. Tokens embed an expiry and are
/// rejected as expired afterwards; they never outlive their reader anyway.
pub(crate) const CURSOR_TTL_SECS: u64 = 15 * 60;

pub struct Storage {
    root: PathBuf,
    policy: crate::StoragePolicy,
}
pub struct RevisionReader {
    pub(crate) conn: Connection,
    pub(crate) repository: String,
    pub(crate) revision: String,
    pub(crate) snapshot: Value,
    pub(crate) artifacts: Value,
    pub(crate) secret: String,
    pub(crate) policy: crate::StoragePolicy,
}
impl Storage {
    pub fn open(root: &Path, policy: crate::StoragePolicy) -> Result<Self> {
        policy.validate()?;
        let root = root.canonicalize()?;
        if !root.join(".cie/meta.json").is_file() {
            return Err(StorageError::new(
                "RevisionNotFound",
                "No ingested revision",
            ));
        }
        Ok(Self { root, policy })
    }
    pub fn revisions(&self) -> Result<Vec<String>> {
        Ok(self
            .revision_numbers()?
            .into_iter()
            .map(|n| n.to_string())
            .collect())
    }
    /// §4: typed, bounded revision listing.
    pub fn list_revisions(&self, request: RevisionListRequest) -> Result<RevisionPage> {
        if request.limit == 0 || request.limit > self.policy.max_page_size {
            return Err(StorageError::new(
                "InvalidRequest",
                "Revision page size out of policy",
            ));
        }
        let head = self.head_revision()?;
        let numbers = self.revision_numbers()?;
        let total = numbers.len();
        let start = match &request.after {
            Some(after) => {
                let after = after.parse::<u64>().map_err(|_| {
                    StorageError::new("CursorInvalid", "Revision continuation must be numeric")
                })?;
                let position = numbers.iter().position(|n| *n > after).ok_or_else(|| {
                    StorageError::new("CursorInvalid", "Revision continuation exhausted")
                })?;
                position
            }
            None => 0,
        };
        let items = numbers[start..]
            .iter()
            .take(request.limit)
            .map(|n| RevisionInfo {
                revision: n.to_string(),
                head: *n == head,
                complete: true,
            })
            .collect::<Vec<_>>();
        let continuation = items
            .last()
            .filter(|_| start + items.len() < total)
            .map(|info| info.revision.clone());
        Ok(RevisionPage {
            items,
            total,
            continuation,
        })
    }
    fn head_revision(&self) -> Result<u64> {
        let meta: Value =
            serde_json::from_slice(&std::fs::read(self.root.join(".cie/meta.json"))?)?;
        meta["last_rev"]
            .as_u64()
            .ok_or_else(|| StorageError::new("CorruptData", "Missing revision head"))
    }
    fn revision_numbers(&self) -> Result<Vec<u64>> {
        let head = self.head_revision()?;
        let mut revisions = Vec::new();
        for entry in std::fs::read_dir(self.root.join(".cie/revs"))? {
            let entry = entry?;
            if let Ok(n) = entry.file_name().to_string_lossy().parse::<u64>() {
                if n <= head
                    && entry.path().join("repository.sqlite").is_file()
                    && entry.path().join("snapshot.json").is_file()
                {
                    revisions.push(n);
                }
            }
        }
        revisions.sort();
        Ok(revisions)
    }
    pub fn open_revision(&self, selector: RevisionSelector) -> Result<RevisionReader> {
        // Publication lock spans DB, core snapshot, and committed head.
        let _lock = crate::repository::ProjectLock::acquire(&self.root.join(".cie"))?;
        let head = self.head_revision()?;
        let rev = match selector {
            RevisionSelector::Latest => head,
            RevisionSelector::Id(s) => s.parse::<u64>().map_err(|_| {
                StorageError::new(
                    "InvalidRequest",
                    "Revision must be an unsigned decimal string",
                )
            })?,
        };
        if rev == 0 || rev > head {
            return Err(StorageError::new(
                "RevisionNotFound",
                "Revision has not been published",
            ));
        }
        let dir = self.root.join(".cie/revs").join(rev.to_string());
        let db = dir.join("repository.sqlite");
        if !db.is_file() {
            return Err(StorageError::new(
                "RevisionUnavailable",
                "This revision predates retained retrieval databases; ingest a changed revision",
            ));
        }
        let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        self.policy.configure(&conn)?;
        conn.execute_batch("BEGIN; SELECT count(*) FROM files;")?;
        let snapshot: Value = serde_json::from_slice(&std::fs::read(dir.join("snapshot.json"))?)?;
        if snapshot["rev"].as_u64() != Some(rev) {
            return Err(StorageError::new(
                "CorruptData",
                "Snapshot revision mismatch",
            ));
        }
        let artifacts = match std::fs::read(dir.join("web-bundle.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(e) => return Err(e.into()),
        };
        let repository = blake3::hash(self.root.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        let secret = format!("{}:{}:{:?}", repository, rev, Instant::now());
        Ok(RevisionReader {
            conn,
            repository,
            revision: rev.to_string(),
            snapshot,
            artifacts,
            secret,
            policy: self.policy.clone(),
        })
    }
}
pub(crate) struct Execution {
    deadline: Instant,
    max_visited: usize,
    visited: usize,
}
impl Execution {
    pub(crate) fn check(&mut self, n: usize) -> Result<()> {
        self.visited = self.visited.saturating_add(n);
        if Instant::now() > self.deadline || self.visited > self.max_visited {
            Err(StorageError::new(
                "BudgetExceeded",
                "Query time or work limit exceeded",
            ))
        } else {
            Ok(())
        }
    }
}
impl RevisionReader {
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn capabilities(&self) -> Value {
        json!({"api_version":1,"repository":self.repository,"revision":self.revision,"entities":["file","symbol","concept","summary","contract","finding","duplicate_pair","cluster"],"relationships":["defined_in","calls","member_of_concept","child_concept_of","member_of_cluster","summarizes","constrains","left_symbol","right_symbol","canonical_symbol"],"outputs":["entities","relationships","graph","evidence","cube","facets","detail","artifact","view"],"details":{"source":{"ranges":["declaration","whole_file","bytes"],"context_lines":true,"truncation_continuation":true},"function_ir":{"include_calls":true},"impact":{"availability":["build_if_missing"]}},"compare_ops":["eq","in","prefix","contains","prefix_fold","contains_fold","gt","ge","lt","le"],"quantifiers":["any","none","all"],"membership_scopes":["direct","include_descendants"],"dimensions":["file.path","language","entity_kind","kind","concept","cluster"],"hierarchies":{"file.path":{"levels":["file","directory"]}},"bucketing":{"numeric":{"boundary_rule":"lower_inclusive_upper_exclusive"},"time":{"timezones":"utc_or_fixed_offset_only","units":["seconds","minutes","hours","days"]}},"facets":{"self_exclusion":true},"cube_limits":{"dimensions":8,"measures":32,"evaluated_groups":1_000_000},"details_list":["source","function_ir","impact"],"artifact_kinds":["report","viewspec","concepts","stack","deps","runtime","stories","quality","overlap","drift","cochange","fault","polyglotSpecs","bootstrap"],"view_renderers":["viz-adapter","cie-polyglot","cie-cube"],"symbol_resolution":true,"limits":{"page":self.policy.max_page_size,"outputs":16,"depth":8,"bytes":16_777_216,"cursor_ttl_seconds":CURSOR_TTL_SECS},"coverage":self.snapshot["coverage"],"source":"captured_only","unsupported_features":["cross_revision_queries","approximate_aggregates","natural_language_execution","component_hierarchy","calendar_time_units"]})
    }
    pub fn retrieve(&self, request: Request) -> Result<Response> {
        if request.api_version != 1 || request.outputs.is_empty() || request.outputs.len() > 16 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Expected API version 1 and 1..16 outputs",
            ));
        }
        if request.budget.max_response_bytes == 0
            || request.budget.max_response_bytes > 16 * 1024 * 1024
            || request.budget.timeout_ms == 0
            || request.budget.timeout_ms > 30_000
            || request.budget.max_visited == 0
            || request.budget.max_visited > 1_000_000
        {
            return Err(StorageError::new("InvalidRequest", "Budget exceeds policy"));
        }
        let mut names = BTreeSet::new();
        for output in &request.outputs {
            if output.id.is_empty() || !names.insert(&output.id) {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Output IDs must be unique and nonempty",
                ));
            }
        }
        let mut ex = Execution {
            deadline: Instant::now() + Duration::from_millis(request.budget.timeout_ms),
            max_visited: request.budget.max_visited,
            visited: 0,
        };
        let deadline = ex.deadline;
        self.conn
            .progress_handler(1000, Some(move || Instant::now() > deadline));
        let dataset = self.dataset(&mut ex)?;
        let matches = self.evaluate(&request.query, &dataset, &mut ex, 0)?;
        let hash = blake3::hash(&serde_json::to_vec(&request.query)?)
            .to_hex()
            .to_string();
        let mut prior = BTreeMap::new();
        let mut outputs = Vec::new();
        for output in &request.outputs {
            ex.check(0)?;
            let result = self.output(
                &output.output,
                &request.query,
                &matches,
                &dataset,
                &hash,
                &mut ex,
                &prior,
            );
            let (status, data, error) = match result {
                Ok(data) => {
                    let status = if data.get("truncated") == Some(&json!(true)) {
                        "truncated"
                    } else if matches.is_empty()
                        && !matches!(&output.output, Output::Artifact { .. })
                    {
                        "empty"
                    } else {
                        "ready"
                    };
                    (status.to_string(), data, None)
                }
                Err(e) if request.best_effort => ("failed".into(), Value::Null, Some(e)),
                Err(e) => return Err(e),
            };
            prior.insert(output.id.clone(), data.clone());
            outputs.push(OutputResult {
                id: output.id.clone(),
                status,
                schema_version: 1,
                origin: "stored_or_derived".into(),
                data,
                error,
            });
        }
        let response = Response {
            api_version: 1,
            repository: self.repository.clone(),
            revision: self.revision.clone(),
            query_hash: hash,
            matches: matches.len(),
            coverage: self.snapshot["coverage"].clone(),
            outputs,
        };
        if serde_json::to_vec(&response)?.len() > request.budget.max_response_bytes {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Response exceeds byte limit; request fewer fields or a smaller page",
            ));
        }
        Ok(response)
    }
    pub(crate) fn rows(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(sql)?;
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map(params, |r| {
            let mut obj = serde_json::Map::new();
            for (i, col) in columns.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(b) => json!(String::from_utf8_lossy(b)),
                    ValueRef::Blob(b) => json!({"binary_bytes":b.len()}),
                };
                obj.insert(col.clone(), v);
            }
            Ok(Value::Object(obj))
        })?;
        let rows = rows
            .take(100_001)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.len() > 100_000 {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Stored row limit exceeded",
            ));
        }
        Ok(rows)
    }
}
