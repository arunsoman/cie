# CIE storage retrieval and ViewSpec API specification

Status: Proposed; implementation contract, not a declaration of existing capabilities.
Version: 1.3. Date: 2026-10-23.
Related: [Storage architecture](storage-module-architecture.md), [ingestion architecture](ingest-polyglot-architecture.md).

## 1. Objective and boundary

Expose one typed retrieval API for repository entities, relationships, artifacts,
evidence, aggregates, graphs and renderer-compatible ViewSpecs. A caller expresses
what to find and which representations to return. Storage owns physical access,
query planning, caching, pagination and revision consistency.

Storage remains the `cie-storage` Rust library inside the caller's process. This
specification introduces no daemon, HTTP service, IPC protocol or additional
process. The existing web server may serialize requests and responses through its
own routes; that transport is outside this module contract.

Ingestion submits evidence and analysis outputs through storage's write API. A
retrieval request must never invoke ingestion, read the live source tree, clone a
repository or silently replace missing evidence with model-generated assertions.
Domain algorithms remain in analysis/projection modules. Storage invokes registered
projection providers when requested and permitted.

## 2. Current implementation and required additions

| Area | Existing foundation | Required by this specification |
|---|---|---|
| Reader | `RepositoryReader::open`, `file_facts`, `symbols` | Revision-pinned domain reader and composable queries |
| Artifacts | `RevisionArtifacts::get`, typed `Artifact` selector | Capability discovery and versioned retrieval envelope |
| Persistence | Repository writer, revision publication, policy profiles | Consistent leases spanning all queried stores/artifacts |
| Views | Persisted polyglot bundle and existing visualization producers | Unified validated ViewSpec output adapter |
| Queries | Basic `SymbolQuery` | Relationship predicates, set expressions, bounded traversal |
| Results | Individual method-specific records | Multiple typed outputs from one logical match set |

Existing files are authoritative for current behavior. The types below specify the
target API; code fragments are interface sketches rather than compilable patches.

## 3. User stories

1. Find a symbol by file and qualified name, or by a source byte position.
2. Retrieve concepts containing a symbol and the evidence for each membership.
3. Find symbols in a file that belong to a concept, optionally including its descendants.
4. Find files implementing a concept, without duplicate file records.
5. Find members shared by two concepts, in either concept, or in one but not the other.
6. Find callers of the matching symbols and paths from routes to those symbols.
7. Retrieve duplicate families and historical coupling associated with a symbol.
8. Return entity details, supporting evidence and a ViewSpec in one request.
9. Count matches by file, language or concept over the complete logical match set.
10. Explain why a view is empty, incomplete, unavailable or truncated.
11. Continue paging or drill down while new ingestion publishes another revision.
12. Discover supported entity types, relationships, artifacts and views before querying.
13. Slice and dice the same evidence across dimensions, pivot measures, roll up hierarchies and drill through a cell to its source entities.

## 4. Public entry points

```rust
impl Storage {
    pub fn open(root: &Path, policy: StoragePolicy) -> StorageResult<Self>;
    pub fn revisions(&self, request: RevisionListRequest)
        -> StorageResult<Page<RevisionInfo>>;
    pub fn open_revision(&self, selector: RevisionSelector)
        -> StorageResult<RevisionReader>;
}

impl RevisionReader {
    pub fn info(&self) -> &RevisionInfo;
    pub fn capabilities(&self) -> &RevisionCapabilities;
    pub fn resolve_symbol(&self, lookup: SymbolLookup)
        -> StorageResult<SymbolResolution>;
    pub fn retrieve(&self, request: RetrievalRequest)
        -> StorageResult<RetrievalResponse>;
}

enum RevisionSelector { Latest, Id(RevisionId) }
```

`Latest` resolves once when opening the reader. All later calls use that exact
revision. The reader pins required resources until dropped; garbage collection
must respect the pin. A missing historical backend returns `RevisionUnavailable`,
never data from the latest database under an older revision label. Opening latest
must see either the previous complete revision or the new complete revision,
never intermediate ingestion state. This guarantee requires implementation across
both database and artifact backends, not merely attaching a revision field.

`RevisionCapabilities` lists supported operations, captured domain coverage,
artifact schema versions, registered view providers, available cluster algorithms
and effective limits. Support and captured availability are separate properties.

## 5. Entity identity and domain relationships

Every entity reference includes repository identity, revision, entity kind and an
opaque ID. IDs are stable within a revision; cross-revision identity is not assumed.
Cross-repository or cross-revision references in a request are rejected.

| Entity kind | Examples |
|---|---|
| File | Repository-relative source file |
| Symbol | Function, method, class, module |
| Concept | FCA/Godin concept node |
| Component | Evidenced architecture component |
| Route | Framework route or entry point |
| Dependency | Package or module dependency |
| Finding | Security, testing or other analysis finding |
| Cluster | Duplicate family or an explicitly registered clustering result |
| State / Workflow | Stored state or workflow entities, when captured |
| Evidence | Source span, extractor record or analysis provenance |

Relationships are typed and have declared source/target kinds, direction,
availability and provenance: `DefinedIn`, `Calls`, `MemberOfConcept`,
`ChildConceptOf`, `Imports`, `DependsOn`, `ExposesRoute`, `RouteEntrySymbol`,
`MemberOfCluster`, `SupportedBy`, plus registered domain extensions.

FCA/Godin concept membership, exact duplicate families and co-change are distinct.
A co-change edge is weighted historical coupling; it is not a cluster unless an
explicit clustering algorithm produced a persisted cluster result. Cluster records
identify algorithm, version, parameters and input revision. Similarity is not proof
of concept membership or functional equivalence.

## 6. Symbol resolution

```rust
enum SymbolLookup {
    ByName { file: RepoPath, qualified_name: String },
    AtPosition { file: RepoPath, byte_offset: u64 },
}
enum SymbolResolution {
    Found(EntityRef),
    NotFound,
    Ambiguous { candidates: Vec<SymbolCandidate> },
}
```

