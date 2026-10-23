# Storage retrieval API validation against the current UI

Date: 2026-10-23. Method: static source/contract review of the checked-out CIE UI
and storage API specification v1.1. This is not runtime proof of an implemented
retrieval engine. No UI behavior was changed by this review.

## Verdict

The proposed API is a suitable foundation but is **not yet a complete replacement
contract for the present UI**. Generic entity queries, traversal, cubes and
ViewSpecs cover the main interaction model. Several concrete payloads and execution
boundaries remain unspecified. Implementing the spec unchanged would leave gaps.

The current UI still loads large artifact bundles, directly queries SQLite from
Node, reads live source files and invokes the CLI for impact. A richer Rust API does
not automatically migrate any of these consumers. Actual implementation and an
in-process Node-to-Rust binding are separate required work.

## Evidence and coverage matrix

“Covered” means the proposed contract expresses the need, not that implementation
exists. “Gap” means additional explicit contract/schema is needed. “Outside” means
the responsibility should remain outside storage.

| Current requirement | Source evidence | Assessment | Required action |
|---|---|---|---|
| Repository map and source-linked nodes | `web/src/components/cie/map-view.tsx`, `web/src/lib/cie-types.ts` | Covered | Preserve IDs, locations, scope and hidden ledger through adapter |
| Concept tree and membership drill-down | `web/src/components/cie/concepts-mindmap.tsx`, `concept-views.tsx` | Covered | Implement concept hierarchy and membership relations |
| Symbol search by substring | `web/src/lib/polyglot-db.ts::searchPolyglotSymbols` | Gap | Declare substring operator, case rules and current shortest-match ordering |
| File, concept, caller and graph selections | `web/src/lib/atlas.ts`, `scope-digest.ts` | Covered | Translate selection into pinned typed query without scope widening |
| Named C4, state, workflow and impact views | `web/src/components/cie/polyglot-views.tsx`, `web/src/lib/polyglot-types.ts` | Covered | Register providers and explicitly report missing captured families |
| Existing ViewSpec variants and chart rendering | `web/src/lib/cie-types.ts`, `polyglot-types.ts`, `viz/chart-spec.ts` | Gap | Version adapters for CieViewSpec, PolyglotViewSpec/bundle and viz-adapter ViewSpec; validate gates/hashes |
| Findings and security views | `web/src/components/cie/sections.tsx`, `web/src/lib/cie-types.ts::Finding` | Covered | Register exact severity/kind/location/evidence fields |
| Plain-language summaries and effects | `web/src/lib/cie-types.ts::SummaryInfo`, `chart-filter.ts` | Gap | Define Summary entity/output with effects and scope relations |
| Contracts/promises and authority categories | `web/src/lib/cie-types.ts::Contract`, `chart-filter.ts` | Gap | Define Contract entity/output and written/inferred authority semantics |
| Trust, warnings and analysis coverage | `web/src/lib/cie-types.ts::CieReport`, `sections.tsx` | Gap | Specify existing trust/evidence diagnostics; generic coverage alone is insufficient |
| Framework/mastery routes, injections and handler branches | `web/src/lib/stack-types.ts`, `mastery-view.tsx` | Gap | Typed Stack artifact mapping must preserve every existing TechStack field |
| Dependency declarations and usage | `web/src/lib/cie-types.ts::CieDeps`, `runtime-views.tsx` | Covered | Preserve declared-versus-used semantics and unresolved usage |
| Runtime CPU, spans, trials and coverage | `web/src/lib/cie-types.ts::CieRuntime`, `runtime-views.tsx` | Gap | Register measured-runtime artifact and units/tool/run provenance; unavailable when not collected |
| Quality, stories, drift, overlap, co-change and fault payloads | `web/src/lib/cie-store.ts::loadDemo`, `cie-types.ts`, `ingestion_job.rs` | Gap | Enumerate typed artifact catalog and per-artifact availability; do not substitute generic findings |
| Linked chart filtering | `web/src/lib/chart-filter.ts`, `web/test/xfilter.test.cjs` | Gap | Preserve per-population applicability, certainty and authority; do not silently turn an inapplicable filter into a relationship join |
| Slice/dice, facets, pivots and aggregate drill-through | Spec §16; existing charts/filter modules | Covered | Implement cubes and contribution descriptors; new multi-filter UI is separate work |
| Duplicate pair table and its filters | `web/src/lib/polyglot-db.ts::getDuplicates`, `duplicate-analysis-view.tsx` | Gap | Pair record needs endpoints, similarity, tier, detection, epistemic status, counterargument and canonical symbol |
| Exact duplicate families and member locations | `web/src/lib/polyglot-db.ts::getDuplicateClusters`, `duplicate-clusters-view.tsx` | Covered | Preserve persisted membership, algorithm identity, counts and bounded members |
| Function CFG/IR inspection | `web/src/lib/polyglot-db.ts::getFunctionIr`, `function-ir-view.tsx` | Gap | Typed FunctionIr output for blocks/instructions/edges/calls, spans and unresolved targets |
| Source panel, popup and byte snippets | `web/src/components/cie/source-panel.tsx`, `code-popup.tsx`, `api/polyglot/snippet/route.ts` | Gap | SourceText/SourceSlice output from captured blobs with hash, encoding, byte range and line mapping |
| Admin table inventory and raw rows | `web/src/lib/polyglot-db.ts::getPolyglotTableInventory`, `getPolyglotTableRows` | Gap | Explicit diagnostic dataset API or accept intentional product change; domain queries cannot promise raw-table parity |
| Impact results and minimum tests | `web/src/app/api/impact/route.ts`, `web/src/lib/cie-types.ts::CieImpact` | Gap | Typed impact result must preserve affected symbols/concepts and minimum_test_set; reachability alone does not compute tests |
| Job progress, consent, browsing and branch selection | `web/src/app/api/analyze/*`, `api/fs/browse`, `api/repo/*` | Outside | Keep orchestration and repository discovery APIs |
| LLM conversation, provider settings and workspace state | `web/src/app/api/chat/route.ts`, `llm-settings.tsx`, `workspace-memory.ts` | Outside | Storage supplies evidence; conversation, credentials, camera and selection remain application concerns |

