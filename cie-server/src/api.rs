use crate::{
    answers::{self, Question},
    model::*,
    repository,
    storage::Storage,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub struct App {
    pub storage: Storage,
    pub allowed_roots: Vec<PathBuf>,
    index_lock: Mutex<()>,
}
impl App {
    pub fn open(data: &Path, allowed_roots: Vec<PathBuf>) -> Result<Arc<Self>> {
        if allowed_roots.is_empty() {
            return Err(Error::new(
                400,
                "NO_SCOPE",
                "At least one allowed repository root is required",
            ));
        }
        let mut roots = allowed_roots
            .into_iter()
            .map(std::fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        let storage = Storage::open(data)?;
        let samples = storage.directory.join("samples");
        std::fs::create_dir_all(&samples)?;
        roots.push(std::fs::canonicalize(samples)?);
        Ok(Arc::new(Self {
            storage,
            allowed_roots: roots,
            index_lock: Mutex::new(()),
        }))
    }
    pub fn dispatch(self: &Arc<Self>, method: &str, path: &str, body: &[u8]) -> Result<Value> {
        if body.len() > 1024 * 1024 {
            return Err(Error::new(413, "REQUEST_LIMIT", "Request exceeds 1 MiB"));
        }
        if let Some(result) = crate::plans::dispatch(self, method, path, body) {
            return result;
        }
        if let Some(result) = crate::queries::dispatch(self, method, path, body) {
            return result;
        }
        if let Some(result) = crate::orientation::dispatch(self, method, path, body) {
            return result;
        }
        if let Some(result) = crate::observations::dispatch(self, method, path, body) {
            return result;
        }
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        match (method,parts.as_slice()){
 ("GET",["api","health"])=>Ok(json!({"status":"ready","version":env!("CARGO_PKG_VERSION"),"mode":"singleUserLocal","cloudEnabled":false})),
 ("GET",["api","capabilities"])=>Ok(json!({"mode":"deterministicLocal","supportedQuestions":["overview","search","callers","callees","impact","tests","findings","concepts"],"representations":["table","graph","outline"],"unavailable":["runtime causality","capacity prediction","arbitrary charts without measurements","autonomous implementation","hosted model egress","multi-tenant access control"],"stories":serde_json::from_str::<Value>(include_str!("../web/stories.json")).unwrap_or(json!([]))})),
 ("GET",["api","state"])=>{let s=self.storage.read()?;Ok(json!({"repositories":s.repositories.iter().map(|r|json!({"id":r.id,"name":r.name,"path":r.path,"revisions":r.revisions.iter().map(|v|json!({"id":v.id,"createdNs":v.created_ns,"gitHead":v.git_head,"symbols":v.snapshot.symbols.len(),"findings":v.snapshot.findings.len()})).collect::<Vec<_>>() })).collect::<Vec<_>>(),"jobs":s.jobs,"answers":s.answers.iter().map(|a|json!({"id":a.id,"repositoryId":a.repository_id,"revisionId":a.revision_id,"question":a.question,"status":a.status})).collect::<Vec<_>>(),"workspaces":s.workspaces,"features":s.features,"privacy":{"cloudEnabled":false,"authorshipCollected":false,"scope":"One local user; no multi-tenant or remote access guarantee"}}))},
 ("POST",["api","repositories"])=>{
 #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Connect{path:String,name:Option<String>}
 let req:Connect=parse(body)?;let root=repository::allowed_path(Path::new(&req.path),&self.allowed_roots)?;if self.storage.directory.starts_with(&root)&&!self.storage.directory.file_name().is_some_and(|n|n==".cie-workspace"||n==".cie-product"){return Err(Error::new(400,"DATA_INSIDE_REPOSITORY","Place custom workspace data outside the connected repository, or use .cie-workspace"))}let name=req.name.unwrap_or_else(||root.file_name().unwrap_or_default().to_string_lossy().to_string());validate_text(&name,120)?;
 let repo=self.storage.transaction(|s|{if s.repositories.len()>=32{return Err(Error::new(409,"REPOSITORY_QUOTA","At most 32 repositories can be connected"))}
if let Some(repo)=s.repositories.iter().find(|r|Path::new(&r.path)==root){return Ok(repo.clone())}let repo=Repository{id:s.id("repo"),name,path:root.to_string_lossy().into(),revisions:vec![]};s.repositories.push(repo.clone());Ok(repo)})?;Ok(json!(repo))
 },
 ("DELETE",["api","repositories",id])=>{self.storage.transaction(|s|{if s.jobs.iter().any(|j|j.repository_id==*id&&j.active()){return Err(Error::new(409,"JOB_ACTIVE","Cancel active jobs before deleting this repository"))}
if !s.repositories.iter().any(|r|r.id==*id){return Err(not_found())}let removed:std::collections::BTreeSet<_>=s.answers.iter().filter(|a|a.repository_id==*id).map(|a|a.id.clone()).collect();s.repositories.retain(|r|r.id!=*id);s.jobs.retain(|j|j.repository_id!=*id);s.answers.retain(|a|a.repository_id!=*id);s.features.retain(|f|f.repository_id!=*id);s.observations.retain(|o|o.repository_id!=*id);s.investigations.retain(|i|i.repository_id!=*id);s.plans.retain(|p|p.repository_id!=*id);s.reviews.retain(|r|!removed.contains(&r.answer_id));for w in &mut s.workspaces{w.answer_ids.retain(|a|!removed.contains(a));if w.active_answer.as_ref().is_some_and(|a|removed.contains(a)){w.active_answer=None;}}Ok(())})?;Ok(json!({"deleted":true,"scope":"Derived workspace records deleted; connected source files were not changed"}))},
 ("POST",["api","repositories",id,"index"])=>Ok(json!(self.start_job(id)?)),
 ("GET",["api","repositories",id])=>{let s=self.storage.read()?;let r=s.repositories.iter().find(|r|r.id==*id).ok_or_else(not_found)?;let latest=r.revisions.last();Ok(json!({"id":r.id,"name":r.name,"revision":latest.map(|v|&v.id),"gitHead":latest.and_then(|v|v.git_head.clone()),"freshness":latest.map(|v|repository::freshness(r,v)).unwrap_or("notIndexed"),"coverage":latest.map(|v|&v.snapshot.coverage),"evidence":latest.map(|v|&v.snapshot.evidence),"skipped":latest.map(|v|&v.skipped),"symbolCount":latest.map(|v|v.snapshot.symbols.len()).unwrap_or(0)}))},
 ("POST",["api","jobs",id,"cancel"])=>{self.storage.transaction(|s|{let j=s.jobs.iter_mut().find(|j|j.id==*id).ok_or_else(not_found)?;if j.active(){j.state="cancelling".into();j.issue=Some("Cancellation requested. Active analysis may continue until its budget ends; publication is blocked.".into());}Ok(json!(j))})},
 ("POST",["api","questions"])=>{let req:Question=parse(body)?;if let Some(key)=&req.request_id{validate_text(key,120)?;}let fingerprint=answers::request_fingerprint(&req)?;let s=self.storage.read()?;
 if let Some(key)=&req.request_id{if let Some(existing)=s.answers.iter().find(|a|a.repository_id==req.repository_id&&a.request_id.as_ref()==Some(key)){if existing.request_fingerprint.as_ref()!=Some(&fingerprint){return Err(Error::new(409,"REQUEST_ID_CONFLICT","Request ID was used with different analysis input"));}return Ok(json!(answers::render(&s,&existing.id)?));}}
let repo=s.repositories.iter().find(|r|r.id==req.repository_id).ok_or_else(not_found)?;let rev=repo.revisions.last().ok_or_else(||Error::new(409,"NOT_INDEXED","Index this repository before asking a question"))?;if req.expected_revision.as_ref().is_some_and(|expected|*expected!=rev.id){return Err(Error::new(409,"REVISION_CHANGED","Question expected another source snapshot; it was not answered against a substitute"));}
if let Some(parent)=&req.parent_answer{if !s.answers.iter().any(|a|a.id==*parent&&a.repository_id==repo.id){return Err(Error::new(400,"INVALID_CONTEXT","Follow-up context is not an answer from this repository"))}}
 let mut answer=answers::generate(repo,rev,&req)?;let revision_id=rev.id.clone();self.storage.transaction(|s|{if let Some(key)=&req.request_id{if let Some(existing)=s.answers.iter().find(|a|a.repository_id==req.repository_id&&a.request_id.as_ref()==Some(key)){if existing.request_fingerprint.as_ref()!=Some(&fingerprint){return Err(Error::new(409,"REQUEST_ID_CONFLICT","Request ID was used with different analysis input"));}answer.id=existing.id.clone();return Ok(());}}
if s.answers.len()>=1000{return Err(Error::new(409,"ANSWER_QUOTA","Remove old answers before creating more"))}
if s.repositories.iter().find(|r|r.id==req.repository_id).and_then(|r|r.revisions.last()).is_none_or(|r|r.id!=revision_id){return Err(Error::new(409,"REVISION_CHANGED","Evidence changed while answering; retry on the current snapshot"))}answer.id=s.id("answer");s.answers.push(answer.clone());let w=&mut s.workspaces[0];w.answer_ids.push(answer.id.clone());w.active_answer=Some(answer.id.clone());Ok(())})?;Ok(json!(answers::render(&self.storage.read()?,&answer.id)?))},
 ("GET",["api","answers",id])=>Ok(json!(answers::render(&self.storage.read()?,id)?)),
 ("GET",["api","answers",id,"export"])=>Ok(json!(answers::render(&self.storage.read()?,id)?)),
 ("DELETE",["api","answers",id])=>{self.storage.transaction(|s|{if !s.answers.iter().any(|a|a.id==*id){return Err(not_found())}s.answers.retain(|a|a.id!=*id);s.reviews.retain(|r|r.answer_id!=*id);for w in &mut s.workspaces{w.answer_ids.retain(|a|a!=id);if w.active_answer.as_deref()==Some(id){w.active_answer=None;}}Ok(())})?;Ok(json!({"deleted":true}))},
 ("POST",["api","reviews"])=>{
 #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Feedback{answer_id:String,item_id:String,verdict:String,reason:String}
 let r:Feedback=parse(body)?;validate_text(&r.reason,2000)?;if r.reason.trim().len()<5||!["confirm","dispute","refute"].contains(&r.verdict.as_str()){return Err(Error::new(400,"INVALID_REVIEW","Choose confirm, dispute or refute and give a meaningful explanation"))}
 self.storage.transaction(|s|{let a=s.answers.iter().find(|a|a.id==r.answer_id).ok_or_else(not_found)?;if !a.items.iter().any(|i|i.id==r.item_id){return Err(Error::new(400,"UNKNOWN_ITEM","Claim not in this answer"))}s.reviews.push(Review{answer_id:r.answer_id.clone(),item_id:r.item_id,verdict:r.verdict,reason:r.reason,created_ns:now()});Ok(())})?;Ok(json!(answers::render(&self.storage.read()?,&r.answer_id)?))
 },
 ("POST",["api","workspaces"])=>{
 #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Save{name:String,answer_ids:Vec<String>,active_answer:Option<String>}
 let r:Save=parse(body)?;validate_text(&r.name,120)?;self.storage.transaction(|s|{if s.workspaces.len()>=100{return Err(Error::new(409,"WORKSPACE_QUOTA","Workspace limit reached"))}
if r.answer_ids.len()>100||r.answer_ids.iter().any(|id|!s.answers.iter().any(|a|a.id==*id))||r.active_answer.as_ref().is_some_and(|id|!r.answer_ids.contains(id)){return Err(Error::new(400,"INVALID_WORKSPACE","Workspace refers to an unavailable answer"))}let w=Workspace{id:s.id("workspace"),name:r.name,answer_ids:r.answer_ids,active_answer:r.active_answer};s.workspaces.push(w.clone());Ok(json!(w))})
 },
 ("POST",["api","features"])=>{
 #[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct Draft{repository_id:String,prompt:String,criteria:Vec<String>}
 let r:Draft=parse(body)?;validate_text(&r.prompt,4000)?;if r.criteria.is_empty()||r.criteria.len()>30{return Err(Error::new(400,"INVALID_CRITERIA","Provide 1–30 explicit acceptance criteria"))}for c in &r.criteria{validate_text(c,500)?;}
 self.storage.transaction(|s|{let repo=s.repositories.iter().find(|repo|repo.id==r.repository_id).ok_or_else(not_found)?;let rev=repo.revisions.last().ok_or_else(||Error::new(409,"NOT_INDEXED","Index repository first"))?;let related_symbols=rev.snapshot.symbols.iter().filter(|symbol|r.prompt.to_lowercase().contains(&symbol.name.to_lowercase())).map(|s|s.qual_name.clone()).take(20).collect();let revision_id=rev.id.clone();let draft=FeatureDraft{id:s.id("feature"),repository_id:r.repository_id,revision_id,prompt:r.prompt,criteria:r.criteria,related_symbols,status:"planningOnly".into(),limitations:vec!["Name overlap is a search hint, not proof of equivalent behavior.".into(),"No code was generated, executed, applied or published. Isolated candidate implementation and acceptance execution are not available in this workspace.".into()]};s.features.push(draft.clone());Ok(json!(draft))})
 },
 ("POST",["api","search"])=>{
 #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Search{query:String}
 let req:Search=parse(body)?;validate_text(&req.query,500)?;let s=self.storage.read()?;let mut results=vec![];for repo in &s.repositories{if let Some(rev)=repo.revisions.last(){for symbol in &rev.snapshot.symbols{if symbol.qual_name.to_lowercase().contains(&req.query.to_lowercase()){results.push(json!({"repositoryId":repo.id,"repositoryName":repo.name,"revisionId":rev.id,"symbolId":symbol.id.0.to_string(),"name":symbol.qual_name,"citation":answers::citation(repo,rev,symbol,None)}));if results.len()>=200{break}}}}
if results.len()>=200{break}}
 Ok(json!({"results":results,"capped":results.len()>=200,"scope":"Indexed snapshots only; freshness must be checked before relying on results"}))
 },
 _=>Err(not_found()),
 }
    }
    pub fn start_job(self: &Arc<Self>, repo_id: &str) -> Result<Job> {
        let job = self.storage.transaction(|s| {
            if !s.repositories.iter().any(|r| r.id == repo_id) {
                return Err(not_found());
            }
            if s.jobs
                .iter()
                .any(|j| j.repository_id == repo_id && j.active())
            {
                return Err(Error::new(
                    409,
                    "JOB_ACTIVE",
                    "An index job is already active for this repository",
                ));
            }
            if s.jobs.iter().filter(|j| j.active()).count() >= 8 || s.jobs.len() >= 1000 {
                return Err(Error::new(409, "JOB_QUOTA", "Job limit reached"));
            }
            let j = Job {
                id: s.id("job"),
                repository_id: repo_id.into(),
                state: "queued".into(),
                stage: "waiting".into(),
                completed_units: 0,
                total_units: 3,
                issue: None,
                revision_id: None,
                created_ns: now(),
            };
            s.jobs.push(j.clone());
            Ok(j)
        })?;
        let app = Arc::clone(self);
        let id = job.id.clone();
        std::thread::spawn(move || app.execute_job(&id));
        Ok(job)
    }
    fn cancelled(&self, id: &str) -> bool {
        self.storage
            .read()
            .map(|s| {
                s.jobs
                    .iter()
                    .find(|j| j.id == id)
                    .is_none_or(|j| j.state == "cancelling" || j.state == "cancelled")
            })
            .unwrap_or(true)
    }
    fn job_stage(&self, id: &str, stage: &str, progress: u32) -> Result<()> {
        self.storage.transaction(|s| {
            let job = s
                .jobs
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(not_found)?;
            if job.state != "cancelling" {
                job.state = "running".into();
                job.stage = stage.into();
                job.completed_units = progress;
            }
            Ok(())
        })
    }
    fn execute_job(&self, id: &str) {
        let result = (|| -> Result<()> {
            let _guard = self
                .index_lock
                .lock()
                .map_err(|_| Error::new(500, "INDEX_LOCK", "Index worker unavailable"))?;
            if self.cancelled(id) {
                return Err(Error::new(409, "CANCELLED", "Cancelled before publication"));
            }
            let state = self.storage.read()?;
            let job = state
                .jobs
                .iter()
                .find(|j| j.id == id)
                .ok_or_else(not_found)?;
            let repo = state
                .repositories
                .iter()
                .find(|r| r.id == job.repository_id)
                .ok_or_else(not_found)?
                .clone();
            let root = repository::allowed_path(Path::new(&repo.path), &self.allowed_roots)?;
            self.job_stage(id, "capturing immutable sources", 0)?;
            let captured = repository::capture(&root)?;
            if captured.sources.is_empty() {
                return Err(Error::new(
                    400,
                    "NO_SUPPORTED_FILES",
                    "No supported source files were found; nothing was published",
                ));
            }
            if self.cancelled(id) {
                return Err(Error::new(409, "CANCELLED", "Cancelled before analysis"));
            }
            let stage = self.storage.directory.join("work").join(id);
            repository::stage(&captured, &stage)?;
            self.job_stage(id, "analyzing staged snapshot", 1)?;
            let outcome = cie_ingest::run_ingest(
                &stage,
                &cie_ingest::IngestOptions {
                    budget_ms: Some(25000),
                    ..Default::default()
                },
            )?;
            let outcome = cie_ingest::materialize(outcome)?;
            if self.cancelled(id) {
                return Err(Error::new(
                    409,
                    "CANCELLED",
                    "Cancelled; staged work was not published",
                ));
            }
            self.job_stage(id, "checking freshness and publishing", 2)?;
            let current = repository::capture(&root)?;
            if current.fingerprint != captured.fingerprint {
                return Err(Error::new(
                    409,
                    "SOURCE_CHANGED",
                    "Sources changed during indexing. Nothing new was published; retry explicitly.",
                ));
            }
            self.storage.transaction(|s|{if s.jobs.iter().find(|j|j.id==id).is_none_or(|j|j.state=="cancelling"){return Err(Error::new(409,"CANCELLED","Cancellation blocked publication"))}let revision=Revision{id:s.id("revision"),fingerprint:captured.fingerprint,git_head:captured.git_head,created_ns:now(),snapshot:outcome.snapshot,sources:captured.sources,skipped:captured.skipped};let revision_id=revision.id.clone();let target=s.repositories.iter_mut().find(|r|r.id==repo.id).ok_or_else(not_found)?;if target.revisions.len()>=8{return Err(Error::new(409,"REVISION_QUOTA","Repository has 8 retained snapshots. Export or delete it before indexing more."))}target.revisions.push(revision);let j=s.jobs.iter_mut().find(|j|j.id==id).ok_or_else(not_found)?;j.state="ready".into();j.stage="published".into();j.completed_units=3;j.revision_id=Some(revision_id);Ok(())})?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(self.storage.directory.join("work").join(id));
        if let Err(error) = result {
            let _ = self.storage.transaction(|s| {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.state = if error.code == "CANCELLED" {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    j.stage = "not published".into();
                    j.issue = Some(error.message);
                }
                Ok(())
            });
        }
    }
}
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|_| {
        Error::new(
            400,
            "INVALID_INPUT",
            "Request does not match the supported schema",
        )
    })
}
fn validate_text(text: &str, limit: usize) -> Result<()> {
    if text.trim().is_empty() || text.len() > limit || text.chars().any(|c| c == '\0') {
        Err(Error::new(
            400,
            "INVALID_TEXT",
            "Text is empty or exceeds its size limit",
        ))
    } else {
        Ok(())
    }
}
fn not_found() -> Error {
    Error::new(404, "NOT_FOUND", "Requested resource is unavailable")
}
