# Storage retrieval implementation

Implementation of the proposed [retrieval contract](storage-retrieval-api-spec.md).

## Running

Build `cie-cli`; `npm run dev` and `npm run build` in `web` now build the native
storage addon automatically (Rust/cargo must be on PATH). `npm run build:storage`
is also available explicitly. The native addon runs inside a Node worker, not a separate process.
`CIE_STORAGE_NATIVE` can select the packaged `.node` file. Retained databases are
created by new ingestion revisions; older formats are rebuilt on next ingestion.

## Implemented foundation

- Retained per-revision SQLite databases, published under the existing writer lease.
- Pinned database, core snapshot and compatible artifacts; captured source text.
- Typed query AST, set operators, concept membership (direct/with descendants),
  bounded relation traversal (min/max depth, cycle-safe), Any/None/All relation
  quantifiers and validated relationship endpoints.
- Records, evidence, graph, ordered records (identity tie-break), exact
  cubes/facets (self-excluding), contributor descriptors and expiring cursors.
- Symbol resolution by file plus qualified name or captured byte position, with
  ambiguity candidates, UTF-8 byte correctness and normalized repository paths.
- Captured source (declaration/whole-file/byte ranges, context lines, chunked
  continuation), stored IR (optional calls) and existing impact/minimum-test
  algorithms with an explicit availability policy.
- Cubes with field/related-entity dimensions, numeric bucketing
  (lower-inclusive/upper-exclusive), calendar month/year and IANA/fixed-offset time buckets, named
  measures, HAVING cell filters, explicit cell ordering, pivot layouts, scoped
  totals and the typed unknown member.
- Existing UI SQL readers moved into storage-owned compatibility adapters.
- In-process Node binding (openRevision/retrieve/resolve/diagnostics/
  capabilities/revisions), worker bridge, revision-aware existing UI requests,
  generic `/api/storage` transport and no-ingestion impact retrieval.
- Schema-generated model tool contracts, compact selection facade and byte budgets.
- Relationship ViewSpecs sealed and validated with the existing renderer gate.

The current UI can continue using its artifact shapes. Live filesystem source
browsing remains a separate application operation. Optional uncollected artifacts
remain unavailable. Retaining full databases per revision uses additional disk;
retention/garbage collection policy is not automatic in this foundation.

## Verification

Rust integration fixtures cover publication during reads, historical reopening,
source retrieval after deletion, cursor forgery, concept sets, cube
buckets/HAVING/order/pivot/totals, unknown members, drill-through, quantifiers,
minimum-depth cyclic traversals, facet self-exclusion, symbol resolution with
overloaded names and non-ASCII byte offsets, source ranges and context lines,
unreachable stored-only views and missing-artifact behavior. The native test
exercises the real worker, addon, model facade, resolution and revision listing.

## Contract completion

The five gaps identified in the October 24 review are implemented. See
[the completion verification](audits/storage-retrieval-conformance-2026-10-24.md)
for the requirement-to-code/test mapping.

Output payload schema version 2 is a closed Rust/JSON-Schema union. It adds the
`data.type` discriminator while preserving record/chart/source fields. Artifact
results carry `artifact_kind`, `schema_version` and a typed `document`; consumers
must handle the declared artifact kind and version. Existing UI compatibility
adapters keep their earlier wire formats.

Only the explicitly optional tokenizer-specific budgets and opaque large-result
handles are not enabled. Byte budgets and inline payloads remain supported;
unknown tokenizer budgets return `UnsupportedBudget`. Stored-only requests
correctly return unavailable when the selected revision has no compatible stored
artifact or complete cached projection.

Exact file/source lookups use a prepared indexed path lookup without decoding
symbol, summary or concept fields. Other expressions retain bounded, exact fact
evaluation; broader optimizer strategies are performance choices rather than
additional public API modes. Reader-local projection caches remain bounded at
32 entries / 8 MiB total / 1 MiB per entry and are revision-bound. Retained URL
jobs follow the existing seven-day cleanup policy.

## Audit repair stage (2026-10-23, historical)

The seven previously failing audit cases are now regular CLI integration tests.
Output dependencies execute topologically and preserve requested response order;
missing/cyclic dependencies and non-cube chart sources are rejected. Failed
outputs cannot supply chart data. Donuts require a complete disjoint count
partition. Unicode folded matching uses full Unicode case folding. Fault data
is read from the persisted report. Retrieval skips unrelated duplicate tables.