Paths are normalized repository-relative paths using `/`; absolute paths, traversal
outside the repository and invalid encodings are rejected. Path case follows the
stored repository path, independent of the host filesystem. Byte offsets are
zero-based UTF-8 byte offsets into the captured revision's source, not live disk.
An at-position lookup chooses the narrowest containing declaration span; equal
candidates remain ambiguous. Missing captured spans yield an explicit unavailable
error. A name lookup must not silently choose among overloads.

## 7. Query algebra

```rust
struct RetrievalRequest {
    api_version: u32,
    query: SetExpr,
    outputs: Vec<NamedOutputRequest>,
    budget: QueryBudget,
    failure_mode: FailureMode,
}

enum SetExpr {
    CellContributors { descriptor: DrillThroughDescriptor },
    Select { kind: EntityKind, filter: Predicate },
    Union(Vec<SetExpr>),
    Intersect(Vec<SetExpr>),
    Difference { left: Box<SetExpr>, right: Box<SetExpr> },
    Follow { input: Box<SetExpr>, traversal: Traversal },
}

enum Predicate {
    All,
    Ids(Vec<EntityRef>),
    Field { field: FieldId, op: CompareOp, value: TypedValue },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
    Not(Box<Predicate>),
    Related {
        relation: RelationKind,
        direction: Direction,
        target: Box<SetExpr>,
        quantifier: RelationQuantifier,
    },
    ConceptMembership { concepts: Vec<EntityRef>, scope: MembershipScope },
}

enum MembershipScope { Direct, IncludeDescendants }
enum RelationQuantifier { Any, None, All }
```

`Select` returns a homogeneous set. Union, intersection and difference require
compatible entity kinds and deduplicate by entity identity. `Not` is evaluated
within the selected entity domain. Unknown fields, wrong value types, incompatible
relationship endpoints and empty logical/operator lists are validation errors.

`Related` tests a relationship without changing the selected entity kind. `Follow`
returns destination entities and therefore can change the kind. `Any` means at
least one related entity is in the target set; `None` means no such entity. `All`
requires a non-empty target set and a matching relation to every target entity.

`ConceptMembership` means membership in any listed concept. Intersect two such
selections to require both. `Direct` uses explicitly recorded membership.
`IncludeDescendants` also follows `ChildConceptOf` transitively and unions the
members of all descendant concepts; it does not follow parents. Inherited matches
retain their direct concept and membership path as provenance.

Traversal declares relation kinds, direction, minimum/maximum depth, maximum
visited entities and whether paths are required. Depth is bounded; cycles are
visited safely. Duplicate destination entities are returned once, while requested
paths are separately bounded. The API exposes recorded call relationships, not a
claim that every possible runtime dispatch was resolved.

No raw SQL, arbitrary executable expressions or unbounded recursion is accepted.

## 8. Outputs: one match set, several representations

```rust
struct NamedOutputRequest { id: String, output: OutputRequest }
enum OutputRequest {
    Entities { fields: Vec<FieldId>, order: Vec<Sort>, page: PageRequest },
    Relationships { relations: Vec<RelationKind>, page: PageRequest },
    Graph { expansion: Traversal, limits: GraphLimits },
    View { source: ViewSource, kind: ViewKind, options: ViewOptions },
    Evidence { scope: EvidenceScope, page: PageRequest },
    Cube { request: CubeRequest },
    Facets { request: FacetRequest },
    Detail { kind: DetailKind, options: DetailOptions },
    Artifact { kind: ArtifactKind },
}
```

Output IDs must be unique and responses preserve request order. Entity outputs
return the query's entity kind. To retrieve files from symbols, use `Follow` with
`DefinedIn`, rather than a hidden implicit conversion. An artifact is a complete
revision artifact and explicitly has revision scope; it is not silently filtered
by the query. A future scoped-artifact provider must declare that capability.

Relationship output includes edges whose endpoints are both matches by default.
Graph expansion can add context nodes, which must be marked separately from
matches. Evidence scope selects direct match evidence or match plus derivation
paths. View providers declare which input kinds they support.

Each output has independent pagination/limits but shares the same logical match
set and revision. Aggregates run over the complete match set before entity paging
or graph truncation. Count supports distinct entities; relationship weights and
numeric fields support only measures declared in the capability schema. One
entity in multiple groups contributes once to each group; group totals need not
sum to the distinct overall total. Approximate aggregates require explicit opt-in
and an error/estimator description; otherwise budget exhaustion is an error.

Storage may stream or materialize matches internally. This contract does not
require an unlimited in-memory result set or promise one physical database scan.

## 9. ViewSpec contract

Supported target view kinds: repository map, C4 at a named level, state machines,
workflows, concept hierarchy, duplicate clusters, co-change relationships,
dependencies, impact and generic relationships. Providers advertise actual support;
listing a target here does not make it implemented.

The result is a tagged adapter around existing renderer-compatible ViewSpec
schemas, carrying renderer family and schema version. It must reuse existing
contracts rather than introduce a visually similar incompatible schema. A request
can yield multiple specs, such as one per state machine.

`StoredOnly` loads a compatible stored view or returns unavailable.
`BuildIfMissing` permits bounded projection from stored evidence in the pinned
revision. It does not permit source parsing, ingestion, network access or LLM calls.
An unscoped stored view cannot satisfy a scoped query unless the provider can
correctly project it. No silent fallback to the whole repository is permitted.

Each view declares its applied query/scope, displayed matches, context nodes,
evidence references, coverage and truncation. Rendered IDs map back to entity
references. Limits cannot produce dangling edges. Collapsed nodes retain a bounded
drill-down descriptor tied to the same revision. Missing inputs return unavailable;
zero matches return empty. Renderer/schema incompatibility is explicit.

