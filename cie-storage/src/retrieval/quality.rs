//! File quality is projected from the captured artifact, never measured by reads.
use super::query::Dataset;
use super::*;
fn is_quality(field: &str) -> bool {
    field == "quality" || field.starts_with("quality.")
}
fn predicate_needs(p: &Predicate) -> bool {
    match p {
        Predicate::Field { field, .. } => is_quality(field),
        Predicate::And(p) | Predicate::Or(p) => p.iter().any(predicate_needs),
        Predicate::Not(p) => predicate_needs(p),
        Predicate::Related { target, .. } => query_needs(target),
        _ => false,
    }
}
fn query_needs(q: &Query) -> bool {
    match q {
        Query::Select { filter, .. } => predicate_needs(filter),
        Query::Union(q) | Query::Intersect(q) => q.iter().any(query_needs),
        Query::Difference { left, right } => query_needs(left) || query_needs(right),
        Query::Follow { input, .. } => query_needs(input),
        _ => false,
    }
}
pub(crate) fn needed(request: &Request) -> bool {
    query_needs(&request.query)
        || request.outputs.iter().any(|o| match &o.output {
            Output::Entities { fields, order, .. } => {
                fields.iter().any(|f| is_quality(f)) || order.iter().any(|s| is_quality(&s.field))
            }
            Output::Facets { dimensions, .. } => dimensions.iter().any(|d| is_quality(d)),
            Output::Cube(c) => {
                c.dimensions.iter().any(|d| match &d.dimension {
                    DimensionRef::Field(f)
                    | DimensionRef::NumericBucket { field: f, .. }
                    | DimensionRef::TimeBucket { field: f, .. } => is_quality(f),
                    _ => false,
                }) || c.measures.iter().any(|m| match &m.measure {
                    Measure::Sum(f) | Measure::Average(f) | Measure::Min(f) | Measure::Max(f) => {
                        is_quality(f)
                    }
                    _ => false,
                })
            }
            _ => false,
        })
}
fn excluded(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part,
            "node_modules"
                | "vendor"
                | "target"
                | "dist"
                | "build"
                | "generated"
                | "__generated__"
                | "codegen"
        )
    }) || path.ends_with(".pb.go")
        || path.ends_with("_pb2.py")
        || path.ends_with(".generated.ts")
}
fn number(v: &Value) -> Value {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map_or(Value::Null, |_| v.clone())
}
fn percent(v: &Value) -> Value {
    v.as_f64()
        .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
        .map_or(Value::Null, |_| v.clone())
}
fn scanned(section: &Value, path: &str) -> bool {
    section["scanned_files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|f| f.as_str() == Some(path)))
}
fn insert_section(
    index: &mut BTreeMap<String, Value>,
    section: &Value,
    fields: &[(&str, &str)],
    ex: &mut Execution,
) -> Result<()> {
    for row in section.as_array().into_iter().flatten() {
        ex.check(1)?;
        let Some(path) = row["file"].as_str() else {
            continue;
        };
        let entry = index.entry(path.to_string()).or_insert_with(|| json!({}));
        for (source, target) in fields {
            entry[*target] = if *source == "percent" {
                percent(&row[*source])
            } else {
                number(&row[*source])
            };
        }
    }
    Ok(())
}
impl RevisionReader {
    pub(crate) fn quality_fields(&self, ds: &mut Dataset, ex: &mut Execution) -> Result<()> {
        let q = &self.artifacts["quality"];
        let mut index = BTreeMap::new();
        insert_section(
            &mut index,
            &q["coverage_heatmap"]["files"],
            &[("percent", "coverage_percent")],
            ex,
        )?;
        insert_section(
            &mut index,
            &q["complexity"]["files"],
            &[("avg_cc", "avg_cc"), ("max_cc", "max_cc")],
            ex,
        )?;
        insert_section(
            &mut index,
            &q["hotspots"]["rows"],
            &[("risk", "hotspot_risk")],
            ex,
        )?;
        let mut duplicates: BTreeMap<String, usize> = BTreeMap::new();
        for pair in q["duplication"]["pairs"].as_array().into_iter().flatten() {
            ex.check(1)?;
            let files: BTreeSet<_> = [pair["a_file"].as_str(), pair["b_file"].as_str()]
                .into_iter()
                .flatten()
                .collect();
            if files.iter().any(|file| excluded(file)) {
                continue;
            }
            for file in files {
                *duplicates.entry(file.to_string()).or_default() += 1;
            }
        }
        let mut security: BTreeMap<String, usize> = BTreeMap::new();
        for issue in q["security"]["issues"].as_array().into_iter().flatten() {
            ex.check(1)?;
            if let Some(file) = issue["file"].as_str() {
                *security.entry(file.to_string()).or_default() += 1;
            }
        }
        for entity in ds
            .entities
            .values_mut()
            .filter(|e| e.kind == EntityKind::File)
        {
            ex.check(1)?;
            let path = entity
                .fields
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("");
            let values = index.get(path).cloned().unwrap_or_else(|| json!({}));
            let duplicate_count = duplicates.get(path).copied().or_else(|| {
                (q["duplication"]["pairs"].is_array() && scanned(&q["duplication"], path))
                    .then_some(0)
            });
            let security_count = security.get(path).copied().or_else(|| {
                (q["security"]["issues"].is_array() && scanned(&q["security"], path)).then_some(0)
            });
            let projection = json!({"available":q.is_object(),"excluded":excluded(path),"coverage_percent":values["coverage_percent"],"avg_cc":values["avg_cc"],"max_cc":values["max_cc"],"duplicate_pairs":duplicate_count,"hotspot_risk":values["hotspot_risk"],"security_findings":security_count,"cohesion_status":"not yet measured","collected_at":q["collected_at"],"honesty":q["honesty"],"risk_formula":q["hotspots"]["formula"]});
            entity.fields.insert("quality".into(), projection);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_paths_and_invalid_measurements_are_not_treated_as_real_scores() {
        assert!(excluded("src/generated/A.java"));
        assert!(excluded("x/a.pb.go"));
        assert!(!excluded("src/building/A.java"));
        assert_eq!(percent(&json!(0)), json!(0));
        assert_eq!(percent(&json!(101)), Value::Null);
        assert_eq!(number(&Value::Null), Value::Null);
    }
}
