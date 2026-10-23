use super::query::{field_value, strip_dimension_filters, Dataset};
use super::*;
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl RevisionReader {
    /// Permanent token: bound to the reader secret only. Used for cell
    /// descriptors, which stay valid as long as their revision does.
    pub(crate) fn encode(&self, value: Value) -> Result<String> {
        self.encode_token(value, None)
    }
    /// Continuation token with a declared expiry (§11).
    pub(crate) fn encode_expiring(&self, value: Value) -> Result<String> {
        self.encode_token(value, Some(CURSOR_TTL_SECS))
    }
    fn encode_token(&self, mut value: Value, ttl: Option<u64>) -> Result<String> {
        if let Some(secs) = ttl {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("exp".into(), json!(now_epoch_secs().saturating_add(secs)));
            }
        }
        let bytes = serde_json::to_vec(&value)?;
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let key = blake3::derive_key("cie.storage.cursor.v1", self.secret.as_bytes());
        let mac = blake3::keyed_hash(&key, &bytes).to_hex();
        Ok(format!("{hex}.{mac}"))
    }
    pub(crate) fn decode(&self, token: &str) -> Result<Value> {
        if token.len() > 2 * 1024 * 1024 {
            return Err(StorageError::new("CursorInvalid", "Token too large"));
        }
        let (hex, mac) = token
            .split_once('.')
            .ok_or_else(|| StorageError::new("CursorInvalid", "Malformed token"))?;
        if !hex.is_ascii() || hex.len() % 2 != 0 {
            return Err(StorageError::new("CursorInvalid", "Malformed token"));
        }
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| StorageError::new("CursorInvalid", "Malformed token"))?;
        let key = blake3::derive_key("cie.storage.cursor.v1", self.secret.as_bytes());
        if blake3::keyed_hash(&key, &bytes).to_hex().as_str() != mac {
            return Err(StorageError::new(
                "CursorInvalid",
                "Token belongs to a different reader or was modified",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if let Some(exp) = value["exp"].as_u64() {
            if now_epoch_secs() >= exp {
                return Err(StorageError::new(
                    "CursorExpired",
                    "Continuation expired; repeat the request with the original options",
                ));
            }
        }
        Ok(value)
    }
    fn page(
        &self,
        items: Vec<Value>,
        page: &PageRequest,
        hash: &str,
        output: &Output,
    ) -> Result<Value> {
        if page.limit == 0 || page.limit > self.policy.max_page_size {
            return Err(StorageError::new(
                "InvalidRequest",
                "Page size exceeds policy",
            ));
        }
        let mut normalized = output.clone();
        match &mut normalized {
            Output::Entities { page, .. }
            | Output::Relationships { page }
            | Output::Evidence { page }
            | Output::Facets { page, .. } => page.continuation = None,
            Output::Cube(c) => c.page.continuation = None,
            _ => {}
        }
        let output_hash = blake3::hash(&serde_json::to_vec(&normalized)?)
            .to_hex()
            .to_string();
        let start = if let Some(token) = &page.continuation {
            let v = self.decode(token)?;
            if v["type"] != "page" || v["query"] != hash || v["output"] != output_hash {
                return Err(StorageError::new(
                    "CursorInvalid",
                    "Continuation does not match query/output",
                ));
            }
            v["offset"]
                .as_u64()
                .ok_or_else(|| StorageError::new("CursorInvalid", "Missing offset"))?
                as usize
        } else {
            0
        };
        if start > items.len() {
            return Err(StorageError::new("CursorInvalid", "Offset outside results"));
        }
        let end = start.saturating_add(page.limit).min(items.len());
        let continuation = if end < items.len() {
            Some(self.encode_expiring(
                json!({"type":"page","query":hash,"output":output_hash,"offset":end}),
            )?)
        } else {
            None
        };
        Ok(
            json!({"items":items[start..end],"total":items.len(),"continuation":continuation,"truncated":end<items.len()}),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn output(
        &self,
        o: &Output,
        query: &Query,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        hash: &str,
        ex: &mut Execution,
        prior: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        match o {
            Output::Entities {
                fields,
                order,
                page,
            } => {
                if order.len() > 8 {
                    return Err(StorageError::new("InvalidRequest", "At most 8 sort fields"));
                }
                for sort in order {
                    let known = if matches.is_empty() {
                        // No matches to probe: accept fields known somewhere.
                        ds.entities
                            .values()
                            .any(|e| field_value(e, &sort.field).is_some())
                    } else {
                        matches
                            .iter()
                            .all(|id| field_value(&ds.entities[id], &sort.field).is_some())
                    };
                    if !known {
                        return Err(StorageError::new(
                            "UnsupportedQuery",
                            format!("Unknown sort field {}", sort.field),
                        ));
                    }
                }
                let mut rows: Vec<(Vec<Value>, Value)> = Vec::new();
                for id in matches {
                    ex.check(1)?;
                    let e = &ds.entities[id];
                    let row = if fields.is_empty() {
                        json!(e)
                    } else {
                        let mut selected = serde_json::Map::new();
                        for field in fields {
                            let value = field_value(e, field).ok_or_else(|| {
                                StorageError::new(
                                    "UnsupportedQuery",
                                    format!("Unknown field {field}"),
                                )
                            })?;
                            selected.insert(field.clone(), value);
                        }
                        json!({"id":id,"kind":e.kind,"fields":selected})
                    };
                    rows.push((
                        order
                            .iter()
                            .map(|s| field_value(e, &s.field).unwrap_or(Value::Null))
                            .collect(),
                        row,
                    ));
                }
                // Stable ordering with entity identity as the final tie-breaker.
                rows.sort_by(|(a, ae), (b, be)| {
                    for (i, sort) in order.iter().enumerate() {
                        let ord = cmp_json(&a[i], &b[i]);
                        let ord = if sort.direction == SortDirection::Desc {
                            ord.reverse()
                        } else {
                            ord
                        };
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                    ae["id"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(be["id"].as_str().unwrap_or(""))
                });
                self.page(
                    rows.into_iter().map(|(_, row)| row).collect(),
                    page,
                    hash,
                    o,
                )
            }
            Output::Relationships { page } => self.page(
                ds.edges
                    .iter()
                    .filter(|e| matches.contains(&e.source) && matches.contains(&e.target))
                    .map(|e| json!(e))
                    .collect(),
                page,
                hash,
                o,
            ),
            Output::Graph {
                max_nodes,
                max_edges,
            } => {
                if *max_nodes == 0 || *max_nodes > 5000 || *max_edges > 10000 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Graph limits exceed policy",
                    ));
                }
                let ids: Vec<_> = matches.iter().take(*max_nodes).cloned().collect();
                let ids_ref: BTreeSet<_> = ids.iter().cloned().collect();
                let edges: Vec<_> = ds
                    .edges
                    .iter()
                    .filter(|e| ids_ref.contains(&e.source) && ids_ref.contains(&e.target))
                    .collect();
                Ok(
                    json!({"nodes":ids.iter().map(|id|&ds.entities[id]).collect::<Vec<_>>(),"edges":edges.iter().take(*max_edges).collect::<Vec<_>>(),"truncated":ids_ref.len()<matches.len()||edges.len()>*max_edges}),
                )
            }
            Output::Evidence { page } => {
                let rows=matches.iter().map(|id|{let e=&ds.entities[id];json!({"id":format!("evidence:{id}"),"entity":id,"revision":self.revision,"location":e.fields.get("location").or_else(||e.fields.get("loc")),"provenance":e.fields.get("provenance"),"source_kind":"captured_revision","coverage":self.snapshot["coverage"]})}).collect();
                self.page(rows, page, hash, o)
            }
            Output::Artifact { kind } => self.artifact(kind),
            Output::Detail(detail) => self.detail(detail, matches, ds, ex),
            Output::Cube(request) => self.cube(request, matches, ds, hash, o, ex),
            Output::Facets {
                dimensions,
                self_excluding,
                page,
            } => {
                if dimensions.is_empty() || dimensions.len() > 8 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Expected 1..8 facet dimensions",
                    ));
                }
                let mut rows = Vec::new();
                for dimension in dimensions {
                    // Self-excluding facets recompute the population with only
                    // that dimension's own filters removed (§16.4).
                    let scope: BTreeSet<String> = if self_excluding.contains(dimension) {
                        let stripped = strip_dimension_filters(query, dimension)?;
                        self.evaluate(&stripped, ds, ex, 0)?
                    } else {
                        matches.clone()
                    };
                    let mut counts: BTreeMap<String, (Value, usize)> = BTreeMap::new();
                    for id in &scope {
                        ex.check(1)?;
                        let value = field_value(&ds.entities[id], dimension).ok_or_else(|| {
                            StorageError::new(
                                "UnsupportedQuery",
                                format!("Unknown dimension {dimension}"),
                            )
                        })?;
                        let values = if let Some(a) = value.as_array() {
                            a.clone()
                        } else {
                            vec![value]
                        };
                        let mut seen = BTreeSet::new();
                        for value in values {
                            let key = value.to_string();
                            if seen.insert(key.clone()) {
                                let entry = counts.entry(key).or_insert((value, 0));
                                entry.1 += 1;
                            }
                        }
                    }
                    let excluded = self_excluding.contains(dimension);
                    for (_, (value, count)) in counts {
                        rows.push(
                            json!({"dimension":dimension,"member":value,"count":count,"self_excluded":excluded}),
                        );
                    }
                }
                self.page(rows, page, hash, o)
            }
            Output::View {
                kind,
                source,
                availability,
                max_nodes,
                max_edges,
            } => {
                if let ViewSource::Output(id) = source {
                    if *availability == AvailabilityPolicy::StoredOnly {
                        return Err(StorageError::new(
                            "ProjectionUnavailable",
                            "Cube charts are derived projections; no stored prebuilt chart",
                        ));
                    }
                    let cube = prior.get(id).ok_or_else(|| {
                        StorageError::new(
                            "InvalidRequest",
                            "View source must name an earlier successful output",
                        )
                    })?;
                    if !["table", "hbar", "heatmap", "stat", "donut"].contains(&kind.as_str()) {
                        return Err(StorageError::new(
                            "UnsupportedOutput",
                            "Cube chart kind unavailable",
                        ));
                    }
                    return Ok(
                        json!({"renderer":"cie-cube","schema_version":1,"kind":kind,"cells":cube,"revision":self.revision,"coverage":self.snapshot["coverage"]}),
                    );
                }
                if kind == "relationships" {
                    if *availability == AvailabilityPolicy::StoredOnly {
                        return Err(StorageError::new(
                            "ProjectionUnavailable",
                            "Relationship views are derived from stored evidence",
                        ));
                    }
                    return self.relationship_view(matches, ds, *max_nodes, *max_edges);
                }
                let bundle = self.artifact(&ArtifactKind::ArchitectureBundle)?;
                let specs = bundle["specs"].as_object().ok_or_else(|| {
                    StorageError::new("ArtifactUnavailable", "Architecture bundle unavailable")
                })?;
                // Stored architecture views currently have repository scope; never silently broaden a selection.
                if matches.len()
                    != ds
                        .entities
                        .values()
                        .filter(|e| e.kind == EntityKind::File)
                        .count()
                    || matches
                        .iter()
                        .any(|id| ds.entities[id].kind != EntityKind::File)
                {
                    return Err(StorageError::new(
                        "ProjectionUnavailable",
                        "Stored architecture views require a whole-repository file selection",
                    ));
                }
                let selected: serde_json::Map<String, Value> = specs
                    .iter()
                    .filter(|(k, _)| k.starts_with(kind))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if selected.is_empty() {
                    return Err(StorageError::new(
                        "ProjectionUnavailable",
                        "No stored view of this kind",
                    ));
                }
                Ok(
                    json!({"renderer":"cie-polyglot","schema_version":1,"specs":selected,"coverage":self.snapshot["coverage"]}),
                )
            }
        }
    }
    fn relationship_view(
        &self,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        max_nodes: usize,
        max_edges: usize,
    ) -> Result<Value> {
        use cie_viz::*;
        if max_nodes == 0 || max_nodes > ELEMENT_CAP || max_edges > 10000 {
            return Err(StorageError::new(
                "InvalidRequest",
                "View limits exceed renderer policy",
            ));
        }
        let snapshot: cie_incremental::snapshot::RevSnapshot =
            serde_json::from_value(self.snapshot.clone())?;
        let mut spec = produce(&snapshot, &ProduceOptions::default())
            .map_err(|e| StorageError::new("ProjectionUnavailable", format!("{e:?}")))?;
        spec.elements.clear();
        spec.edges.clear();
        spec.hidden.clear();
        spec.gaps.clear();
        spec.groups = None;
        for id in matches {
            let e = &ds.entities[id];
            let mut location = e.fields.get("location").cloned();
            if e.kind == EntityKind::File {
                location = Some(
                    json!({"file":e.fields["path"],"start":0,"end":e.fields.get("size_bytes").unwrap_or(&json!(0))}),
                );
            }
            if location.is_none() {
                for edge in &ds.edges {
                    if edge.source == *id || edge.target == *id {
                        let other = if edge.source == *id {
                            &edge.target
                        } else {
                            &edge.source
                        };
                        if let Some(loc) = ds.entities[other].fields.get("location") {
                            location = Some(loc.clone());
                            break;
                        }
                    }
                }
            }
            let evidence = location.and_then(|loc| {
                Some(EvidenceRef {
                    file: loc["file"].as_str()?.into(),
                    start: loc["start"].as_u64()?,
                    end: loc["end"].as_u64()?,
                })
            });
            if spec.elements.len() >= max_nodes || evidence.is_none() {
                spec.hidden.push(HiddenCandidate {
                    id: id.clone(),
                    reason: "View cap or no captured source location".into(),
                    salience_weight: 0.0,
                    label: None,
                    kind: Some(format!("{:?}", e.kind)),
                });
                continue;
            }
            spec.elements.push(ViewElement{id:id.clone(),element_kind:format!("{:?}",e.kind).to_lowercase(),display_mode:DisplayMode::Inference,label:e.fields.get("qualified_name").or_else(||e.fields.get("label")).or_else(||e.fields.get("path")).and_then(Value::as_str).unwrap_or(id).into(),evidence:evidence.unwrap(),claim_id:Some(format!("stored:{id}")),counter_argument:None,fog_reason:None,salience:Salience{factors:vec!["QUERY_MATCH".into()],weight:1.0,why_shown:"Matched the explicit revision query; derived relationships retain analysis limitations".into()},fields:e.fields.clone(),parent_id:None});
        }
        let ids: BTreeSet<_> = spec.elements.iter().map(|e| e.id.clone()).collect();
        let mut omitted_edges = 0;
        for edge in &ds.edges {
            if ids.contains(&edge.source) && ids.contains(&edge.target) {
                if spec.edges.len() >= max_edges {
                    omitted_edges += 1;
                    continue;
                }
                let evidence = spec
                    .elements
                    .iter()
                    .find(|e| e.id == edge.source)
                    .unwrap()
                    .evidence
                    .clone();
                spec.edges.push(ViewEdge {
                    id: format!("edge:{}", spec.edges.len()),
                    source: edge.source.clone(),
                    target: edge.target.clone(),
                    edge_kind: format!("{:?}", edge.relation),
                    display_mode: DisplayMode::Inference,
                    evidence,
                    counter_argument: None,
                });
            }
        }
        let truncated = !spec.hidden.is_empty() || omitted_edges > 0;
        spec.footer = EpistemicFooter {
            facts: 0,
            inferences: spec.elements.len() as u32,
            hypotheses: 0,
            fog: 0,
            stale: 0,
            caption: "Stored analysis relationships; query scope and captured coverage apply."
                .into(),
        };
        if truncated {
            spec.gaps.push(Gap {
                id: "query-omissions".into(),
                area: "scope".into(),
                why: format!(
                    "{} nodes and {omitted_edges} edges omitted",
                    spec.hidden.len()
                ),
            });
        }
        hash::seal(&mut spec);
        gate::validate(&spec)
            .map_err(|e| StorageError::new("ProjectionUnavailable", format!("{e:?}")))?;
        Ok(
            json!({"renderer":"viz-adapter","schema_version":1,"spec":spec,"truncated":truncated,"revision":self.revision,"coverage":self.snapshot["coverage"]}),
        )
    }
    fn artifact(&self, kind: &ArtifactKind) -> Result<Value> {
        let value = match kind {
            ArtifactKind::Report => self
                .artifacts
                .get("report")
                .unwrap_or(&self.snapshot)
                .clone(),
            ArtifactKind::Concepts => self.snapshot["concepts"].clone(),
            ArtifactKind::Stack => self.snapshot["stack"].clone(),
            ArtifactKind::UiBootstrapV1 => self.artifacts.clone(),
            _ => self
                .artifacts
                .get(kind.key())
                .cloned()
                .unwrap_or(Value::Null),
        };
        if value.is_null() || value == json!({}) {
            return Err(StorageError::new(
                "ArtifactUnavailable",
                format!("{} was not captured", kind.key()),
            ));
        }
        Ok(value)
    }
    fn detail(
        &self,
        detail: &Detail,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        ex: &mut Execution,
    ) -> Result<Value> {
        if matches.len() != 1 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Detail requires exactly one entity",
            ));
        }
        let entity = &ds.entities[matches.first().unwrap()];
        match detail {
            Detail::Source {
                range,
                context_lines,
                continuation,
            } => self.source_detail(entity, range, *context_lines, continuation, ex),
            Detail::FunctionIr { include_calls } => self.ir_detail(entity, *include_calls, ex),
            Detail::Impact { availability } => {
                if *availability == AvailabilityPolicy::StoredOnly {
                    return Err(StorageError::new(
                        "ProjectionUnavailable",
                        "Impact is a bounded derived result; no stored impact artifact exists",
                    ));
                }
                self.impact(entity, ex)
            }
        }
    }
    /// §18.3 source payload: captured blob only, half-open byte ranges,
    /// context expansion that preserves the requested range, explicit
    /// WholeFile truncation with a continuation.
    fn source_detail(
        &self,
        entity: &Entity,
        range: &SourceRange,
        context_lines: u16,
        continuation: &Option<String>,
        ex: &mut Execution,
    ) -> Result<Value> {
        let path = if entity.kind == EntityKind::File {
            entity.fields.get("path")
        } else {
            entity.fields.get("file").and_then(|f| f.get("path"))
        }
        .and_then(Value::as_str)
        .ok_or_else(|| StorageError::new("InvalidRequest", "Source requires a file or symbol"))?;
        use rusqlite::OptionalExtension;
        let cache: Option<(Vec<u8>, usize, String)> = self
            .conn
            .query_row(
                "SELECT zstd_bytes,size_bytes,content_hash FROM captured_sources WHERE path=?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (bytes, size, hash) = cache
            .ok_or_else(|| StorageError::new("SourceUnavailable", "Source was not retained"))?;
        if size > 16 * 1024 * 1024 {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Captured file exceeds decompression limit",
            ));
        }
        let source = zstd::bulk::decompress(&bytes, size).map_err(std::io::Error::other)?;
        let text = String::from_utf8(source)
            .map_err(|_| StorageError::new("CorruptData", "Captured source is not UTF-8"))?;
        let len = text.len() as u64;
        const SOURCE_CHUNK: u64 = 1024 * 1024;
        // Resolve the requested range from the captured file, never live disk.
        let (a, b, whole_file) = match range {
            SourceRange::WholeFile => {
                let start = if let Some(token) = continuation {
                    let v = self.decode(token)?;
                    if v["type"] != "source" || v["path"].as_str() != Some(path) {
                        return Err(StorageError::new(
                            "CursorInvalid",
                            "Source continuation does not match this file",
                        ));
                    }
                    v["offset"].as_u64().ok_or_else(|| {
                        StorageError::new("CursorInvalid", "Missing source offset")
                    })?
                } else {
                    0
                };
                if start >= len {
                    return Err(StorageError::new(
                        "CursorInvalid",
                        "Source offset beyond file",
                    ));
                }
                let end = (start + SOURCE_CHUNK).min(len);
                (start as usize, end as usize, true)
            }
            SourceRange::Declaration => {
                let (start, end) = if entity.kind == EntityKind::Symbol {
                    let loc = &entity.fields["location"];
                    (
                        loc["start"].as_u64().unwrap_or(0),
                        loc["end"].as_u64().unwrap_or(len),
                    )
                } else {
                    (0, len)
                };
                (start as usize, end as usize, false)
            }
            SourceRange::Bytes { start, end } => {
                if *end > len || *start >= *end {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Byte range must satisfy 0 <= start < end <= file length",
                    ));
                }
                (*start as usize, *end as usize, false)
            }
        };
        // Context expansion reports the expanded returned range and preserves
        // the requested range.
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(
                text.bytes()
                    .enumerate()
                    .filter(|(_, b)| *b == b'\n')
                    .map(|(i, _)| i + 1),
            )
            .collect();
        let mut start = a;
        let mut end = b;
        if context_lines > 0 && !whole_file {
            // Expand to surrounding lines; the returned range reports the
            // expansion while the requested range is preserved above.
            let line_of = |pos: usize| {
                line_starts
                    .partition_point(|&ls| ls <= pos)
                    .saturating_sub(1)
            };
            let first_line = line_of(start);
            start = line_starts[first_line.saturating_sub(context_lines as usize)];
            let end_line = line_of(end);
            let last = end_line + context_lines as usize + 1;
            let expanded = if last < line_starts.len() {
                line_starts[last]
            } else {
                len as usize
            };
            end = expanded.max(end).min(len as usize);
        }
        let slice = text
            .get(start..end)
            .ok_or_else(|| {
                StorageError::new(
                    "InvalidRequest",
                    "Range is not aligned to UTF-8 boundaries or out of bounds",
                )
            })?
            .to_string();
        ex.check(slice.len() / 64)?;
        let requested = match range {
            SourceRange::WholeFile => json!({"whole_file":true}),
            SourceRange::Declaration => json!({"declaration":true}),
            SourceRange::Bytes { start, end } => json!({"start":start,"end":end}),
        };
        let truncated = whole_file && (end as u64) < len;
        let continuation_token = if truncated {
            Some(self.encode_expiring(json!({"type":"source","path":path,"offset":end}))?)
        } else {
            None
        };
        Ok(json!({
            "path":path,
            "content_hash":hash,
            "encoding":"utf-8",
            "requested":requested,
            "returned":{"start":start,"end":end},
            "text":slice,
            "line_starts":line_starts,
            "revision":self.revision,
            "truncated":truncated,
            "continuation":continuation_token,
        }))
    }
    /// §18.3: stored IR only; ordered blocks and instructions; never fabricate
    /// operands or call targets.
    fn ir_detail(&self, entity: &Entity, include_calls: bool, ex: &mut Execution) -> Result<Value> {
        if entity.kind != EntityKind::Symbol {
            return Err(StorageError::new("InvalidRequest", "IR requires a symbol"));
        }
        let loc = &entity.fields["location"];
        let rows=self.rows("SELECT s.id FROM symbols s JOIN files f ON f.id=s.file_id WHERE f.path=?1 AND s.start_byte=?2 AND s.end_byte=?3",&[&loc["file"].as_str().unwrap_or(""),&(loc["start"].as_u64().unwrap_or(0) as i64),&(loc["end"].as_u64().unwrap_or(0) as i64)])?;
        if rows.len() != 1 {
            return Err(StorageError::new(
                "ArtifactUnavailable",
                "No unambiguous stored IR for symbol",
            ));
        }
        let id = rows[0]["id"].as_i64().unwrap();
        let blocks = self.rows(
            "SELECT * FROM ir_blocks WHERE symbol_id=?1 ORDER BY block_index",
            &[&id],
        )?;
        let instructions=self.rows("SELECT i.* FROM ir_instructions i JOIN ir_blocks b ON b.id=i.block_id WHERE b.symbol_id=?1 ORDER BY b.block_index,i.inst_index",&[&id])?;
        let edges=self.rows("SELECT e.* FROM ir_edges e JOIN ir_blocks b ON b.id=e.from_block_id WHERE b.symbol_id=?1 ORDER BY e.from_block_id,e.to_block_id",&[&id])?;
        let calls = if include_calls {
            self.rows(
                "SELECT * FROM calls WHERE caller_symbol_id=?1 ORDER BY stmt_index,id",
                &[&id],
            )?
        } else {
            Vec::new()
        };
        ex.check(blocks.len() + instructions.len() + edges.len() + calls.len())?;
        Ok(
            json!({"symbol":entity,"blocks":blocks,"instructions":instructions,"edges":edges,"calls":calls}),
        )
    }
}