Projection cache keys include repository/revision identity, canonical query,
provider and schema versions, view options and limits. Only complete successful
projections enter the reusable cache. Providers execute outside long write locks;
cache writes cannot mutate the captured source revision.

## 10. Response envelope and failure semantics

```rust
struct RetrievalResponse {
    meta: RetrievalMeta,
    matches: MatchSummary,
    outputs: Vec<NamedOutputResult>,
}
struct RetrievalMeta {
    api_version: u32,
    repository: RepositoryId,
    revision: RevisionId,
    query_hash: String,
    coverage: Coverage,
}
enum OutputStatus { Ready, Empty, Unavailable, Truncated, Failed }
enum FailureMode { Atomic, BestEffort }
```

Each output contains its ID, tagged payload, schema version, status, origin
(stored/cached projection/derived projection), continuation when applicable, and
structured diagnostics. No untyped `anything` payload is allowed: output variants
form an extensible, versioned union. New extension types declare their schema and
capabilities; callers must explicitly support them.

A query's match count is `Exact(n)` or `Unknown`; absence of a cheap count must not
be reported as zero. Coverage distinguishes complete captured inputs, known
exclusions, partial analysis and unknown coverage. Empty results under partial
coverage do not establish absence in the source repository.

`Atomic` is the default: query or requested-output failure returns an error and
no successful response bundle. Internal deterministic cache work may still occur.
`BestEffort` returns successful outputs and typed per-output errors. Query
validation, revision failure and failure to establish the logical match set remain
request-level errors in both modes. Consumers never receive partial data without
an explicit truncated status or output error.

Error codes: `InvalidRequest`, `UnsupportedQuery`, `UnsupportedOutput`,
`RevisionNotFound`, `RevisionUnavailable`, `MixedRevisionReference`,
`ArtifactUnavailable`, `ProjectionUnavailable`, `BudgetExceeded`, `Cancelled`,
`CursorInvalid`, `CursorExpired`, `SchemaMismatch`, `CorruptData`, `Io`.
Errors include a safe message and structured diagnostic context, without exposing
credentials or unrelated source content.

## 11. Limits, pagination and performance

Proposed interactive defaults, configurable within storage-enforced maximums:

| Limit | Default | Hard maximum |
|---|---:|---:|
| Entity page | 100 | 500 |
| Graph nodes | 500 | 5,000 |
| Graph edges | 1,000 | 10,000 |
| Traversal depth | 2 | 8 |
| Traversal visited entities | 10,000 | 100,000 |
| Response bytes | 4 MiB | 16 MiB |
| Request execution | 2 seconds | 30 seconds |
| Outputs per request | 4 | 16 |
| Query nesting | 8 | 16 |

The byte budget includes evidence and ViewSpecs. Budgets apply across all outputs,
not separately to each. A cancellation token and deadline are checked during
traversals and projections. Providers must support cancellation or be rejected for
bounded interactive execution. Policy profiles may impose smaller limits.

Stable ordering always includes entity identity as a final tie-breaker. Opaque
continuation cursors bind to repository, revision, canonical query, output options
and ordering. A continuation executes against the original revision; released or
collected revisions produce an explicit error. No cursor implies an indefinite
resource lease. Cursors have a declared expiry and must be validated against
modification when exposed outside the process.

Initial indexes should cover file-to-symbol, symbol-to-file, both call directions,
concept membership in both directions, concept hierarchy and cluster membership.
Filter pushdown, prepared statements and shared intermediate matches are preferred.
Avoid deserializing the whole repository merely to answer a file lookup. Query
plans and timings are diagnostic capabilities, not a public physical-schema API.
These defaults are design limits, not measured latency guarantees. Benchmark cold
and warm lookup, concept intersection, traversal and scoped ViewSpec on small,
medium and large fixtures before setting performance acceptance thresholds.

## 12. Worked query

Question: symbols in `payments/service.rs` belonging to a selected concept or its
subconcepts, returned as records, evidence and a relationship ViewSpec.

```json
{
  "api_version": 1,
  "query": {
    "select": {
      "kind": "symbol",
      "filter": {
        "and": [
          {"field": "file.path", "op": "eq", "value": "payments/service.rs"},
          {"concept_membership": {
            "concepts": [{"repository": "repo-1", "revision": 42, "kind": "concept", "id": "concept-17"}],
            "scope": "include_descendants"
          }}
        ]
      }
    }
  },
  "outputs": [
    {"id": "symbols", "entities": {"fields": ["id", "qualified_name", "kind", "location"], "order": [{"field": "qualified_name", "direction": "asc"}], "page": {"limit": 100}}},
    {"id": "proof", "evidence": {"scope": "matches_and_derivations", "page": {"limit": 100}}},
    {"id": "diagram", "view": {"source": "matches", "kind": "relationships", "options": {"detail": "overview", "max_nodes": 100, "max_edges": 200, "availability": "build_if_missing"}}}
  ],
  "budget": {"max_response_bytes": 4194304, "timeout_ms": 2000},
  "failure_mode": "atomic"
}
```

This JSON illustrates the required serialization shape. Implementation must publish
machine-readable request/response schemas, discriminators and default expansion
rules before clients depend on it. Omitted budget fields use the selected policy's
bounded defaults. The caller opens revision 42 first; embedding a different
revision in a concept reference is rejected.

Additional query recipes:

| Need | Expression |
|---|---|
| Symbols in file and concept | Select symbols with file predicate AND concept membership |
| Concepts for a symbol | Follow symbol via `MemberOfConcept` outgoing |
| Files implementing a concept | Follow concept via membership incoming, then `DefinedIn` outgoing |
| Shared concept members | Intersect two symbol selections |
| Either concept's members | Union two symbol selections |
| Members of A excluding B | Difference of symbol selections |
| Callers of those members | Follow symbol matches via `Calls` incoming |
| Duplicate families | Follow symbol matches via `MemberOfCluster` outgoing, filter algorithm |

## 13. Module layout and dependency constraints

