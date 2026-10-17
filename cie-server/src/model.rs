use cie_incremental::snapshot::RevSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub path: String,
    pub revisions: Vec<Revision>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: String,
    pub fingerprint: String,
    pub git_head: Option<String>,
    pub created_ns: u64,
    pub snapshot: RevSnapshot,
    pub sources: BTreeMap<String, String>,
    pub skipped: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub repository_id: String,
    pub state: String,
    pub stage: String,
    pub completed_units: u32,
    pub total_units: u32,
    pub issue: Option<String>,
    pub revision_id: Option<String>,
    pub created_ns: u64,
}
impl Job {
    pub fn active(&self) -> bool {
        matches!(self.state.as_str(), "queued" | "running" | "cancelling")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub repository_id: String,
    pub revision_id: String,
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub source_hash: String,
    pub excerpt: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub claim_class: String,
    pub citations: Vec<Citation>,
    pub detail: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub claim_class: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Intent {
    Overview,
    Search,
    Callers,
    Callees,
    Impact,
    Tests,
    Findings,
    Concepts,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub id: String,
    pub repository_id: String,
    pub revision_id: String,
    pub question: String,
    pub interpretation: String,
    pub intent: Intent,
    pub summary: String,
    pub status: String,
    pub gaps: Vec<String>,
    pub items: Vec<Item>,
    pub edges: Vec<Edge>,
    pub preferred_view: String,
    pub parent_answer: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub request_fingerprint: Option<String>,
    pub created_ns: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub answer_id: String,
    pub item_id: String,
    pub verdict: String,
    pub reason: String,
    pub created_ns: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub answer_ids: Vec<String>,
    pub active_answer: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDraft {
    pub id: String,
    pub repository_id: String,
    pub revision_id: String,
    pub prompt: String,
    pub criteria: Vec<String>,
    pub related_symbols: Vec<String>,
    pub status: String,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default)]
    pub observations: Vec<Observation>,
    #[serde(default)]
    pub plans: Vec<AnalysisPlan>,
    #[serde(default)]
    pub investigations: Vec<Investigation>,
    pub schema: u32,
    pub next_id: u64,
    pub repositories: Vec<Repository>,
    pub jobs: Vec<Job>,
    pub answers: Vec<Answer>,
    pub reviews: Vec<Review>,
    pub workspaces: Vec<Workspace>,
    pub features: Vec<FeatureDraft>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            observations: vec![],
            plans: vec![],
            investigations: vec![],
            schema: 1,
            next_id: 1,
            repositories: vec![],
            jobs: vec![],
            answers: vec![],
            reviews: vec![],
            workspaces: vec![Workspace {
                id: "default".into(),
                name: "My investigation".into(),
                answer_ids: vec![],
                active_answer: None,
            }],
            features: vec![],
        }
    }
}
impl State {
    pub fn id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }
}
pub fn now() -> u64 {
    cie_core::types::now_ns()
}
#[derive(Clone, Debug)]
pub struct Error {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::new(
            500,
            "STORAGE_ERROR",
            "Local operation failed; inspect the server log or storage permissions",
        )
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedEvent {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub start_micros: u64,
    pub end_micros: u64,
    pub outcome: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub source_hash: Option<String>,
    pub samples: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub id: String,
    pub repository_id: String,
    pub revision_id: String,
    pub environment: String,
    pub producer: String,
    pub kind: String,
    pub events: Vec<ObservedEvent>,
    pub created_ns: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hypothesis {
    pub id: String,
    pub statement: String,
    pub status: String,
    pub support: Vec<String>,
    pub counter_evidence: Vec<String>,
    pub next_step: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Investigation {
    pub id: String,
    pub repository_id: String,
    pub revision_id: String,
    pub symptom: String,
    pub status: String,
    pub hypotheses: Vec<Hypothesis>,
    pub notes: Vec<String>,
    pub created_ns: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisStep {
    pub question: String,
    pub intent: Option<Intent>,
    pub symbol_id: Option<String>,
    pub from_step: Option<usize>,
    pub status: String,
    pub answer_id: Option<String>,
    pub issue: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPlan {
    pub id: String,
    pub repository_id: String,
    pub revision_id: String,
    pub status: String,
    pub steps: Vec<AnalysisStep>,
    pub created_ns: u64,
}
