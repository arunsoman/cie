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

## Further contract coverage

The specification remains the target contract. Optional tokenizer-specific token
budgets and opaque result handles are not enabled; byte budgets and inline
payloads remain the supported transport. Unknown token budgets return
`UnsupportedBudget`. Dedicated persisted impact artifacts are unavailable;
`build_if_missing` computes impact from captured facts. Specialized diagrams
are restricted to evidenced architecture providers (C4, state machines,
workflows, duplicate impact), relationship views and cube charts; arbitrary
sequence/map projections are not inferred from unsupported input.

The reader still opens the core snapshot as JSON and evaluates most predicates
in memory. Optional duplicate/domain tables are loaded from typed query
requirements, but this is not a general index-pushdown query planner. Projection
caches are reader-local (32 entries, 8 MiB total, 1 MiB per entry), not persistent
across reader expiry or process restart. Retained URL/snapshot jobs use the
existing seven-day job cleanup policy; local history pruning is not automatic.

## Audit repair stage (2026-10-23)

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

## Extended implementation stage (2026-10-23)

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

### Validation notes

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
