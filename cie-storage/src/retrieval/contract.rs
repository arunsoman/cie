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
    Route,
    Framework,
    Component,
    Dependency,
    StateMachine,
    State,
    Workflow,
    Evidence,
}
impl EntityKind {
    pub const ALL: &'static [Self] = &[
        Self::File,
        Self::Symbol,
        Self::Concept,
        Self::Summary,
        Self::Contract,
        Self::Finding,
        Self::DuplicatePair,
        Self::Cluster,
        Self::Route,
        Self::Framework,
        Self::Component,
        Self::Dependency,
        Self::StateMachine,
        Self::State,
        Self::Workflow,
        Self::Evidence,
    ];
}
/// Typed entity reference. IDs embed the repository and revision; a reference
/// from another revision is rejected wherever it is validated.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntityRef {
    pub repository: String,
    pub revision: String,
    pub kind: EntityKind,
    pub id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub kind: EntityKind,
    pub fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
}
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    DefinedIn,
    Calls,
    /// Source-declared targets, including candidate runtime dispatch.
    DeclaredCalls,
    MemberOfConcept,
    ChildConceptOf,
    MemberOfCluster,
    Summarizes,
    Constrains,
    LeftSymbol,
    RightSymbol,
    CanonicalSymbol,
    MemberOfComponent,
    ChildComponentOf,
    UsesDependency,
    SourceSymbol,
    StateOf,
    HasEvidence,
    CoChanged,
}
impl RelationKind {
    /// Declared endpoint kinds. Incompatible endpoints are validation errors.
    pub fn allows(self, source: EntityKind, target: EntityKind) -> bool {
        use EntityKind::*;
        if self == Self::HasEvidence {
            return target == Evidence && source != Evidence;
        }
        matches!(
            (self, source, target),
            (RelationKind::CoChanged, File, File)
                | (RelationKind::MemberOfComponent, Symbol, Component)
                | (RelationKind::ChildComponentOf, Component, Component)
                | (RelationKind::UsesDependency, File, Dependency)
                | (RelationKind::SourceSymbol, StateMachine | Workflow, Symbol)
                | (RelationKind::StateOf, State, StateMachine)
                | (
                    RelationKind::DefinedIn,
                    Route | Framework | Dependency | Component,
                    File
                )
                | (RelationKind::DefinedIn, Symbol, File)
                | (RelationKind::Calls | RelationKind::DeclaredCalls, Symbol, Symbol)
                | (RelationKind::MemberOfConcept, Symbol, Concept)
                | (RelationKind::MemberOfConcept, File, Concept)
                | (RelationKind::ChildConceptOf, Concept, Concept)
                | (RelationKind::MemberOfCluster, Symbol, Cluster)
                | (RelationKind::Summarizes, Summary, Symbol)
                | (RelationKind::Summarizes, Summary, File)
                | (RelationKind::Constrains, Contract, Symbol)
                | (RelationKind::Constrains, Contract, File)
                | (RelationKind::LeftSymbol, DuplicatePair, Symbol)
                | (RelationKind::RightSymbol, DuplicatePair, Symbol)
                | (RelationKind::CanonicalSymbol, DuplicatePair, Symbol)
        )
    }
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
        scope: MembershipScope,
    },
    Related {
        relation: RelationKind,
        #[serde(default)]
        incoming: bool,
        target: Box<Query>,
        #[serde(default)]
        quantifier: RelationQuantifier,
    },
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MembershipScope {
    #[default]
    Direct,
    IncludeDescendants,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelationQuantifier {
    #[default]
    Any,
    None,
    All,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    In,
    Prefix,
    Contains,
    /// Unicode case-folded prefix, per the compatibility field semantics.
    PrefixFold,
    /// Unicode case-folded substring, per the compatibility field semantics.
    ContainsFold,
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
        #[serde(default = "one")]
        min_depth: u8,
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
        order: Vec<Sort>,
        #[serde(default)]
        page: PageRequest,
    },
    Relationships {
        #[serde(default)]
        page: PageRequest,
    },
    Graph {
        #[serde(default)]
        expansion: Option<GraphExpansion>,
        #[serde(default = "default_nodes")]
        max_nodes: usize,
        #[serde(default = "default_edges")]
        max_edges: usize,
    },
    Evidence {
        #[serde(default)]
        scope: EvidenceScope,
        #[serde(default)]
        page: PageRequest,
    },
    Cube(CubeRequest),
    Facets {
        dimensions: Vec<String>,
        /// Fields whose own filters are removed for the tagged facet counts.
        #[serde(default)]
        self_excluding: Vec<String>,
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
        level: Option<C4Level>,
        #[serde(default)]
        source: ViewSource,
        #[serde(default)]
        availability: AvailabilityPolicy,
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewSource {
    #[default]
    Matches,
    Output(String),
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityPolicy {
    StoredOnly,
    /// Bounded projection from stored evidence in the pinned revision.
    #[default]
    BuildIfMissing,
}
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sort {
    pub field: String,
    #[serde(default)]
    pub direction: SortDirection,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    Source {
        #[serde(default)]
        range: SourceRange,
        #[serde(default)]
        context_lines: u16,
        #[serde(default)]
        continuation: Option<String>,
    },
    FunctionIr {
        #[serde(default = "yes")]
        include_calls: bool,
    },
    Impact {
        #[serde(default)]
        availability: AvailabilityPolicy,
    },
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceRange {
    #[default]
    Declaration,
    WholeFile,
    Bytes {
        start: u64,
        end: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Report,
    #[serde(alias = "viewspec")]
    RepositoryMap,
    Concepts,
    Stack,
    #[serde(alias = "deps")]
    Dependencies,
    Runtime,
    Stories,
    Quality,
    Overlap,
    Drift,
    #[serde(alias = "cochange")]
    CoChange,
    Fault,
    #[serde(alias = "polyglotSpecs")]
    ArchitectureBundle,
    #[serde(alias = "bootstrap")]
    UiBootstrapV1,
}
impl ArtifactKind {
    pub const ALL: &'static [Self] = &[
        Self::Report,
        Self::RepositoryMap,
        Self::Concepts,
        Self::Stack,
        Self::Dependencies,
        Self::Runtime,
        Self::Stories,
        Self::Quality,
        Self::Overlap,
        Self::Drift,
        Self::CoChange,
        Self::Fault,
        Self::ArchitectureBundle,
        Self::UiBootstrapV1,
    ];
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
/// Slice-and-dice analytical request over the outer query's population.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CubeRequest {
    #[serde(default)]
    pub dimensions: Vec<DimensionSelection>,
    #[serde(default)]
    pub measures: Vec<NamedMeasure>,
    #[serde(default)]
    pub layout: CubeLayout,
    /// Measure predicates applied after grouping (HAVING).
    #[serde(default)]
    pub cell_filter: Option<MeasurePredicate>,
    #[serde(default)]
    pub order: Vec<CellSort>,
    #[serde(default)]
    pub page: PageRequest,
    #[serde(default)]
    pub totals: TotalsRequest,
    #[serde(default)]
    pub missing: MissingMemberPolicy,
    #[serde(default)]
    pub drill_through: DrillThroughPolicy,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionSelection {
    pub id: String,
    pub dimension: DimensionRef,
    /// Named hierarchy level, declared in the capability catalog.
    #[serde(default)]
    pub level: Option<HierarchyLevel>,
    #[serde(default)]
    pub membership: DimensionMembership,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DimensionRef {
    Field(String),
    RelatedEntity {
        relation_path: Vec<RelationStep>,
        attribute: String,
    },
    NumericBucket {
        field: String,
        boundaries: Vec<f64>,
    },
    TimeBucket {
        field: String,
        interval: TimeInterval,
        timezone: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelationStep {
    pub relation: RelationKind,
    #[serde(default)]
    pub incoming: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyLevel {
    Directory,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DimensionMembership {
    /// Only the directly recorded member value.
    #[default]
    RecordedOnly,
    /// Roll up through declared ancestry (directory chain) into every level.
    IncludeAncestors,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimeInterval {
    pub unit: TimeUnit,
    #[serde(default = "one_u64")]
    pub count: u64,
}
fn one_u64() -> u64 {
    1
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CubeLayout {
    #[default]
    Cells,
    Pivot {
        rows: Vec<String>,
        columns: Vec<String>,
    },
}
/// Measure definitions with explicit IDs referenced by filters and ordering.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedMeasure {
    pub id: String,
    #[serde(default)]
    pub measure: Measure,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    #[default]
    Count,
    Sum(String),
    Average(String),
    Min(String),
    Max(String),
}
/// Post-grouping measure predicate (HAVING semantics).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasurePredicate {
    Measure {
        id: String,
        op: CompareOp,
        value: f64,
    },
    And(Vec<MeasurePredicate>),
    Or(Vec<MeasurePredicate>),
    Not(Box<MeasurePredicate>),
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellSort {
    pub by: CellSortKey,
    #[serde(default)]
    pub direction: SortDirection,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CellSortKey {
    Measure(String),
    Dimension(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TotalsRequest {
    /// Grand totals over all pre-HAVING matches (the default scope).
    #[serde(default = "yes")]
    pub grand: bool,
    /// Separate totals over the displayed page.
    #[serde(default)]
    pub page: bool,
}
impl Default for TotalsRequest {
    fn default() -> Self {
        Self {
            grand: true,
            page: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissingMemberPolicy {
    #[default]
    Exclude,
    IncludeUnknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DrillThroughPolicy {
    /// Cells carry an opaque contributor descriptor bound to this query.
    #[default]
    Enabled,
    Disabled,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evidence: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<Value>,
    pub limits: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphExpansion {
    #[serde(default)]
    pub relations: Vec<RelationKind>,
    #[serde(default)]
    pub incoming: bool,
    #[serde(default = "one")]
    pub max_depth: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutputResult {
    pub id: String,
    pub status: String,
    pub schema_version: u32,
    pub origin: String,
    pub kind: String,
    pub filter_application: String,
    pub data: Option<super::payload::OutputData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<StorageError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevisionSelector {
    Latest,
    Id(String),
}
/// Revision listing request and result page (§4).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RevisionListRequest {
    #[serde(default = "default_page")]
    pub limit: usize,
    #[serde(default)]
    pub after: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevisionInfo {
    pub revision: String,
    pub head: bool,
    /// Published retained databases are complete; absent ones are not listed.
    pub complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevisionPage {
    pub items: Vec<RevisionInfo>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}
/// Symbol resolution by qualified name or captured byte position (§6).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SymbolLookup {
    ByName {
        file: String,
        qualified_name: String,
    },
    AtPosition {
        file: String,
        byte_offset: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum SymbolResolution {
    Found(EntityRef),
    NotFound,
    Ambiguous { candidates: Vec<SymbolCandidate> },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolCandidate {
    pub entity: EntityRef,
    pub file: String,
    pub qualified_name: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceScope {
    #[default]
    MatchesAndDerivations,
    Matches,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum C4Level {
    Context,
    Container,
    Component,
}