Proposed public modules: `retrieval::{request,response,error}`, `identity`,
`capabilities`, `query`, `projection`, plus internal `planner`, `executor` and
`cursor`. Public records use domain IDs and typed values, not SQL rows.

A small shared contract module/crate holds retrieval and ViewSpec envelope types
when needed to avoid dependency cycles. Projection implementations register through
a trait and read a revision-scoped evidence interface. They must not depend on the
web application or call ingestion. The composition root registers existing
visualization producers; storage must not acquire a cyclic dependency on itself
through those producers. Existing APIs remain available during migration.

## 14. Implementation stages and acceptance criteria

| Stage | Deliverable | Acceptance |
|---|---|---|
| 1 | Versioned contracts, schema and capability registry | Invalid type/relationship combinations rejected; schema round trips |
| 2 | Revision pinning and basic entity reader | Concurrent publication never mixes revisions; unavailable history fails explicitly |
| 3 | Membership, set operations and traversal | File/concept queries, union/intersection/difference and cyclic calls verified |
| 4 | Multiple outputs, evidence, aggregates, cursors | Aggregation independent of paging; deterministic continuation; budget enforcement |
| 5 | Stored ViewSpecs and projection providers | Existing renderer validation passes; scope/evidence retained; no dangling edges |
| 6 | Web/CLI migration and performance validation | Consumers use public retrieval API; bounded benchmarks and compatibility checks |

Required behavioral fixtures include overloaded names, nested declarations,
non-ASCII byte offsets, concept diamonds, empty concepts, duplicate memberships,
call cycles, unresolved calls, deleted files across revisions, partial extraction,
missing artifacts, incompatible ViewSpec versions, expired cursors, cancellation,
projection failure and ingestion publication during a paginated query.

A multi-output test must demonstrate identical revision and match provenance across
records, evidence and diagram. An instrumented retrieval test must prove no source
files are opened and no ingestion child/process is started. Legacy direct database
readers are removed only after their replacement passes equivalent behavior tests.

## 15. Out of scope for version 1

Cross-repository federation, cross-revision identity reconciliation, natural-language
query interpretation, arbitrary user code execution, live-source retrieval,
automatic LLM enrichment and new clustering algorithms. These require separate
contracts; they must not appear as implicit behavior of retrieval.

## 16. First-class slice-and-dice analytics

Slice-and-dice is a required retrieval capability, not a visualization-only feature.
The API must allow users to select, combine and rearrange dimensions while retaining
the exact revision, logical entity population, coverage and evidence. No new
ingestion is required when changing an analytical perspective.

| Operation | Required behavior | Example |
|---|---|---|
| Slice | Fix one dimension to a value | Python symbols only |
| Dice | Combine dimension predicates and value sets | Python/Java symbols in payments/auth with high-severity findings |
| Group | Aggregate by one or more dimensions | Symbol count by component and language |
| Pivot | Place dimension members on rows/columns with explicit measures | Components as rows, languages as columns |
| Roll up | Move to a declared ancestor level | File → directory → component, where a mapping exists |
| Drill down | Expand a selected group to a finer declared level | Concept → child concepts → member symbols |
| Drill through | Retrieve entities contributing to a particular cell | Open symbols counted in a concept/language cell |
| Facet | Return available dimension values and matching entity counts | Languages and concepts available after current filters |
| Compare | Show measures for explicit selections within the same revision | Payment component versus authentication component |

Cross-revision comparisons remain outside version 1. Neither a pivot nor a roll-up
may infer a semantic hierarchy from labels or fabricate membership links.

### 16.1 Analytical request contract

```rust
struct CubeRequest {
    dimensions: Vec<DimensionSelection>,
    measures: Vec<NamedMeasure>,
    layout: CubeLayout,
    cell_filter: Option<MeasurePredicate>,
    order: Vec<CellSort>,
    page: CellPageRequest,
    totals: TotalsRequest,
    missing: MissingMemberPolicy,
    drill_through: DrillThroughPolicy,
}
struct DimensionSelection {
    id: String,
    dimension: DimensionRef,
    level: Option<HierarchyLevel>,
    membership: DimensionMembership,
}
enum DimensionRef {
    Field(FieldId),
    RelatedEntity { relation_path: Vec<RelationStep>, attribute: FieldId },
    NumericBucket { field: FieldId, boundaries: Vec<FiniteNumber> },
    TimeBucket { field: FieldId, interval: TimeInterval, timezone: String },
}
enum CubeLayout {
    Cells,
    Pivot { rows: Vec<String>, columns: Vec<String> },
}
enum DimensionMembership { RecordedOnly, IncludeAncestors }
enum MissingMemberPolicy { Exclude, IncludeUnknown }
```

The outer query applies entity predicates before aggregation (slice/dice).
`cell_filter` applies measure predicates after grouping, equivalent to HAVING.
Layout changes the representation, not the underlying measure semantics. Dimension
IDs referenced by a pivot must exist and each selected dimension must occur exactly
once across rows and columns. A dimensionless request is a single aggregate cell.

Capability discovery declares each dimension's type, supported entity populations,
cardinality estimate, missing-value meaning, supported hierarchy levels, available
measures and whether membership is single-valued or multi-valued. Concept, cluster,
file, language, directory, component, dependency, finding severity and historical
time are target dimensions only where captured evidence supports the requested
population. For example, severity for symbols requires a declared relationship to
findings; time requires captured historical facts. Unsupported dimensions return
`UnsupportedQuery`, never an empty successful cube.

Hierarchies are named and validated. Directory ancestry can derive from normalized
stored paths; component membership requires recorded mapping. File-to-component
is not necessarily single-valued. Concept ancestry can be a DAG, not a strict tree;
roll-up deduplicates entities within each ancestor cell. `IncludeAncestors` exposes
that behavior explicitly and preserves membership provenance.

### 16.2 Counting, joins and totals

