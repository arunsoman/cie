# Atlas semantic architecture — governing design

Date: 2026-10-25. Status: implementation contract. This supersedes the generic
items/links-only renderer boundary in the first Atlas core and conflicting gesture
rules in the charter, UX specification and persona PRDs. It does not declare the
persona-specific panels or analysis engines complete.

## The primary product rule: any chart, anywhere, one shared selection

**Any chart can be placed in any Atlas layout slot. Clicking a data element in
any chart changes one shared selection. Every other analytical chart re-renders
the data related to that selection.** Chart type, position and persona never
change this behaviour. No chart is allowed a private filter or an opt-out.

Every chart is both a selection sender and a selection receiver. Adding a chart
connects it to the current selection automatically. Moving a chart changes only
layout; it never changes the query or clears selection. If no related data exists,
the chart shows an explicit empty state. A global find/navigation utility may
search the repository, but it is not an analytical chart exempted from filtering.

### Worked example: SmsController.java → submitSms()

Start with these six charts in any arrangement:

| Chart | Initially shows |
| --- | --- |
| File table | All files |
| Dependency graph | Dependencies across the repository |
| Flowchart | Available execution flows |
| Sequence diagram | Available call sequences |
| Complexity bars | Complexity across functions |
| Concepts chart | Concepts across the repository |

Click **SmsController.java** in the file table:

| Chart | Required result |
| --- | --- |
| File table | Marks SmsController.java selected and shows its related file population |
| Dependency graph | Shows dependencies related to that file |
| Flowchart | Shows flows associated with that file's functions |
| Sequence diagram | Shows calls associated with those functions |
| Complexity bars | Shows complexity for those functions |
| Concepts chart | Shows concepts connected to that file |

Then click **submitSms()** in the complexity chart. That further narrows the shared
selection. All charts show the relevant function, API flow, calls, dependencies
and concepts. The selected file and function remain visible as active constraints.

Click submitSms() again: remove only that function's constraint; every chart
returns to the SmsController.java selection. Remove the file selection: every
chart returns to the repository population. Double-click either element opens
its captured source without changing the final selection.

Supporting dependencies, callees and context nodes may be included when related
to the selected subject, but must be labelled as context and must not inflate
selected-population counts. Missing call bodies, concepts or measurements remain
explicit gaps; the UI must not invent data to fill a chart.

### Mandatory acceptance exercise

1. Place each registered chart in a different slot, including a duplicate view.
2. Click a file in one chart and verify every analytical chart updates.
3. Click a function in a different chart and verify the population narrows again.
4. Toggle the function off and verify the file selection is restored everywhere.
5. Clear the file and verify all charts return to repository scope.
6. Move charts while selected; verify identities, counts and constraints persist.
7. Add a chart while selected; verify its first result uses the current selection.
8. Double-click source-backed elements; verify source identity and unchanged selection.
9. Verify meaningful empty states for charts with no related data.

This exercise must be tested at the shared core/renderer boundary. Persona files
must not implement any of these behaviours separately. A chart is not ready to
ship in Atlas until it passes this contract.

## Requirements

1. Every data-bearing element has a stable visual occurrence ID and a separate
   revision-pinned subject/contributor reference. Selecting it changes the shared
   set expression. Selecting it again withdraws only its contribution.
2. Double-click opens captured evidence without changing the final selection.
   Keyboard selection and evidence opening have independent accessible actions.
3. One logical population produces multiple semantically different views. A call
   graph is not control flow, and no layout change can turn it into control flow.
4. Preserve containment, predicates, branch kinds, statement order, message
   occurrences, participants, guards, measurements and units as structured data.
   Never reconstruct them from display labels.
5. G6 remains a first-class graph host: layouts, custom nodes/edges, navigation,
   minimaps, plugins and supported grouping must remain reachable through host
   profiles. SVG/PNG are render/export formats, never the core's input contract.
6. Aggregate/chart evidence may be a stored fact, exact contributor descriptor,
   document span or runtime sample. Do not invent a code span to make it drawable.
7. Persona TSX files compose registered producers, presentation profiles and
   layout slots. They do not implement storage queries, set operations or gestures.
8. Renderer capability gaps, missing evidence, collection gaps, truncation and
   static-resolution limits are visible states. Never silently replace a sequence
   or control-flow diagram with a generic graph.

## Four contracts

| Contract | Owns | Does not own |
| --- | --- | --- |
| PopulationSpec | repository/revision, expression, universe, version, matches, coverage | pixels or chart geometry |
| SemanticViewSpec | typed C4 architecture/dependency/hierarchy/control-flow/sequence/data-flow/metric/table structures and provenance | engine options or SVG paths |
| PresentationSpec | profile, layout intent, detail, label and capability requirements | facts, query identity or selection algebra |
| RenderHandle | physical gestures, focus, camera, highlighting, export and disposal | independent filtering or source resolution |

The existing sealed visualization-adapter ViewSpec remains the render/audit
boundary. A semantic compiler prepares it for graph hosts; semantic models remain
attached and authoritative. Do not create a second geometry engine in the core.

## Semantic models

