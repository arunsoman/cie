# Atlas core — implementation contract and delivery ledger

Date: 2026-10-25. Governing design: [Atlas semantic architecture](atlas-semantic-architecture.md).

## The rule every implementation must satisfy

**Any chart can occupy any Atlas slot. Clicking any data element updates one
shared selection, and every analytical chart renders the related data. Clicking
that element again removes only its contribution.** Chart position, renderer and
persona never change this behaviour. Double-click opens captured evidence without
changing the final selection.

The [SmsController.java → submitSms() worked example](atlas-semantic-architecture.md#worked-example-smscontrollerjava--submitsms)
is the acceptance scenario. It covers file selection, function refinement,
deselection, clearing, chart movement, adding a chart while selected and evidence
opening. The scenario is executable in `web/test/atlas-universal-selection.test.cjs`.

A chart must be both a sender and receiver. `binding: selection` is required for
analytical charts. A global search/navigation utility may use repository scope,
but cannot be registered as an analytical chart that ignores filters. In particular,
the file chart is filtered too; it is not an exception to the rule.

## Architecture

| Boundary | Contract | Implementation |
| --- | --- | --- |
| Selected data | PopulationSpec | selection algebra, registered relation mappings, pinned/versioned storage query |
| Meaning | SemanticViewSpec | typed C4 architecture, dependency, hierarchy, control-flow, sequence, data-flow, metric, table |
| Presentation | PresentationSpec | registered profiles, layout intent, capability requirements and detail |
| Engine | RenderHandle | existing visualization negotiator/G6 host, shared gestures, camera and export |

The core does not accept SVG as data and does not calculate SVG paths. G6 receives
semantic nodes/relations or explicit message occurrences through the existing
sealed adapter boundary. Source-label parsing is legacy compatibility only; new
sequence views supply participant IDs, labels and message order as typed fields.
A call graph cannot become a flowchart by changing a layout or a string tag.

## Modules

- `atlas-core.ts`: immutable shared state, batched queries, revisions and lifecycle.
- `atlas-selection.ts` / `atlas-populations.ts`: contribution algebra and declared
  entity-universe mappings; unsupported mappings fail visibly.
- `atlas-semantic.ts` / `atlas-semantic-validation.ts`: typed models, occurrences,
  provenance, hierarchy/endpoint/order validation.
- `atlas-semantic-producers.ts`: captured hierarchy, function IR and static call
  occurrence projections. No runtime execution or missing analysis is fabricated.
- `atlas-presentation.ts` / `atlas-visualization.ts`: profile planning and semantic
  compilation into the existing sealed visualization adapter contract.
- `atlas-registry.ts` / `atlas-catalog.ts` / `atlas-plans.ts`: reusable producers,
  renderer capabilities and persona plans.
- `atlas-layout.ts`: named-area layout validation at desktop/tablet/mobile sizes.
- `atlas-navigation.ts` / `atlas-dimensions.ts`: search and typed cube controls.
- `atlas-workspace.ts` / `atlas-revisions.ts`: named snapshots, permalinks and
  bounded captured-revision comparisons.
- `atlas-policy.ts` / `atlas-storage.ts`: authority policy and typed transport.
- Persona TSX: composition only. Shared components own layout slots and controls.

## Selection and identity

OR contributions within one group, AND across groups; explicit intersection and
exclusion are additional operations. Removing one contribution preserves any
members still contributed elsewhere. There is no chip cap that silently evicts
constraints. Spatial scope, source opening, focus and camera operations have
separate commands. Layout movement does not issue a data query when the chart is
already loaded. A newly added chart uses the existing selection on its first query.

Visual occurrence IDs are distinct from subject identity and contributor identity.
Repeated messages remain separate while selecting the same underlying subject.
Graph/call context is labelled separately and never added to selected counts.

## Evidence and rendering

Semantic evidence supports captured source/document spans, stored facts, exact
aggregate contributor descriptors and runtime samples with their collection pin.
The client render gate accepts recorded evidence without inventing a file/span.
Existing Rust source-span export DTOs remain unchanged; the richer semantic model
is a TypeScript projection over stored facts, not a new Rust ingestion format.

Source opening checks repository, revision, file, requested range and optional
content hash. Multiple locations open a chooser. Explicit older evidence uses a
reader for that captured revision, never today's file of the same name.

Facts-only is applied to both outlines and semantic models before rendering;
Unknown is never upgraded to Fact. Population counts remain distinct from visible
claim counts. Missing authority/evidence, unavailable IR, uncollected profiling,
truncation and incomplete static resolution are disclosed.

G6 extensions are registered in the host layer (`atlas-host-profiles.ts`) and may
provide native layout/shape/plugin options. They cannot override data, container,
interaction ownership or required capabilities. Statistical/table hosts remain
appropriate choices. Capability failures use an explicit fallback/outline; they
do not silently change diagram meaning.

## Session and lifecycle

A session retains selection through persona and layout switches. Revision changes
abort old work, close readers and explicitly clear rather than silently rebase
revision-local selections. Stale view/source/search responses are rejected.

Saved views include layout placements, expansion, dock, chart controls, policy,
selection and change context. Same-revision restore is validated. Default aggregate
cells carry durable replay predicates; sharing a cell with no durable replay query
fails explicitly rather than saving an expiring token as if permanent. Permalinks
use the URL fragment and contain references, not captured source contents.

## Delivery status

| Capability | Status |
| --- | --- |
| Shared selection, toggle, intersection/exclusion, cancellation, cursor reset | Implemented |
| Arbitrary registered-chart placement, duplication and selected-state loading | Implemented |
| Typed hierarchy, captured control flow, static occurrence sequence and metric projections | Implemented |
| G6 compilation, profiles, plugin extension point, minimap, shared physical gestures | Implemented |
| Source identity guards and recorded/aggregate evidence | Implemented |
| Named layouts, search, workspace serialization, revision/change context services | Implemented reference infrastructure; not a completed persona product |
| Reviewer screen | Shared-core reference composition, not the complete Reviewer impact/drift/test-confidence PRD |
| Other persona screens | Fresher and Architect compose shared-core orientation slices; Auditor and Consultant remain legacy, Performance has no selectable UI |
| Existing chart inventory | Only registered/migrated producers are covered; each remaining chart needs contributor/projection conformance |
| Ask-CIE and MCP selection integration | Not migrated; Reviewer discloses its separate chat context |
| Runtime trace reconstruction, exhaustive exception/async analysis | Producer limitations; not claimed |
| Full REG-01…30 data collection, digest generation, measured 100k-file latency budgets, full i18n/accessibility audit | Not completed by this core delivery |

## Verification

From `web`:

- `node --test --test-concurrency=1 test/*.test.cjs`
- `npx tsc --noEmit`
- `npx next build && node scripts/package-storage-native.cjs`
- `ATLAS_BROWSER=/path/to/chromium node test/atlas-core-browser.cjs`

The universal-selection fixture verifies the documented six-chart choreography
against an independent set oracle. Semantic fixtures verify hierarchy, branch and
message preservation, repeated/self calls, recorded evidence without code spans,
and semantic-hash stability under presentation changes. Native tests ingest source
and verify captured IR, summaries, contributor queries and source reads through
the actual storage module. Browser tests validate the built application.

## Persona migration checkpoint

See [chart migration matrix](atlas-migration-matrix.md) for the complete legacy panel inventory and explicit persona gaps. Fresher now composes the registered catalog with a guided layout; it does not claim C4, concept, entry-point or data-flow parity. Reviewer and Fresher share selection, evidence and placement logic.

## Generated concept producer checkpoint

`concepts` is an additional registered selection-bound view. It preserves primary
and extra parent relationships as a DAG, marks expanded parents as context, and
keeps generated labels/membership as Inference. Selecting a node includes recorded
descendants up to eight levels, then maps contributors through storage relations.
Double-click uses a recorded representative member source where available; it is
not evidence that the concept name is semantically correct. Graph/evidence limits,
low or missing confidence and incomplete coverage are disclosed. Facts-only hides
these interpretations in both the semantic model and render envelope.

Storage intersection narrowing is required for polymorphic concept membership;
see the retrieval API specification. No new process or persistence responsibility
has been added to ingestion.

## Entry-point and API checkpoint

`entries` and `routes` are registered selection-bound tables. Storage exposes
`entry_point` entities from captured `stack.mains` and extends `source_symbol` to
uniquely resolved application/route targets. Resolution uses captured file and
recorded name/owner, never a whole-file fallback. Duplicate/overloaded targets
remain ambiguous. Source relations are absent when resolution is unsupported.
The current symbol-bound tables therefore include only uniquely resolved entries;
this partial coverage is disclosed in each table. The full captured records,
including unresolved/ambiguous statuses, remain retrievable from storage.

API cells select their handler symbol. File cells carry separate contributor
references and select the file; double-click opens the evidence represented by
that cell. `TableView.rows[].cellBindings` preserves those identities in semantic
exports. Cell bindings use the row's authority, declared column IDs and pinned
revision, and are validated before rendering. Facts-only retains Fact row/cell
bindings together and removes inferred rows together.

The declared call graph expands to eight levels with a 100-node / 300-edge cap;
the sequence retains its existing occurrence and depth limits. Function IR stays
local to the selected handler; the call graph/sequence show interprocedural calls.
Neither is a complete runtime execution trace. Generated or overridable dispatch
is not promoted to Fact. Java discovery of uniquely named private/static/final
helpers is now independent of declaration order and isolated to each class;
overloads and virtual calls remain unresolved rather than guessed.

## Typed C4 and Architect orientation checkpoint

`ArchitectureView` adds an explicit C4 level (`container` or `component`) and typed
system/container/component roles. The existing renderer compiler preserves these
roles as domain/container/component render elements and records C4 metadata; a
relabelled dependency graph is not accepted as this model. Facts-only filters both
the semantic architecture and its envelope. Architectural boundary classifications
remain Inference, even when their underlying captured build/annotation record is
real. No runtime deployment guarantee is made.

Storage projects captured `stack.apps` as `container` entities and records
`member_of_container` ownership by the unique longest application directory,
respecting segment boundaries and excluding ambiguous duplicate roots. Captured
framework classes become component identities with their actual declaration
children as contributors; existing stored generated components remain available.
`component_in_container` records context, and `communicates_with` exposes uniquely
matched captured cross-service records. Projection reads the captured revision,
not the working tree; ingestion remains unchanged.

The component view currently displays stored parent/container relationships,
not a complete component dependency map. External Docker/image systems are not
invented as symbol-owning containers. Representative source belongs to a recorded
member, not necessarily a captured deployment manifest. Empty and bounded results
are disclosed. `ArchitectAtlas` composes these registered charts with the same
selection, layout, policy and source services as Reviewer/Fresher. Its orientation
layout is partial PRD delivery, not completion of dependency/drift/risk analysis.

Chart placements are now saved per layout in `workspace.layoutPlacements` while
`workspace.placements` remains the active layout's override map. Persona switching
archives/restores that layout's choices without changing shared selection; it
closes an expanded panel when the layout changes. Saved layout maps validate every
slot and view before use. This prevents a Fresher chart replacement from silently
replacing Architect's same-named slot. Existing schema-2 sessions without placement
history still restore their active layout and acquire history when switched.

Canvas rendering is now isolated per asynchronous render host. The displayed
render state is keyed by the current render hash, stale interactions are ignored,
and each cleanup disposes only its own graph/DOM host. This prevents an old chart
from briefly appearing beneath a replacement chart's title. The shared source and
selection services remain independent of this rendering lifecycle.
