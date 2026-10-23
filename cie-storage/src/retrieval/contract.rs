use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, StorageError>;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StorageError {
    pub code: String,
    pub message: String,
}
impl StorageError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for StorageError {}
impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::new("Io", e.to_string())
    }
}
impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("CorruptData", e.to_string())
    }
}
impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("InvalidRequest", e.to_string())
    }
}
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    File,
    Symbol,
    Concept,
    Summary,
    Contract,
    Finding,
    DuplicatePair,
    Cluster,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    pub fields: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    DefinedIn,
    Calls,
    MemberOfConcept,
    ChildConceptOf,
    MemberOfCluster,
    Summarizes,
    Constrains,
    LeftSymbol,
    RightSymbol,
    CanonicalSymbol,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub relation: RelationKind,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    #[default]
    All,
    Ids(Vec<String>),
    Field {
        field: String,
        op: CompareOp,
        value: Value,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
    Not(Box<Predicate>),
    ConceptMembership {
        concepts: Vec<String>,
        #[serde(default)]
        include_descendants: bool,
    },
    Related {
        relation: RelationKind,
        #[serde(default)]
        incoming: bool,
        target: Box<Query>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    In,
    Prefix,
    Contains,
    Gt,
    Ge,
    Lt,
    Le,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    Select {
        kind: EntityKind,
        #[serde(default)]
        filter: Predicate,
    },
    Union(Vec<Query>),
    Intersect(Vec<Query>),
    Difference {
        left: Box<Query>,
        right: Box<Query>,
    },
    Follow {
        input: Box<Query>,
        relation: RelationKind,
        #[serde(default)]
        incoming: bool,
        #[serde(default = "one")]
        max_depth: u8,
    },
    CellContributors {
        descriptor: String,
    },
}
fn one() -> u8 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default = "one_u32")]
    pub api_version: u32,
    pub query: Query,
    pub outputs: Vec<NamedOutput>,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default)]
    pub best_effort: bool,
}
fn one_u32() -> u32 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedOutput {
    pub id: String,
    pub output: Output,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Entities {
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        page: PageRequest,
    },
    Relationships {
        #[serde(default)]
        page: PageRequest,
    },
    Graph {
        #[serde(default = "default_nodes")]
        max_nodes: usize,
        #[serde(default = "default_edges")]
        max_edges: usize,
    },
    Evidence {
        #[serde(default)]
        page: PageRequest,
    },
    Cube(CubeRequest),
    Facets {
        dimensions: Vec<String>,
        #[serde(default)]
        page: PageRequest,
    },
    Detail(Detail),
    Artifact {
        kind: ArtifactKind,
    },
    View {
        kind: String,
        #[serde(default)]
        source_output: Option<String>,
        #[serde(default = "default_nodes")]
        max_nodes: usize,
        #[serde(default = "default_edges")]
        max_edges: usize,
    },
}
fn default_nodes() -> usize {
    500
}
fn default_edges() -> usize {
    1000
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    Source {
        #[serde(default)]
        start: Option<usize>,
        #[serde(default)]
        end: Option<usize>,
    },
    FunctionIr,
    Impact,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Report,
    RepositoryMap,
    Concepts,
    Stack,
    Dependencies,
    Runtime,
    Stories,
    Quality,
    Overlap,
    Drift,
    CoChange,
    Fault,
    ArchitectureBundle,
    UiBootstrapV1,
}
impl ArtifactKind {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::RepositoryMap => "viewspec",
            Self::Concepts => "concepts",
            Self::Stack => "stack",
            Self::Dependencies => "deps",
            Self::Runtime => "runtime",
            Self::Stories => "stories",
            Self::Quality => "quality",
            Self::Overlap => "overlap",
            Self::Drift => "drift",
            Self::CoChange => "cochange",
            Self::Fault => "fault",
            Self::ArchitectureBundle => "polyglotSpecs",
            Self::UiBootstrapV1 => "bootstrap",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageRequest {
    #[serde(default = "default_page")]
    pub limit: usize,
    #[serde(default)]
    pub continuation: Option<String>,
}
fn default_page() -> usize {
    100
}
impl Default for PageRequest {
    fn default() -> Self {
        Self {
            limit: 100,
            continuation: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(default = "default_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_visited")]
    pub max_visited: usize,
}
fn default_bytes() -> usize {
    4 * 1024 * 1024
}
fn default_timeout() -> u64 {
    2000
}
fn default_visited() -> usize {
    100_000
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_response_bytes: default_bytes(),
            timeout_ms: default_timeout(),
            max_visited: default_visited(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CubeRequest {
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub measures: Vec<Measure>,
    #[serde(default)]
    pub rows: Vec<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub include_unknown: bool,
    #[serde(default)]
    pub page: PageRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Count,
    Sum(String),
    Average(String),
    Min(String),
    Max(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub api_version: u32,
    pub repository: String,
    pub revision: String,
    pub query_hash: String,
    pub matches: usize,
    pub coverage: Value,
    pub outputs: Vec<OutputResult>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutputResult {
    pub id: String,
    pub status: String,
    pub schema_version: u32,
    pub origin: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<StorageError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevisionSelector {
    Latest,
    Id(String),
}
