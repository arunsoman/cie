//! Versioned output algebra. JSON is only allowed in declared field/member bags
//! and legacy artifact documents, never as an arbitrary result payload.
use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputData {
    Entities(Page<Entity>),
    Relationships(Page<Edge>),
    Graph(GraphData),
    Evidence(Page<EvidenceData>),
    Facets(Page<FacetData>),
    Cube(CubeData),
    Source(SourceData),
    FunctionIr(IrData),
    Impact(ImpactData),
    Artifact(ArtifactData),
    View(ViewData),
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub continuation: Option<String>,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphData {
    pub nodes: Vec<Entity>,
    pub edges: Vec<Edge>,
    pub truncated: bool,
    pub scope: GraphScope,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphScope {
    pub matched: Vec<String>,
    pub context: Vec<String>,
    pub expansion: Option<GraphExpansion>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceData {
    pub id: String,
    pub entity: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default)]
    pub provenance: Option<Value>,
    pub source_kind: String,
    pub coverage: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    #[serde(default)]
    pub file: Option<Value>,
    #[serde(default)]
    pub start: Option<u64>,
    #[serde(default)]
    pub end: Option<u64>,
    #[serde(default)]
    pub line: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FacetData {
    pub dimension: String,
    pub member: Value,
    pub count: usize,
    pub self_excluded: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Numeric(#[schemars(with = "f64")] pub serde_json::Number);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CubeCell {
    pub members: BTreeMap<String, Value>,
    pub measures: BTreeMap<String, Option<Numeric>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contributors: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_key: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MeasureDefinition {
    pub definition: Measure,
    pub additivity: String,
    pub grain: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PivotDomain {
    pub rows: Vec<String>,
    pub columns: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CubeData {
    #[serde(flatten)]
    pub page: Page<CubeCell>,
    pub dimensions: Vec<DimensionSelection>,
    pub measures: BTreeMap<String, MeasureDefinition>,
    pub layout: CubeLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<PivotDomain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grand_totals: Option<BTreeMap<String, Option<Numeric>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_totals: Option<BTreeMap<String, Option<Numeric>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_total_scope: Option<String>,
    pub partition_verified: bool,
    pub coverage: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RequestedRange {
    Bytes(ByteRange),
    Whole { whole_file: bool },
    Declaration { declaration: bool },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceData {
    pub path: String,
    pub content_hash: String,
    pub encoding: String,
    pub requested: RequestedRange,
    pub returned: ByteRange,
    pub text: String,
    pub line_starts: Vec<usize>,
    pub revision: String,
    pub truncated: bool,
    pub continuation: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IrBlock {
    pub id: i64,
    pub symbol_id: i64,
    pub block_index: i64,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BinarySummary {
    pub binary_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IrInstruction {
    pub id: i64,
    pub block_id: i64,
    pub inst_index: i64,
    pub op: String,
    pub operands: BinarySummary,
    pub ast_node_id: Option<i64>,
    pub span_start: u64,
    pub span_end: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IrEdge {
    pub from_block_id: i64,
    pub to_block_id: i64,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IrCall {
    pub id: i64,
    pub file_id: i64,
    pub call_node_id: i64,
    pub caller_symbol_id: i64,
    pub callee_symbol_id: Option<i64>,
    pub callee_text: Option<String>,
    pub resolution: String,
    pub stmt_index: i64,
    pub block_id: Option<i64>,
    pub call_kind: String,
    pub throws_set: Option<BinarySummary>,
    pub guard_summary: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IrData {
    pub symbol: Entity,
    pub blocks: Vec<IrBlock>,
    pub instructions: Vec<IrInstruction>,
    pub edges: Vec<IrEdge>,
    pub calls: Vec<IrCall>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImpactData {
    pub symbol: String,
    pub affected_symbols: Vec<String>,
    pub minimum_test_set: Vec<String>,
    pub affected_concepts: Vec<String>,
    pub provenance: String,
    pub coverage: Value,
}
/// A named existing artifact contract. Only explicit artifact requests can
/// return these versioned extension documents; records/views have closed DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactData {
    pub schema_version: u32,
    #[serde(flatten)]
    pub content: ArtifactDocument,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "renderer")]
pub enum ViewData {
    #[serde(rename = "cie-empty")]
    Empty {
        schema_version: u32,
        kind: String,
        revision: String,
        scope: ViewScope,
        coverage: Value,
        truncated: bool,
    },
    #[serde(rename = "cie-cube")]
    Cube {
        schema_version: u32,
        kind: String,
        cells: CubeData,
        revision: String,
        coverage: Value,
    },
    #[serde(rename = "viz-adapter")]
    Canonical {
        schema_version: u32,
        spec: cie_viz::ViewSpec,
        truncated: bool,
        revision: String,
        coverage: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_version: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<ViewScope>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        entity_refs: BTreeMap<String, Vec<EntityRef>>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        drill_down: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        derivations: Vec<CochangeProof>,
    },
    #[serde(rename = "cie-polyglot")]
    Architecture {
        schema_version: u32,
        provider_version: u32,
        revision: String,
        specs: BTreeMap<String, ArchitectureSpec>,
        entity_refs: BTreeMap<String, Vec<EntityRef>>,
        drill_down: BTreeMap<String, String>,
        omitted: usize,
        truncated: bool,
        coverage: Value,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ViewScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_hash: Option<String>,
    pub matched: Vec<String>,
    pub context: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CochangeProof {
    pub source: String,
    pub target: String,
    pub support: usize,
    pub supporting_commits: Vec<String>,
    pub provenance: String,
}
/// Legacy architecture specs retain their producer/gate schema; scope and
/// footer are declared metadata bags, while nodes/edges use the canonical DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArchitectureSpec {
    pub elements: Vec<ArchitectureElement>,
    pub edges: Vec<ArchitectureEdge>,
    #[serde(default)]
    pub footer: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub gaps: Vec<BTreeMap<String, Value>>,
    pub scope: BTreeMap<String, Value>,
    #[serde(rename = "layoutHint", default)]
    pub layout_hint: Option<cie_viz::LayoutHint>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureElement {
    pub id: String,
    pub label: String,
    pub display_mode: cie_viz::DisplayMode,
    pub evidence: cie_viz::EvidenceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_argument: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fog_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salience: Option<cie_viz::Salience>,
}
impl OutputData {
    pub fn from_output(output: &Output, value: Value) -> Result<Self> {
        let name = match output {
            Output::Entities { .. } => "entities",
            Output::Relationships { .. } => "relationships",
            Output::Graph { .. } => "graph",
            Output::Evidence { .. } => "evidence",
            Output::Facets { .. } => "facets",
            Output::Cube(_) => "cube",
            Output::View { .. } => "view",
            Output::Detail(Detail::Source { .. }) => "source",
            Output::Detail(Detail::FunctionIr { .. }) => "function_ir",
            Output::Detail(Detail::Impact { .. }) => "impact",
            Output::Artifact { .. } => "artifact",
        };
        let mut value = if let Output::Artifact { kind, section, .. } = output {
            if section.is_some() {
                // A section carries the ReportSection document shape itself;
                // the kind names it so typed parsing stays closed.
                json!({"artifact_kind":"report_section","schema_version":1,"document":value})
            } else {
                json!({"artifact_kind":kind,"schema_version":1,"document":value})
            }
        } else {
            value
        };
        value
            .as_object_mut()
            .ok_or_else(|| StorageError::new("SchemaMismatch", "Output must be an object"))?
            .insert("type".into(), json!(name));
        serde_json::from_value(value)
            .map_err(|e| StorageError::new("SchemaMismatch", e.to_string()))
    }
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("Typed response serialization")
    }
}
impl OutputResult {
    pub fn json(&self) -> Value {
        self.data
            .as_ref()
            .map(OutputData::to_json)
            .unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureEdge {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub source: String,
    pub target: String,
    pub edge_kind: String,
    pub display_mode: cie_viz::DisplayMode,
    pub evidence: cie_viz::EvidenceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_argument: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
}