Every cube declares its base population, such as symbols or findings. Relationship
joins must not multiply the base population accidentally. The default count is
`DistinctEntityCount` of that population. Counting findings, relationships or paths
requires an explicit measure with its own declared grain.

Multi-valued dimensions can place the same entity in multiple cells. Cell counts
remain distinct within each cell; grand totals are recalculated over the matching
population and must not be computed by summing overlapping cells. Responses declare
whether measures are additive, non-additive or semi-additive across each dimension.
Numeric sums and averages require an unambiguous value at their declared grain;
an ambiguous join is rejected unless an explicit supported reduction is selected.

`IncludeUnknown` creates a reserved, typed unknown member for missing captured
values. It is distinct from an empty string, an unavailable dimension and a literal
user label named “Unknown”. Unknown analysis coverage remains disclosed separately.

Numeric bucket boundaries are finite, ordered and non-overlapping. Bins are
lower-inclusive and upper-exclusive, with explicit underflow/overflow handling.
Time buckets require a declared timezone and calendar interval; no silent host
timezone is used. Non-finite numeric values are rejected.

Subtotals and grand totals identify whether they cover all pre-HAVING matches,
post-HAVING cells, or the displayed page. The default is all pre-HAVING matches;
page totals, if requested, are separate. Top-K, paging and display suppression never
silently redefine grand totals. Missing cells are absent in sparse results; a dense
pivot may show zero only when absence is established under the declared coverage.

### 16.3 Cube result and drill-through

A cube result includes dimension/member dictionaries with stable revision-scoped
keys, sparse cells, measure definitions and units, requested totals, coverage,
continuation, effective limits and explicit truncation status. A pivot is a layout
of those cells; storage does not allocate an unbounded dense matrix.

Each cell may include an opaque drill-through descriptor bound to repository,
revision, canonical query, dimension members, membership rules and measure grain.
The reader accepts this descriptor through a `SetExpr::CellContributors` extension.
It reconstructs the contributing entity set and supports the same entity, evidence,
graph and ViewSpec outputs as any other query. Descriptors cannot inject SQL or
bypass normal revision checks, budgets or cursor validation.

The distinct contributors for a count cell must match its displayed count when
fully retrieved under the same coverage. For sums/averages, drill-through returns
contributing records and values at the measure grain, not an unrelated base-entity
list. Derived comparisons expose the contributing selections separately.

A chart ViewSpec can consume a named cube output through an explicit output
reference. Output references form an acyclic dependency graph within the request;
unknown references and cycles are validation errors. This ensures a chart and its
table share computed cells rather than independently regrouping evidence. Chart
providers must advertise cube support and preserve cell drill-through descriptors.
Generic graph ViewSpecs do not automatically become chart providers.

### 16.4 Facets and interactive changes

A `Facets` output returns bounded dimension members and distinct-population counts
for a selected set of dimensions. Default counts include all current query filters.
Optional self-excluding facets require tagged dimension filters so storage can
remove exactly that dimension's filter while retaining the others; unsupported or
ambiguous self-exclusion is rejected. Facet values are paginated with stable keys.

Changing a slice, dimension, hierarchy level or measure creates a new canonical
request. Reuse cached intermediates when valid, but do not reuse a cursor from the
previous request. Readers retain the pinned revision during interactive exploration.

### 16.5 Analytical budgets and validation

In addition to shared request limits, proposed defaults are 4 dimensions, 8 measures,
1,000 returned cells and 100,000 evaluated groups; hard maximums are 8 dimensions,
32 measures, 10,000 returned cells and 1,000,000 evaluated groups. Facet pages use
entity page limits. All analytical work shares the request deadline and byte budget.
Hard maximums are ceilings, not promises that a request will fit its time budget.

Exceeding intermediate-group limits returns `BudgetExceeded`; the system must not
return silently inaccurate aggregates from a partial scan. Explicit top-K limits
returned cells, not the work required to establish exact top-K and totals. Budget
failure in a chart's source cube makes that chart unavailable/failed according to
the request's failure mode; it must not render an apparently complete chart.

Required acceptance tests extend stages 3–6: multi-dimension filtering; identical
measures before/after pivot; DAG roll-up deduplication; multi-concept double-count
prevention; unknown members; timezone boundaries; numeric bucket boundaries;
HAVING and top-K total scopes; exact cell drill-through; facet self-exclusion;
chart/table cell parity; intermediate-cardinality rejection; and no source reads
when changing slices. Performance fixtures include high-cardinality dimensions and
many-to-many concept memberships. Machine-readable schemas must include cube,
facet, cell-contributor and output-reference variants before client migration.

## 17. Current UI compatibility review

The [source-based compatibility validation](storage-ui-compatibility-validation.md) records the gaps found in v1.1. Section 18 specifies their adaptations in v1.2. Contract coverage is proposed; runtime parity and migration remain unverified.

## 18. Simple API with adapters for existing UI requirements

This section resolves the v1.1 review gaps at the specification level. Where earlier
examples use `ViewSpec`, `CubeViewSpec` or `Aggregate` output variants, normalize them
to `View` or `Cube` below; they are explanatory shorthand, not additional public
methods or wire variants. Version 1 has not shipped, so no legacy aliases need be
implemented. The JSON example uses the normalized `view` variant with `source: matches`,
`kind: relationships` and availability inside its options.

### 18.1 One retrieval call, typed outputs

The normal public workflow remains:

```rust
let reader = storage.open_revision(RevisionSelector::Latest)?;
let response = reader.retrieve(request)?;
```

No separate methods for every screen. Existing query algebra, budgets, revision
rules and response envelope apply to all outputs. Specialized domain records are
retrieved using `Detail`; complete compatibility documents use `Artifact`.

```rust
enum DetailKind { Source, FunctionIr, Impact }
enum DetailOptions {
    Source { range: SourceRange, context_lines: u16 },
    FunctionIr { include_calls: bool },
    Impact { availability: AvailabilityPolicy },
}
enum SourceRange { WholeFile, Bytes { start: u64, end: u64 }, Declaration }
enum ViewSource { Matches, Output { id: String } }
```

