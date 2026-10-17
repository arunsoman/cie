//! Comparison is conditional on producer-supplied workload metadata. Failed and
//! omitted work remains in the result; a lower duration is not an established
//! performance improvement or a functional acceptance result.
use crate::{api::App, model::*};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
pub(crate) fn validate(m: &ProfileMeasurement) -> Result<()> {
    if m.workload_key.trim().is_empty()
        || m.workload_key.len() > 500
        || m.environment_key.trim().is_empty()
        || m.environment_key.len() > 500
        || m.failed_operations > m.recorded_operations
        || [
            m.expected_operations,
            m.recorded_operations,
            m.failed_operations,
            m.omitted_operations,
            m.lost_samples,
            m.elapsed_micros,
            m.sampling_interval_micros,
            m.warmup_micros,
        ]
        .into_iter()
        .any(|n| n > 9_007_199_254_740_991)
    {
        return Err(Error::new(400,"MEASUREMENT_SCHEMA","Use bounded counters, declared workload/environment keys and failedOperations no greater than recordedOperations"));
    }
    Ok(())
}
fn metric(s: &State, o: &Observation) -> Value {
    let source_freshness = s
        .repositories
        .iter()
        .find(|r| r.id == o.repository_id)
        .and_then(|repo| {
            repo.revisions
                .iter()
                .find(|v| v.id == o.revision_id)
                .map(|rev| crate::repository::freshness(repo, rev))
        })
        .unwrap_or("unavailable");
    let m = &o.measurement;
    json!({"reportId":o.id,"sourceFreshness":source_freshness,"sourceFreshnessScope":crate::repository::FRESHNESS_SCOPE,"repositoryId":o.repository_id,"revisionId":o.revision_id,"environment":o.environment,"producer":o.producer,"recordCount":o.events.len(),"reportedErrorRecords":o.events.iter().filter(|e|["error","failed"].contains(&e.outcome.as_str())).count(),"measurement":m,"successfulOperations":m.as_ref().map(|m|m.recorded_operations-m.failed_operations),"claimClass":"importedObservation"})
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    if method != "POST" || path != "/api/profile-comparisons" {
        return None;
    }
    Some((|| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Compare {
            baseline_id: String,
            candidate_id: String,
            equivalence_note: String,
        }
        let req: Compare = serde_json::from_slice(body)
            .map_err(|_| Error::new(400, "INVALID_COMPARISON", "Comparison schema is invalid"))?;
        if req.equivalence_note.trim().len() < 10 || req.equivalence_note.len() > 2000 {
            return Err(Error::new(
                400,
                "EQUIVALENCE_REQUIRED",
                "Explain the declared workload equivalence in 10–2000 characters",
            ));
        }
        let s = app.storage.read()?;
        let baseline = s
            .observations
            .iter()
            .find(|o| o.id == req.baseline_id)
            .ok_or_else(|| Error::new(404, "REPORT_UNAVAILABLE", "Baseline report unavailable"))?;
        let candidate = s
            .observations
            .iter()
            .find(|o| o.id == req.candidate_id)
            .ok_or_else(|| Error::new(404, "REPORT_UNAVAILABLE", "Candidate report unavailable"))?;
        let mut reasons = vec![];
        if baseline.id == candidate.id {
            reasons.push("Baseline and candidate are the same report".to_string());
        }
        if baseline.repository_id != candidate.repository_id {
            reasons.push("Reports belong to different repositories; no cross-repository equivalence was inferred".into());
        }
        if baseline.kind != "profile" || candidate.kind != "profile" {
            reasons.push("Both reports must be profiles".into());
        }
        let mut delta = None;
        if let (Some(a), Some(b)) = (&baseline.measurement, &candidate.measurement) {
            if a.workload_key != b.workload_key {
                reasons.push("Declared workload keys differ".into());
            }
            if a.environment_key != b.environment_key
                || baseline.environment != candidate.environment
            {
                reasons.push("Declared environments differ".into());
            }
            if baseline.producer != candidate.producer {
                reasons.push("Recording producers differ".into());
            }
            if a.sampling_interval_micros == 0
                || a.sampling_interval_micros != b.sampling_interval_micros
            {
                reasons.push("Sampling intervals differ or are unknown".into());
            }
            if a.warmup_micros != b.warmup_micros {
                reasons.push("Warmup conditions differ".into());
            }
            if a.expected_operations == 0
                || a.expected_operations != b.expected_operations
                || a.recorded_operations != b.recorded_operations
            {
                reasons.push("Expected/recorded workload populations differ or are empty".into());
            }
            if a.failed_operations != b.failed_operations {
                reasons.push("Failed-operation populations differ; dropping failures cannot establish an improvement".into());
            }
            if !a.capture_complete
                || !b.capture_complete
                || a.omitted_operations > 0
                || b.omitted_operations > 0
                || a.lost_samples > 0
                || b.lost_samples > 0
            {
                reasons.push("Capture is incomplete or work/samples were omitted".into());
            }
            if a.recorded_operations + a.omitted_operations != a.expected_operations
                || b.recorded_operations + b.omitted_operations != b.expected_operations
            {
                reasons
                    .push("Reported operation counters do not cover the expected workload".into());
            }
            if a.elapsed_micros == 0 || b.elapsed_micros == 0 {
                reasons.push("Elapsed capture duration is missing or zero".into());
            }
            delta = Some(b.elapsed_micros as i64 - a.elapsed_micros as i64);
        } else {
            reasons.push("Declared workload and measurement metadata is missing".into());
        }
        let eligible = reasons.is_empty();
        let verdict = if !eligible {
            "notComparable"
        } else {
            match delta.unwrap_or(0).cmp(&0) {
                std::cmp::Ordering::Less => "recordedElapsedLower",
                std::cmp::Ordering::Greater => "recordedElapsedHigher",
                std::cmp::Ordering::Equal => "recordedElapsedEqual",
            }
        };
        Ok(
            json!({"verdict":verdict,"comparisonEligible":eligible,"equivalenceNote":req.equivalence_note,"equivalenceVerifiedByCie":false,"performanceImprovementEstablished":false,"rows":[metric(&s,baseline),metric(&s,candidate)],"recordedElapsedDeltaMicros":delta,"reasons":reasons,"gaps":["Workload/environment equivalence and completeness are producer/user declarations, not independently verified facts.","Failed runs remain in the comparison population. A shorter capture is not proof of improved behavior, throughput or correctness.","No repeated-run variance, confidence interval or representative latency distribution is available; statistical significance is not determined.","No per-frame correspondence is inferred across revisions. Source mapping still requires each report's exact source hash.","The elapsed delta describes the reported whole capture, not a per-request latency percentile."]}),
        )
    })())
}