## Specific mismatches that matter

### Source retrieval is not evidence-location retrieval

Evidence spans identify where a claim came from; they do not guarantee retrievable
source bytes. Today the UI reads local files directly and snippets use byte ranges.
The spec prohibits reading live files during retrieval, so it needs a captured-source
output and an ingestion retention policy. If source was not retained, report
`SourceUnavailable`. Live-source viewing can remain an explicit separate action,
labelled as live content and checked against the captured hash. It must not be
presented as the source of an older revision.

### The current linked-filter semantics are population-specific

`chart-filter.ts` explicitly defines findings, summaries, contracts, map elements
and canvas as separate populations. Severity/kind affect findings; effects affect
summaries; authority affects contracts; certainty affects map/canvas; file affects
several populations. Unsupported dimensions leave the unrelated view unchanged.
The current UI allows one active chart filter, toggles it off on a repeated click,
and replaces it on another bucket. The storage API may support richer multi-filter
queries while a compatibility adapter preserves this UI behavior.

Each output therefore needs an applied-filter report: applied, inapplicable or
rejected, with a reason. Introducing a join from a symbol to a severe finding is a
valid new query, but must be explicit; it is not equivalent to the current filter
behavior. Facts-only display filters must not upgrade certainty or erase source
coverage limitations.

### A generic graph does not replace every diagram contract

The UI consumes legacy CieViewSpec, PolyglotViewSpec bundles and the shared
viz-adapter ViewSpec. The latter is gated and carries semantic/render hashes.
Existing slice code rebuilds hashes, reports omitted elements and re-runs the gate.
A storage projection adapter must preserve equivalent validation, display modes,
footer counts, hidden items, gaps, layout hints and entity/evidence links. Each
supported chat-chart kind also needs an advertised adapter; cube chart support alone
does not cover sequence, flow, UML and tree diagrams.