- Dependency: typed directed relations, subjects and optional containment.
- Hierarchy: explicit parents, roots and groups. Cycle/dangling-parent validation.
- Control flow: entry/exit/basic/decision blocks, ordered statements, predicates
  and typed true/false/back/jump/exception edges. Only render constructs supported
  by captured IR; unsupported semantics stay disclosed.
- Sequence: explicit participant IDs and ordered per-call message occurrences;
  self calls, repeated messages, guards and recursion/dispatch boundaries survive.
  Alternative/loop fragments require producer evidence. No runtime scheduling is
  inferred from a static call graph.
- Data flow: producers/consumers/stores and typed read/write/transform relations.
- Metric: typed dimensions, measures, units, aggregate cells and exact contributors.
- Table: typed columns, row/cell values and contributor identity.

More view kinds are registrations. Do not register a flame graph merely because
rectangles can be drawn: it requires measured stack/sample structure. Hosts may
return Unsupported rather than fabricate a shape.

## Evidence and authority

Evidence is a tagged union: source, document, stored-fact, aggregate or runtime.
Every record is pinned; runtime evidence carries its collection revision/run.
Unknown authority is never promoted to Fact. Facts-only is a policy over claims,
not a cosmetic opacity switch. Population counts and visible evidence counts are
separate and disclosed.

Source opening verifies repository, revision, file, returned range and any expected
content hash. Paired DOC/CODE or FROM/TO evidence keeps its roles; one location is
not chosen arbitrarily. Missing source does not make valid aggregate evidence
invalid. No SVG or adapter configuration may supply or rewrite provenance.

## Selection and presentation

Visual occurrence IDs identify appearances; subject IDs identify program entities;
contributor references identify the set operand. Several messages may refer to
one function without losing their distinct ordering or call-site evidence.

Selection, deselection and explicit scope/drill change population. Pan, zoom,
focus, layout changes and visual collapse do not. Group expansion that retrieves
more data is explicit and retains population/version stamps. Renderer highlights
are synchronized programmatically without feeding another selection event back.

Default grouping is OR within a dimension and AND across dimensions; explicit
intersection and exclusion are supported. View producers map entities through
registered storage relations instead of path/name guessing. Unsupported mappings
fail visibly rather than returning a misleading empty intersection.

## Profiles and capabilities

A profile names its semantic view kind, required/preferred capabilities, layout
family/orientation and detail intent. It does not name a storage query or embed
engine-native code. The host translates the profile into supported G6 options and
may register audited engine extensions. Capability negotiation must report the
actual engine, layout and fidelity. Required semantic capabilities cannot be
substituted away. Unsupported optional capabilities are disclosed.

Tables/metrics can use appropriate registered hosts. Statistical charts need not
be coerced into graph nodes. All hosts must emit the same physical-gesture events
and preserve contributor bindings. Accessible outlines remain fully interactive.

## Shared application services

The core also coordinates persona plans, named-area layouts, scope history,
change/revision context, search, cube controls, workspace serialization and index
freshness. These services remain independent of rendering. A persona may simplify
controls but cannot disable universal selection semantics. Explicit repository
reference panels are labelled and do not masquerade as selected-population counts.

## Conformance and rollout

First retain the tested selection kernel, replace the lossy bridge and prove four
models: hierarchy, actual captured control flow, ordered call sequence and metric
aggregation over the same selected population. Then expand persona projections.

Tests must cover semantic preservation through presentation changes, contributor
identity across occurrences, aggregates without source spans, source guards,
capability rejection/fallback, layout validity, stale responses, revisions and
native-storage/browser integration. A passing selection suite alone is not proof
that all persona requirements are implemented. Maintain a delivery ledger in
atlas-core-contract.md distinguishing implemented, partial and producer-dependent.


## Architect default composition and display density

`architect.default` follows the Architect PRD §4 seven-panel grid. Persona TSX
only chooses presentation density and composes the shared layout and controls.
`AtlasDisplayContext` hides prose, chart outlines and workspace/placement controls
by default for Architect; it does not change producer queries, populations,
selection, evidence policy or rendering semantics. Essential chart labels,
active constraint chips, missing-data states and errors stay visible. Details can
be restored explicitly. Native G6 canvases remain the graph renderers.

`declares_dependency` links Container → Dependency only through an exact captured
manifest evidence path, with ambiguous ownership left unlinked. Reverse mapping
selects declaring modules. It is distinct from File → `uses_dependency` and from
runtime calls. `unfilteredTargetUniverse` lets container charts include manifest-only
modules at an unrestricted repository root; any selection, breadcrumb, change
scope or filtered base disables this expansion. An empty module cannot fabricate
symbol membership. Duplicate-pair mapping includes both left and right sources.

The `driftboard` producer is registered once for all personas and projects recorded
claim verifications through `source_symbol`; unmapped claims do not silently
acquire guessed source ownership. The risk producer uses native file quality and
a dedicated metric treemap presentation. Binary area packing preserves measured
risk proportions; zero/unknown scores never receive invented area, and remain
selectable in a separate list. These renderers preserve the universal gestures.
