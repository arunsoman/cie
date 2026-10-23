use super::query::{field_value, Dataset};
use super::*;
impl RevisionReader {
    pub(crate) fn encode(&self, value: Value) -> Result<String> {
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
        Ok(serde_json::from_slice(&bytes)?)
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
            Some(
                self.encode(json!({"type":"page","query":hash,"output":output_hash,"offset":end}))?,
            )
        } else {
            None
        };
        Ok(
            json!({"items":items[start..end],"total":items.len(),"continuation":continuation,"truncated":end<items.len()}),
        )
    }
    pub(crate) fn output(
        &self,
        o: &Output,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        hash: &str,
        ex: &mut Execution,
        prior: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        match o {
            Output::Entities { fields, page } => {
                let mut rows = Vec::new();
                for id in matches {
                    ex.check(1)?;
                    let e = &ds.entities[id];
                    if fields.is_empty() {
                        rows.push(json!(e));
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
                        rows.push(json!({"id":id,"kind":e.kind,"fields":selected}));
                    }
                }
                self.page(rows, page, hash, o)
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
                let ids: BTreeSet<_> = matches.iter().take(*max_nodes).cloned().collect();
                let edges: Vec<_> = ds
                    .edges
                    .iter()
                    .filter(|e| ids.contains(&e.source) && ids.contains(&e.target))
                    .collect();
                Ok(
                    json!({"nodes":ids.iter().map(|id|&ds.entities[id]).collect::<Vec<_>>(),"edges":edges.iter().take(*max_edges).collect::<Vec<_>>(),"truncated":ids.len()<matches.len()||edges.len()>*max_edges}),
                )
            }
            Output::Evidence { page } => {
                let rows=matches.iter().map(|id|{let e=&ds.entities[id];json!({"id":format!("evidence:{id}"),"entity":id,"revision":self.revision,"location":e.fields.get("location").or_else(||e.fields.get("loc")),"provenance":e.fields.get("provenance"),"source_kind":"captured_revision","coverage":self.snapshot["coverage"]})}).collect();
                self.page(rows, page, hash, o)
            }
            Output::Artifact { kind } => self.artifact(kind),
            Output::Detail(detail) => self.detail(detail, matches, ds, ex),
            Output::Cube(request) => self.cube(request, matches, ds, hash, o, ex),
            Output::Facets { dimensions, page } => {
                if dimensions.is_empty() || dimensions.len() > 8 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Expected 1..8 facet dimensions",
                    ));
                }
                let mut rows = Vec::new();
                for dimension in dimensions {
                    let mut counts: BTreeMap<String, (Value, usize)> = BTreeMap::new();
                    for id in matches {
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
                    for (_, (value, count)) in counts {
                        rows.push(json!({"dimension":dimension,"member":value,"count":count}));
                    }
                }
                self.page(rows, page, hash, o)
            }
            Output::View {
                kind,
                source_output,
                max_nodes,
                max_edges,
            } => {
                if let Some(source) = source_output {
                    let cube = prior.get(source).ok_or_else(|| {
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
            Detail::Source { start, end } => {
                let path = if entity.kind == EntityKind::File {
                    entity.fields.get("path")
                } else {
                    entity.fields.get("file").and_then(|f| f.get("path"))
                }
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    StorageError::new("InvalidRequest", "Source requires a file or symbol")
                })?;
                use rusqlite::OptionalExtension;
                let cache:Option<(Vec<u8>,usize,String)>=self.conn.query_row("SELECT zstd_bytes,size_bytes,content_hash FROM captured_sources WHERE path=?1",[path],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
                let (bytes, size, hash) = cache.ok_or_else(|| {
                    StorageError::new("SourceUnavailable", "Source was not retained")
                })?;
                if size > 16 * 1024 * 1024 {
                    return Err(StorageError::new(
                        "BudgetExceeded",
                        "Captured file exceeds decompression limit",
                    ));
                }
                let source = zstd::bulk::decompress(&bytes, size).map_err(std::io::Error::other)?;
                let text = String::from_utf8(source).map_err(|_| {
                    StorageError::new("CorruptData", "Captured source is not UTF-8")
                })?;
                let a = start.unwrap_or_else(|| {
                    if entity.kind == EntityKind::Symbol {
                        entity.fields["location"]["start"].as_u64().unwrap_or(0) as usize
                    } else {
                        0
                    }
                });
                let b = end.unwrap_or_else(|| {
                    if entity.kind == EntityKind::Symbol {
                        entity.fields["location"]["end"]
                            .as_u64()
                            .unwrap_or(text.len() as u64) as usize
                    } else {
                        text.len()
                    }
                });
                let slice = text.get(a..b).ok_or_else(|| {
                    StorageError::new("InvalidRequest", "Invalid UTF-8 byte range")
                })?;
                ex.check(slice.len() / 64)?;
                let line_starts: Vec<usize> = std::iter::once(0)
                    .chain(
                        text.bytes()
                            .enumerate()
                            .filter(|(_, b)| *b == b'\n')
                            .map(|(i, _)| i + 1),
                    )
                    .collect();
                Ok(
                    json!({"path":path,"content_hash":hash,"encoding":"utf-8","start":a,"end":b,"text":slice,"line_starts":line_starts,"revision":self.revision}),
                )
            }
            Detail::FunctionIr => {
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
                let calls = self.rows(
                    "SELECT * FROM calls WHERE caller_symbol_id=?1 ORDER BY stmt_index,id",
                    &[&id],
                )?;
                ex.check(blocks.len() + instructions.len() + edges.len() + calls.len())?;
                Ok(
                    json!({"symbol":entity,"blocks":blocks,"instructions":instructions,"edges":edges,"calls":calls}),
                )
            }
            Detail::Impact => self.impact(entity, ex),
        }
    }
    fn cube(
        &self,
        c: &CubeRequest,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        hash: &str,
        o: &Output,
        ex: &mut Execution,
    ) -> Result<Value> {
        if c.dimensions.len() > 8 || c.measures.len() > 32 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Cube exceeds dimension/measure limits",
            ));
        }
        if !c.rows.is_empty() || !c.columns.is_empty() {
            let layout: Vec<_> = c.rows.iter().chain(&c.columns).collect();
            let unique: BTreeSet<_> = layout.iter().copied().collect();
            if layout.len() != c.dimensions.len()
                || unique.len() != layout.len()
                || c.dimensions.iter().any(|d| !unique.contains(d))
            {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Pivot dimensions must occur exactly once",
                ));
            }
        }
        let mut groups: BTreeMap<String, (Vec<Value>, BTreeSet<String>)> = BTreeMap::new();
        for id in matches {
            ex.check(1)?;
            let e = &ds.entities[id];
            let mut combinations: Vec<Vec<Value>> = vec![vec![]];
            for dimension in &c.dimensions {
                let values = self.dimension(e, dimension, ds)?;
                let values = if values.is_empty() {
                    if c.include_unknown {
                        vec![Value::Null]
                    } else {
                        vec![]
                    }
                } else {
                    values
                };
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
                if groups.len() > 100_000 {
                    return Err(StorageError::new(
                        "BudgetExceeded",
                        "Cube group limit exceeded",
                    ));
                }
            }
        }
        let measures = if c.measures.is_empty() {
            vec![Measure::Count]
        } else {
            c.measures.clone()
        };
        let mut cells = Vec::new();
        for (_, (values, ids)) in groups {
            ex.check(1)?;
            let measures = self.measures(&measures, &ids, ds)?;
            let descriptor = self.encode(json!({"type":"cell","query":hash,"ids":ids}))?;
            cells.push(json!({"members":values,"measures":measures,"contributors":descriptor}));
        }
        let totals = self.measures(&measures, matches, ds)?;
        let mut result = self.page(cells, &c.page, hash, o)?;
        result["dimensions"] = json!(c.dimensions);
        result["layout"] = json!({"rows":c.rows,"columns":c.columns});
        result["grand_totals"] = totals;
        result["total_scope"] = json!("all_matches_before_paging");
        result["count_grain"] = json!("distinct_entity");
        result["multi_membership"] = json!("distinct_per_cell_totals_recomputed");
        Ok(result)
    }
    fn dimension(&self, e: &Entity, dimension: &str, ds: &Dataset) -> Result<Vec<Value>> {
        if dimension == "concept" || dimension == "cluster" {
            let relation = if dimension == "concept" {
                RelationKind::MemberOfConcept
            } else {
                RelationKind::MemberOfCluster
            };
            let ids: BTreeSet<_> = ds
                .edges
                .iter()
                .filter(|edge| edge.source == e.id && edge.relation == relation)
                .map(|edge| json!(edge.target).to_string())
                .collect();
            return ids
                .into_iter()
                .map(|s| Ok(serde_json::from_str(&s)?))
                .collect();
        }
        let value = field_value(e, dimension).ok_or_else(|| {
            StorageError::new("UnsupportedQuery", format!("Unknown dimension {dimension}"))
        })?;
        let values = if let Some(a) = value.as_array() {
            a.clone()
        } else if value.is_null() {
            vec![]
        } else {
            vec![value]
        };
        let mut seen = BTreeSet::new();
        Ok(values
            .into_iter()
            .filter(|v| seen.insert(v.to_string()))
            .collect())
    }
    fn measures(
        &self,
        measures: &[Measure],
        ids: &BTreeSet<String>,
        ds: &Dataset,
    ) -> Result<Value> {
        let mut out = serde_json::Map::new();
        for (i, measure) in measures.iter().enumerate() {
            let value = if matches!(measure, Measure::Count) {
                json!(ids.len())
            } else {
                let field = match measure {
                    Measure::Sum(f) | Measure::Average(f) | Measure::Min(f) | Measure::Max(f) => f,
                    _ => unreachable!(),
                };
                let mut values = Vec::new();
                for id in ids {
                    let v = field_value(&ds.entities[id], field)
                        .and_then(|v| v.as_f64())
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| {
                            StorageError::new(
                                "InvalidRequest",
                                format!("Measure {field} is not finite numeric at entity grain"),
                            )
                        })?;
                    values.push(v);
                }
                if values.is_empty() {
                    Value::Null
                } else {
                    json!(match measure {
                        Measure::Sum(_) => values.iter().sum::<f64>(),
                        Measure::Average(_) => values.iter().sum::<f64>() / values.len() as f64,
                        Measure::Min(_) => values.iter().copied().fold(f64::INFINITY, f64::min),
                        _ => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    })
                }
            };
            out.insert(format!("m{i}"), json!({"definition":measure,"value":value}));
        }
        Ok(Value::Object(out))
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