### Impact has analysis semantics beyond graph traversal

The current impact endpoint launches `cie impact` and expects affected symbols,
affected concepts and `minimum_test_set`. Returning reachable callers under a new
label would be a regression. The provider must load a stored impact artifact or
run the existing impact algorithm using captured test/dependency evidence, without
invoking ingestion. If that evidence was not retained, return unavailable.

### Raw administration is an explicit exception

Current admin screens expose a whitelist of physical tables, counts, columns and
pages, with binary values redacted and long text shortened. The proposed domain API
intentionally hides physical schemas. To preserve this screen, add a separate
read-only diagnostics namespace owned by storage: dataset inventory, schema and
bounded rows. It uses allowlisted dataset IDs, no SQL, redacted values and explicit
truncation. Dataset versions must be declared; clients cannot rely on SQL table
names as stable domain contracts. Existing offset/page navigation needs a cursor
adapter or a declared bounded offset compatibility method.

### The in-process binding is missing

The current Next.js server is Node, while `cie-storage` is Rust. Replacing Node
SQLite reads and CLI retrieval calls requires an in-process binding, for example a
Node native addon, with typed request/response serialization and cancellation.
Choose and test the binding before claiming end-to-end coverage. It must call the
same library; an HTTP sidecar or new retrieval process would violate the intended
module boundary. Keep the existing ingestion CLI job architecture separate.

## Required specification amendments before implementation sign-off

1. Publish an explicit entity/field/relation catalog including Summary, Contract,
   DuplicatePair, IR records and their revision-scoped references.
2. Publish typed SourceSlice, FunctionIr and Impact output contracts, including
   unavailable cases, provenance and per-output limits.
3. Enumerate the compatibility artifact catalog: report, concepts, stack,
   dependencies, runtime, stories, quality, overlap, drift, co-change, fault and
   architecture bundles. A catalog entry does not imply the data was collected.
4. Name every supported renderer schema/version and provider, including semantic
   gates, certainty categories, hashes and omission disclosure.
5. Define population-specific filter applicability and an applied-filter envelope.
6. Define read-only diagnostic dataset access for admin parity.
7. Define Rust/Node binding, ID translation and revision lifecycle for UI requests.
   Current symbol IDs can belong to different core/polyglot spaces; they cannot
   be equated merely because their numeric values match.
8. Add a bootstrap compatibility adapter: current report locations index a files
   array and components expect full artifacts. Initially preserve those shapes;
   migrate to paged entities deliberately rather than silently truncating bundles.

## Validation required for implementation acceptance

- Contract fixtures for every payload in the matrix, including optional/missing
  artifacts and demo-only data. Test absence honestly rather than inventing data.
- Existing UI suites remain green, particularly linked filters, scoped evidence,
  renderer gates and duplicate browsing. Passing these alone does not validate a
  new backend until they exercise its adapter.
- Compare old/new symbol search ranking, duplicate filters/facets/counts, IR block
  ordering, source byte spans and impact minimum-test results on the same fixture.
- Render equivalent old/new map, concept, architecture and chat ViewSpecs; preserve
  click targets, hidden counts, certainty and source navigation.
- Exercise all supported screens with a pinned revision while ingesting a new one;
  verify source bytes, symbol identities and drill-through never cross revisions.
- Instrument the migrated retrieval path: zero direct Node SQLite opens, zero live
  source reads for captured-source outputs, and zero ingestion/retrieval CLI spawns.
- Measure bounded cold/warm retrieval and high-cardinality cubes before removing
  existing paths. No performance claim follows from this static review.

## Decision

Proceed with the architecture, but amend the contract before implementation
sign-off. Do not claim that “any typed output” establishes UI compatibility: each
existing payload, projection and interaction above needs an explicit adapter or a
documented intentional UI change. This review identifies gaps; it does not yet
implement those amendments or migrate the UI.