The kind/options pair must match. Serialized requests use a discriminated detail
union so invalid pairs cannot be constructed on the wire. Source and IR accept one
selected file/symbol and one selected symbol respectively; impact accepts one
resolved symbol. Multi-selection is rejected for these details in version 1.
Convenience builders may create these requests, but do not introduce more wire
operations. `WholeFile` remains byte-budget bounded and can return explicit
truncation with a continuation; range requests must never silently change ranges.

`ViewSource::Matches` uses the query matches. `Output` references a named compatible
cube/graph/detail output in the same request; compatibility is capability-checked.
Its provider options include `StoredOnly` or `BuildIfMissing` availability. A view
output has a renderer family, schema version, validated payload and entity/cell
links. Output references use the acyclic execution rules of §16.3.

### 18.2 Required entity and field adapters

| Kind | Required fields and semantics |
|---|---|
| Summary | Symbol/file references, descriptive text, effects, certainty and analysis provenance |
| Contract | Subject symbol/file references, condition, written/inferred authority and evidence |
| DuplicatePair | Both symbol references, similarity, tier, detection, epistemic status, optional counterargument and canonical symbol |
| Finding | Kind, severity, source location, message and evidence |
| Concept | Label, kind, recorded symbol memberships, hierarchy and recorded behavior/rules |
| Symbol | Qualified name, kind, file, declaration span, language and source identity |

These extend §5 and the field capability catalog. Summary/Contract relations include
`Summarizes` and `Constrains`, with explicit file/symbol endpoint kinds.
DuplicatePair endpoints are named roles `LeftSymbol`, `RightSymbol` and optional
`CanonicalSymbol`; it is not interchangeable with a duplicate cluster. Register
remaining existing fields through versioned schemas; no adapter may discard an
existing UI field merely because it is not enumerated in this table.

String comparisons include exact, prefix and substring with explicit case-sensitive
or Unicode case-folded semantics. The compatibility symbol-search preset uses the
current database search behavior, qualified-name length ascending and deterministic
identity tie-breaking. Its case/Unicode behavior must be frozen from fixtures
against the existing SQLite search rather than silently adopting a different
case-folding rule. This named preset can use a registered ranking order without
exposing SQL expressions.

### 18.3 Specialized payload contracts

| Detail | Payload | Availability and limits |
|---|---|---|
| Source | Captured file reference, content hash, encoding, requested/returned byte range, text, line-start map and truncation metadata | Retained captured blob only; missing blob is `SourceUnavailable`; shared byte limits |
| FunctionIr | Symbol reference, ordered blocks, ordered instructions with operation/spans, CFG edges and outgoing calls including unresolved callee text | Stored IR only; per-output node/instruction/edge limits and explicit truncation |
| Impact | Subject symbol, affected symbols, affected concepts, minimum test set, analysis provenance and coverage | Stored result or bounded existing algorithm over captured dependencies/test evidence |

Source ranges are half-open UTF-8 byte ranges. Invalid/reversed/out-of-bounds ranges
are rejected. Unaligned UTF-8 boundaries return `InvalidRequest` rather than damaged
text. Context expansion reports the expanded returned range and preserves the
requested range. Line-start offsets refer to the captured file. Ingestion must
record whether source retention is enabled; no read-time fallback to live files.
Live-source inspection remains a separate application action labelled as such.

IR block order uses stored block index and instruction index; edges cannot reference
omitted blocks without an explicit boundary reference. Do not fabricate missing
operands or resolved targets. Impact must invoke the existing algorithm semantics,
not rename a reachability result. It cannot run ingestion. Add `SourceUnavailable`
to the structured error catalog.

### 18.4 Compatibility artifact catalog and bootstrap

Required artifact kinds: `Report`, `RepositoryMap`, `Concepts`, `Stack`,
`Dependencies`, `Runtime`, `Stories`, `Quality`, `Overlap`, `Drift`, `CoChange`,
`Fault`, and `ArchitectureBundle`. Each has a registered schema/version, availability
and provenance. `Report` preserves summaries, findings, contracts, warnings,
coverage/evidence diagnostics, state model and usecase projection. `Stack` preserves
the complete TechStack schema including routes, injections and handler branches.
`Runtime` preserves tool/run identity, units, CPU, spans, trials and coverage.
Demo-only or uncollected artifacts remain explicitly unavailable.

A `UiBootstrapV1` artifact composes these into the existing client bundle shape,
including null optional artifacts and the stable file-array indices used by report
locations. It declares required versus optional members and per-member status;
its payload remains compatible while status metadata lives in the response envelope.
Storage assembles this through registered adapters; it is not a new ingestion stage.
Do not silently truncate a bootstrap document. If it cannot fit the response budget,
return `BudgetExceeded` and require the paged client path. Application-owned labels,
job IDs and local live-source permissions are attached by the web layer, not stored
as repository facts.

### 18.5 View families and filtering compatibility

Register three concrete renderer families: legacy CieViewSpec, PolyglotViewSpec
(and ArchitectureBundle), and viz-adapter ViewSpec. Capabilities name each actual
schema/version and accepted input kinds. Preserve evidence links, display modes,
footer counts, hidden/omission records, coverage, scope and layout hints. For the
shared adapter schema, apply its existing gate and hash algorithms after projection.
Never copy old hashes onto changed content.

Chart providers advertise the existing menu kinds individually: hbar, donut, stat,
table, UML, flow, tree, heatmap, sequence, C4 and mindmap. Cubes can supply statistical
charts; structural diagrams require the corresponding captured relationships and
provider. Missing support is explicit, not a generic graph substituted under the
requested diagram name.

