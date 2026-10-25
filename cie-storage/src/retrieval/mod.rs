//! Revision-pinned retrieval. All physical access stays behind this module.
mod artifact_payload;
mod contract;
mod document;
mod outputs;
mod payload;
pub use artifact_payload::*;
mod plan;
mod providers;
pub use payload::*;
mod query;
mod architecture;
mod quality;
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
    pub(crate) snapshot: document::PinnedDocument,
    pub(crate) artifacts: document::PinnedDocument,
    pub(crate) secret: String,
    pub(crate) policy: crate::StoragePolicy,
    projections: std::cell::RefCell<BTreeMap<String, (Value, usize)>>,
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
    /// Retain committed storage independently of a disposable source checkout.
    /// The destination must be new; no source files or symlinks are copied.
    pub fn retain_to(&self, destination: &Path) -> Result<()> {
        let _lease = crate::repository::ProjectLock::acquire_timeout(
            &self.root.join(".cie"),
            self.policy.busy_timeout,
        )?;
        std::fs::create_dir(destination)?;
        fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
            std::fs::create_dir(target)?;
            for entry in std::fs::read_dir(source)? {
                let entry = entry?;
                if entry.file_name() == "ingest.lock" {
                    continue;
                }
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err(std::io::Error::other("Storage archive contains a symlink"));
                }
                if kind.is_dir() {
                    copy_tree(&entry.path(), &target.join(entry.file_name()))?;
                } else if kind.is_file() {
                    std::fs::copy(entry.path(), target.join(entry.file_name()))?;
                }
            }
            Ok(())
        }
        let result = (|| -> Result<()> {
            copy_tree(&self.root.join(".cie"), &destination.join(".cie"))?;
            let identity = self.repository_identity()?;
            std::fs::write(destination.join(".cie/repository-id"), identity)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(destination);
        }
        result
    }
    fn repository_identity(&self) -> Result<String> {
        match std::fs::read_to_string(self.root.join(".cie/repository-id")) {
            Ok(id) if id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()) => Ok(id),
            Ok(_) => Err(StorageError::new(
                "CorruptData",
                "Invalid repository identity",
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(blake3::hash(self.root.to_string_lossy().as_bytes())
                    .to_hex()
                    .to_string())
            }
            Err(e) => Err(e.into()),
        }
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
        let _lock = crate::repository::ProjectLock::acquire_timeout(
            &self.root.join(".cie"),
            self.policy.busy_timeout,
        )?;
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
        let snapshot = document::PinnedDocument::parse(&std::fs::read(dir.join("snapshot.json"))?)?;
        if snapshot["rev"].as_u64() != Some(rev) {
            return Err(StorageError::new(
                "CorruptData",
                "Snapshot revision mismatch",
            ));
        }
        let artifacts = match std::fs::read(dir.join("web-bundle.json")) {
            Ok(bytes) => document::PinnedDocument::parse(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
            Err(e) => return Err(e.into()),
        };
        let repository = self.repository_identity()?;
        let secret = format!("{}:{}:{:?}", repository, rev, Instant::now());
        Ok(RevisionReader {
            conn,
            repository,
            revision: rev.to_string(),
            snapshot,
            artifacts,
            secret,
            policy: self.policy.clone(),
            projections: Default::default(),
        })
    }
}
pub(crate) struct Execution {
    deadline: Instant,
    max_visited: usize,
    visited: usize,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Execution {
    pub(crate) fn check(&mut self, n: usize) -> Result<()> {
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::new("Cancelled", "Retrieval cancelled"));
        }
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
        let mut capabilities = json!({"api_version":1,"repository":self.repository,"revision":self.revision,"entities":["file","symbol","concept","summary","contract","finding","duplicate_pair","cluster","route","entry_point","framework","component","container","dependency","state_machine","state","workflow","evidence"],"relationships":["defined_in","calls","declared_calls","member_of_concept","child_concept_of","member_of_cluster","summarizes","constrains","left_symbol","right_symbol","canonical_symbol","member_of_component","member_of_container","component_in_container","communicates_with","child_component_of","uses_dependency","source_symbol","state_of","has_evidence","co_changed"],"outputs":["entities","relationships","graph","evidence","cube","facets","detail","artifact","view"],"details":{"source":{"ranges":["declaration","whole_file","bytes"],"context_lines":true,"truncation_continuation":true},"function_ir":{"include_calls":true},"impact":{"availability":["build_if_missing"]}},"compare_ops":["eq","in","prefix","contains","prefix_fold","contains_fold","gt","ge","lt","le"],"quantifiers":["any","none","all"],"membership_scopes":["direct","include_descendants"],"dimensions":["file.path","language","entity_kind","kind","concept","cluster"],"hierarchies":{"file.path":{"levels":["file","directory"]}},"bucketing":{"numeric":{"boundary_rule":"lower_inclusive_upper_exclusive"},"time":{"timezones":"iana_or_fixed_offset","units":["seconds","minutes","hours","days","months","years"]}},"facets":{"self_exclusion":true},"cube_limits":{"dimensions":8,"measures":32,"evaluated_groups":1_000_000},"details_list":["source","function_ir","impact"],"artifact_kinds":["report","viewspec","concepts","stack","deps","runtime","stories","quality","overlap","drift","cochange","fault","polyglotSpecs","bootstrap"],"projection_providers":[{"id":"relationships","version":1,"scope":"matches","renderer":"viz-adapter"},{"id":"cube","version":1,"scope":"output","kinds":["table","hbar","heatmap","stat","donut"],"renderer":"cie-cube"},{"id":"architecture","version":1,"scope":"evidenced_matches","availability":["stored_only","build_if_missing"],"renderer":"cie-polyglot"}],"projection_cache":{"entries":32,"max_bytes":8388608,"max_entry_bytes":1048576,"version":1,"scope":"reader"},"view_renderers":["viz-adapter","cie-polyglot","cie-cube"],"symbol_resolution":true,"limits":{"page":self.policy.max_page_size,"outputs":16,"depth":8,"bytes":16_777_216,"cursor_ttl_seconds":CURSOR_TTL_SECS},"coverage":self.snapshot["coverage"],"source":"captured_only","unsupported_features":["cross_revision_queries","approximate_aggregates","natural_language_execution"]});
        capabilities["artifact_kinds"] = json!(ArtifactKind::ALL);
        capabilities["output_schema_version"] = json!(2);
        capabilities["projection_providers"] = json!([
            {"id":"relationships","version":1,"renderer":"viz-adapter","scope":"matches"},
            {"id":"cube","version":1,"renderer":"cie-cube","scope":"output"},
            {"id":"repository_map","version":1,"renderer":"viz-adapter","scope":"matches"},
            {"id":"concept_hierarchy","version":1,"renderer":"viz-adapter","scope":"matches"},
            {"id":"duplicate_clusters","version":1,"renderer":"viz-adapter","scope":"matches"},
            {"id":"cochange","version":1,"renderer":"viz-adapter","scope":"matches","required_inputs":["cochange"]},
            {"id":"dependencies","version":1,"renderer":"viz-adapter","scope":"matches","required_inputs":["deps"]},
            {"id":"c4","version":1,"renderer":"cie-polyglot","scope":"evidenced_matches","levels":["context","container","component"]},
            {"id":"state-machine","version":1,"renderer":"cie-polyglot","scope":"evidenced_matches"},
            {"id":"workflow","version":1,"renderer":"cie-polyglot","scope":"evidenced_matches"},
            {"id":"impact","version":1,"renderer":"cie-polyglot","scope":"evidenced_matches"}
        ]);
        capabilities["available_inputs"] = json!({"captured_artifacts":self.artifacts.keys(),"captured_snapshot_fields":self.snapshot.keys()});
        capabilities
    }
    /// Bound diagnostics and resolver SQL too; all public transports share
    /// the same cancellation flag without taking the reader registry lock.
    pub fn run_bounded<T>(
        &self,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        action: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::new("Cancelled", "Request cancelled"));
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let token = cancelled.clone();
        self.conn.progress_handler(
            1000,
            Some(move || {
                Instant::now() > deadline || token.load(std::sync::atomic::Ordering::Relaxed)
            }),
        );
        struct Reset<'a>(&'a Connection);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.progress_handler(0, None::<fn() -> bool>);
            }
        }
        let _reset = Reset(&self.conn);
        let result = action();
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::new("Cancelled", "Request cancelled"));
        }
        if Instant::now() > deadline {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Request deadline exceeded",
            ));
        }
        result
    }
    pub fn retrieve(&self, request: Request) -> Result<Response> {
        self.retrieve_with_cancel(request, Default::default())
    }
    pub fn retrieve_with_cancel(
        &self,
        request: Request,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Response> {
        let timeout = Duration::from_millis(request.budget.timeout_ms);
        let started = Instant::now();
        let result = self.retrieve_inner(request, cancelled.clone());
        self.snapshot.check()?;
        self.artifacts.check()?;
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StorageError::new("Cancelled", "Retrieval cancelled"));
        }
        if result.is_err() && started.elapsed() >= timeout {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Retrieval deadline exceeded",
            ));
        }
        result
    }
    fn retrieve_inner(
        &self,
        request: Request,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Response> {
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
            cancelled: cancelled.clone(),
        };
        ex.check(0)?;
        struct ResetProgress<'a>(&'a Connection);
        impl Drop for ResetProgress<'_> {
            fn drop(&mut self) {
                self.0.progress_handler(0, None::<fn() -> bool>);
            }
        }
        let _reset = ResetProgress(&self.conn);
        let deadline = ex.deadline;
        self.conn.progress_handler(
            1000,
            Some(move || {
                Instant::now() > deadline || cancelled.load(std::sync::atomic::Ordering::Relaxed)
            }),
        );
        let (duplicates, domains) = query::dataset_needs(&request)?;
        let mut dataset = if let Some(dataset) = self.file_plan(&request, &mut ex)? {
            dataset
        } else {
            self.dataset_with_duplicates(&mut ex, duplicates, domains)?
        };
        if quality::needed(&request) { self.quality_fields(&mut dataset, &mut ex)?; }
        let matches = self.evaluate(&request.query, &dataset, &mut ex, 0)?;
        let hash = blake3::hash(&serde_json::to_vec(&request.query)?)
            .to_hex()
            .to_string();
        // Resolve the dependency graph before executing any outputs. Response order
        // remains the caller's order, independently of execution order.
        fn visit(
            index: usize,
            outputs: &[NamedOutput],
            marks: &mut [u8],
            order: &mut Vec<usize>,
        ) -> Result<()> {
            if marks[index] == 2 {
                return Ok(());
            }
            if marks[index] == 1 {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Cyclic output dependency",
                ));
            }
            marks[index] = 1;
            if let Output::View {
                source: ViewSource::Output(id),
                ..
            } = &outputs[index].output
            {
                let dependency = outputs.iter().position(|o| &o.id == id).ok_or_else(|| {
                    StorageError::new("InvalidRequest", "Unknown output dependency")
                })?;
                if !matches!(outputs[dependency].output, Output::Cube(_)) {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Chart source must be a cube output",
                    ));
                }
                visit(dependency, outputs, marks, order)?;
            }
            marks[index] = 2;
            order.push(index);
            Ok(())
        }
        let mut order = Vec::new();
        let mut marks = vec![0; request.outputs.len()];
        for index in 0..request.outputs.len() {
            visit(index, &request.outputs, &mut marks, &mut order)?;
        }
        let mut prior = BTreeMap::new();
        let mut results = BTreeMap::new();
        for index in order {
            let output = &request.outputs[index];
            ex.check(0)?;
            let cache_key = if let Output::View { source, .. } = &output.output {
                let mut definition = serde_json::to_value(&output.output)?;
                definition["view"]
                    .as_object_mut()
                    .unwrap()
                    .remove("availability");
                let dependency = if let ViewSource::Output(id) = source {
                    prior.get(id)
                } else {
                    None
                };
                Some(blake3::hash(&serde_json::to_vec(&json!({"provider_version":1,"schema_version":2,"repository":self.repository,"revision":self.revision,"query":hash,"definition":definition,"dependency":dependency}))?).to_hex().to_string())
            } else {
                None
            };
            let cached = cache_key.as_ref().and_then(|key| {
                self.projections
                    .borrow()
                    .get(key)
                    .map(|entry| entry.0.clone())
            });
            let from_cache = cached.is_some();
            let result = if let Some(data) = cached {
                Ok(data)
            } else {
                self.output(
                    &output.output,
                    &request.query,
                    &matches,
                    &dataset,
                    &hash,
                    &mut ex,
                    &prior,
                )
            };
            let result = result.and_then(|value| {
                OutputData::from_output(&output.output, value.clone())?;
                Ok(value)
            });
            if let (Some(key), Ok(data)) = (cache_key, &result) {
                let bytes = serde_json::to_vec(data)?.len();
                if data["truncated"] != json!(true) && bytes <= 1024 * 1024 {
                    let mut cache = self.projections.borrow_mut();
                    cache.remove(&key);
                    while cache.len() >= 32
                        || cache.values().map(|entry| entry.1).sum::<usize>() + bytes
                            > 8 * 1024 * 1024
                    {
                        cache.pop_first();
                    }
                    cache.insert(key, (data.clone(), bytes));
                }
            }
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
                Err(e) if request.best_effort => (
                    if e.code == "ArtifactUnavailable" || e.code == "ProjectionUnavailable" {
                        "unavailable"
                    } else {
                        "failed"
                    }
                    .into(),
                    Value::Null,
                    Some(e),
                ),
                Err(e) => return Err(e),
            };
            if error.is_none() {
                prior.insert(output.id.clone(), data.clone());
            }
            results.insert(
                index,
                OutputResult {
                    id: output.id.clone(),
                    status,
                    schema_version: 2,
                    kind: serde_json::to_value(&output.output)?
                        .as_object()
                        .unwrap()
                        .keys()
                        .next()
                        .unwrap()
                        .clone(),
                    filter_application: if matches!(output.output, Output::Artifact { .. }) {
                        "inapplicable"
                    } else {
                        "applied"
                    }
                    .into(),
                    origin: if from_cache {
                        "cache"
                    } else if matches!(
                        output.output,
                        Output::Artifact { .. }
                            | Output::Detail(Detail::Source { .. } | Detail::FunctionIr { .. })
                    ) {
                        "stored"
                    } else {
                        "derived"
                    }
                    .into(),
                    data: if error.is_none() {
                        Some(OutputData::from_output(&output.output, data)?)
                    } else {
                        None
                    },
                    error,
                },
            );
        }
        let response = Response {
            api_version: 1,
            repository: self.repository.clone(),
            revision: self.revision.clone(),
            query_hash: hash,
            matches: matches.len(),
            coverage: self.snapshot["coverage"].clone(),
            outputs: results.into_values().collect(),
            evidence: Default::default(),
            next_actions: Vec::new(),
            limits: json!({"timeout_ms":request.budget.timeout_ms,"max_visited":request.budget.max_visited,"max_response_bytes":request.budget.max_response_bytes}),
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
