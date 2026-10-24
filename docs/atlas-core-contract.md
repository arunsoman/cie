# Atlas core: mandatory selection and projection contract

Status: implemented core and first Reviewer vertical slice. Other persona panels
remain legacy and are not certified as conformant by this document.

## Product invariant

Every data-bearing Atlas element resolves to a revision-pinned contributor set.
Single-click toggles its contribution to the shared selection expression.
Double-click opens captured source/evidence while preserving the final selection.
Every analytical view derives its content and counts from the same selection.
Personas change composition and guidance, never the meaning of these gestures.
Decorative captions and navigation controls do not require contributor identities.

This contract overrides conflicting click/POPUP and single-active-filter rules
in atlas-ux-spec.md. Existing legacy components must migrate before claiming
conformance. A new persona must not disable the core's selection semantics.

## Implementation boundaries

Public entry: `web/src/lib/atlas-core.ts`.

| Module | Responsibility |
| --- | --- |
| atlas-types | Pin, item, operand, commands, ViewSpec, storage and registry contracts |
| atlas-selection | Pure set-expression compilation and contribution reduction |
| atlas-core | Immutable state, atomic view batches, session export/restore, lifecycle |
| atlas-interaction | Pointer arbitration, keyboard/open actions and cancellation |
| atlas-storage | Existing storage API transport; no SQL or persistence policy |
| atlas-projections | Registered tables, file/kind metrics and declared-call graph |
| atlas-conformance | Registration and emitted-item validation |
| atlas-provider.tsx | React subscription and revision-scoped session lifetime |
| atlas-views.tsx | Shared renderers and accessible gesture bindings |
| reviewer-atlas.tsx | Persona composition only; no queries or selection handlers |

## Selection algebra

The core owns one base universe (symbols in the first slice). Every operand must
resolve into that universe. Projectors normalize file/concept/relationship
contributors before combining them; mixing entity kinds without a relationship
mapping is not a meaningful intersection.

Union contributions in the same group are OR'd. Different groups are AND'd.
Explicit intersection contributions are additional AND constraints. Exclusions
are subtracted from the resulting positive population. With no positive
constraints, exclusions are subtracted from the base universe.

Example: `(File A OR File B) AND database-writers MINUS tests`.
Deselecting A removes A's operand, not any member that B still contributes.
There is no chip cap that silently removes constraints. Item IDs must represent
semantic identity across charts: a file cell and its file bar share one ID.

The storage adapter uses existing select/union/intersect/difference/follow/
cell_contributors queries. Aggregate cells retain opaque storage-issued exact
contributor descriptors, not regenerated label filters. Descriptors may expire;
restore never invents replacement contributors or silently drops an operand.

## One population, multiple ViewSpecs

All views with binding `selection` use one compiled query and one storage request
per update, with multiple named outputs. This is a shared logical population,
not a new materialized database table. All bindings are committed as one state
batch. Response repository/revision and per-view selection versions are checked.
A superseded request cannot overwrite newer data even if transport ignores abort.

Binding `repository` is an explicit reference population, labelled in the UI.
The Reviewer file chooser uses it so another file can be added without clearing
the selection. Reference counts must never be presented as selected counts.

ViewSpecs distinguish members from expanded context and disclose graph limits,
partial pages and unavailable outputs. Graph context does not inflate the
selected count. A declared call graph is labelled as a call graph, never a
flowchart or an observed execution sequence.

Pagination belongs to the core. Cursors remain associated with the original
query and reset on selection changes. Previous consistent results may remain
visible during loading/error but their interactions are disabled.

## Gesture and evidence rules

- Click an unselected item: add its operand and chip.
- Click a selected item: remove only its contribution.
- Double-click: open captured evidence; do not change the final selection.
- Space: immediate selection toggle. Enter: open evidence.
- Multiple locations: evidence chooser, never an arbitrary endpoint.
- No location: explicit unavailable state, never an unrelated file.
- Evidence uses the pinned storage reader, file and byte range.

Pointer toggles use a configurable 600ms arbitration delay. Standard double-clicks
cancel the pending toggle. If an input device reports a slower double-click after
that delay, the controller restores that item's original contribution while
preserving other selections. Such slow gestures may cause a transient query;
they do not silently change the final selection. No browser paint-atomicity
assumption is used. Host-specific delay tuning is supported.

## Persona and revision lifecycle

Persona switches retain the mounted core and selection. The first migrated
persona is Reviewer; the other four still render their legacy panels inside the
same session lifetime. Their old selection systems are not yet unified with the
core. The existing Ask-CIE dock and MCP consumers have not yet been migrated
to this new selection expression; the Reviewer slice discloses its separate chat
context. This is a core-plus-vertical-slice delivery, not all persona PRDs. Returning to Reviewer preserves its core contributions.

A repository/revision change disposes the old core, aborts source and view work,
and closes its reader. It starts a fresh selection; old IDs are never rebased.
Session serialization is versioned and rejects a different pin. Persistent
cross-revision selection migration requires explicit entity revalidation and is
not performed automatically. Reload persistence is not enabled by default.

## Adding a registered view

1. Define an AtlasViewDefinition with unique ID and explicit population binding.
2. Declare named outputs over the provided query; do not create a private filter.
3. Project results to an AtlasViewSpec with refs for every data item and edge.
4. Provide evidence refs or an explicit empty evidence list.
5. Compose AtlasRegisteredView in a persona TSX layout.
6. Add fixtures to the shared conformance suite, including cross-view identities.

Registration rejects duplicate IDs; output validation rejects missing selection
refs, mismatched pins, invalid evidence paths and dangling graph edges. Adding
a renderer kind requires extending both its renderer and conformance validation.

## Verification

`node --test test/atlas-core.test.cjs` from web covers set algebra against an
independent set evaluator, overlaps, revision isolation, stale responses, batched
projection commits, source chooser/unavailable states, interaction arbitration,
session restore, cursor reset, disposal and canonical file identities.

The native integration/browser checks exercise the existing storage transport,
not a second frontend-only data store. This contract does not claim completeness
of static call resolution, runtime dispatch, business concepts, async scheduling,
or exception/control-flow analysis. Those remain producer responsibilities.

## Validated delivery

- TypeScript no-emit check: passed.
- Web suite: 140 tests passed, including 19 pure core tests and a new real-native
  Atlas storage integration test.
- Next production build: passed.
- Production Playwright fixture: selection, cross-view file filtering, cursor
  paging, deselection, double-click captured source, persona round-trip retention
  and no browser exceptions passed.

Commands (from web): `node --test --test-concurrency=1 test/*.test.cjs`,
`npx tsc --noEmit`, `npm run build`, `node test/atlas-core-browser.cjs`.
The browser check requires Playwright Chromium or `ATLAS_BROWSER=/path/to/chromium`.