/// Canonical value ordering for sorts: numbers first, then strings, then any
/// other value by canonical form; null sorts smallest. Deterministic.
pub(crate) fn cmp_json(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Number(_) => 1,
            Value::String(_) => 2,
            _ => 3,
        }
    }
    let (ra, rb) = (rank(a), rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => match (a.as_str(), b.as_str()) {
            (Some(x), Some(y)) => x.cmp(y),
            _ => a.to_string().cmp(&b.to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// Cube (§16): slice-and-dice analytics over the outer query's population.
// ---------------------------------------------------------------------------

/// One evaluated cell: aligned member values, distinct contributor IDs and
/// post-grouping measures.
type CubeCell = (Vec<Value>, BTreeSet<String>, BTreeMap<String, Value>);

fn unknown_member() -> Value {
    json!({"$unknown": true})
}

/// Hinnant civil-date algorithms for declared timezones only.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m as i64 + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m as u32, d as u32)
}
fn format_stamp(epoch: i64, suffix: &str) -> String {
    let local = epoch;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{suffix}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}
/// Parse a restricted RFC 3339 subset or epoch number; returns epoch seconds in UTC.
fn parse_timestamp(field: Value) -> Option<i64> {
    if let Some(n) = field.as_f64() {
        return Some(n as i64);
    }
    let s = field.as_str()?;
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    let mut offset = 0i64;
    if bytes.len() > 10 {
        let t = &s[10..];
        let t = t.strip_prefix(['T', 't', ' ']).unwrap_or(t);
        let parts: Vec<&str> = t.splitn(2, ['Z', 'z', '+', '-']).collect();
        let clock: Vec<&str> = parts[0]
            .split('.')
            .next()
            .unwrap_or("")
            .split(':')
            .collect();
        if clock.is_empty() {
            return None;
        }
        hour = clock.first()?.parse().ok()?;
        minute = clock.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        second = clock.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        if t.contains('+') || t.contains('-') {
            let sign = if t.contains('-') { -1 } else { 1 };
            let idx = t.find(['+', '-'])?;
            let off = &t[idx + 1..];
            let oh: Vec<&str> = off.split(':').collect();
            let h: i64 = oh.first()?.parse().ok()?;
            let m: i64 = oh.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            offset = sign * (h * 3600 + m * 60);
        }
    }
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}
fn parse_timezone(tz: &str) -> Result<i64> {
    if tz.eq_ignore_ascii_case("UTC") || tz == "Z" {
        return Ok(0);
    }
    if tz.len() == 6 && tz.starts_with(['+', '-']) {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let h: i64 = tz[1..3].parse().map_err(|_| {
            StorageError::new(
                "UnsupportedQuery",
                "Timezone must be UTC, Z or a fixed ±HH:MM offset",
            )
        })?;
        let m: i64 = tz[4..6].parse().map_err(|_| {
            StorageError::new(
                "UnsupportedQuery",
                "Timezone must be UTC, Z or a fixed ±HH:MM offset",
            )
        })?;
        if h <= 23 && m <= 59 {
            return Ok(sign * (h * 3600 + m * 60));
        }
    }
    Err(StorageError::new(
        "UnsupportedQuery",
        "Only declared UTC/fixed-offset timezones are supported; IANA names would not be silent-safe here",
    ))
}

impl RevisionReader {
    fn cube(
        &self,
        c: &CubeRequest,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        hash: &str,
        o: &Output,
        ex: &mut Execution,
    ) -> Result<Value> {
        if c.dimensions.len() > 8 {
            return Err(StorageError::new(
                "InvalidRequest",
                "At most 8 cube dimensions",
            ));
        }
        if c.measures.len() > 32 {
            return Err(StorageError::new(
                "InvalidRequest",
                "At most 32 cube measures",
            ));
        }
        // Dimension IDs: nonempty and unique.
        let mut ids = BTreeSet::new();
        for d in &c.dimensions {
            if d.id.is_empty() || !ids.insert(&d.id) {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Dimension IDs must be unique and nonempty",
                ));
            }
            if d.membership == DimensionMembership::IncludeAncestors && d.level.is_none() {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "IncludeAncestors requires a declared hierarchy level",
                ));
            }
            if let Some(level) = d.level {
                if !matches!(&d.dimension, DimensionRef::Field(f) if matches!(f.as_str(), "file.path"|"path"))
                {
                    return Err(StorageError::new(
                        "UnsupportedQuery",
                        "Only file path dimensions declare a directory hierarchy",
                    ));
                }
                if !matches!(level, HierarchyLevel::Directory) {
                    return Err(StorageError::new(
                        "UnsupportedQuery",
                        "Unsupported hierarchy level",
                    ));
                }
            }
            self.validate_dimension(&d.dimension, ds)?;
        }
        let measures = if c.measures.is_empty() {
            vec![NamedMeasure {
                id: "count".into(),
                measure: Measure::Count,
            }]
        } else {
            c.measures.clone()
        };
        let measure_ids: BTreeSet<_> = measures.iter().map(|m| m.id.clone()).collect();
        if measure_ids.len() != measures.len() {
            return Err(StorageError::new(
                "InvalidRequest",
                "Measure IDs must be unique and nonempty",
            ));
        }
        if let Some(filter) = &c.cell_filter {
            for id in measure_ids_in(filter) {
                if !measure_ids.contains(&id) {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        format!("cell_filter references unknown measure {id}"),
                    ));
                }
            }
        }
        for sort in &c.order {
            match &sort.by {
                CellSortKey::Measure(id) if !measure_ids.contains(id) => {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        format!("order references unknown measure {id}"),
                    ))
                }
                CellSortKey::Dimension(id) if !ids.contains(id) => {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        format!("order references unknown dimension {id}"),
                    ))
                }
                _ => {}
            }
        }
        // Pivot layout: each selected dimension occurs exactly once.
        let (row_dims, column_dims) = match &c.layout {
            CubeLayout::Cells => (Vec::new(), Vec::new()),
            CubeLayout::Pivot { rows, columns } => {
                let mut seen = BTreeSet::new();
                for id in rows.iter().chain(columns) {
                    if id.is_empty() || !ids.contains(id) || !seen.insert(id) {
                        return Err(StorageError::new(
                            "InvalidRequest",
                            "Pivot dimensions must be declared and unique",
                        ));
                    }
                }
                if seen.len() != c.dimensions.len() {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Pivot dimensions must cover every selected dimension exactly once",
                    ));
                }
                (rows.clone(), columns.clone())
            }
        };
        // Group the population. Every entity contributes once per distinct
        // member combination; multi-valued dimensions may overlap cells.
        let mut groups: BTreeMap<String, (Vec<Value>, BTreeSet<String>)> = BTreeMap::new();
        for id in matches {
            ex.check(1)?;
            let e = &ds.entities[id];
            let mut combinations: Vec<Vec<Value>> = vec![vec![]];
            for selection in &c.dimensions {
                let mut values = self.dimension_members(selection, e, ds, ex)?;
                if values.is_empty() {
                    if c.missing == MissingMemberPolicy::IncludeUnknown {
                        values = vec![unknown_member()];
                    } else {
                        continue;
                    }
                }
                let mut next = Vec::new();
                for prefix in &combinations {
                    for value in &values {
                        ex.check(1)?;
                        let mut p = prefix.clone();
                        p.push(value.clone());
                        next.push(p);
                        if next.len() > 100_000 {
                            return Err(StorageError::new(
                                "BudgetExceeded",
                                "Cube fanout exceeded",
                            ));
                        }
                    }
                }
                combinations = next;
            }
            for values in combinations {
                let key = serde_json::to_string(&values)?;
                let entry = groups.entry(key).or_insert((values, BTreeSet::new()));
                entry.1.insert(id.clone());
            }
        }
        if groups.len() > 1_000_000 {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Evaluated group limit exceeded; narrow a dimension",
            ));
        }
        // Measures per cell first so HAVING failures propagate loudly, then
        // post-grouping filtering (HAVING).
        let mut cells: Vec<CubeCell> = groups
            .into_iter()
            .map(|(_, (members, ids))| {
                let m = self.measure_map(&measures, &ids, ds)?;
                Ok((members, ids, m))
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(filter) = &c.cell_filter {
            cells.retain(|(_, _, m)| self.having(filter, m));
        }
        // Explicit top-K ordering, applied back-to-front as stable passes so
        // combined keys order lexicographically. Entity identity (canonical
        // members) is the final tie-breaker via the deterministic base order.
        let canonical =
            |members: &[Value]| serde_json::to_string(&json!(members)).unwrap_or_default();
        cells.sort_by_key(|(a, _, _)| canonical(a));
        let dim_index = |id: &str| c.dimensions.iter().position(|d| d.id == id);
        for sort in c.order.iter().rev() {
            cells.sort_by(|(a, _, ma), (b, _, mb)| {
                let ord = match &sort.by {
                    CellSortKey::Measure(id) => cmp_measure(ma.get(id), mb.get(id)),
                    CellSortKey::Dimension(id) => match dim_index(id) {
                        Some(i) => cmp_json(&a[i], &b[i]),
                        None => Ordering::Equal,
                    },
                };
                if sort.direction == SortDirection::Desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        // Assemble cells; contributors descriptors are bound to this query and
        // revision. Members are keyed by dimension ID.
        let aligned = |members: &Value| -> Vec<Value> {
            c.dimensions
                .iter()
                .map(|d| members.get(&d.id).cloned().unwrap_or(Value::Null))
                .collect()
        };
        let contributors: BTreeMap<String, &BTreeSet<String>> = cells
            .iter()
            .map(|(members, ids, _)| (canonical(members), ids))
            .collect();
        let rows: Vec<Value> = cells
            .iter()
            .map(|(members, ids, m)| {
                let mut map = serde_json::Map::new();
                for (selection, value) in c.dimensions.iter().zip(members) {
                    map.insert(selection.id.clone(), value.clone());
                }
                let members = Value::Object(map.clone());
                let mut cell = json!({"members":members,"measures":m});
                if c.drill_through == DrillThroughPolicy::Enabled {
                    cell["contributors"] =
                        json!(self.encode(json!({"type":"cell","query":hash,"ids":ids}))?);
                }
                if !row_dims.is_empty() || !column_dims.is_empty() {
                    cell["row_key"] = json!(restricted_members(&members, &row_dims).to_string());
                    cell["column_key"] =
                        json!(restricted_members(&members, &column_dims).to_string());
                }
                Ok(cell)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut result = self.page(rows, &c.page, hash, o)?;
        // Dimension and measure dictionaries.
        result["dimensions"] = json!(c
            .dimensions
            .iter()
            .map(|d| {
                let mut entry =
                    json!({"id":d.id,"dimension":d.dimension,"membership":d.membership});
                if let Some(level) = d.level {
                    entry["level"] = json!(level);
                }
                entry
            })
            .collect::<Vec<_>>());
        result["measures"] = Value::Object(
            measures
                .iter()
                .map(|m| {
                    let additivity = match &m.measure {
                        Measure::Count => "non_additive",
                        Measure::Sum(_) => "additive",
                        Measure::Average(_) => "non_additive",
                        Measure::Min(_) | Measure::Max(_) => "semi_additive",
                    };
                    (
                        m.id.clone(),
                        json!({"definition":m.measure,"additivity":additivity,"grain":"distinct_contributor"}),
                    )
                })
                .collect(),
        );
        result["layout"] = json!(&c.layout);
        if !row_dims.is_empty() || !column_dims.is_empty() {
            let mut row_keys = BTreeSet::new();
            let mut column_keys = BTreeSet::new();
            for (members, _, _) in &cells {
                let mut map = serde_json::Map::new();
                for (selection, value) in c.dimensions.iter().zip(members) {
                    map.insert(selection.id.clone(), value.clone());
                }
                let members = Value::Object(map);
                row_keys.insert(restricted_members(&members, &row_dims).to_string());
                column_keys.insert(restricted_members(&members, &column_dims).to_string());
            }
            result["pivot"] = json!({"rows":row_keys,"columns":column_keys});
        }
        // Grand totals default to the complete pre-HAVING population; page
        // totals, when requested, cover only the displayed cells.
        if c.totals.grand {
            result["grand_totals"] = json!(self.measure_map(&measures, matches, ds)?);
            result["total_scope"] = json!("all_matches_before_having");
        }
        if c.totals.page {
            let mut page_ids = BTreeSet::new();
            if let Some(items) = result["items"].as_array() {
                for item in items {
                    let members = item["members"].clone();
                    if let Some(ids) = contributors.get(&canonical(&aligned(&members))) {
                        page_ids.extend(ids.iter().cloned());
                    }
                }
            }
            result["page_totals"] = json!(self.measure_map(&measures, &page_ids, ds)?);
            result["page_total_scope"] = json!("displayed_page");
        }
        result["coverage"] = self.snapshot["coverage"].clone();
        Ok(result)
    }
    fn validate_dimension(&self, d: &DimensionRef, ds: &Dataset) -> Result<()> {
        match d {
            DimensionRef::Field(field) => {
                if !ds
                    .entities
                    .values()
                    .any(|e| field_value(e, field).is_some())
                {
                    return Err(StorageError::new(
                        "UnsupportedQuery",
                        format!("Unknown dimension {field}"),
                    ));
                }
                Ok(())
            }
            DimensionRef::RelatedEntity {
                relation_path,
                attribute: _,
            } => {
                if relation_path.is_empty() || relation_path.len() > 4 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Related dimension paths are 1..4 steps",
                    ));
                }
                Ok(())
            }
            DimensionRef::NumericBucket { boundaries, .. } => {
                if boundaries.is_empty() || boundaries.len() > 64 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Numeric buckets need 1..64 boundaries",
                    ));
                }
                for b in boundaries {
                    if !b.is_finite() {
                        return Err(StorageError::new(
                            "InvalidRequest",
                            "Bucket boundaries must be finite numbers",
                        ));
                    }
                }
                if boundaries.windows(2).any(|w| w[0] >= w[1]) {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Bucket boundaries must be strictly ascending",
                    ));
                }
                Ok(())
            }
            DimensionRef::TimeBucket {
                interval, timezone, ..
            } => {
                let (unit_secs, label) = match interval.unit {
                    TimeUnit::Seconds => (1u64, "second"),
                    TimeUnit::Minutes => (60u64, "minute"),
                    TimeUnit::Hours => (3600u64, "hour"),
                    TimeUnit::Days => (86_400u64, "day"),
                };
                if interval.count == 0 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Time interval count must be positive",
                    ));
                }
                let span = unit_secs.saturating_mul(interval.count);
                if span == 0 || span > 31_622_400 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Time interval must be at most one (leap) year",
                    ));
                }
                let _ = label;
                parse_timezone(timezone).map(|_| ())
            }
        }
    }
    /// Members of one dimension for one entity. Missing values yield an empty
    /// list; the request's missing-member policy decides the reserved member.
    fn dimension_members(
        &self,
        selection: &DimensionSelection,
        e: &Entity,
        ds: &Dataset,
        ex: &mut Execution,
    ) -> Result<Vec<Value>> {
        ex.check(1)?;
        let values: Vec<Value> = match &selection.dimension {
            DimensionRef::Field(field) => {
                let raw = field_value(e, field).unwrap_or(Value::Null);
                flatten_values(&raw)
            }
            DimensionRef::RelatedEntity {
                relation_path,
                attribute,
            } => {
                // Follow the declared relation path; cycles are visited once.
                let mut frontier = vec![e.id.clone()];
                let mut visited: BTreeSet<String> = frontier.iter().cloned().collect();
                for step in relation_path {
                    let mut next = Vec::new();
                    for edge in &ds.edges {
                        ex.check(1)?;
                        if edge.relation != step.relation {
                            continue;
                        }
                        let (a, b) = if step.incoming {
                            (&edge.target, &edge.source)
                        } else {
                            (&edge.source, &edge.target)
                        };
                        if frontier.contains(a) && !visited.contains(b) {
                            visited.insert(b.clone());
                            next.push(b.clone());
                        }
                    }
                    frontier = next;
                    if frontier.is_empty() {
                        break;
                    }
                }
                let mut values = Vec::new();
                for id in frontier {
                    if let Some(target) = ds.entities.get(&id) {
                        if let Some(v) = field_value(target, attribute) {
                            if !v.is_null() {
                                values.push(v);
                            }
                        }
                    }
                }
                flatten_values(&Value::Array(values))
            }
            DimensionRef::NumericBucket { field, boundaries } => {
                match field_value(e, field).and_then(|v| v.as_f64()) {
                    Some(v) if v.is_finite() => vec![json!(bucket_label(v, boundaries))],
                    _ => Vec::new(),
                }
            }
            DimensionRef::TimeBucket {
                field,
                interval,
                timezone,
            } => match field_value(e, field).and_then(parse_timestamp) {
                Some(epoch) => {
                    let offset = parse_timezone(timezone)?;
                    let unit_secs = match interval.unit {
                        TimeUnit::Seconds => 1i64,
                        TimeUnit::Minutes => 60i64,
                        TimeUnit::Hours => 3600i64,
                        TimeUnit::Days => 86_400i64,
                    };
                    let span = unit_secs * interval.count as i64;
                    let shifted = epoch + offset;
                    let start_shifted = shifted.div_euclid(span) * span;
                    let start_utc = start_shifted - offset;
                    let suffix = if offset == 0 {
                        "Z".to_string()
                    } else {
                        let sign = if offset < 0 { '-' } else { '+' };
                        let abs = offset.abs();
                        format!("{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
                    };
                    vec![json!(format_stamp(start_utc, &suffix))]
                }
                None => Vec::new(),
            },
        };
        // Declared directory roll-up for path dimensions.
        if let (Some(HierarchyLevel::Directory), DimensionRef::Field(field)) =
            (selection.level, &selection.dimension)
        {
            if !matches!(field.as_str(), "file.path" | "path") {
                return Ok(values);
            }
            let mut rolled = Vec::new();
            for value in &values {
                let path = value.as_str().unwrap_or("");
                let dirs = directory_chain(path, selection.membership);
                for dir in dirs {
                    rolled.push(json!(dir));
                }
            }
            return Ok(dedup_values(rolled));
        }
        Ok(dedup_values(values))
    }
    fn measure_map(
        &self,
        measures: &[NamedMeasure],
        ids: &BTreeSet<String>,
        ds: &Dataset,
    ) -> Result<BTreeMap<String, Value>> {
        let mut out = BTreeMap::new();
        for m in measures {
            let value = match &m.measure {
                Measure::Count => json!(ids.len()),
                Measure::Sum(f) | Measure::Average(f) | Measure::Min(f) | Measure::Max(f) => {
                    let mut values = Vec::new();
                    for id in ids {
                        let v = field_value(&ds.entities[id], f)
                            .and_then(|v| v.as_f64())
                            .filter(|n| n.is_finite())
                            .ok_or_else(|| {
                                StorageError::new(
                                    "InvalidRequest",
                                    format!("Measure {f} is not finite numeric at entity grain"),
                                )
                            })?;
                        values.push(v);
                    }
                    if values.is_empty() {
                        Value::Null
                    } else {
                        json!(match &m.measure {
                            Measure::Sum(_) => values.iter().sum::<f64>(),
                            Measure::Average(_) => {
                                values.iter().sum::<f64>() / values.len() as f64
                            }
                            Measure::Min(_) => {
                                values.iter().copied().fold(f64::INFINITY, f64::min)
                            }
                            _ => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        })
                    }
                }
            };
            out.insert(m.id.clone(), value);
        }
        Ok(out)
    }
    fn having(&self, p: &MeasurePredicate, m: &BTreeMap<String, Value>) -> bool {
        match p {
            MeasurePredicate::Measure { id, op, value } => {
                let actual = m.get(id).and_then(Value::as_f64);
                match (actual, op) {
                    (Some(a), CompareOp::Eq) => a == *value,
                    (Some(a), CompareOp::Gt) => a > *value,
                    (Some(a), CompareOp::Ge) => a >= *value,
                    (Some(a), CompareOp::Lt) => a < *value,
                    (Some(a), CompareOp::Le) => a <= *value,
                    _ => false,
                }
            }
            MeasurePredicate::And(ps) => ps.iter().all(|p| self.having(p, m)),
            MeasurePredicate::Or(ps) => ps.iter().any(|p| self.having(p, m)),
            MeasurePredicate::Not(p) => !self.having(p, m),
        }
    }
    fn impact(&self, entity: &Entity, ex: &mut Execution) -> Result<Value> {
        if entity.kind != EntityKind::Symbol {
            return Err(StorageError::new(
                "InvalidRequest",
                "Impact requires a symbol",
            ));
        }
        let snapshot: cie_incremental::snapshot::RevSnapshot =
            serde_json::from_value(self.snapshot.clone())
                .map_err(|e| StorageError::new("CorruptData", e.to_string()))?;
        let n = entity
            .fields
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                StorageError::new("ArtifactUnavailable", "Impact requires core symbol mapping")
            })? as u32;
        let id = cie_core::types::SymbolId(n);
        let store = cie_semantic::facts::SymbolStore::build(snapshot.symbols.clone());
        if store.get(id).is_none() {
            return Err(StorageError::new(
                "ArtifactUnavailable",
                "Missing symbol in analysis snapshot",
            ));
        }
        let mut callees = BTreeMap::new();
        let mut reverse: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for summary in &snapshot.summaries {
            ex.check(summary.callees.len() + 1)?;
            callees.insert(summary.sym, summary.callees.clone());
            for callee in &summary.callees {
                reverse.entry(*callee).or_default().insert(summary.sym);
            }
        }
        let map = cie_analysis::testing::build_test_map(&store, &callees);
        let mut affected = cie_analysis::testing::affected_by_change(&reverse, &[id]);
        affected.insert(id);
        let tests = cie_analysis::testing::minimum_test_set(
            &map,
            &affected.iter().copied().collect::<Vec<_>>(),
        );
        ex.check(affected.len() + tests.len())?;
        let names = |ids: &BTreeSet<cie_core::types::SymbolId>| {
            ids.iter()
                .filter_map(|id| store.get(*id).map(|s| s.qual_name.clone()))
                .collect::<Vec<_>>()
        };
        Ok(
            json!({"symbol":store.get(id).unwrap().qual_name,"affected_symbols":names(&affected),"minimum_test_set":tests.iter().filter_map(|id|store.get(*id).map(|s|s.qual_name.clone())).collect::<Vec<_>>(),"affected_concepts":snapshot.concepts.nodes.iter().filter(|c|c.symbols.iter().any(|s|affected.contains(s))).map(|c|c.label.clone()).collect::<Vec<_>>(),"provenance":"cie-analysis::testing","coverage":snapshot.coverage}),
        )
    }
    pub fn diagnostics(&self, dataset: Option<&str>, limit: usize, offset: usize) -> Result<Value> {
        const TABLES: &[&str] = &[
            "files",
            "symbols",
            "ast_nodes",
            "node_kinds",
            "uast_kinds",
            "calls",
            "ir_blocks",
            "ir_instructions",
            "ir_edges",
            "framework_roles",
            "concept_nodes",
            "c4_relationships",
            "state_machines",
            "workflows",
            "workflow_nodes",
            "duplicates",
            "duplicate_clusters",
            "stage_status",
        ];
        if let Some(table) = dataset {
            if !TABLES.contains(&table)
                || limit == 0
                || limit > self.policy.max_page_size
                || offset > 100_000
            {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Unknown dataset or invalid diagnostic page",
                ));
            }
            let exists: i64 = self.conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(StorageError::new(
                    "ArtifactUnavailable",
                    "Dataset not captured",
                ));
            }
            let mut rows = self.rows(
                &format!("SELECT * FROM \"{table}\" ORDER BY rowid LIMIT ?1 OFFSET ?2"),
                &[&(limit as i64), &(offset as i64)],
            )?;
            for row in &mut rows {
                if let Some(obj) = row.as_object_mut() {
                    for (k, v) in obj.iter_mut() {
                        if ["secret", "password", "token", "source", "text", "value"]
                            .iter()
                            .any(|s| k.to_lowercase().contains(s))
                        {
                            *v = json!({"redacted":true});
                        } else if let Some(s) = v.as_str() {
                            if s.len() > 2000 {
                                *v = json!({"text":s.chars().take(500).collect::<String>(),"truncated":true});
                            }
                        }
                    }
                }
            }
            let count: i64 =
                self.conn
                    .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
                        r.get(0)
                    })?;
            Ok(
                json!({"dataset":table,"schema_version":1,"rows":rows,"total":count,"revision":self.revision}),
            )
        } else {
            let rows = self.rows(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                &[],
            )?;
            Ok(
                json!({"datasets":rows.into_iter().filter(|r|r["name"].as_str().map(|s|TABLES.contains(&s)).unwrap_or(false)).collect::<Vec<_>>(),"revision":self.revision}),
            )
        }
    }
}

