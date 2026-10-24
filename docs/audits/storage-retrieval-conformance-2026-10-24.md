# Storage retrieval completion verification — October 24, 2026

Target: `docs/storage-retrieval-api-spec.md` v1.3. This verification closes the five
source-review findings raised after pulling `267ecaa`. It does not redefine the
specification to exclude these requirements.

| Requirement | Implementation | Acceptance evidence |
|---|---|---|
| §10 typed/versioned output algebra | `payload.rs` closed `OutputData` union; typed pages, graph, evidence, facets, cube, source, IR, impact and renderer adapters; `artifact_payload.rs` named artifact roots; generated response schema | Malformed/unknown payload rejection; nine existing exported artifact formats deserialize through the actual DTOs; native response discriminator/version assertions |
| §9 empty view semantics | Supported provider validation followed by typed `cie-empty` response before looking for stored inputs | All ten match-based providers tested with both `stored_only` and `build_if_missing` |
| §9 named provider coverage | Repository map, concept hierarchy, duplicate clusters, dependencies, co-change; existing C4/state-machine/workflow/impact/relationship and cube providers retained; named C4 levels | Positive captured-fact projection for each new provider; real gate validation; contributor drill-through and stored-only cache reuse; C4 level/identity/cache test |
| §11 avoid whole-repository decoding for file lookup | Immutable raw captured fields decoded on demand; parameterized indexed exact file lookup; storage-owned file-path index; logical explain endpoint | 100,001-symbol fixture completes a file lookup under ten visits; explain confirms symbols/summaries were never decoded; missing file returns empty |
| §19 executable documented facade | Spec-shaped selection, concept membership, flattened outputs, evidence scope and optional context; mutually exclusive continuation mode; focused schema discovery; host-bound contexts | Verbatim-shaped request executes; focused topics return requested schemas/catalogs; next action executes; native worker checks handle reuse and rejects mismatches |

## Wire compatibility

The API remains version 1; each output advertises payload schema version 2.
`data.type` is the payload discriminator. The outer `kind` remains the requested
output family (`detail` contains a `source`, `function_ir`, or `impact` payload).
Failed/unavailable outputs have null data and a structured error. Artifact data
uses the named `artifact_kind` / `document` union, not an arbitrary root value.
Declared adapter and analysis metadata bags remain extensible inside those roots.

The LLM request accepts the specification's form:

```json
{
  "context": "ctx_host_selected",
  "select": {
    "kind": "symbol",
    "where": [{"field": "file.path", "op": "eq", "value": "payments/service.rs"}],
    "concept": {"id": "context_bound_concept_id", "membership": "include_descendants"}
  },
  "outputs": [
    {"id": "rows", "entities": {"fields": ["id", "qualified_name", "location"], "page": {"limit": 20}}},
    {"id": "proof", "evidence": {"scope": "matches_and_derivations", "page": {"limit": 20}}}
  ],
  "response": {"profile": "compact", "max_bytes": 12000}
}
```

Actual paths and concept/context identities come from the authorized pinned
revision. Legacy wrapped named outputs, `predicates`, and concept `scope` are
accepted for existing clients. Continuation tokens are signed, expiring and
reader-bound; they carry the original selection/options and cannot introduce SQL
or code. Context mismatch is rejected before retrieval.

## Verification performed

- Focused storage integration: 19 acceptance cases plus 7 retrieval-contract
  cases and the ingestion/export coordinator case passed.
- Artifact compatibility: 2 tests passed, including existing report, repository
  map, concept, dependency, runtime, story, quality, overlap and drift exports.
- Final functional Rust workspace suite: 593 passed, 11 ignored. The existing
  relative-timing drift assertion was run separately from concurrent build work;
  its isolated suite passed all three tests.
- Web suite: 107 passed. TypeScript: `tsc --noEmit` passed.
- `npm run build` passed, including native compilation and standalone packaging.
  Geist is supplied locally through the packaged font dependency.
- `node web/test/storage-standalone-smoke.cjs` passed against the production
  standalone server: HTTP discovery, context-bound typed retrieval, and captured
  source after deleting the original file. This is an HTTP/runtime smoke test,
  not a claim of complete visual/browser interaction coverage.

Optional tokenizer-specific token budgets and opaque large-result handles remain
optional and unadvertised. Unsupported budgets fail explicitly; no byte/token
conversion is fabricated. Missing captured inputs remain unavailable instead of
being manufactured by parsing source, ingestion, network calls or an LLM.
