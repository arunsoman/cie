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

/// Unicode case-folded comparison form for compatibility string presets.
pub(crate) fn fold_text(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase()).collect()
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
        Query::Union(qs) | Query::Intersect(qs) => {
            let mut kinds = BTreeSet::new();
            for q in qs {
                kinds.extend(kinds_of(q, ds, depth + 1)?);
            }
            kinds
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
                for edge in &ds.edges {
                    if edge.relation == *relation {
                        let (from, to) = if *incoming {
                            (&edge.target, &edge.source)
                        } else {
                            (&edge.source, &edge.target)
                        };
                        if ds.entities.get(from).map(|e| e.kind) == Some(source) {
                            if let Some(e) = ds.entities.get(to) {
                                kinds.insert(e.kind);
                            }
                        }
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
    for edge in &ds.edges {
        if edge.relation != relation {
            continue;
        }
        let source = ds.entities.get(&edge.source).map(|e| e.kind);
        let target = ds.entities.get(&edge.target).map(|e| e.kind);
        let allowed = if incoming {
            matches!((target, source), (Some(k), Some(_)) if k == from_kind)
        } else {
            matches!((source, target), (Some(k), Some(_)) if k == from_kind)
        };
        if allowed {
            return true;
        }
    }
    false
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
