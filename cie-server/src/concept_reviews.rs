//! Judgment agreement statistics are not probabilities of semantic correctness.
use crate::{api::App, model::*};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
pub fn dispatch(app: &Arc<App>, method: &str, path: &str) -> Option<Result<Value>> {
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    let ["api", "concept-reviews", repo_id, rev_id] = parts.as_slice() else {
        return None;
    };
    if method != "GET" {
        return None;
    }
    Some((|| {
        let s = app.storage.read()?;
        let repo = s
            .repositories
            .iter()
            .find(|r| r.id == *repo_id)
            .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
        let rev = repo
            .revisions
            .iter()
            .find(|v| v.id == *rev_id)
            .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Concept snapshot unavailable"))?;
        let mut latest = BTreeMap::new();
        for review in s
            .concept_reviews
            .iter()
            .filter(|c| c.repository_id == *repo_id && c.revision_id == *rev_id)
        {
            latest.insert(&review.item_id, review);
        }
        let synthetic =
            std::path::Path::new(&repo.path).starts_with(app.storage.directory.join("samples"));
        let eligible = latest.len() >= 30 && !synthetic;
        let cards:Vec<_>=rev.snapshot.concepts.nodes.iter().take(500).map(|c|{let key=format!("concept:{}",c.id);let review=latest.get(&key);json!({"id":key,"label":c.label,"uncalibratedScore":c.confidence,"citations":c.symbols.iter().take(3).filter_map(|id|rev.snapshot.symbols.iter().find(|s|s.id==*id)).filter_map(|symbol|crate::answers::citation(repo,rev,symbol,None)).collect::<Vec<_>>(),"judgment":review.map(|r|r.verdict.as_str()),"reason":review.map(|r|r.reason.as_str())})}).collect();
        let bins:Vec<_>=(0..5).map(|b|{let lower=b as f32/5.0;let upper=(b+1)as f32/5.0;let group:Vec<_>=rev.snapshot.concepts.nodes.iter().filter(|c|c.confidence>=lower&&(c.confidence<upper||b==4&&c.confidence<=1.0)).filter_map(|c|latest.get(&format!("concept:{}",c.id))).collect();let count=group.len();json!({"lowerScore":lower,"upperScore":upper,"reviewed":count,"confirm":group.iter().filter(|r|r.verdict=="confirm").count(),"dispute":group.iter().filter(|r|r.verdict=="dispute").count(),"refute":group.iter().filter(|r|r.verdict=="refute").count(),"displayable":eligible&&count>=5})}).collect();
        Ok(
            json!({"repositoryId":repo.id,"revisionId":rev.id,"cards":cards,"cardsCapped":rev.snapshot.concepts.nodes.len()>500,"reviewedUniqueCards":latest.len(),"minimumForAgreementSummary":30,"synthetic":synthetic,"agreementSummaryAvailable":eligible,"bins":if eligible{json!(bins)}else{json!([])},"calibratedProbabilityAvailable":false,"gaps":["Engine scores are not calibrated probabilities. User judgments are not independently established expert truth.","No agreement chart is offered until 30 distinct cards are reviewed, and bins need at least five judgments.","Generated sample repositories are excluded from agreement statistics; their individual reviews remain inspectable.","Refuted concepts remain withheld across answers for this exact snapshot; other snapshots require their own review."]}),
        )
    })())
}
