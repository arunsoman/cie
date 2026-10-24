//! A safe indexed fast path for common file/source lookups. Predicates and
//! projections needing other entity families use the general bounded planner.
use super::query::Dataset;
use super::*;
impl RevisionReader {
    pub(crate) fn file_plan(
        &self,
        request: &Request,
        ex: &mut Execution,
    ) -> Result<Option<Dataset>> {
        let Query::Select {
            kind: EntityKind::File,
            filter,
        } = &request.query
        else {
            return Ok(None);
        };
        if request.outputs.iter().any(|named| {
            !matches!(
                named.output,
                Output::Entities { .. }
                    | Output::Evidence { .. }
                    | Output::Artifact { .. }
                    | Output::Detail(Detail::Source { .. })
            )
        }) {
            return Ok(None);
        }
        let Predicate::Field {
            field,
            op: CompareOp::Eq,
            value,
        } = filter
        else {
            return Ok(None);
        };
        if !matches!(field.as_str(), "path" | "file.path") {
            return Ok(None);
        }
        let Some(path) = value.as_str() else {
            return Err(StorageError::new(
                "InvalidRequest",
                "File path must be a string",
            ));
        };
        let path = query::normalize_repo_path(path)?;
        let rows=self.rows("SELECT id,path,language,size_bytes,lower(hex(content_hash)) AS content_hash FROM files WHERE path=?1 ORDER BY id",&[&path])?;
        let mut entities = BTreeMap::new();
        for row in rows {
            ex.check(1)?;
            let id = self.id(
                "file",
                row["id"]
                    .as_i64()
                    .ok_or_else(|| StorageError::new("CorruptData", "Invalid file identity"))?,
            );
            entities.insert(
                id.clone(),
                Entity {
                    id,
                    kind: EntityKind::File,
                    evidence_refs: Vec::new(),
                    fields: serde_json::from_value(row)?,
                },
            );
        }
        // Semantic-only files have stable identities in the core file table.
        if entities.is_empty() {
            if let Some(index) = self.snapshot["files"]
                .as_array()
                .and_then(|files| files.iter().position(|file| file.as_str() == Some(&path)))
            {
                let id = self.id("semantic_file", index);
                entities.insert(
                    id.clone(),
                    Entity {
                        id,
                        kind: EntityKind::File,
                        evidence_refs: Vec::new(),
                        fields: serde_json::from_value(json!({"path":path,"language":"unknown"}))?,
                    },
                );
            }
        }
        Ok(Some(Dataset {
            entities,
            edges: Vec::new(),
        }))
    }
    /// Logical planning diagnostics disclose strategy, not physical schema or SQL.
    pub fn explain(&self, request: &Request) -> Value {
        let indexed = matches!(&request.query,Query::Select{kind:EntityKind::File,filter:Predicate::Field{field,op:CompareOp::Eq,..}} if field=="path"||field=="file.path")
            && request.outputs.iter().all(|o| {
                matches!(
                    o.output,
                    Output::Entities { .. }
                        | Output::Evidence { .. }
                        | Output::Artifact { .. }
                        | Output::Detail(Detail::Source { .. })
                )
            });
        json!({"revision":self.revision,"strategy":if indexed{"indexed_file_lookup"}else{"bounded_fact_query"},"decoded_snapshot_fields":self.snapshot.decoded_fields()})
    }
}
