//! Architecture identities over captured build/stack records, never a live-tree walk.
use super::query::Dataset;
use super::*;

/// Ownership is the unique longest captured application directory, respecting
/// path segment boundaries. Duplicate roots are ambiguous, not first-wins.
fn owner(path: &str, apps: &[(String, String)]) -> Option<String> {
    let candidates: Vec<_> = apps
        .iter()
        .filter(|(_, dir)| dir.is_empty() || path.starts_with(&format!("{dir}/")))
        .collect();
    let longest = candidates.iter().map(|(_, dir)| dir.len()).max()?;
    let best: Vec<_> = candidates
        .into_iter()
        .filter(|(_, dir)| dir.len() == longest)
        .collect();
    (best.len() == 1).then(|| best[0].0.clone())
}
impl RevisionReader {
    /// Captured document/code verification, linked by recorded symbol IDs only.
    pub(crate) fn drift_entities(&self, ds: &mut Dataset, ex: &mut Execution) -> Result<()> {
        let drift = &self.artifacts["drift"];
        for verification in drift["verifications"].as_array().into_iter().flatten() {
            ex.check(1)?;
            let Some(claim) = drift["claims"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["id"] == verification["claim"])
            else {
                continue;
            };
            let Some(number) = claim["id"].as_u64() else {
                continue;
            };
            let id = self.id("drift_claim", number);
            let ids: BTreeSet<u64> = verification["symbols"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .chain(
                    drift["mappings"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|m| m["claim"] == claim["id"])
                        .flat_map(|m| m["symbols"].as_array().into_iter().flatten())
                        .filter_map(Value::as_u64),
                )
                .collect();
            let mut sources = Vec::new();
            for n in ids {
                let sid = self.id("symbol", n);
                if let Some(symbol) = ds.entities.get(&sid) {
                    sources.push(json!({"id":sid,"location":symbol.fields.get("location")}));
                    ds.edges.push(Edge {
                        source: id.clone(),
                        target: sid,
                        relation: RelationKind::SourceSymbol,
                    });
                }
            }
            let fields = json!({"origin":"captured_document_drift","claim":claim["text"],"status":verification["status"],"reason":verification["reason"],"document":claim["loc"],"sources":sources,"complete":drift["complete"],"incomplete_reason":drift["incomplete_reason"],"checker":verification["checker"]});
            ds.entities.insert(
                id.clone(),
                Entity {
                    id,
                    kind: EntityKind::Finding,
                    fields: serde_json::from_value(fields)?,
                    evidence_refs: vec![],
                },
            );
        }
        Ok(())
    }
    pub(crate) fn architecture_entities(&self, ds: &mut Dataset, ex: &mut Execution) -> Result<()> {
        let apps: Vec<Value> = self.snapshot["stack"]["apps"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut scopes = Vec::new();
        for (index, app) in apps.iter().enumerate() {
            ex.check(1)?;
            let Some(dir) = app["dir"].as_str() else {
                continue;
            };
            if dir.starts_with('/') || dir.split('/').any(|p| p == "..") {
                continue;
            }
            let id = self.id("container", index);
            let mut fields = app.clone();
            fields["origin"] = json!("captured_stack_application");
            ds.entities.insert(
                id.clone(),
                Entity {
                    id: id.clone(),
                    kind: EntityKind::Container,
                    fields: serde_json::from_value(fields)?,
                    evidence_refs: vec![],
                },
            );
            scopes.push((id, dir.trim_end_matches('/').to_string()));
        }
        // A declaration belongs to an app only through its captured manifest evidence.
        // Do not infer runtime use or discard a module with zero declarations.
        for (index, dependency) in self.artifacts["deps"]["table"]["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            ex.check(1)?;
            let Some(source) = dependency["source"].as_str() else {
                continue;
            };
            let owners: Vec<_> = apps
                .iter()
                .enumerate()
                .filter(|(_, app)| {
                    app["evidence"].as_str() == Some(source)
                        || app["manifests"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|m| m["evidence"].as_str() == Some(source))
                })
                .map(|(i, _)| self.id("container", i))
                .filter(|id| ds.entities.contains_key(id))
                .collect();
            let target = self.id("dependency", index);
            if owners.len() == 1 && ds.entities.contains_key(&target) {
                ds.edges.push(Edge {
                    source: owners[0].clone(),
                    target,
                    relation: RelationKind::DeclaresDependency,
                });
            }
        }
        let symbols: Vec<_> = ds
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Symbol)
            .cloned()
            .collect();
        let files: Vec<_> = ds
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::File)
            .cloned()
            .collect();
        for entity in symbols.iter().chain(files.iter()) {
            ex.check(1)?;
            let path = if entity.kind == EntityKind::File {
                entity.fields.get("path")
            } else {
                entity.fields.get("file").and_then(|f| f.get("path"))
            }
            .and_then(Value::as_str)
            .unwrap_or("");
            if path.is_empty() {
                continue;
            }
            if let Some(target) = owner(path, &scopes) {
                ds.edges.push(Edge {
                    source: entity.id.clone(),
                    target: target.clone(),
                    relation: RelationKind::MemberOfContainer,
                });
                // Representative captured member source, never a fabricated manifest span.
                if let Some(location) = entity.fields.get("location") {
                    ds.entities
                        .get_mut(&target)
                        .unwrap()
                        .fields
                        .entry("location".into())
                        .or_insert(location.clone());
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut records = Vec::new();
        for key in ["services", "repositories", "entities", "configClasses"] {
            for record in self.snapshot["stack"]["jvm"][key]
                .as_array()
                .into_iter()
                .flatten()
            {
                records.push((
                    record["rel"].as_str().unwrap_or("").to_string(),
                    record["name"].as_str().unwrap_or("").to_string(),
                    key.to_string(),
                ));
            }
        }
        for route in self.snapshot["stack"]["routes"]
            .as_array()
            .into_iter()
            .flatten()
        {
            records.push((
                route["rel"].as_str().unwrap_or("").to_string(),
                route["owner"].as_str().unwrap_or("").to_string(),
                "controller".into(),
            ));
        }
        for (path, name, role) in records {
            ex.check(1)?;
            if path.is_empty()
                || name.is_empty()
                || !seen.insert((path.clone(), name.clone(), role.clone()))
            {
                continue;
            }
            let candidates: Vec<_> = symbols
                .iter()
                .filter(|s| {
                    s.fields
                        .get("file")
                        .and_then(|f| f.get("path"))
                        .and_then(Value::as_str)
                        == Some(&path)
                        && s.fields.get("name").and_then(Value::as_str) == Some(&name)
                        && matches!(
                            s.fields.get("kind").and_then(Value::as_str),
                            Some("Class" | "Interface")
                        )
                })
                .collect();
            if candidates.len() != 1 {
                continue;
            }
            let class = candidates[0];
            let Some(unit) = owner(&path, &scopes) else {
                continue;
            };
            let id = self.id("stack_component", format!("{}:{role}", class.id));
            let fields = json!({"label":name,"kind":role,"origin":"captured_framework_role","provenance":"captured stack role; architectural grouping is inferred","source_symbol_ref":class.id,"location":class.fields.get("location"),"container_ref":unit});
            ds.entities.insert(
                id.clone(),
                Entity {
                    id: id.clone(),
                    kind: EntityKind::Component,
                    fields: serde_json::from_value(fields)?,
                    evidence_refs: vec![],
                },
            );
            ds.edges.push(Edge {
                source: id.clone(),
                target: unit,
                relation: RelationKind::ComponentInContainer,
            });
            let mut members: BTreeSet<u64> = class
                .fields
                .get("id")
                .and_then(Value::as_u64)
                .into_iter()
                .collect();
            loop {
                let before = members.len();
                for symbol in &symbols {
                    ex.check(1)?;
                    if symbol
                        .fields
                        .get("parent")
                        .and_then(Value::as_u64)
                        .is_some_and(|p| members.contains(&p))
                    {
                        if let Some(n) = symbol.fields.get("id").and_then(Value::as_u64) {
                            members.insert(n);
                        }
                    }
                }
                if members.len() == before {
                    break;
                }
            }
            for symbol in &symbols {
                if symbol
                    .fields
                    .get("id")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| members.contains(&n))
                {
                    ds.edges.push(Edge {
                        source: symbol.id.clone(),
                        target: id.clone(),
                        relation: RelationKind::MemberOfComponent,
                    });
                }
            }
        }
        for record in self.snapshot["stack"]["crossService"]
            .as_array()
            .into_iter()
            .flatten()
        {
            ex.check(1)?;
            let find = |name: &Value| -> Option<String> {
                let matched: Vec<_> = apps
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a["name"] == *name)
                    .collect();
                if name.is_string() && matched.len() == 1 {
                    Some(self.id("container", matched[0].0))
                } else {
                    None
                }
            };
            if let Some((source, target)) = find(&record["fromApp"]).zip(find(&record["toApp"])) {
                if ds.entities.contains_key(&source) && ds.entities.contains_key(&target) {
                    ds.edges.push(Edge {
                        source,
                        target,
                        relation: RelationKind::CommunicatesWith,
                    });
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_ownership_is_segment_bounded_longest_and_ambiguity_preserving() {
        let apps = vec![
            ("root".into(), "".into()),
            ("api".into(), "api".into()),
            ("nested".into(), "api/admin".into()),
        ];
        assert_eq!(owner("api/admin/A.java", &apps), Some("nested".into()));
        assert_eq!(owner("api/A.java", &apps), Some("api".into()));
        assert_eq!(owner("apix/A.java", &apps), Some("root".into()));
        assert_eq!(
            owner(
                "api/A.java",
                &[("a".into(), "api".into()), ("b".into(), "api".into())]
            ),
            None
        );
    }
}