URL and branch-snapshot jobs retain `.cie` independently of checkout cleanup.
The storage module owns archive copying and preserves repository identity;
the UI carries a separate `storageRoot`, including workspace restoration.
Revision-pinned source panels use captured content with integrity verification.
Reader publication-lock waits are bounded. Best-effort missing artifacts and
projections return `unavailable`. The HTTP transport exposes resolve,
capabilities and revision listing. Normal dev/build invokes the native build;
standalone packaging copies the worker and addon explicitly.

Numbers now includes a storage-backed chart explorer, grouping symbols by
language, kind or file and retrieving contributor records. Storage cube
projections are adapted to the existing chart renderer. Native integration
covers archive relocation and identity preservation. This stage does not claim
that the further contract coverage listed above is complete.

## Extended implementation stage (2026-10-23, historical)

- Added queryable route, framework, component, dependency, state-machine, state,
  workflow and evidence entities, plus typed relationships to captured facts.
- Query planning inspects the typed AST rather than user text. Set-kind and
  relationship endpoint validation also works for empty results.
- Related concept/component dimensions support cycle-safe ancestry rollup.
- Month/year and named IANA timezone buckets use calendar-aware boundaries;
  repeated DST hours retain distinct offsets. Nonexistent boundaries fail
  explicitly rather than guessing.
- Graph expansion is bounded and reports matched nodes separately from context.
- Versioned projection capabilities and a bounded reader cache support
  `stored_only` reuse. Scoped architecture projections filter captured evidence,
  disclose parent context/omissions, enforce caps, and rerun the producer gate.
- Per-request atomic cancellation reaches Rust loops and SQLite. The HTTP bridge
  passes abort signals and cancels a request without terminating the shared
  worker. Compatibility/resolver SQL has a deadline as well.
- Outputs carry a kind discriminator, stored/derived/cache origin, filter
  applicability and effective execution limits.
- Model discovery supports compact/full profiles and query/output/view/limit/
  evidence topics. Tool retrieval defaults to compact, 20-row pages, deduplicated
  evidence references with captured source hashes, and schema-backed next-page
  actions. Full mode keeps inline evidence fields.
- Standalone packaging includes the native addon, control entry, worker, public
  assets and static assets under either Next output layout. The start helper
  locates the generated server and sets its working directory.

The regular audit acceptance suite now covers all seven original failures plus
reader reuse after cancellation, incompatible empty sets, captured domain
traversal, cached availability, scoped architecture filtering, executable model
continuations and graph context. Calendar unit tests cover month boundaries and
both occurrences of a repeated daylight-saving hour.

### Historical validation notes

- The native addon builds, the 14 storage audit acceptance tests pass, and the
  web suite passes 103 tests after rebasing the parallel persona/chat changes. The web TypeScript check passes separately.
- Final functional workspace run: 586 passed, 11 ignored, with only the existing
  drift relative-timing assertion selected out. Its isolated suite passed all
  three tests, including that assertion. An earlier unfiltered full workspace
  run also passed; repeated combined runs showed timing variance.
- `npm run build` reaches Turbopack and builds the native addon, then fails while
  downloading Geist/Geist Mono from Google Fonts in this environment. Standalone
  asset packaging is tested for both flat and nested Next output layouts; a
  complete production/browser run is not claimed.

## October 24 completion

- Added named repository-map, concept-hierarchy, duplicate-cluster, dependency
  and co-change providers. They use captured facts, preserve inference/evidence,
  apply selection scope, enforce limits, expose entity references and signed
  drill-through handles, and pass the existing ViewSpec gate.
- C4 accepts named context/container/component levels with distinct cache keys.
- Every supported provider returns a typed empty view for zero matches, under
  both availability modes, without requiring irrelevant artifacts.
- Captured JSON is pinned as raw fields and decoded on demand. Opening a reader
  and performing a file lookup no longer constructs the repository symbol graph.
  A fixture with 100,001 symbol entries verifies this under a ten-visit budget.
- The LLM facade accepts the specification's `where`, concept `membership`,
  flattened named outputs, evidence scope, optional context and continuation-only
  requests. Legacy `predicates`, `scope` and wrapped outputs remain accepted.
- Focused discovery supports `symbol.fields`, `concept.relationships`,
  `outputs.source`, `views.c4` and corresponding declared families. Schemas are
  generated from the same Rust DTOs. Host-authorized describe can open a context;
  repeating describe with a context does not advance it. Mismatched handles fail.
- Named artifact roots validate existing exported documents; malformed roots and
  unknown output variants are rejected. Schema-version metadata is explicit.
- Production uses packaged local Geist fonts. Builds no longer require a Google
  Fonts connection, and a standalone HTTP smoke test exercises the packaged
  native worker and captured-source retrieval after original source deletion.
