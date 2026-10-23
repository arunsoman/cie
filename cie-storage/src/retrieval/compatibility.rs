//! Transitional adapters preserving existing UI payloads. SQL is storage-owned.
use super::*;
impl RevisionReader {
    pub fn ui_compatibility(&self, kind: &str, args: &Value) -> Result<Value> {
        let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as i64;
        let offset = args["offset"].as_u64().unwrap_or(0).min(100_000) as i64;
        match kind {
            "symbols" => {
                let q = args["query"]
                    .as_str()
                    .unwrap_or("")
                    .replace('!', "!!")
                    .replace('%', "!%")
                    .replace('_', "!_");
                self.rows("SELECT s.id AS symbolId,s.qual_name AS qualName,s.kind,f.path FROM symbols s JOIN files f ON f.id=s.file_id WHERE s.qual_name LIKE ?1 ESCAPE '!' ORDER BY length(s.qual_name),s.id LIMIT ?2",&[&format!("%{q}%"),&limit]).map(|r|json!(r))
            }
            "function_ir" => self.compat_ir(
                args["symbolId"]
                    .as_i64()
                    .ok_or_else(|| StorageError::new("InvalidRequest", "Missing symbolId"))?,
            ),
            "duplicates" => self.compat_duplicates(args, limit, offset),
            "duplicate_clusters" => self.compat_clusters(args),
            "summary" => self.compat_summary(),
            "tables" => {
                let mut tables = Vec::new();
                for (name, group, note) in DATASETS {
                    let count = self
                        .conn
                        .query_row(&format!("SELECT count(*) FROM \"{name}\""), [], |r| {
                            r.get::<_, i64>(0)
                        })
                        .ok();
                    tables.push(json!({"name":name,"group":group,"note":note,"count":count}));
                }
                Ok(json!(tables))
            }
            "table_rows" => {
                let name = args["table"].as_str().unwrap_or("");
                if !DATASETS.iter().any(|(n, _, _)| *n == name) {
                    return Err(StorageError::new(
                        "InvalidRequest",
                        "Unknown diagnostic dataset",
                    ));
                }
                let sql = format!("SELECT * FROM \"{name}\" ORDER BY rowid LIMIT ?1 OFFSET ?2");
                let mut rows = self.rows(&sql, &[&limit, &offset])?;
                let columns = self
                    .conn
                    .prepare(&format!("SELECT * FROM \"{name}\" LIMIT 0"))?
                    .column_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                for row in &mut rows {
                    for (key, value) in row.as_object_mut().unwrap() {
                        if value.get("binary_bytes").is_some() {
                            *value = json!(format!("<binary {} bytes>", value["binary_bytes"]));
                        } else if let Some(text) = value.as_str() {
                            if text.chars().count() > 2000 {
                                *value = json!(format!(
                                    "{}… ({} chars)",
                                    text.chars().take(2000).collect::<String>(),
                                    text.chars().count()
                                ));
                            }
                        }
                        if key.contains("password") || key.contains("secret") {
                            *value = json!("<redacted>");
                        }
                    }
                }
                let total = self.count(&format!("SELECT count(*) FROM \"{name}\""))?;
                Ok(json!({"table":name,"columns":columns,"rows":rows,"total":total}))
            }
            _ => Err(StorageError::new(
                "UnsupportedOutput",
                "Unknown compatibility adapter",
            )),
        }
    }
    fn count(&self, sql: &str) -> Result<i64> {
        Ok(self.conn.query_row(sql, [], |r| r.get(0))?)
    }
    fn compat_symbol(&self, id: i64) -> Result<Value> {
        Ok(self.rows("SELECT s.id AS symbolId,s.qual_name AS qualName,s.kind,f.path,s.start_byte AS startByte,s.end_byte AS endByte FROM symbols s JOIN files f ON f.id=s.file_id WHERE s.id=?1",&[&id])?.into_iter().next().unwrap_or(Value::Null))
    }
    fn compat_ir(&self, id: i64) -> Result<Value> {
        let symbol = self.compat_symbol(id)?;
        if symbol.is_null() {
            return Err(StorageError::new("ArtifactUnavailable", "Symbol missing"));
        }
        let mut blocks=self.rows("SELECT id,block_index AS blockIndex,kind FROM ir_blocks WHERE symbol_id=?1 ORDER BY block_index",&[&id])?;
        let mut indices = BTreeMap::new();
        for block in &mut blocks {
            let bid = block["id"].as_i64().unwrap();
            indices.insert(bid, block["blockIndex"].clone());
            block["instructions"]=json!(self.rows("SELECT inst_index AS instIndex,op,span_start AS spanStart,span_end AS spanEnd FROM ir_instructions WHERE block_id=?1 ORDER BY inst_index",&[&bid])?);
        }
        let edges=self.rows("SELECT e.* FROM ir_edges e JOIN ir_blocks b ON b.id=e.from_block_id WHERE b.symbol_id=?1 ORDER BY e.from_block_id,e.to_block_id",&[&id])?.into_iter().filter_map(|e|Some(json!({"fromBlockIndex":indices.get(&e["from_block_id"].as_i64()?)?,"toBlockIndex":indices.get(&e["to_block_id"].as_i64()?)?,"kind":e["kind"]}))).collect::<Vec<_>>();
        let calls=self.rows("SELECT c.callee_text AS calleeText,s.qual_name AS calleeQualName,c.resolution,c.call_kind AS callKind,c.stmt_index AS stmtIndex FROM calls c LEFT JOIN symbols s ON s.id=c.callee_symbol_id WHERE c.caller_symbol_id=?1 ORDER BY c.stmt_index,c.id",&[&id])?;
        Ok(json!({"symbol":symbol,"blocks":blocks,"edges":edges,"calls":calls}))
    }
    fn compat_duplicates(&self, args: &Value, limit: i64, offset: i64) -> Result<Value> {
        let mut predicates = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        for field in ["tier", "detection", "epistemic"] {
            if let Some(v) = args[field].as_str() {
                if !v.is_empty() {
                    predicates.push(format!("{field}=?"));
                    values.push(v.to_string().into());
                }
            }
        }
        let where_sql = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };
        let total: i64 = self.conn.query_row(
            &format!("SELECT count(*) FROM duplicates{where_sql}"),
            rusqlite::params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        values.push(limit.into());
        values.push(offset.into());
        let params = values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect::<Vec<_>>();
        let mut rows = self.rows(
            &format!("SELECT * FROM duplicates{where_sql} ORDER BY id LIMIT ? OFFSET ?"),
            &params,
        )?;
        for row in &mut rows {
            row["a"] = self.compat_symbol(row["symbol_a"].as_i64().unwrap())?;
            row["b"] = self.compat_symbol(row["symbol_b"].as_i64().unwrap())?;
            row["canonical"] = if let Some(id) = row["canonical_symbol_id"].as_i64() {
                self.compat_symbol(id)?
            } else {
                Value::Null
            };
            row["counterArgument"] = row["counter_argument"].clone();
        }
        Ok(
            json!({"rows":rows,"total":total,"facets":{"tiers":["canonical","high","medium","low"],"detections":["ir-hash-match","ir-vec","ir-shingle","ir-vec-cosine"],"epistemics":["Fact","Inference","Hypothesis"]}}),
        )
    }
    fn compat_clusters(&self, args: &Value) -> Result<Value> {
        let max_clusters = args["maxClusters"].as_u64().unwrap_or(12).min(500) as usize;
        let max_members = args["maxMembersPerCluster"].as_u64().unwrap_or(24).min(500) as usize;
        let rows = self.rows(
            "SELECT cluster_id,symbol_id FROM duplicate_clusters ORDER BY cluster_id,ordinal",
            &[],
        )?;
        let mut families: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for row in &rows {
            families
                .entry(row["cluster_id"].as_i64().unwrap())
                .or_default()
                .push(row["symbol_id"].as_i64().unwrap());
        }
        let mut families: Vec<_> = families.into_values().collect();
        families.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
        let total = families.len();
        let mut clusters = Vec::new();
        for family in families.into_iter().take(max_clusters) {
            if family.len() < 2 {
                continue;
            }
            let hub = self.compat_symbol(family[0])?;
            let members = family
                .iter()
                .skip(1)
                .take(max_members)
                .map(|id| self.compat_symbol(*id))
                .collect::<Result<Vec<_>>>()?;
            clusters.push(json!({"hub":hub,"members":members,"totalMembers":family.len()-1}));
        }
        Ok(
            json!({"tierUsed":"canonical","totalFamilies":total,"totalSymbolsInvolved":rows.len(),"clusters":clusters}),
        )
    }
    fn compat_summary(&self) -> Result<Value> {
        let calls = self.count("SELECT count(*) FROM calls")?;
        let resolved = self.count("SELECT count(*) FROM calls WHERE resolution='RESOLVED'")?;
        let grouped = |table: &str| -> Result<Value> {
            let mut map = serde_json::Map::new();
            for row in self.rows(
                &format!("SELECT tier,count(*) AS n FROM {table} GROUP BY tier"),
                &[],
            )? {
                map.insert(row["tier"].as_str().unwrap().into(), row["n"].clone());
            }
            Ok(Value::Object(map))
        };
        Ok(
            json!({"dbPath":format!("revision:{}",self.revision),"ingest":{"files":self.count("SELECT count(*) FROM files")?,"astNodes":self.count("SELECT count(*) FROM ast_nodes")?,"symbols":self.count("SELECT count(*) FROM symbols")?},"ir":{"blocks":self.count("SELECT count(*) FROM ir_blocks")?,"instructions":self.count("SELECT count(*) FROM ir_instructions")?,"calls":calls,"callsResolved":resolved,"callsUnresolved":calls-resolved},
            "frameworks":{"totalRoles":self.count("SELECT count(*) FROM framework_roles")?,"roleSites":self.rows("SELECT fr.feature,fr.framework,fr.evidence_text,fr.confidence,s.qual_name,f.path FROM framework_roles fr LEFT JOIN symbols s ON s.id=fr.symbol_id JOIN files f ON f.id=fr.file_id ORDER BY fr.id LIMIT 2000",&[])?},
            "c4":{"containers":self.rows("SELECT id,label,kind,confidence,provenance FROM concept_nodes WHERE kind='Container' ORDER BY id LIMIT 500",&[])?,"components":self.rows("SELECT id,label,kind,confidence,provenance FROM concept_nodes WHERE kind='Component' ORDER BY id LIMIT 2000",&[])?,"relationships":self.rows("SELECT r.kind,r.resolution,r.confidence,a.label AS from_label,b.label AS to_label FROM c4_relationships r JOIN concept_nodes a ON a.id=r.from_node JOIN concept_nodes b ON b.id=r.to_node ORDER BY r.confidence DESC LIMIT 2000",&[])?},
            "stateMachines":{"total":self.count("SELECT count(*) FROM state_machines")?,"byTier":grouped("state_machines")?,"rows":self.rows("SELECT sm.id,sm.name,sm.tier,sm.epistemic,sm.coverage,sm.counter_argument,sm.fog_reason,s.qual_name FROM state_machines sm LEFT JOIN symbols s ON s.id=sm.source_symbol_id ORDER BY sm.id LIMIT 2000",&[])?},
            "workflows":{"total":self.count("SELECT count(*) FROM workflows")?,"totalNodes":self.count("SELECT count(*) FROM workflow_nodes")?,"rows":self.rows("SELECT w.id,w.name,w.entry_role,w.epistemic,w.counter_argument,s.qual_name,(SELECT count(*) FROM workflow_nodes wn WHERE wn.workflow_id=w.id) AS node_count FROM workflows w LEFT JOIN symbols s ON s.id=w.entry_symbol_id ORDER BY node_count DESC LIMIT 2000",&[])?},
            "duplicates":{"totalPairs":self.count("SELECT count(*) FROM duplicates")?,"byTier":grouped("duplicates")?}}),
        )
    }
}