Each filtered output reports `applied_filters`, `inapplicable_filters` and reasons.
Normal typed queries reject invalid field/population combinations. A named
`CurrentUiLinkedFilters` adapter preserves today's one-slot filter behavior across
findings, summaries, contracts, elements and canvas: severity/kind target findings,
effects target summaries, authority targets contracts, certainty targets elements
and canvas, and file applies only to populations with recorded file association.
Inapplicable filters leave that output unchanged and disclose the fact. The UI owns
toggle/replacement state; the API is stateless. New multi-dimensional queries use
normal explicit predicates and joins rather than changing this compatibility rule.

### 18.6 Small, separate diagnostic namespace

```rust
impl RevisionReader {
    pub fn diagnostics(&self, request: DiagnosticRequest)
        -> StorageResult<DiagnosticResponse>;
}
enum DiagnosticRequest {
    Datasets,
    Rows { dataset: DatasetId, page: DiagnosticPageRequest },
}
```

`Datasets` returns allowlisted IDs, versions, display groups, columns, row counts
and availability. `Rows` returns bounded typed cells, column metadata, total and
continuation. Binary values are summarized, long text explicitly shortened and
sensitive fields redacted by policy. No arbitrary SQL, physical paths or user-chosen
table names. Storage owns mappings from versioned dataset IDs to physical tables.
An optional bounded offset page supports current admin navigation; caps are
advertised. Domain clients use retrieval, not diagnostics. Unsupported dataset
versions return an explicit error.

### 18.7 In-process web integration

Provide a Node native binding to the same Rust library. The binding exposes
`openRevision`, `retrieve`, `diagnostics`, `capabilities` and `close`; handles are
opaque and bound to repository/revision. Existing routes remain thin compatibility
adapters initially. No retrieval subprocess, sidecar or HTTP storage service is
introduced. Execute blocking storage calls on workers with deadline/cancellation
propagation, not the Node event loop. Release revision leases when handles close;
server policy bounds idle handle lifetime and returns explicit expiry errors.

Public wire IDs are opaque strings; use explicit mappings for core versus polyglot
symbol spaces and lossless serialization of revision IDs and counters exceeding
JavaScript's safe-integer range. Published JSON schemas and generated TypeScript
contracts define these encodings. Route authorization and filesystem permission
checks remain the web application's responsibility; an opaque handle is not an
access-control policy.

### 18.8 Acceptance and migration

The adaptations above close the identified specification gaps; they do not imply
implemented parity. Extend stages 1–6 with fixtures from the compatibility review:
bootstrap shape/indices, all artifact schemas, symbol ranking, IR, duplicate pairs,
source-byte correctness, impact minimum tests, renderer gates, filter applicability,
diagnostic redaction and native-binding cancellation/revision release.

Initially keep existing route shapes and use adapters internally. Migrate screens
to paged retrieval incrementally. Delete direct Node SQL and retrieval CLI paths
only after parity tests run against the new binding. Keep ingestion jobs, live
filesystem browsing, branch selection, LLM conversation and workspace preferences
outside storage. This preserves a small public API without moving unrelated UI
responsibilities into the storage module.

## 19. LLM-friendly retrieval contract

### 19.1 Principle and deployment

Make retrieval easy for a tool-calling model without putting an LLM inside storage.
A thin application-owned tool adapter translates constrained JSON into the same
`RevisionReader::retrieve` calls. It does not generate SQL, invent entity links,
choose arbitrary functions or execute model-generated code. Tools run in the same
host process through the library/native binding. Publishing them through an
existing agent host is optional and introduces no storage service requirement.
Natural-language interpretation remains in the calling assistant; the storage
module remains deterministic and usable without any model.

### 19.2 Two tools, one pinned context

| Tool | Input | Result |
|---|---|---|
| `cie_describe` | Optional context handle and requested capability topics | Pinned repository/revision context, compact entity/field/relation/output catalog, limits and examples |
| `cie_retrieve` | Context handle, bounded selection, requested outputs and response budget | Typed results, evidence references, coverage, continuation and diagnostics |

The application selects and authorizes the repository before tool use. If no
context is supplied, `cie_describe` opens the application-selected repository's
latest complete revision and returns an opaque context. It cannot select an
arbitrary filesystem path. Repeating it with an existing context never advances
the revision. Context expiry is explicit; switching to a newer revision requires
a new application-authorized context, not a silent retry.

Describe defaults to a short catalog. Topic selection expands only needed schemas,
for example `symbol.fields`, `concept.relationships`, `outputs.source`, or
`views.c4`. A capability being supported is distinct from its evidence being
available in the selected revision. Tool schemas are generated from the same
versioned contracts as the Rust/TypeScript API, not manually maintained copies.

### 19.3 Simple selection with an advanced escape hatch

Expose a compact `select` shape for common questions; normalize it into the query
algebra of §7. Model-facing fields and operators are allowlisted, schema-described
and capability-checked. The advanced alternative is the existing typed query AST,
not raw SQL or arbitrary expressions. `select`, `query`, and `continuation` are
mutually exclusive request modes.

```json
{
  "context": "ctx_42",
  "select": {
    "kind": "symbol",
    "where": [{"field": "file.path", "op": "eq", "value": "payments/service.rs"}],
    "concept": {"id": "concept_17", "membership": "include_descendants"}
  },
  "outputs": [
    {"id": "rows", "entities": {"fields": ["id", "qualified_name", "location"], "page": {"limit": 20}}},
    {"id": "proof", "evidence": {"scope": "matches_and_derivations", "page": {"limit": 20}}}
  ],
  "response": {"profile": "compact", "max_bytes": 12000}
}
```

Compact IDs in this tool facade resolve only within its context to full repository,
revision and entity references. They are opaque strings, never numeric offsets or
names masquerading as identity. Resolve ambiguous names through an entity-selection
request using name filters; return candidates with file, kind and qualified name.
The model must select a candidate ID before requesting single-symbol details.
A concept label is likewise not an authoritative identifier.

