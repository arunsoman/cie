//! Closed, versioned artifact roots. The open bags below preserve each
//! existing producer's declared adapter/analysis metadata, not arbitrary output
//! variants. Identity, structure and stable catalog fields remain typed.
use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "artifact_kind", content = "document", rename_all = "snake_case")]
pub enum ArtifactDocument {
    Report(ReportDocument),
    RepositoryMap(cie_viz::ViewSpec),
    Concepts(ConceptDocument),
    Stack(StackDocument),
    Dependencies(DependencyDocument),
    Runtime(RuntimeDocument),
    Stories(StoriesDocument),
    Quality(QualityDocument),
    Overlap(OverlapDocument),
    Drift(DriftDocument),
    CoChange(CochangeDocument),
    Fault(FaultDocument),
    ArchitectureBundle(ArchitectureBundleDocument),
    UiBootstrapV1(BootstrapDocument),
    /// One stored report field ("section"), whole or as an array window —
    /// the bounded transport for report artifacts whose whole document
    /// exceeds the response-budget policy on large repositories.
    ReportSection(ReportSectionDocument),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ReportSectionDocument {
    /// Which captured report field this section carries.
    pub section: String,
    /// Array sections: total stored elements; any other shape reports 0.
    #[serde(default)]
    pub total: u64,
    /// Array sections: the window's first element's index in stored order.
    #[serde(default)]
    pub offset: u64,
    /// false while a continuation offset can fetch more of this section.
    pub complete: bool,
    /// The section's whole value, or the requested window out of an array.
    pub document: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapturedSpan {
    pub file: u32,
    pub start: u64,
    pub end: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapturedSymbol {
    pub id: u32,
    pub name: String,
    pub qual_name: String,
    pub kind: String,
    pub loc: CapturedSpan,
    #[serde(flatten)]
    pub attributes: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReportDocument {
    pub rev: u64,
    pub files: Vec<String>,
    pub symbols: Vec<CapturedSymbol>,
    #[serde(flatten)]
    pub analysis: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConceptRecord {
    pub id: u32,
    pub label: String,
    pub kind: String,
    pub symbols: Vec<u32>,
    #[serde(flatten)]
    pub attributes: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConceptDocument {
    pub nodes: Vec<ConceptRecord>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FrameworkRecord {
    pub id: String,
    pub name: String,
    pub evidence: String,
    pub modules: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RouteRecord {
    pub verb: String,
    pub path: String,
    pub owner: String,
    pub method: String,
    pub service: String,
    pub rel: String,
    pub line: u32,
    pub params: Vec<String>,
    pub returns: String,
    pub evidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StackDocument {
    pub v: u32,
    pub frameworks: Vec<FrameworkRecord>,
    pub routes: Vec<RouteRecord>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyRecord {
    pub name: String,
    pub ecosystem: String,
    pub version_req: String,
    pub licence: Option<String>,
    pub scope: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyTable {
    pub deps: Vec<DependencyRecord>,
    pub unparsed: Vec<String>,
    pub files_scanned: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyUsage {
    pub name: String,
    pub ecosystem: String,
    pub imported_by: Vec<String>,
    pub resolved: bool,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyDocument {
    pub table: DependencyTable,
    pub usage: Vec<DependencyUsage>,
    #[serde(default)]
    pub what_if: Option<BTreeMap<String, Value>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CochangeRecord {
    pub a: String,
    pub b: String,
    pub support: usize,
    pub cross_directory: bool,
    pub supporting_commits: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CochangeDocument {
    pub sample_commits: usize,
    pub counted_commits: usize,
    pub excluded_bulk: usize,
    pub rename_events: usize,
    pub edges: Vec<CochangeRecord>,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OverlapRecord {
    pub a: String,
    pub b: String,
    pub shared: Vec<String>,
    pub only_a: Vec<String>,
    pub only_b: Vec<String>,
    pub similarity: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OverlapDocument {
    pub pairs: Vec<OverlapRecord>,
    pub unmapped: Vec<String>,
    pub min_shared: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FaultDocument {
    pub revision: u64,
    pub status: String,
    pub unknown_reason: Option<String>,
    pub property: Option<String>,
    pub lineage: BTreeMap<String, Value>,
    pub experiments: Vec<BTreeMap<String, Value>>,
    pub execution_performed: bool,
    pub runtime_observed: bool,
    pub notes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeDocument {
    pub project: String,
    pub collected_at: String,
    pub environment: BTreeMap<String, Value>,
    pub honesty: String,
    pub datasets: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoriesDocument {
    pub project: String,
    pub collected_at: String,
    pub environment: BTreeMap<String, Value>,
    pub honesty: String,
    pub counts: BTreeMap<String, Value>,
    pub registry: BTreeMap<String, Value>,
    pub stories: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QualityDocument {
    pub project: String,
    pub collected_at: String,
    pub honesty: String,
    pub scopes: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub sections: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftDocument {
    pub rev: u64,
    pub docs: Vec<String>,
    pub claims: Vec<BTreeMap<String, Value>>,
    pub mappings: Vec<BTreeMap<String, Value>>,
    pub verifications: Vec<BTreeMap<String, Value>>,
    pub reverse_drift: Vec<BTreeMap<String, Value>>,
    pub aggregate: BTreeMap<String, Value>,
    pub complete: bool,
    pub generated_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArchitectureBundleDocument {
    pub specs: BTreeMap<String, ArchitectureSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(
        default,
        rename = "spec-count",
        skip_serializing_if = "Option::is_none"
    )]
    pub spec_count: Option<usize>,
    #[serde(default, rename = "impact-centers-truncated")]
    pub impact_centers_truncated: bool,
    #[serde(default)]
    pub skipped: Vec<BTreeMap<String, Value>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapDocument {
    pub report: ReportDocument,
    pub viewspec: Option<cie_viz::ViewSpec>,
    pub concepts: Option<ConceptDocument>,
    pub stack: Option<StackDocument>,
    #[serde(rename = "polyglotSpecs")]
    pub polyglot_specs: Option<ArchitectureBundleDocument>,
    pub deps: Option<DependencyDocument>,
    pub cochange: Option<CochangeDocument>,
    pub overlap: Option<OverlapDocument>,
    pub drift: Option<DriftDocument>,
    pub runtime: Option<RuntimeDocument>,
}
