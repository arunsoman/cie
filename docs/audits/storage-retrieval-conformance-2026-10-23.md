# Storage retrieval spec conformance audit

Audited source: `ed111ed19785ccb0abf014f3bebd80534bfc537c` on `main`.
Includes retrieval implementation `f7cc4f7`, bundler fix `94f6972` and gap implementation `21f8336`.
Specification: `docs/storage-retrieval-api-spec.md`, v1.3.
Method: fresh clone, `git pull --ff-only origin main`, source/contract review, existing tests and independent executable conformance probes.

## Verdict

**Partially implemented; not ready for full specification sign-off.** The newer
commit implements substantial query and cube behavior, but passing its existing
tests does not establish complete contract coverage. Seven independent probes
reproduce incorrect behavior. Additional declared requirements remain absent or
restricted. This audit changes no production code and does not count lost,
unpushed work from an earlier execution environment as delivered.

## Reproduced failures

Each probe asserts the required behavior and intentionally fails on this baseline.
The source is retained outside Cargo's default test discovery in
[`storage-spec-audit-probes.rs`](storage-spec-audit-probes.rs).

| ID | Priority | Requirement | Expected | Observed | Source |
|---|---|---|---|---|---|
| A1 | P1 | Output dependency integrity, §10/§16.3 | A chart dependent on a failed cube must fail or be unavailable | With best-effort enabled, the cube fails, its null data enters `prior`, and the dependent chart is `ready` with null cells | `retrieval/mod.rs` output loop; `retrieval/outputs.rs` View branch |
| A2 | P1 | Typed compatible output sources, §8/§16.3 | Reject entity records as a cube-chart source | An Entities output is accepted as the source of a table chart with renderer `cie-cube` | `retrieval/outputs.rs` View branch |
| A3 | P1 | Bounded efficient lookup, §11 | A one-file lookup should not load unrelated duplicate pairs | A database with 100,001 duplicate pairs causes an exact file lookup to fail `BudgetExceeded: Stored row limit exceeded` before evaluating the file predicate | `retrieval/query.rs::dataset`, `retrieval/mod.rs::rows` |
| A4 | P2 | Acyclic output references, §16.3 | Schedule the referenced cube before its chart, regardless of request order | Chart-before-cube returns `InvalidRequest: View source must name an earlier successful output` | `retrieval/mod.rs::retrieve`, `retrieval/outputs.rs` |
| A5 | P2 | Compatibility Fault artifact, §18.4 | Return existing persisted fault-analysis data | Data under `report.fault_analysis` cannot be retrieved as Artifact::Fault; returns `ArtifactUnavailable` because lookup expects a top-level `fault` key | `cie-cli/src/ingestion_job.rs`, `retrieval/outputs.rs::artifact` |
| A6 | P2 | Unicode case-folded comparison, §18.2 | `Straße` matches `STRASSE` under case-folded search | Zero matches; implementation performs lowercase conversion, which is not full Unicode case folding | `retrieval/query.rs::fold_text` |
| A7 | P2 | Multi-membership aggregation semantics, §16.2 | Validate that a whole/parts chart uses a disjoint additive population, or explicitly change and disclose its grain | A symbol in two concepts produces two count-1 cells with grand total 1; provider accepts those cells as a donut without partition/additivity validation | `retrieval/outputs.rs` cube and View branches |

P1 means a correctness or practical usability blocker for sign-off. P2 means a
required behavior is missing or incorrect but has a bounded workaround. These are
engineering priorities, not security severity ratings.

The A7 fixture explicitly places the same recorded symbol in two concepts. It does
not assume that every real concept hierarchy overlaps. The A5 fixture injects an
identifiable persisted fault payload to isolate routing from the fault algorithm;
the production exporter uses exactly the tested `report.fault_analysis` location.
A3 creates duplicate-pair rows in the archived database before opening its reader;
no production source files or unrelated user repositories are modified.

## Implemented behavior supported by existing tests

| Area | Assessment | Evidence |
|---|---|---|
| In-process storage module and real native worker | Implemented | Rust library/native addon plus `web/test/storage-native.test.cjs` |
| Pinned revision, historical reopening and source after deletion | Implemented for retained local storage | `pinned_revisions_source_paging_and_forged_cursor` |
| File/name/byte-position symbol resolution and ambiguity | Implemented | `symbol_resolution_name_position_ambiguity_and_non_ascii_bytes` |
| Relation Any/None/All and min/max traversal with endpoint validation | Implemented on covered fixtures | `quantifiers_min_depth_and_relation_endpoint_validation` |
| Basic concept sets, exact grouping and drill-through | Implemented on covered fixtures | `concept_sets_cubes_drill_through_and_views` |
| Numeric buckets, HAVING, ordering, pivot keys and totals | Implemented on covered fixtures | `cube_buckets_having_order_pivot_totals_and_facets` |
| Typed unknown members and supported facet self-exclusion | Implemented, with restricted filter shapes | Same cube fixture and `strip_dimension_filters` |
| Source ranges, context, UTF-8 boundaries and chunking | Implemented on covered fixtures | `revisions_list_source_ranges_context_and_stored_only_views` |
| Stored IR and computed impact/minimum tests | Implemented | Core retrieval fixture and existing analysis functions |
| Relationship ViewSpec gate and hashes | Implemented | `relationship_view` uses existing `cie-viz` gate/sealing |
| Existing UI SQL compatibility adapters | Implemented | Server functions delegate through native worker; web regression suite |

## Remaining contract gaps from source review

These are separate from the seven executable failures above.