fn flatten_values(raw: &Value) -> Vec<Value> {
    match raw {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        other => vec![other.clone()],
    }
}
fn dedup_values(values: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for v in values {
        if v.is_null() {
            continue;
        }
        let key = v.to_string();
        if seen.insert(key) {
            out.push(v);
        }
    }
    out
}
/// Directory roll-up. RecordedOnly returns the parent; IncludeAncestors adds
/// every ancestor directory up to the root (roll-up dedupes entities).
fn directory_chain(path: &str, membership: DimensionMembership) -> Vec<String> {
    let mut dirs: Vec<String> = path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("")
        .split('/')
        .filter(|p| !p.is_empty())
        .scan(String::new(), |acc, part| {
            if acc.is_empty() {
                *acc = part.to_string();
            } else {
                *acc = format!("{acc}/{part}");
            }
            Some(acc.clone())
        })
        .collect();
    if membership == DimensionMembership::IncludeAncestors {
        // Already ascending: a, a/b, ... Add the root.
        dirs.push(String::new());
    }
    dirs.into_iter()
        .map(|d| if d.is_empty() { ".".to_string() } else { d })
        .collect()
}
fn bucket_label(value: f64, boundaries: &[f64]) -> String {
    if value < boundaries[0] {
        return format!("<{}", boundaries[0]);
    }
    for w in boundaries.windows(2) {
        if value >= w[0] && value < w[1] {
            return format!("[{},{})", w[0], w[1]);
        }
    }
    format!(">={}", boundaries[boundaries.len() - 1])
}
fn measure_ids_in(p: &MeasurePredicate) -> Vec<String> {
    match p {
        MeasurePredicate::Measure { id, .. } => vec![id.clone()],
        MeasurePredicate::And(ps) | MeasurePredicate::Or(ps) => {
            ps.iter().flat_map(measure_ids_in).collect()
        }
        MeasurePredicate::Not(p) => measure_ids_in(p),
    }
}
/// Measure ordering: values compare numerically; missing values sort last.
fn cmp_measure(a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match (a.and_then(Value::as_f64), b.and_then(Value::as_f64)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
fn restricted_members(members: &Value, keep: &[String]) -> Value {
    match members {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if keep.contains(k) {
                    out.insert(k.clone(), v.clone());
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}
