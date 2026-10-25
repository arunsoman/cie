use super::*;
pub(crate) struct Dataset {
    pub entities: BTreeMap<String, Entity>,
    pub edges: Vec<Edge>,
}
impl RevisionReader {
    pub(crate) fn id(&self, kind: &str, id: impl std::fmt::Display) -> String {
        format!("{}:{}:{kind}:{id}", &self.repository[..16], self.revision)
    }
    pub(crate) fn dataset(&self, ex: &mut Execution) -> Result<Dataset> {
        self.dataset_with_duplicates(ex, false, false)
    }
    pub(crate) fn dataset_with_duplicates(
        &self,
        ex: &mut Execution,
        duplicates: bool,
        domains: bool,
    ) -> Result<Dataset> {
        let mut ds = Dataset {
            entities: BTreeMap::new(),
            edges: Vec::new(),
        };
        let limit = (ex.max_visited + 1) as i64;
        let files=self.rows("SELECT id,path,language,size_bytes,lower(hex(content_hash)) AS content_hash FROM files ORDER BY id LIMIT ?1",&[&limit])?;
        let mut paths = BTreeMap::new();
        for f in &files {
            let id = self.id("file", f["id"].as_i64().unwrap());
            paths.insert(f["path"].as_str().unwrap().to_string(), id.clone());
            add(&mut ds, ex, EntityKind::File, id, f.clone())?;
        }
        let core_files = self.snapshot["files"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // File identity covers semantic-only languages too.
        for (i, path) in core_files.iter().enumerate() {
            if let Some(path) = path.as_str() {
                if !paths.contains_key(path) {
                    let id = self.id("semantic_file", i);
                    paths.insert(path.into(), id.clone());
                    add(
                        &mut ds,
                        ex,
                        EntityKind::File,
                        id,
                        json!({"path":path,"language":"unknown"}),
                    )?;
                }
            }
        }
        let mut core = BTreeMap::new();
        let mut spans = BTreeMap::new();
        for sym in self.snapshot["symbols"].as_array().into_iter().flatten() {
            let n = sym["id"]
                .as_u64()
                .ok_or_else(|| StorageError::new("CorruptData", "Symbol ID"))?;
            let id = self.id("symbol", n);
            core.insert(n, id.clone());
            let mut value = sym.clone();
            let fi = sym["loc"]["file"].as_u64().unwrap_or(u64::MAX) as usize;
            let path = core_files.get(fi).and_then(Value::as_str).unwrap_or("");
            value["file"] = json!({"path":path});
            value["qualified_name"] = sym["qual_name"].clone();
            value["location"] =
                json!({"file":path,"start":sym["loc"]["start"],"end":sym["loc"]["end"]});
            value["language"] = files
                .iter()
                .find(|f| f["path"] == path)
                .map(|f| f["language"].clone())
                .unwrap_or(Value::Null);
            spans.insert(
                (
                    path.to_string(),
                    sym["loc"]["start"].as_u64(),
                    sym["loc"]["end"].as_u64(),
                ),
                id.clone(),
            );
            if let Some(fid) = paths.get(path) {
                ds.edges.push(Edge {
                    source: id.clone(),
                    target: fid.clone(),
                    relation: RelationKind::DefinedIn,
                });
            }
            add(&mut ds, ex, EntityKind::Symbol, id, value)?;
        }
        for (kind, key, tag) in [
            (EntityKind::Summary, "summaries", "summary"),
            (EntityKind::Contract, "contracts", "contract"),
            (EntityKind::Finding, "findings", "finding"),
        ] {
            for (i, record) in self.snapshot[key]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let id = self.id(tag, i);
                let mut value = record.clone();
                let n = record["sym"].as_u64().or_else(|| record["symbol"].as_u64());
                if let Some(sid) = n.and_then(|n| core.get(&n)) {
                    value["symbol_ref"] = json!(sid);
                    if let Some(symbol) = ds.entities.get(sid) {
                        value["file"] = symbol.fields.get("file").cloned().unwrap_or(Value::Null);
                    }
                    let relation = if kind == EntityKind::Summary {
                        RelationKind::Summarizes
                    } else {
                        RelationKind::Constrains
                    };
                    ds.edges.push(Edge {
                        source: id.clone(),
                        target: sid.clone(),
                        relation,
                    });
                }
                add(&mut ds, ex, kind, id, value)?;
            }
        }
        for node in self.snapshot["concepts"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let n = node["id"]
                .as_u64()
                .ok_or_else(|| StorageError::new("CorruptData", "Concept ID"))?;
            let id = self.id("concept", n);
            for member in node["symbols"].as_array().into_iter().flatten() {
                if let Some(sid) = member.as_u64().and_then(|n| core.get(&n)) {
                    ds.edges.push(Edge {
                        source: sid.clone(),
                        target: id.clone(),
                        relation: RelationKind::MemberOfConcept,
                    });
                }
            }
            if let Some(parent) = node["parent"].as_u64() {
                ds.edges.push(Edge {
                    source: id.clone(),
                    target: self.id("concept", parent),
                    relation: RelationKind::ChildConceptOf,
                });
            }
            for extra in node["extra_edges"].as_array().into_iter().flatten() {
                if let Some(parent) = extra[0].as_u64() {
                    ds.edges.push(Edge {
                        source: id.clone(),
                        target: self.id("concept", parent),
                        relation: RelationKind::ChildConceptOf,
                    });
                }
            }
            add(&mut ds, ex, EntityKind::Concept, id, node.clone())?;
        }
        for summary in self.snapshot["summaries"].as_array().into_iter().flatten() {
            if let Some(source) = summary["sym"].as_u64().and_then(|n| core.get(&n)) {
                let declared: BTreeSet<u64> = summary["callees"].as_array().into_iter().flatten().filter_map(Value::as_u64)
                    .chain(summary["call_sites"].as_array().into_iter().flatten().flat_map(|c|c["target_symbols"].as_array().into_iter().flatten()).filter_map(Value::as_u64)).collect();
                for target in declared.iter().filter_map(|id|core.get(id)) {
                    ds.edges.push(Edge{source:source.clone(),target:target.clone(),relation:RelationKind::DeclaredCalls});
                }
                for callee in summary["callees"].as_array().into_iter().flatten() {
                    if let Some(target) = callee.as_u64().and_then(|n| core.get(&n)) {
                        ds.edges.push(Edge {
                            source: source.clone(),
                            target: target.clone(),
                            relation: RelationKind::Calls,
                        });
                    }
                }
            }
        }
        // Map independent polyglot IDs by captured location, never numeric equality.
        let sql="SELECT s.id,f.path,s.name,s.qual_name AS qualified_name,s.kind,s.start_byte,s.end_byte FROM symbols s JOIN files f ON f.id=s.file_id ORDER BY s.id LIMIT ?1";
        let mut poly = BTreeMap::new();
        for sym in self.rows(sql, &[&limit])? {
            let n = sym["id"].as_i64().unwrap();
            let key = (
                sym["path"].as_str().unwrap().to_string(),
                sym["start_byte"].as_u64(),
                sym["end_byte"].as_u64(),
            );
            let id = spans
                .get(&key)
                .cloned()
                .unwrap_or_else(|| self.id("poly_symbol", n));
            poly.insert(n, id.clone());
            if !ds.entities.contains_key(&id) {
                let mut value = sym.clone();
                value["file"] = json!({"path":sym["path"]});
                value["location"] =
                    json!({"file":sym["path"],"start":sym["start_byte"],"end":sym["end_byte"]});
                if let Some(fid) = sym["path"].as_str().and_then(|p| paths.get(p)) {
                    ds.edges.push(Edge {
                        source: id.clone(),
                        target: fid.clone(),
                        relation: RelationKind::DefinedIn,
                    });
                }
                add(&mut ds, ex, EntityKind::Symbol, id, value)?;
            }
        }
        if duplicates {
            for row in self.rows("SELECT * FROM duplicates ORDER BY id LIMIT ?1", &[&limit])? {
                let id = self.id("duplicate_pair", row["id"].as_i64().unwrap());
                for (field, relation) in [
                    ("symbol_a", RelationKind::LeftSymbol),
                    ("symbol_b", RelationKind::RightSymbol),
                    ("canonical_symbol_id", RelationKind::CanonicalSymbol),
                ] {
                    if let Some(sid) = row[field].as_i64().and_then(|n| poly.get(&n)) {
                        ds.edges.push(Edge {
                            source: id.clone(),
                            target: sid.clone(),
                            relation,
                        });
                    }
                }
                add(&mut ds, ex, EntityKind::DuplicatePair, id, row)?;
            }
            let mut clusters = BTreeSet::new();
            for row in self.rows(
                "SELECT * FROM duplicate_clusters ORDER BY cluster_id,symbol_id LIMIT ?1",
                &[&limit],
            )? {
                ex.check(1)?;
                let n = row["cluster_id"].as_i64().unwrap();
                let id = self.id("cluster", n);
                clusters.insert((n, id.clone()));
                if let Some(sid) = row["symbol_id"].as_i64().and_then(|n| poly.get(&n)) {
                    ds.edges.push(Edge {
                        source: sid.clone(),
                        target: id,
                        relation: RelationKind::MemberOfCluster,
                    });
                }
            }
            for (n, id) in clusters {
                add(
                    &mut ds,
                    ex,
                    EntityKind::Cluster,
                    id,
                    json!({"cluster_id":n,"algorithm":"exact_duplicate_union_find","tier":"canonical"}),
                )?;
            }
        }
        if domains {
            // Project captured domain facts into the same identity/relationship space.
            // SQL identifiers are fixed here, never supplied by callers.
            for (kind, key, tag) in [
                (EntityKind::Route, "routes", "route"),
                (EntityKind::EntryPoint, "mains", "entry_point"),
                (EntityKind::Framework, "frameworks", "framework"),
            ] {
                for (index, record) in self.snapshot["stack"][key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let mut value = record.clone();
                    let path = record["rel"].as_str().or_else(|| {
                        record["evidence"]
                            .as_str()
                            .filter(|p| paths.contains_key(*p))
                    });
                    let id = self.id(tag, index);
                    if let Some(path) = path {
                        value["file"] = json!({"path":path});
                        value["location"] = json!({"file":path,"line":record["line"]});
                        if let Some(file) = paths.get(path) {
                            ds.edges.push(Edge {
                                source: id.clone(),
                                target: file.clone(),
                                relation: RelationKind::DefinedIn,
                            });
                        }
                    }
                    if matches!(kind, EntityKind::Route | EntityKind::EntryPoint) {
                        let candidates = entry_candidates(&ds, record, kind);
                        value["handler_resolution"] = json!(match candidates.len() {
                            0 => "unresolved",
                            1 => "unique",
                            _ => "ambiguous",
                        });
                        value["handler_candidate_count"] = json!(candidates.len());
                        if candidates.len() == 1 {
                            let symbol = &ds.entities[&candidates[0]];
                            value["source_symbol_ref"] = json!(symbol.id);
                            value["handler_location"] =
                                serde_json::to_value(symbol.fields.get("location"))
                                    .unwrap_or(Value::Null);
                            ds.edges.push(Edge {
                                source: id.clone(),
                                target: candidates[0].clone(),
                                relation: RelationKind::SourceSymbol,
                            });
                        }
                    }
                    add(&mut ds, ex, kind, id, value)?;
                }
            }
            for (index, record) in self.artifacts["deps"]["table"]["deps"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let id = self.id("dependency", index);
                add(
                    &mut ds,
                    ex,
                    EntityKind::Dependency,
                    id.clone(),
                    record.clone(),
                )?;
                if let Some(file) = record["source"].as_str().and_then(|p| paths.get(p)) {
                    ds.edges.push(Edge {
                        source: id.clone(),
                        target: file.clone(),
                        relation: RelationKind::DefinedIn,
                    });
                }
                for usage in self.artifacts["deps"]["usage"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|u| {
                        u["name"] == record["name"] && u["ecosystem"] == record["ecosystem"]
                    })
                {
                    for path in usage["imported_by"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if let Some(file) = paths.get(path) {
                            ds.edges.push(Edge {
                                source: file.clone(),
                                target: id.clone(),
                                relation: RelationKind::UsesDependency,
                            });
                        }
                    }
                }
            }
            for (table, kind, tag, source_key) in [
                ("concept_nodes", EntityKind::Component, "component", ""),
                (
                    "state_machines",
                    EntityKind::StateMachine,
                    "state_machine",
                    "source_symbol_id",
                ),
                (
                    "workflows",
                    EntityKind::Workflow,
                    "workflow",
                    "entry_symbol_id",
                ),
            ] {
                for mut row in self.rows(
                    &format!("SELECT * FROM {table} ORDER BY id LIMIT ?1"),
                    &[&limit],
                )? {
                    let n = row["id"].as_i64().unwrap();
                    let id = self.id(tag, n);
                    if let Some(symbol) = row[source_key].as_i64().and_then(|n| poly.get(&n)) {
                        ds.edges.push(Edge {
                            source: id.clone(),
                            target: symbol.clone(),
                            relation: RelationKind::SourceSymbol,
                        });
                    }
                    if kind == EntityKind::StateMachine {
                        for field in ["states", "transitions"] {
                            if let Some(text) = row[field].as_str() {
                                row[field] = serde_json::from_str(text).map_err(|_| {
                                    StorageError::new(
                                        "CorruptData",
                                        "Invalid captured state-machine JSON",
                                    )
                                })?;
                            }
                        }
                        for (index, state) in
                            row["states"].as_array().into_iter().flatten().enumerate()
                        {
                            let state_id = self.id("state", format!("{n}:{index}"));
                            add(
                                &mut ds,
                                ex,
                                EntityKind::State,
                                state_id.clone(),
                                json!({"name":state,"machine":id,"epistemic":row["epistemic"]}),
                            )?;
                            ds.edges.push(Edge {
                                source: state_id,
                                target: id.clone(),
                                relation: RelationKind::StateOf,
                            });
                        }
                    }
                    add(&mut ds, ex, kind, id, row)?;
                }
            }
            for row in self.rows(
                "SELECT * FROM concept_node_symbols ORDER BY concept_id,symbol_id LIMIT ?1",
                &[&limit],
            )? {
                if let Some(symbol) = row["symbol_id"].as_i64().and_then(|n| poly.get(&n)) {
                    ds.edges.push(Edge {
                        source: symbol.clone(),
                        target: self.id("component", row["concept_id"].as_i64().unwrap()),
                        relation: RelationKind::MemberOfComponent,
                    });
                }
            }
            for row in self.rows(
                "SELECT * FROM concept_edges ORDER BY parent_id,child_id LIMIT ?1",
                &[&limit],
            )? {
                ds.edges.push(Edge {
                    source: self.id("component", row["child_id"].as_i64().unwrap()),
                    target: self.id("component", row["parent_id"].as_i64().unwrap()),
                    relation: RelationKind::ChildComponentOf,
                });
            }
            self.architecture_entities(&mut ds, ex)?;
            let evidence: Vec<_> = ds
                .entities
                .values()
                .filter_map(|entity| {
                    entity
                        .fields
                        .get("location")
                        .filter(|v| !v.is_null())
                        .map(|location| {
                            (
                                entity.id.clone(),
                                location.clone(),
                                entity.fields.get("epistemic").cloned(),
                            )
                        })
                })
                .collect();
            for (index, (source, location, epistemic)) in evidence.into_iter().enumerate() {
                let id = self.id("evidence", index);
                add(
                    &mut ds,
                    ex,
                    EntityKind::Evidence,
                    id.clone(),
                    json!({"entity":source,"location":location,"revision":self.revision,"epistemic":epistemic,"source_kind":"captured_revision"}),
                )?;
                ds.edges.push(Edge {
                    source,
                    target: id,
                    relation: RelationKind::HasEvidence,
                });
            }
        }
        if domains {
            for record in self.artifacts["cochange"]["edges"]
                .as_array()
                .into_iter()
                .flatten()
            {
                ex.check(1)?;
                if let Some((source, target)) = record["a"]
                    .as_str()
                    .and_then(|path| paths.get(path))
                    .zip(record["b"].as_str().and_then(|path| paths.get(path)))
                {
                    ds.edges.push(Edge {
                        source: source.clone(),
                        target: target.clone(),
                        relation: RelationKind::CoChanged,
                    });
                }
            }
        }
        ds.edges
            .retain(|e| ds.entities.contains_key(&e.source) && ds.entities.contains_key(&e.target));
        ex.check(ds.edges.len())?;
        Ok(ds)
    }
    pub(crate) fn evaluate(
        &self,
        q: &Query,
        ds: &Dataset,
        ex: &mut Execution,
        depth: usize,
    ) -> Result<BTreeSet<String>> {
        ex.check(1)?;
        if depth > 16 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Query nesting exceeds 16",
            ));
        }
        match q {
            Query::Select { kind, filter } => {
                self.validate_predicate(filter, ds, depth + 1, *kind)?;
                let mut out = BTreeSet::new();
                for e in ds.entities.values().filter(|e| &e.kind == kind) {
                    ex.check(1)?;
                    if self.predicate(filter, e, ds, ex, depth + 1)? {
                        out.insert(e.id.clone());
                    }
                }
                Ok(out)
            }
            Query::Union(qs) | Query::Intersect(qs) => {
                if qs.is_empty() {
                    return Err(StorageError::new("InvalidRequest", "Empty set expression"));
                }
                let mut expected = kinds_of(&qs[0], ds, depth + 1)?;
                let is_union = matches!(q, Query::Union(_));
                for operand in &qs[1..] {
                    let actual = kinds_of(operand, ds, depth + 1)?;
                    // Intersections can narrow a polymorphic traversal to an explicit
                    // entity kind. Union still requires identical kind domains.
                    if !expected.is_empty()
                        && !actual.is_empty()
                        && (if is_union {
                            expected != actual
                        } else {
                            expected.is_disjoint(&actual)
                        })
                    {
                        return Err(StorageError::new(
                            "InvalidRequest",
                            "Set operands must have compatible entity kinds",
                        ));
                    }
                    if !actual.is_empty() {
                        expected = if expected.is_empty() {
                            actual
                        } else if is_union {
                            expected
                        } else {
                            expected.intersection(&actual).copied().collect()
                        };
                    }
                }
                let mut result = self.evaluate(&qs[0], ds, ex, depth + 1)?;
                for operand in &qs[1..] {
                    let next = self.evaluate(operand, ds, ex, depth + 1)?;
                    if is_union {
                        self.compatible_sets(&result, &next, ds)?;
                    }
                    result = if is_union {
                        result.union(&next).cloned().collect()
                    } else {
                        result.intersection(&next).cloned().collect()
                    };
                }
                Ok(result)
            }
            Query::Difference { left, right } => {
                let a_kind = kinds_of(left, ds, depth + 1)?;
                let b_kind = kinds_of(right, ds, depth + 1)?;
                if !a_kind.is_empty() && !b_kind.is_empty() && a_kind != b_kind {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Set operands must have compatible entity kinds",
                    ));
                }
                let a = self.evaluate(left, ds, ex, depth + 1)?;
                let b = self.evaluate(right, ds, ex, depth + 1)?;
                self.compatible_sets(&a, &b, ds)?;
                Ok(a.difference(&b).cloned().collect())
            }
            Query::Follow {
                input,
                relation,
                incoming,
                max_depth,
                min_depth,
            } => self.follow(
                input,
                *relation,
                *incoming,
                *min_depth,
                *max_depth,
                ds,
                ex,
                depth + 1,
            ),
            Query::CellContributors { descriptor } => {
                let value = self.decode(descriptor)?;
                if value["type"] != "cell" {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Expected cell descriptor",
                    ));
                }
                let ids: Vec<String> = serde_json::from_value(value["ids"].clone())?;
                for id in &ids {
                    if !ds.entities.contains_key(id) {
                        return Err(StorageError::new("InvalidRequest", "Unknown contributor"));
                    }
                }
                Ok(ids.into_iter().collect())
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn follow(
        &self,
        input: &Query,
        relation: RelationKind,
        incoming: bool,
        min_depth: u8,
        max_depth: u8,
        ds: &Dataset,
        ex: &mut Execution,
        depth: usize,
    ) -> Result<BTreeSet<String>> {
        if min_depth == 0 || max_depth == 0 || max_depth > 8 || min_depth > max_depth {
            return Err(StorageError::new(
                "InvalidRequest",
                "Traversal depth must satisfy 1 <= min <= max <= 8",
            ));
        }
        for kind in kinds_of(input, ds, depth)? {
            if !relation_allows_direction(relation, kind, incoming, ds) {
                return Err(StorageError::new(
                    "InvalidRequest",
                    format!("{relation:?} does not connect {kind:?} along this direction"),
                ));
            }
        }
        let mut frontier = self.evaluate(input, ds, ex, depth)?;
        let mut visited = frontier.clone();
        let mut result = BTreeSet::new();
        for current in 1..=max_depth {
            let mut next = BTreeSet::new();
            for edge in &ds.edges {
                ex.check(1)?;
                let (a, b) = if incoming {
                    (&edge.target, &edge.source)
                } else {
                    (&edge.source, &edge.target)
                };
                if edge.relation == relation && frontier.contains(a) && !visited.contains(b) {
                    next.insert(b.clone());
                }
            }
            visited.extend(next.clone());
            if current >= min_depth {
                result.extend(next.clone());
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        Ok(result)
    }
    fn compatible_sets(
        &self,
        a: &BTreeSet<String>,
        b: &BTreeSet<String>,
        ds: &Dataset,
    ) -> Result<()> {
        let kinds: BTreeSet<_> = a.iter().chain(b).map(|id| ds.entities[id].kind).collect();
        if kinds.len() > 1 {
            Err(StorageError::new(
                "InvalidRequest",
                "Set operands must have compatible entity kinds",
            ))
        } else {
            Ok(())
        }
    }
    fn validate_predicate(
        &self,
        p: &Predicate,
        ds: &Dataset,
        depth: usize,
        kind: EntityKind,
    ) -> Result<()> {
        if depth > 16 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Predicate nesting exceeds 16",
            ));
        }
        match p {
            Predicate::And(ps) | Predicate::Or(ps) => {
                if ps.is_empty() {
                    return Err(StorageError::new("InvalidRequest", "Empty predicate list"));
                }
                for p in ps {
                    self.validate_predicate(p, ds, depth + 1, kind)?;
                }
            }
            Predicate::Not(p) => self.validate_predicate(p, ds, depth + 1, kind)?,
            Predicate::Ids(ids) => {
                for id in ids {
                    if !ds.entities.contains_key(id) {
                        return Err(StorageError::new(
                            "MixedRevisionReference",
                            "Unknown or foreign entity ID",
                        ));
                    }
                }
            }
            Predicate::Field { field, .. } => {
                if !matches!(field.as_str(), "id" | "entity_kind")
                    && !super::tools::entity_fields(kind).contains(&field.as_str())
                    && !(kind == EntityKind::File && field == "file.path")
                    && !ds
                        .entities
                        .values()
                        .any(|e| e.kind == kind && field_value(e, field).is_some())
                {
                    return Err(StorageError::new(
                        "UnsupportedQuery",
                        format!("Unknown field {field}"),
                    ));
                }
            }
            Predicate::ConceptMembership { concepts, .. } => {
                if concepts.is_empty()
                    || concepts
                        .iter()
                        .any(|id| ds.entities.get(id).map(|e| e.kind) != Some(EntityKind::Concept))
                {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Expected concept IDs from this revision",
                    ));
                }
            }
            Predicate::Related {
                relation,
                incoming,
                target,
                ..
            } => {
                for target_kind in kinds_of(target, ds, depth + 1)? {
                    let allowed = if *incoming {
                        relation.allows(target_kind, kind)
                    } else {
                        relation.allows(kind, target_kind)
                    };
                    if !allowed {
                        return Err(StorageError::new(
                            "InvalidRequest",
                            format!("{relation:?} does not connect {kind:?} and {target_kind:?}"),
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn predicate(
        &self,
        p: &Predicate,
        e: &Entity,
        ds: &Dataset,
        ex: &mut Execution,
        depth: usize,
    ) -> Result<bool> {
        ex.check(1)?;
        if depth > 16 {
            return Err(StorageError::new("InvalidRequest", "Predicate nesting"));
        }
        Ok(match p {
            Predicate::All => true,
            Predicate::Ids(ids) => ids.contains(&e.id),
            Predicate::And(ps) => {
                let mut yes = true;
                for p in ps {
                    yes &= self.predicate(p, e, ds, ex, depth + 1)?;
                }
                yes
            }
            Predicate::Or(ps) => {
                let mut yes = false;
                for p in ps {
                    yes |= self.predicate(p, e, ds, ex, depth + 1)?;
                }
                yes
            }
            Predicate::Not(p) => !self.predicate(p, e, ds, ex, depth + 1)?,
            Predicate::Field { field, op, value } => {
                let actual = field_value(e, field).unwrap_or(Value::Null);
                match op {
                    CompareOp::Eq => actual == *value,
                    CompareOp::In => value
                        .as_array()
                        .ok_or_else(|| StorageError::new("InvalidRequest", "In requires an array"))?
                        .contains(&actual),
                    CompareOp::Prefix
                    | CompareOp::Contains
                    | CompareOp::PrefixFold
                    | CompareOp::ContainsFold => {
                        let a = actual.as_str().ok_or_else(|| {
                            StorageError::new(
                                "InvalidRequest",
                                "String comparison requires a string field",
                            )
                        })?;
                        let b = value.as_str().ok_or_else(|| {
                            StorageError::new("InvalidRequest", "Expected string value")
                        })?;
                        match op {
                            CompareOp::Prefix => a.starts_with(b),
                            CompareOp::Contains => a.contains(b),
                            CompareOp::PrefixFold => fold_text(a).starts_with(&fold_text(b)),
                            _ => fold_text(a).contains(&fold_text(b)),
                        }
                    }
                    _ => {
                        let a = actual.as_f64().ok_or_else(|| {
                            StorageError::new(
                                "InvalidRequest",
                                "Numeric comparison requires numbers",
                            )
                        })?;
                        let b = value.as_f64().ok_or_else(|| {
                            StorageError::new("InvalidRequest", "Expected numeric value")
                        })?;
                        match op {
                            CompareOp::Gt => a > b,
                            CompareOp::Ge => a >= b,
                            CompareOp::Lt => a < b,
                            _ => a <= b,
                        }
                    }
                }
            }
            Predicate::ConceptMembership { concepts, scope } => {
                let mut concepts: BTreeSet<_> = concepts.iter().cloned().collect();
                if *scope == MembershipScope::IncludeDescendants {
                    loop {
                        let before = concepts.len();
                        for edge in &ds.edges {
                            ex.check(1)?;
                            if edge.relation == RelationKind::ChildConceptOf
                                && concepts.contains(&edge.target)
                            {
                                concepts.insert(edge.source.clone());
                            }
                        }
                        if concepts.len() == before {
                            break;
                        }
                    }
                }
                ds.edges.iter().any(|edge| {
                    edge.relation == RelationKind::MemberOfConcept
                        && edge.source == e.id
                        && concepts.contains(&edge.target)
                })
            }
            Predicate::Related {
                relation,
                incoming,
                target,
                quantifier,
            } => {
                let target = self.evaluate(target, ds, ex, depth + 1)?;
                let mut related = BTreeSet::new();
                let mut any = false;
                for edge in &ds.edges {
                    ex.check(1)?;
                    let (a, b) = if *incoming {
                        (&edge.target, &edge.source)
                    } else {
                        (&edge.source, &edge.target)
                    };
                    if &edge.relation == relation && a == &e.id && target.contains(b) {
                        any = true;
                        related.insert(b.clone());
                    }
                }
                match quantifier {
                    RelationQuantifier::Any => any,
                    RelationQuantifier::None => !any,
                    RelationQuantifier::All => {
                        // Requires a non-empty target set and a relation to
                        // every target entity.
                        !target.is_empty() && related.len() == target.len()
                    }
                }
            }
        })
    }
}
pub(crate) fn field_value(e: &Entity, field: &str) -> Option<Value> {
    if e.kind == EntityKind::File && field == "file.path" {
        return e.fields.get("path").cloned();
    }
    if field == "id" {
        return Some(json!(e.id));
    }
    if field == "entity_kind" {
        return Some(json!(e.kind));
    }
    let mut parts = field.split('.');
    let mut v = e.fields.get(parts.next()?)?;
    for p in parts {
        v = v.get(p)?;
    }
    Some(v.clone())
}

fn add(
    ds: &mut Dataset,
    ex: &mut Execution,
    kind: EntityKind,
    id: String,
    value: Value,
) -> Result<()> {
    ex.check(1)?;
    let fields = value
        .as_object()
        .ok_or_else(|| StorageError::new("CorruptData", "Expected entity object"))?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    ds.entities.insert(
        id.clone(),
        Entity {
            id,
            kind,
            fields,
            evidence_refs: Vec::new(),
        },
    );
    Ok(())
}

/// Unicode case-folded comparison form for compatibility string presets.
pub(crate) fn fold_text(s: &str) -> String {
    use unicode_casefold::UnicodeCaseFold;
    s.case_fold().collect()
}

/// Normalized repository-relative path using '/'. Absolute paths, traversal
/// outside the repository and empty components are rejected.
pub(crate) fn normalize_repo_path(input: &str) -> Result<String> {
    if input.is_empty()
        || input.contains('\\')
        || input.starts_with('/')
        || input.split('/').next() == Some("")
    {
        return Err(StorageError::new(
            "InvalidRequest",
            "Paths must be repository-relative with / separators",
        ));
    }
    let mut parts = Vec::new();
    for part in input.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(StorageError::new(
                    "InvalidRequest",
                    "Path traversal outside the repository is rejected",
                ))
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err(StorageError::new(
            "InvalidRequest",
            "Path must name a repository file",
        ));
    }
    Ok(parts.join("/"))
}

/// Static entity-kind inference for a query sub-expression. Cell contributors
/// have unknown kinds until decoded, represented by an empty set.
pub(crate) fn kinds_of(q: &Query, ds: &Dataset, depth: usize) -> Result<BTreeSet<EntityKind>> {
    if depth > 32 {
        return Ok(BTreeSet::new());
    }
    Ok(match q {
        Query::Select { kind, .. } => [*kind].into_iter().collect(),
        Query::Union(qs) => {
            let mut kinds = BTreeSet::new();
            for q in qs {
                kinds.extend(kinds_of(q, ds, depth + 1)?);
            }
            kinds
        }
        Query::Intersect(qs) => {
            let mut result: Option<BTreeSet<EntityKind>> = None;
            for q in qs {
                let kinds = kinds_of(q, ds, depth + 1)?;
                if kinds.is_empty() {
                    continue;
                } // Opaque contributor kind is unknown.
                result = Some(match result {
                    None => kinds,
                    Some(previous) => previous.intersection(&kinds).copied().collect(),
                });
            }
            result.unwrap_or_default()
        }
        Query::Difference { left, .. } => kinds_of(left, ds, depth + 1)?,
        Query::Follow {
            input,
            relation,
            incoming,
            min_depth,
            max_depth: _,
        } => {
            let mut kinds = BTreeSet::new();
            if *min_depth == 0 {
                kinds.extend(kinds_of(input, ds, depth + 1)?);
            }
            for source in kinds_of(input, ds, depth + 1)? {
                for target in EntityKind::ALL {
                    if if *incoming {
                        relation.allows(*target, source)
                    } else {
                        relation.allows(source, *target)
                    } {
                        kinds.insert(*target);
                    }
                }
            }
            kinds
        }
        Query::CellContributors { .. } => BTreeSet::new(),
    })
}

/// Endpoint validation along a traversal direction. Unknown contributor kinds
/// (empty set) are not statically decidable and pass.
pub(crate) fn relation_allows_direction(
    relation: RelationKind,
    from_kind: EntityKind,
    incoming: bool,
    ds: &Dataset,
) -> bool {
    let _ = ds;
    EntityKind::ALL.iter().copied().any(|other| {
        if incoming {
            relation.allows(other, from_kind)
        } else {
            relation.allows(from_kind, other)
        }
    })
}

/// Remove exactly the tagged dimension's own field filters from a top-level
/// Select. Nested or indirect use is ambiguous and rejected (§16.4).
pub(crate) fn strip_dimension_filters(q: &Query, dimension: &str) -> Result<Query> {
    fn field_for(p: &Predicate) -> Option<&String> {
        match p {
            Predicate::Field { field, .. } => Some(field),
            _ => None,
        }
    }
    fn uses_dimension(p: &Predicate, dimension: &str) -> bool {
        match p {
            Predicate::Field { field, .. } => {
                field == dimension || field.starts_with(&format!("{dimension}."))
            }
            Predicate::And(ps) | Predicate::Or(ps) => {
                ps.iter().any(|p| uses_dimension(p, dimension))
            }
            Predicate::Not(p) => uses_dimension(p, dimension),
            _ => false,
        }
    }
    match q {
        Query::Select { kind, filter } => {
            if !uses_dimension(filter, dimension) {
                // No filter for this dimension: exclusion is a no-op.
                return Ok(Query::Select {
                    kind: *kind,
                    filter: filter.clone(),
                });
            }
            match filter {
                Predicate::And(prongs) => {
                    let mut kept = Vec::new();
                    for prong in prongs {
                        if field_for(prong).is_some() && uses_dimension(prong, dimension) {
                            continue;
                        }
                        if uses_dimension(prong, dimension) {
                            return Err(StorageError::new(
                                "InvalidRequest",
                                "Self-exclusion is ambiguous under nested dimension filters",
                            ));
                        }
                        kept.push(prong.clone());
                    }
                    Ok(Query::Select {
                        kind: *kind,
                        filter: if kept.is_empty() {
                            Predicate::All
                        } else {
                            Predicate::And(kept)
                        },
                    })
                }
                Predicate::Field { .. } => {
                    if uses_dimension(filter, dimension) {
                        Ok(Query::Select {
                            kind: *kind,
                            filter: Predicate::All,
                        })
                    } else {
                        Ok(q.clone())
                    }
                }
                _ => Err(StorageError::new(
                    "InvalidRequest",
                    "Self-exclusion is ambiguous under this query shape",
                )),
            }
        }
        _ => Err(StorageError::new(
            "InvalidRequest",
            "Self-excluding facets require a top-level Select query",
        )),
    }
}

impl RevisionReader {
    /// §6: resolve a symbol by file and qualified name, or by a zero-based
    /// UTF-8 byte position inside the captured revision's source.
    pub fn resolve_symbol(&self, lookup: SymbolLookup) -> Result<SymbolResolution> {
        let mut ex = Execution {
            deadline: Instant::now() + Duration::from_secs(30),
            max_visited: 1_000_000,
            visited: 0,
            cancelled: Default::default(),
        };
        let ds = self.dataset(&mut ex)?;
        let file = match &lookup {
            SymbolLookup::ByName { file, .. } => normalize_repo_path(file)?,
            SymbolLookup::AtPosition { file, .. } => normalize_repo_path(file)?,
        };
        let in_file = |e: &Entity| {
            let path = e
                .fields
                .get("file")
                .and_then(|f| f.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            path == file
        };
        let mut symbols: Vec<&Entity> = ds
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Symbol && in_file(e))
            .collect();
        symbols.sort_by(|a, b| a.id.cmp(&b.id));
        if symbols.is_empty() {
            // Explicitly unavailable, never inferred from the live tree.
            return Err(StorageError::new(
                "ArtifactUnavailable",
                "No captured symbol spans for file",
            ));
        }
        match lookup {
            SymbolLookup::ByName { qualified_name, .. } => {
                let matches: Vec<&Entity> = symbols
                    .into_iter()
                    .filter(|e| {
                        e.fields.get("qualified_name").and_then(Value::as_str)
                            == Some(qualified_name.as_str())
                    })
                    .collect();
                match matches.len() {
                    0 => Ok(SymbolResolution::NotFound),
                    1 => Ok(SymbolResolution::Found(self.entity_ref(matches[0]))),
                    _ => Ok(SymbolResolution::Ambiguous {
                        candidates: matches.iter().map(|e| self.candidate(e)).collect(),
                    }),
                }
            }
            SymbolLookup::AtPosition { byte_offset, .. } => {
                // Narrowest containing declaration span; equal spans stay
                // ambiguous. Missing spans were rejected above.
                let mut candidates: Vec<&Entity> = Vec::new();
                let mut best = u64::MAX;
                for e in symbols {
                    let loc = match e.fields.get("location") {
                        Some(l) => l,
                        None => continue,
                    };
                    let (start, end) = match (loc["start"].as_u64(), loc["end"].as_u64()) {
                        (Some(s), Some(t)) if s <= byte_offset && byte_offset < t => (s, t),
                        _ => continue,
                    };
                    let width = end.saturating_sub(start);
                    if width < best {
                        best = width;
                        candidates.clear();
                        candidates.push(e);
                    } else if width == best {
                        candidates.push(e);
                    }
                }
                match candidates.len() {
                    0 => Ok(SymbolResolution::NotFound),
                    1 => Ok(SymbolResolution::Found(self.entity_ref(candidates[0]))),
                    _ => Ok(SymbolResolution::Ambiguous {
                        candidates: candidates.iter().map(|e| self.candidate(e)).collect(),
                    }),
                }
            }
        }
    }
    fn entity_ref(&self, e: &Entity) -> EntityRef {
        EntityRef {
            repository: self.repository.clone(),
            revision: self.revision.clone(),
            kind: e.kind,
            id: e.id.clone(),
        }
    }
    fn candidate(&self, e: &Entity) -> SymbolCandidate {
        SymbolCandidate {
            entity: self.entity_ref(e),
            file: e
                .fields
                .get("file")
                .and_then(|f| f.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            qualified_name: e
                .fields
                .get("qualified_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            kind: e.fields.get("kind").and_then(Value::as_str).map(Into::into),
        }
    }
}

/// Inspect typed expressions, never user string values, to decide which
/// optional fact families this retrieval can reach.
pub(crate) fn dataset_needs(request: &Request) -> Result<(bool, bool)> {
    fn relation(r: RelationKind, needs: &mut (bool, bool)) {
        needs.0 |= matches!(
            r,
            RelationKind::MemberOfCluster
                | RelationKind::LeftSymbol
                | RelationKind::RightSymbol
                | RelationKind::CanonicalSymbol
        );
        needs.1 |= matches!(
            r,
            RelationKind::MemberOfContainer
                | RelationKind::ComponentInContainer
                | RelationKind::CommunicatesWith
                | RelationKind::MemberOfComponent
                | RelationKind::ChildComponentOf
                | RelationKind::UsesDependency
                | RelationKind::SourceSymbol
                | RelationKind::StateOf
                | RelationKind::HasEvidence
                | RelationKind::CoChanged
        );
    }
    fn predicate(p: &Predicate, needs: &mut (bool, bool), depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Predicate nesting exceeds policy",
            ));
        }
        match p {
            Predicate::And(ps) | Predicate::Or(ps) => {
                for p in ps {
                    predicate(p, needs, depth + 1)?;
                }
            }
            Predicate::Not(p) => predicate(p, needs, depth + 1)?,
            Predicate::Related {
                relation: r,
                target,
                ..
            } => {
                relation(*r, needs);
                query(target, needs, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn query(q: &Query, needs: &mut (bool, bool), depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(StorageError::new(
                "InvalidRequest",
                "Query nesting exceeds policy",
            ));
        }
        match q {
            Query::Select { kind, filter } => {
                needs.0 |= matches!(kind, EntityKind::DuplicatePair | EntityKind::Cluster);
                needs.1 |= !matches!(
                    kind,
                    EntityKind::File
                        | EntityKind::Symbol
                        | EntityKind::Concept
                        | EntityKind::Summary
                        | EntityKind::Contract
                        | EntityKind::Finding
                        | EntityKind::DuplicatePair
                        | EntityKind::Cluster
                );
                predicate(filter, needs, depth + 1)?;
            }
            Query::Union(qs) | Query::Intersect(qs) => {
                for q in qs {
                    query(q, needs, depth + 1)?;
                }
            }
            Query::Difference { left, right } => {
                query(left, needs, depth + 1)?;
                query(right, needs, depth + 1)?;
            }
            Query::Follow {
                input, relation: r, ..
            } => {
                relation(*r, needs);
                query(input, needs, depth + 1)?;
            }
            Query::CellContributors { .. } => *needs = (true, true),
        }
        Ok(())
    }
    let mut needs = (false, false);
    query(&request.query, &mut needs, 0)?;
    for named in &request.outputs {
        match &named.output {
            Output::Cube(c) => {
                for d in &c.dimensions {
                    if let DimensionRef::RelatedEntity { relation_path, .. } = &d.dimension {
                        for step in relation_path {
                            relation(step.relation, &mut needs);
                        }
                    }
                }
            }
            Output::Graph {
                expansion: Some(expansion),
                ..
            } => {
                if expansion.relations.is_empty() {
                    needs = (true, true);
                } else {
                    for r in &expansion.relations {
                        relation(*r, &mut needs);
                    }
                }
            }
            Output::View { kind, .. } => {
                needs.0 |= kind == "duplicate_clusters";
                needs.1 |= kind == "dependencies" || kind == "cochange";
            }
            _ => {}
        }
    }
    Ok(needs)
}

#[cfg(test)]
mod kind_narrowing_tests {
    use super::*;
    #[test]
    fn typed_intersection_narrows_polymorphic_follow_and_union_keeps_kind_domain() {
        let dataset = Dataset {
            entities: BTreeMap::new(),
            edges: vec![],
        };
        let query: Query = serde_json::from_value(json!({"intersect":[
            {"select":{"kind":"symbol"}},
            {"follow":{"input":{"select":{"kind":"concept"}},"relation":"member_of_concept","incoming":true,"max_depth":1}}
        ]})).unwrap();
        assert_eq!(
            kinds_of(&query, &dataset, 0).unwrap(),
            [EntityKind::Symbol].into_iter().collect()
        );
        let broad: Query = serde_json::from_value(json!({"follow":{"input":{"select":{"kind":"concept"}},"relation":"member_of_concept","incoming":true,"max_depth":1}})).unwrap();
        assert_eq!(
            kinds_of(&broad, &dataset, 0).unwrap(),
            [EntityKind::Symbol, EntityKind::File].into_iter().collect()
        );
    }
}

/// Resolve only within the captured file. Duplicate names/overloads are ambiguous;
/// no basename, fuzzy-name or whole-file fallback may become an API identity.
fn entry_candidates(ds: &Dataset, record: &Value, kind: EntityKind) -> Vec<String> {
    let Some(path) = record["rel"].as_str() else {
        return vec![];
    };
    let mut candidates = Vec::new();
    for entity in ds
        .entities
        .values()
        .filter(|e| e.kind == EntityKind::Symbol)
    {
        if entity
            .fields
            .get("file")
            .and_then(|f| f.get("path"))
            .and_then(Value::as_str)
            != Some(path)
        {
            continue;
        }
        let name = entity
            .fields
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let qualified = entity
            .fields
            .get("qualified_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let symbol_kind = entity
            .fields
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("");
        let matches = if kind == EntityKind::Route {
            let method = record["method"].as_str().unwrap_or("");
            let owner = record["owner"].as_str().unwrap_or("");
            let owner_method = format!("{owner}.{method}");
            !method.is_empty()
                && matches!(symbol_kind, "Function" | "Method")
                && name == method
                && (owner.is_empty()
                    || qualified == owner_method
                    || qualified.ends_with(&format!(".{owner_method}"))
                    || symbol_kind == "Function")
        } else {
            let entry_name = record["name"].as_str().unwrap_or("");
            let entry_kind = record["kind"].as_str().unwrap_or("");
            // Spring records the application class. Rust serve entries record
            // the module name while explicitly referring to fn main.
            !entry_name.is_empty()
                && (name == entry_name && matches!(symbol_kind, "Function" | "Method" | "Class")
                    || entry_kind.ends_with("-serve")
                        && record["lang"] == "rust"
                        && name == "main"
                        && symbol_kind == "Function")
        };
        if matches {
            candidates.push(entity.id.clone());
        }
    }
    candidates
}

#[cfg(test)]
mod entry_resolution_tests {
    use super::*;
    fn symbol(id: &str, name: &str, qualified: &str, kind: &str, file: &str) -> Entity {
        serde_json::from_value(json!({"id":id,"kind":"symbol","fields":{"name":name,"qualified_name":qualified,"kind":kind,"file":{"path":file},"location":{"file":file,"start":0,"end":20}}})).unwrap()
    }
    #[test]
    fn api_identity_uses_unique_captured_handler_not_every_symbol_in_its_file() {
        let symbols = vec![
            symbol("pay", "pay", "Payments.pay", "Method", "Api.java"),
            symbol("refund", "refund", "Payments.refund", "Method", "Api.java"),
            symbol("other", "pay", "Payments.pay", "Method", "Other.java"),
        ];
        let mut ds = Dataset {
            entities: symbols.into_iter().map(|e| (e.id.clone(), e)).collect(),
            edges: vec![],
        };
        let route = json!({"rel":"Api.java","owner":"Payments","method":"pay"});
        assert_eq!(
            entry_candidates(&ds, &route, EntityKind::Route),
            vec!["pay"]
        );
        ds.entities.insert(
            "overload".into(),
            symbol("overload", "pay", "Payments.pay", "Method", "Api.java"),
        );
        assert_eq!(entry_candidates(&ds, &route, EntityKind::Route).len(), 2);
        assert!(entry_candidates(
            &ds,
            &json!({"rel":"Api.java","owner":"Wrong","method":"pay"}),
            EntityKind::Route
        )
        .is_empty());
    }
    #[test]
    fn bootstrap_class_and_rust_main_are_explicit_but_listen_name_is_not_guessed() {
        let symbols = vec![
            symbol("class", "App", "App", "Class", "App.java"),
            symbol("main", "main", "server.main", "Function", "server.rs"),
        ];
        let ds = Dataset {
            entities: symbols.into_iter().map(|e| (e.id.clone(), e)).collect(),
            edges: vec![],
        };
        assert_eq!(
            entry_candidates(
                &ds,
                &json!({"rel":"App.java","name":"App","kind":"spring-boot"}),
                EntityKind::EntryPoint
            ),
            vec!["class"]
        );
        assert_eq!(
            entry_candidates(
                &ds,
                &json!({"rel":"server.rs","name":"server","kind":"axum-serve","lang":"rust"}),
                EntityKind::EntryPoint
            ),
            vec!["main"]
        );
        assert!(entry_candidates(
            &ds,
            &json!({"rel":"server.rs","name":"server","kind":"express-listen","lang":"typescript"}),
            EntityKind::EntryPoint
        )
        .is_empty());
    }
}
