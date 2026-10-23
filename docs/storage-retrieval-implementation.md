# Storage retrieval implementation

Implementation of the proposed [retrieval contract](storage-retrieval-api-spec.md).

## Running

Build `cie-cli`, then run `npm run build:storage` in `web` (Rust/cargo must be on
PATH). The native addon runs inside a Node worker, not a separate process.
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
  (lower-inclusive/upper-exclusive), UTC/fixed-offset time buckets, named
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

The specification remains the target contract. Full component and dependency
dimension mappings, calendar time buckets (months), IANA timezone databases,
provider projection caching, dedicated stored-impact artifacts and all existing
specialized renderer projection mappings require additional work; unsupported
operations still fail explicitly rather than being silently approximated.
