//! Named projections over captured facts. No parsing, ingestion, network or LLM.
use super::query::Dataset;
use super::*;
pub(crate) const VIEW_KINDS: &[&str] = &[
    "relationships",
    "repository_map",
    "concept_hierarchy",
    "duplicate_clusters",
    "cochange",
    "dependencies",
    "impact",
    "c4",
    "state-machine",
    "workflow",
];
impl RevisionReader {
    pub(crate) fn domain_view(
        &self,
        kind: &str,
        matches: &BTreeSet<String>,
        ds: &Dataset,
        max_nodes: usize,
        max_edges: usize,
        hash: &str,
        ex: &mut Execution,
    ) -> Result<Value> {
        if kind == "dependencies" && self.artifacts.get("deps").is_none_or(Value::is_null) {
            return Err(StorageError::new(
                "ArtifactUnavailable",
                "Dependency facts were not captured",
            ));
        }
        let mut data = Dataset {
            entities: ds.entities.clone(),
            edges: ds.edges.clone(),
        };
        let mut selected = matches.clone();
        let mut derivations = Vec::new();
        if kind == "cochange" {
            data.edges
                .retain(|edge| edge.relation != RelationKind::CoChanged);
            let artifact = self
                .artifacts
                .get("cochange")
                .filter(|value| !value.is_null())
                .ok_or_else(|| {
                    StorageError::new("ArtifactUnavailable", "Co-change history was not captured")
                })?;
            let files: BTreeMap<_, _> = data
                .entities
                .values()
                .filter(|e| e.kind == EntityKind::File)
                .filter_map(|e| {
                    e.fields
                        .get("path")
                        .and_then(Value::as_str)
                        .map(|p| (p.to_owned(), e.id.clone()))
                })
                .collect();
            for record in artifact["edges"].as_array().into_iter().flatten() {
                ex.check(1)?;
                if let Some((a, b)) = record["a"]
                    .as_str()
                    .and_then(|a| files.get(a))
                    .zip(record["b"].as_str().and_then(|b| files.get(b)))
                {
                    if matches.contains(a) || matches.contains(b) {
                        selected.insert(a.clone());
                        selected.insert(b.clone());
                        data.edges.push(Edge {
                            source: a.clone(),
                            target: b.clone(),
                            relation: RelationKind::CoChanged,
                        });
                        derivations.push(json!({"source":a,"target":b,"support":record["support"],"supporting_commits":record["supporting_commits"],"provenance":"captured_git_history"}));
                    }
                }
            }
            data.edges
                .retain(|edge| edge.relation == RelationKind::CoChanged);
        } else {
            // Establish symbol scope for file selections without widening to
            // unrelated files or sibling concepts.
            for edge in &data.edges {
                ex.check(1)?;
                if edge.relation == RelationKind::DefinedIn && matches.contains(&edge.target) {
                    selected.insert(edge.source.clone());
                }
            }
            let seed = selected.clone();
            for edge in &data.edges {
                ex.check(1)?;
                let relevant = match kind {
                    "concept_hierarchy" => {
                        edge.relation == RelationKind::MemberOfConcept
                            && seed.contains(&edge.source)
                    }
                    "duplicate_clusters" => {
                        matches!(
                            edge.relation,
                            RelationKind::MemberOfCluster
                                | RelationKind::LeftSymbol
                                | RelationKind::RightSymbol
                        ) && (seed.contains(&edge.source) || seed.contains(&edge.target))
                    }
                    "dependencies" => {
                        matches!(
                            edge.relation,
                            RelationKind::UsesDependency | RelationKind::DefinedIn
                        ) && (seed.contains(&edge.source) || seed.contains(&edge.target))
                            && (data.entities[&edge.source].kind == EntityKind::Dependency
                                || data.entities[&edge.target].kind == EntityKind::Dependency)
                    }
                    _ => false,
                };
                if relevant {
                    selected.insert(edge.source.clone());
                    selected.insert(edge.target.clone());
                }
            }
            if kind == "concept_hierarchy" {
                loop {
                    let before = selected.len();
                    for edge in &data.edges {
                        ex.check(1)?;
                        if edge.relation == RelationKind::ChildConceptOf
                            && selected.contains(&edge.source)
                        {
                            selected.insert(edge.target.clone());
                        }
                    }
                    if selected.len() == before {
                        break;
                    }
                }
                selected.retain(|id| data.entities[id].kind == EntityKind::Concept);
                for id in &selected {
                    if let Some(location) = ds
                        .edges
                        .iter()
                        .filter(|edge| {
                            edge.relation == RelationKind::MemberOfConcept && edge.target == *id
                        })
                        .find_map(|edge| ds.entities[&edge.source].fields.get("location").cloned())
                    {
                        let entity = data.entities.get_mut(id).unwrap();
                        entity.fields.insert("location".into(), location);
                        entity.fields.insert(
                            "evidence_scope".into(),
                            json!("representative_captured_member"),
                        );
                    }
                }
                data.edges
                    .retain(|edge| edge.relation == RelationKind::ChildConceptOf);
            } else if kind == "duplicate_clusters" {
                let hubs = selected.clone();
                for edge in &data.edges {
                    ex.check(1)?;
                    if edge.relation == RelationKind::MemberOfCluster && hubs.contains(&edge.target)
                    {
                        selected.insert(edge.source.clone());
                    }
                }
                selected.retain(|id| {
                    matches!(
                        data.entities[id].kind,
                        EntityKind::Symbol | EntityKind::Cluster | EntityKind::DuplicatePair
                    )
                });
                data.edges.retain(|edge| {
                    matches!(
                        edge.relation,
                        RelationKind::MemberOfCluster
                            | RelationKind::LeftSymbol
                            | RelationKind::RightSymbol
                            | RelationKind::CanonicalSymbol
                    )
                });
            } else if kind == "dependencies" {
                selected.retain(|id| {
                    matches!(
                        data.entities[id].kind,
                        EntityKind::File | EntityKind::Dependency
                    )
                });
                data.edges.retain(|edge| {
                    edge.relation == RelationKind::UsesDependency
                        || (edge.relation == RelationKind::DefinedIn
                            && data.entities[&edge.source].kind == EntityKind::Dependency)
                });
                // A manifest location is a whole captured file, never a made-up
                // declaration span. Missing captured manifests remain omissions.
                for id in &selected {
                    let entity = data.entities.get_mut(id).unwrap();
                    if entity.kind == EntityKind::Dependency {
                        if let Some(path) = entity.fields.get("source").and_then(Value::as_str) {
                            let rows=self.rows("SELECT size_bytes,content_hash FROM captured_sources WHERE path=?1",&[&path])?;
                            if let Some(row) = rows.first() {
                                entity.fields.insert(
                                    "location".into(),
                                    json!({"file":path,"start":0,"end":row["size_bytes"]}),
                                );
                                entity.fields.insert(
                                    "evidence_scope".into(),
                                    json!("captured_manifest_file"),
                                );
                            }
                        }
                    }
                }
            } else if kind == "repository_map" {
                selected.retain(|id| {
                    matches!(
                        data.entities[id].kind,
                        EntityKind::File | EntityKind::Symbol
                    )
                });
                data.edges.retain(|edge| {
                    matches!(edge.relation, RelationKind::DefinedIn | RelationKind::Calls)
                });
            }
        }
        let mut result = self.relationship_view(&selected, &data, max_nodes, max_edges)?;
        result["provider"] = json!(kind);
        result["provider_version"] = json!(1);
        result["derivations"] = json!(derivations);
        result["scope"] = json!({"query_hash":hash,"matched":selected.intersection(matches).collect::<Vec<_>>(),"context":selected.difference(matches).collect::<Vec<_>>()});
        let rendered: BTreeSet<_> = result["spec"]["elements"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["id"].as_str().map(str::to_owned))
            .collect();
        result["scope"] = json!({"query_hash":hash,"matched":rendered.intersection(matches).collect::<Vec<_>>(),"context":rendered.difference(matches).collect::<Vec<_>>()});
        let mut references = serde_json::Map::new();
        let mut drill = serde_json::Map::new();
        for id in &rendered {
            references.insert(
                id.clone(),
                json!([EntityRef {
                    repository: self.repository.clone(),
                    revision: self.revision.clone(),
                    kind: data.entities[id].kind,
                    id: id.clone()
                }]),
            );
            drill.insert(
                id.clone(),
                json!(self.encode_expiring(json!({"type":"cell","ids":[id]}))?),
            );
        }
        // Also retain collapsed entity identities as bounded retrieval handles.
        for id in selected.difference(&rendered).take(max_nodes) {
            drill.insert(
                id.clone(),
                json!(self.encode_expiring(json!({"type":"cell","ids":[id]}))?),
            );
        }
        result["entity_refs"] = Value::Object(references);
        result["drill_down"] = Value::Object(drill);
        Ok(result)
    }
}