Default queries are exact domain retrieval. Semantic search is advertised only if
an explicit provider, embedding revision, score interpretation and captured index
exist. Similarity results are labelled suggestions and cannot silently satisfy a
recorded membership query.

### 19.4 Compact evidence-first responses

The adapter returns a compact envelope with context/revision, typed outputs,
coverage, omissions, evidence dictionary, continuation and structured diagnostics.
Use caller-selected fields, bounded arrays and dictionary references instead of
repeating source text or long paths. Default entity page is 20; source text and
whole artifacts require explicit requests. Defaults must never expose credentials
or bypass application access checks.

Every factual entity/relationship/measure retains an entity reference and relevant
provenance/evidence references. Evidence entries identify revision, source hash,
file and span where available; algorithm-derived aggregates instead reference the
query, measure grain, coverage and contributors descriptor. Do not fabricate a
source span for an aggregate or a provenance-only claim. Certainty, inference and
unknown coverage remain explicit, even in compact mode.

The model may cite short evidence IDs in its answer; the application resolves them
to source links using this context. A missing evidence ID is invalid and must not
produce a plausible-looking link. Evidence references remain bound to revision;
continuation or context expiry cannot redirect them to latest source.

`compact` and `full` are serialization profiles over the same facts. Compact mode
may omit unrequested fields, but cannot strengthen a claim, hide unavailable data,
or drop the disclosure that a result is incomplete. Its response reports requested
and effective limits, returned item counts, and exact/unknown total semantics.

### 19.5 Output and token budgets

`max_bytes` is enforced against the serialized tool response and shares the storage
request budget. A host may additionally specify `max_tokens` only with a named,
available tokenizer; otherwise return `UnsupportedBudget` or require a byte limit.
Never claim byte count is an exact token count. The schema advertises effective
host/model limits and a reserved budget for coverage/error metadata.

If content exceeds the budget, return bounded pages with continuation and explicit
truncation. Required provenance/coverage must fit; otherwise return `BudgetExceeded`
rather than an apparently complete answer. Large ViewSpec payloads may be returned
as opaque result handles for the UI, with a compact description, coverage and
entity/cell links for the model. Handles are application-owned, context-bound,
expiring and subject to the same access checks; they are not arbitrary URLs.
Evidence required to explain the result remains accessible through retrieval.

### 19.6 Exploration and grounded answers

Expected tool workflow:

1. Describe available evidence and obtain a pinned context.
2. Resolve names or ask the user to disambiguate when several entities match.
3. Retrieve a small relevant set or aggregate; inspect omissions and coverage.
4. Request source/evidence or drill through a cell only when needed.
5. Request a View from those matches or a referenced cube for the UI.
6. Answer with valid evidence references and disclose unavailable or partial facts.

The adapter may include schema-generated next actions such as `next_page`,
`fetch_evidence`, `drill_through`, or `render_view`. Actions are validated typed
request fragments, not free-form executable instructions. A failed request includes
a stable code, field path, permitted alternatives and whether retry is meaningful.
Retries remain under host-controlled call, byte and execution budgets. Never
silently broaden scope, switch revisions or rerun ingestion to manufacture an answer.

### 19.7 Repository text is data

Source, comments, documentation, symbol names and generated labels are untrusted
content. Tool schemas and application instructions are separate from those fields.
A comment telling the assistant to ignore instructions or call another tool is
returned as source data with its provenance, not as a next action. The adapter is
read-only and exposes no write, shell, network or arbitrary-path capabilities.
Diagnostic raw dataset browsing is not included in the default model tool catalog;
it requires an explicitly enabled application mode and bounded redaction policy.

### 19.8 Acceptance criteria

- Generate JSON tool schemas and examples from the contract registry; invalid
  predicates fail before execution with a precise field error.
- Test symbol-in-file/concept, concept intersection, file contribution, caller
  traversal, duplicate detail, IR, source, impact, cubes and ViewSpecs through the
  same adapter used by agents. Results must match direct library calls.
- Verify compact/full result equivalence for selected fields and unchanged certainty,
  revision, evidence validity, missing-data and coverage semantics.
- Exercise ambiguous names, expired contexts, stale/foreign IDs, forged continuations,
  unsupported capabilities, missing source and partial analysis.
- Test byte/token limits, large diagram handles, bounded retries and cancellation.
- Use adversarial repository comments/docs to verify content cannot change the
  selected repository, tool permissions, query scope or execution mode.
- Verify drill-through and diagram output share the answer's match set/revision;
  no source reads, network inference or ingestion occurs during retrieval.

This section specifies the model-facing adapter, not model accuracy guarantees.
End-to-end answer quality requires evaluating the calling assistant separately.

### Typed intersection narrowing

A typed intersection may narrow a polymorphic relation traversal: for example,
`select(symbol) ∩ follow(concept, member_of_concept, incoming=true)` yields only
symbol contributors even though the relation also allows file endpoints. The
narrowed kind propagates into enclosing set operations. Disjoint explicit kinds
remain invalid; unions still require compatible homogeneous operand kinds.
This does not permit a mixed file/symbol union. Atlas uses this bounded traversal
rather than repeatedly evaluating nested reverse-related predicates per entity.

### Captured application and service entry identities

`entry_point` is a retrievable entity kind projected from captured application
entries (`stack.mains`). `route` and `entry_point` records expose
`handler_resolution` (`unique`, `ambiguous`, `unresolved`) and
`handler_candidate_count`. Only `unique` records expose `source_symbol_ref`,
`handler_location` and a `source_symbol` edge to the captured Symbol. `defined_in`
continues to identify the declaration file separately. A route's file is not its
handler scope. Duplicate names/overloads do not receive a guessed symbol edge.

Traverse `source_symbol` forward to select a route's target, or incoming from
selected Symbols to find their routes/application entries. Typed intersections
narrow this polymorphic relation. Application bootstrap classes retain Class
identity; they do not establish a function execution flow. Unsupported inline,
listen and other entries remain queryable as records with unresolved status.
