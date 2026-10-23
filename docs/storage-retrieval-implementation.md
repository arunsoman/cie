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
- Typed query AST, set operators, concept membership, bounded relation traversal.
- Records, evidence, graph, exact cubes/facets, contributor descriptors and cursors.
- Captured source, stored IR and existing impact/minimum-test algorithms.
- Existing UI SQL readers moved into storage-owned compatibility adapters.
- In-process Node binding, worker bridge, revision-aware existing UI requests,
  generic `/api/storage` transport and no-ingestion impact retrieval.
- Schema-generated model tool contracts, compact selection facade and byte budgets.
- Relationship ViewSpecs sealed and validated with the existing renderer gate.

The current UI can continue using its artifact shapes. Live filesystem source
browsing remains a separate application operation. Optional uncollected artifacts
remain unavailable. Retaining full databases per revision uses additional disk;
retention/garbage collection policy is not automatic in this foundation.

## Verification

Rust integration fixtures cover publication during reads, historical reopening,
source retrieval after deletion, cursor forgery, concept sets, cube drill-through,
IR, impact and missing-artifact behavior. The native test exercises the real worker,
addon, model facade and compatibility adapters. See commit validation for results.

## Further contract coverage

The specification remains the target contract. Advanced cube hierarchy/bucketing,
HAVING/order, complete capability catalog, model diagram handles, provider caching,
and all existing specialized renderer projection mappings require additional work;
unsupported operations must be explicit rather than silently approximated.