1. **Entity/relationship coverage is incomplete (§5/§18).** EntityKind contains
   only File, Symbol, Concept, Summary, Contract, Finding, DuplicatePair and
   Cluster. Route, Framework, Component, Dependency, State/Workflow and standalone
   Evidence entities cannot be selected. Full stack/dependency artifacts are useful
   compatibility outputs but do not replace composable domain queries over them.
2. **Concept/component analytical roll-up is incomplete (§16).** Directory levels
   exist; general concept-DAG ancestor roll-up and component/dependency dimension
   mappings are not complete. Calendar months and IANA timezones are explicitly
   unsupported. These restrictions are documented honestly, but do not satisfy
   the full target contract.
3. **Cube-chart rendering is not integrated (§9/§18.5).** Storage emits renderer
   `cie-cube`; no corresponding consumer/registration exists in `web/src` or the
   visualization adapter. The returned object is not an existing ChatChartSpec or
   a gated viz-adapter ViewSpec. Existing client-built charts passing tests do not
   validate rendering of these new storage outputs.
4. **Projection registry, caching and scoped view families are incomplete (§9/§13).**
   There is no provider registry/versioned projection cache. Relationship graphs
   are derived; architecture views load stored bundles and require a whole-repo
   file selection. The API does not yet supply the promised range of scoped
   repository, concept, flow, sequence and other existing diagram projections.
5. **Graph output lacks its declared expansion contract (§8).** It returns an
   induced graph over matches with node/edge caps. The Graph output itself has no
   bounded expansion/context descriptor; callers must construct the desired set
   through separate query expressions.
6. **Cancellation is not per-request (§11/§18.7).** Storage checks deadlines and
   SQL progress, but has no cancellation token; the bridge kills its shared worker
   after a host timeout, invalidating unrelated contexts. Revision opening waits
   on a blocking publication lock outside the retrieval deadline. Compatibility
   queries do not receive the normal retrieval execution budget.
7. **Response semantics are only partial (§10/§18.5).** `origin` is the literal
   `stored_or_derived`; missing artifacts in best-effort mode use `failed`, not a
   distinct unavailable status. Output data is a raw JSON Value, and the response
   lacks a result-kind discriminator and per-output applied/inapplicable filter
   report. The client still preserves its existing linked-filter behavior locally.
8. **LLM facade is a foundation, not the complete §19 adapter.** Select/query
   normalization, generated input schemas and byte limits exist. Topic-based compact
   discovery, compact/full profiles, a deduplicated evidence dictionary, diagram
   result handles and validated next-action fragments are absent. Result handles
   and tokenizer-specific token budgets are optional extensions in the spec and
   should not alone block the core API; their absence must not be advertised as
   implemented. No model-answer accuracy guarantee follows from tool availability.
9. **Temporary checkout retention is incomplete.** `analyze-jobs.ts` deletes the
   entire temporary work directory, including its `.cie` archive, for URL jobs and
   branch snapshots. Only the compatible result JSON is persisted. New storage
   retrieval therefore cannot continue querying those archives after cleanup.
10. **Deployment and transport need validation.** The native addon has a separate
    build command, while the normal web build does not invoke it. Runtime worker
    loading is intentionally opaque to the bundler; explicit standalone packaging
    of worker/addon assets is not configured. The generic web route does not yet
    forward newer native resolve/revisions/capabilities operations. This is a
    configuration/source finding, not a reproduced production deployment failure.
11. **Some static validation depends on returned data.** Set kind compatibility
    is checked from populated result IDs, so an empty incompatible operand can
    evade the intended AST type validation. Field checks search all entities,
    rather than the selected entity domain. These observations need dedicated
    regression cases before claiming complete typed-query validation.

## Verification results

| Check | Result |
|---|---|
| `cargo test --workspace` | 572 passed, 0 failed, 11 ignored |
| Existing focused retrieval and ingestion suites | 7 retrieval tests + 1 coordinator test passed |
| Native addon build | Passed |
| `node --test test/*.test.cjs` in `web` | 91 passed, 0 failed, including the real native-worker test |
| TypeScript `tsc --noEmit` | Passed after installing both web and viz-adapter dependencies |
| Independent specification probes | 0 passed, 7 failed; each failure is reproduced above |

A clean environment required installing Rust and dependencies, and building/copying
the native addon before its tests. Initial missing-dependency/addon setup failures
were resolved before recording these results. No browser-render or standalone
production deployment was exercised; the renderer/deployment findings are static
source checks. There are no changed production files in this audit.

## Reproduction

Build the CLI first, then temporarily register the audit probes:

```sh
cargo build -p cie-cli
cp docs/audits/storage-spec-audit-probes.rs cie-cli/tests/storage_spec_audit_probe.rs
cargo test -p cie-cli --test storage_spec_audit_probe -- --nocapture
rm cie-cli/tests/storage_spec_audit_probe.rs
```

The audit probes are deliberately not in the default test directory: these are
known conformance failures, not changes to production behavior or a claim that the
normal test suite is broken. Keep their expected assertions when implementing fixes;
a repaired implementation should make them pass.

## Recommended repair order

1. Validate and schedule output dependencies; propagate failed-source status and
   reject incompatible source types. Guard whole/parts charts against overlapping
   populations.
2. Make loading/planning query-driven so simple retrieval does not enumerate every
   duplicate record. Add representative large-repository budget benchmarks.
3. Fix fault payload routing and real Unicode case folding; close static type
   validation gaps.
4. Complete missing domain entities, hierarchy mappings and renderer/provider
   integration, then test storage-generated diagrams in the actual UI.
5. Add request cancellation and bounded revision-open waits; retain storage outside
   temporary checkouts; verify standalone packaging and remaining model-adapter
   features.

No production fixes are included in this audit. Existing tests and the conformance
probes answer different questions; both results must be retained for sign-off.