const DATASETS: &[(&str, &str, &str)] = &[
    (
        r#"files"#,
        r#"Ingest & AST"#,
        r#"every ingested file, one row each"#,
    ),
    (
        r#"node_kinds"#,
        r#"Ingest & AST"#,
        r#"tree-sitter grammar node-kind lookup"#,
    ),
    (
        r#"node_text"#,
        r#"Ingest & AST"#,
        r#"grammar node text lookup"#,
    ),
    (
        r#"uast_kinds"#,
        r#"Ingest & AST"#,
        r#"universal-AST kind lookup"#,
    ),
    (
        r#"uast_map"#,
        r#"Ingest & AST"#,
        r#"per-language kind → universal-AST kind mapping"#,
    ),
    (
        r#"ast_nodes"#,
        r#"Ingest & AST"#,
        r#"the parsed AST, one row per node"#,
    ),
    (
        r#"symbols"#,
        r#"Ingest & AST"#,
        r#"named declarations found in the AST"#,
    ),
    (r#"calls"#, r#"IR"#, r#"call-site edges between symbols"#),
    (
        r#"ir_blocks"#,
        r#"IR"#,
        r#"lowered control-flow blocks, one per symbol's block"#,
    ),
    (
        r#"ir_instructions"#,
        r#"IR"#,
        r#"lowered IR instructions inside each block"#,
    ),
    (
        r#"ir_edges"#,
        r#"IR"#,
        r#"control-flow edges between blocks"#,
    ),
    (
        r#"file_source_cache"#,
        r#"IR"#,
        r#"zstd-compressed source per file (blob hidden below)"#,
    ),
    (
        r#"ir_meta"#,
        r#"IR"#,
        r#"per-symbol IR hash + vector (duplicate detection input)"#,
    ),
    (
        r#"framework_adapters"#,
        r#"Frameworks"#,
        r#"static per-language framework-signature registry"#,
    ),
    (
        r#"framework_roles"#,
        r#"Frameworks"#,
        r#"AST sites matching a registry signature"#,
    ),
    (
        r#"cross_language_bindings"#,
        r#"Frameworks"#,
        r#"API-spec ↔ client-call matches"#,
    ),
    (
        r#"concept_nodes"#,
        r#"C4 / Concepts"#,
        r#"C4 containers/components — needs a recognized manifest file"#,
    ),
    (
        r#"concept_edges"#,
        r#"C4 / Concepts"#,
        r#"edges between concept nodes"#,
    ),
    (
        r#"concept_node_symbols"#,
        r#"C4 / Concepts"#,
        r#"which symbols belong to which concept node"#,
    ),
    (
        r#"c4_relationships"#,
        r#"C4 / Concepts"#,
        r#"C4 relationships between containers/components"#,
    ),
    (
        r#"state_machines"#,
        r#"State machines"#,
        r#"detected state machines (T1 fact / T2 inference / T3 hypothesis)"#,
    ),
    (
        r#"workflows"#,
        r#"Workflows"#,
        r#"entry-point call-chain workflows"#,
    ),
    (
        r#"workflow_nodes"#,
        r#"Workflows"#,
        r#"nodes within each workflow"#,
    ),
    (
        r#"workflow_edges"#,
        r#"Workflows"#,
        r#"edges within each workflow"#,
    ),
    (
        r#"duplicates"#,
        r#"Duplicates"#,
        r#"see the Duplicate analysis view below for a resolved, filterable version"#,
    ),
    (
        r#"duplicate_gaps"#,
        r#"Duplicates"#,
        r#"per-symbol count of pairs hidden below the reporting threshold"#,
    ),
    (
        r#"runtime_evidence"#,
        r#"Runtime (OTel)"#,
        r#"needs an OTel trace payload (`cie polyglot --otel`) — empty otherwise"#,
    ),
    (
        r#"runtime_unresolved"#,
        r#"Runtime (OTel)"#,
        r#"needs an OTel trace payload — empty otherwise"#,
    ),
    (
        r#"runtime_summary"#,
        r#"Runtime (OTel)"#,
        r#"needs an OTel trace payload — empty otherwise"#,
    ),
    (
        r#"runtime_drift"#,
        r#"Runtime (OTel)"#,
        r#"needs an OTel trace payload — empty otherwise"#,
    ),
];
