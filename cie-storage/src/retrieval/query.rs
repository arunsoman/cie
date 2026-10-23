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
                self.validate_predicate(filter, ds, depth + 1)?;
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
                let mut result = self.evaluate(&qs[0], ds, ex, depth + 1)?;
                for operand in &qs[1..] {
                    let next = self.evaluate(operand, ds, ex, depth + 1)?;
                    self.compatible_sets(&result, &next, ds)?;
                    result = if matches!(q, Query::Union(_)) {
                        result.union(&next).cloned().collect()
                    } else {
                        result.intersection(&next).cloned().collect()
                    };
                }
                Ok(result)
            }
            Query::Difference { left, right } => {
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
            } => {
                if *max_depth == 0 || *max_depth > 8 {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Traversal depth must be 1..8",
                    ));
                }
                let mut frontier = self.evaluate(input, ds, ex, depth + 1)?;
                let mut result = BTreeSet::new();
                for _ in 0..*max_depth {
                    let mut next = BTreeSet::new();
                    for edge in &ds.edges {
                        ex.check(1)?;
                        let (a, b) = if *incoming {
                            (&edge.target, &edge.source)
                        } else {
                            (&edge.source, &edge.target)
                        };
                        if &edge.relation == relation && frontier.contains(a) && !result.contains(b)
                        {
                            next.insert(b.clone());
                        }
                    }
                    result.extend(next.clone());
                    frontier = next;
                    if frontier.is_empty() {
                        break;
                    }
                }
                Ok(result)
            }
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
    fn validate_predicate(&self, p: &Predicate, ds: &Dataset, depth: usize) -> Result<()> {
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
                    self.validate_predicate(p, ds, depth + 1)?;
                }
            }
            Predicate::Not(p) => self.validate_predicate(p, ds, depth + 1)?,
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
                if !ds
                    .entities
                    .values()
                    .any(|e| field_value(e, field).is_some())
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
                    CompareOp::Prefix | CompareOp::Contains => {
                        let a = actual.as_str().ok_or_else(|| {
                            StorageError::new(
                                "InvalidRequest",
                                "String comparison requires a string field",
                            )
                        })?;
                        let b = value.as_str().ok_or_else(|| {
                            StorageError::new("InvalidRequest", "Expected string value")
                        })?;
                        if matches!(op, CompareOp::Prefix) {
                            a.starts_with(b)
                        } else {
                            a.contains(b)
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
            Predicate::ConceptMembership {
                concepts,
                include_descendants,
            } => {
                let mut concepts: BTreeSet<_> = concepts.iter().cloned().collect();
                if *include_descendants {
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
            } => {
                let target = self.evaluate(target, ds, ex, depth + 1)?;
                ds.edges.iter().any(|edge| {
                    let (a, b) = if *incoming {
                        (&edge.target, &edge.source)
                    } else {
                        (&edge.source, &edge.target)
                    };
                    &edge.relation == relation && a == &e.id && target.contains(b)
                })
            }
        })
    }
}
pub(crate) fn field_value(e: &Entity, field: &str) -> Option<Value> {
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
    ds.entities.insert(id.clone(), Entity { id, kind, fields });
    Ok(())
}
