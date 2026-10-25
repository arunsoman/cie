# PRD — Atlas for the Architect / Long-Term Maintainer

> **Any chart, anywhere:** every analytical chart sends and receives the shared
> selection. Placement never changes filtering. The [SmsController.java → submitSms() example](atlas-semantic-architecture.md#worked-example-smscontrollerjava--submitsms)
> is mandatory acceptance coverage for this persona too.


> **Implementation boundary:** follow [Atlas semantic architecture](atlas-semantic-architecture.md).
> This persona composes producers + presentation profiles + layout slots. Every
> data element selects/deselects contributors; double-click opens evidence. Legacy
> POPUP-on-click and filtering opt-outs below are superseded. Producer gaps and
> unavailable rendering remain explicit; changing layout never changes meaning.


**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `architect`
**Reads first:** `atlas-persona-prd-foundations.md` (`LAYOUT-*`, `REG-04/05/06/03/13/14/15/16`, `ENG-01/02/03`)
**Shares a panel with:** `atlas-prd-reviewer.md` §5.2 `driftboard` (reused by id, not redefined)
**Builds on:** `atlas-charter.md` §6, `atlas-ux-spec.md` §7

## Current delivery boundary (2026-10-25)

Architect now routes through `ArchitectAtlas` and the shared core using
`architect.orientation`. It provides typed C4 application containers, framework and
generated component interpretation, file/symbol/API charts and captured call views.
This is the C4 orientation increment, not the dependency/duplication/drift hero
specified below. Twelve registered core charts remain placeable in any slot and
participate in the shared selection. See the [migration matrix](atlas-migration-matrix.md)
for remaining producer and evidence gaps. Legacy Architect layout descriptions
below are requirements, not claims about the delivered orientation slice.

## 1. Problem

`atlas-presets.ts` ships `hero: "containers"` for the architect preset; the charter names the hero for
this persona as the **dependency graph (E5)** (`atlas-charter.md:112`, `atlas-ux-spec.md:166`). The data
already exists (`deps.json`) — `chart-inventory.md:68` grades this Tier C "data exists, chart not built."
None of the architect's other ★★★ items (bounded contexts, duplication, drift board, timeline, risk
treemap) are in `AtlasPanelId`. Today's architect preset is the fresher's eight panels with facts-only
off and full detail — useful orientation, not an architect's toolkit.

A second gap, found by reading `web/src/lib/quality.ts` directly rather than assuming: the web layer
**already ships 16 real, measured quality charts** — security findings, duplicate code blocks (a literal
clone detector), hotspot risk (`churn × complexity × urgent findings`, formula disclosed), cyclomatic
complexity, coverage, defect density, MTBF and more (`buildQualityCharts`, `quality.ts:614-820`) — and
*none of them appear anywhere in the Atlas*. An architect asking "is this module safe to touch" has to
leave the Atlas and go to the one-tap chart menu to get an answer that is already fully computed. The one
metric a developer genuinely expects here that does **not** exist yet is structural cohesion/coupling
(fan-in/fan-out over the call graph) — `ENG-11` below grades that honestly, separately from the sixteen
that already work today.

## 2. Goals

- **G1.** Opening "Architect" answers "does structure match intent, and where has it rotted" with the
  dependency graph, duplication, and drift board on one screen — the charter's own definition of this
  persona's job.
- **G2.** Every item below is graded by what it actually costs (`REN-*` = render-only, data already
  exists; `ENG-*` = genuine new engine/CLI work) so sequencing is never guessed.
- **G3.** `layer`, `churnBucket`, `dupTier` become real, facetable, filterable dimensions — not just
  colors on a chart.
- **G4.** A per-module scorecard puts coverage, complexity, duplicate count, hotspot risk and security
  findings in one row each — the "is this module safe to touch" question answered without leaving the
  Atlas, using data that is already fully computed today.

## 3. Non-goals

- No cross-revision reconciliation beyond the single timeline chart (`SCOPE(rev,n)`, already spec'd in
  `atlas-ux-spec.md` §3/§7) — comparing two repos or merging identity across forks is out of scope.
- No automatic architecture-violation "fix" — this persona *sees* drift and duplication; remediation
  remains a human/Reviewer decision.

## 4. Information architecture — the custom layout

`AtlasLayout.id = "architect.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx = 110`,
`dock = {defaultOpen: true, widthPx: 280}`. Hero stays `containers` in `atlas-presets.ts` **only as a
fallback**; this PRD makes `depgraph` the hero (FR-1), merged into the one grid as the `isHero` cell
(foundations §A.3 invariant 6, corrected from an earlier draft's separate hero column) — `boundedcontexts`
fills the columns beside it in the same rows, so nothing sits empty under the dependency graph.

```
templateAreas (desktop, 8 cols):
"r4    r4    r4    r4    r4    r4    r4    r4   "
"hero  hero  hero  hero  hero  bctx  bctx  bctx "
"hero  hero  hero  hero  hero  bctx  bctx  bctx "
"hero  hero  hero  hero  hero  bctx  bctx  bctx "
"score score score score score score score score"
"score score score score score score score score"
"dup   dup   dup   dup   drift drift drift drift"
"dup   dup   dup   dup   drift drift drift drift"
"time  time  time  time  time  time  time  time "
"tree  tree  tree  tree  tree  tree  tree  tree "
"tree  tree  tree  tree  tree  tree  tree  tree "
"tree  tree  tree  tree  tree  tree  tree  tree "
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `depgraph` (`isHero: true`) | `canvas` | 330 | the headline question; 5/8 width, `boundedcontexts` fills the rest instead of empty space |
| `bctx` | `boundedcontexts` (NEW) | `canvas` | 330 | beside the hero — a service map, roughly square at 3/8 width × 3 rows |
| `score` | `scorecard` (NEW, §5.1) | `table-dense` | 220 | full width — "is this module safe to touch" answered right after the two graphs, not buried below everything else |
| `dup` | `duplication` (NEW) | `card` | 220 | |
| `drift` | `driftboard` (**reused from Reviewer PRD §5.2**) | `card` | 220 | the identical panel, not a re-implementation — one `driftStatus` field (`REG-03`), one component |
| `time` | `timeline` (NEW) | `card` | 110 | full width, short — a timeline reads left-to-right, never tall |
| `tree` | `risktreemap` (NEW) | `canvas` | 330 | full-width, tall — treemap rectangles need near-square aggregate space to pack legibly |

`templateAreasMd`: `["r4 r4", "hero hero", "bctx bctx", "score score", "dup drift", "time time", "tree tree", "tree tree"]`.
`templateAreasBase`: linear `planOrder`.

## 5. New panels — cost-graded (nothing left to estimate)

| Panel | Chart kind | Cost grade | What's missing exactly |
|---|---|---|---|
| `scorecard` | `table` (§5.1) | **`REN-0`** already shipped, wiring only | every column but one already renders as its own one-tap chart today (`quality.ts`) — zero new engine or render work, pure composition |
| `depgraph` | `uml`/graph (E5) | **`REN-1`** render-only | `deps.json` ecosystems/manifest edges exist (`chart-inventory.md:68`); needs a graph layout over them — zero new facts |
| `boundedcontexts` | graph (C5/C6) | **`REN-2`** render-only | `stack.json` apps + concepts Container tier exist (`chart-inventory.md:79`); needs a cross-container service-graph layout (today only per-container views and the system C4 exist) |
| `duplication` | `table` + chord ribbon | **`REN-0`** table already shipped; chord is **`REN-3`** render-only | the literal duplicate-block **table** is `quality.ts::buildDuplicationSpec` (`REG-15`), already rendered today as the `duplications` one-tap chart — placing it in this panel costs nothing; only the chord-ribbon *visualization* of the same pairs is new (Tier B, G6 built-in layout). The panel also keeps the FCA-based "capability overlap" table (`REG-06`, `buildOverlapSpec`) as a second tab — a different, complementary signal, not a duplicate of the first |
| `driftboard` | reused | — | defined in `atlas-prd-reviewer.md` §5.2; this PRD only *places* it in a new grid area |
| `timeline` | semantic-diff timeline (I5) | **`ENG-04`** genuine engine/CLI gap | `.cie/revs/*` revisions + the timeline crate exist (`chart-inventory.md:75`) but there is no `cie timeline` CLI route emitting `timeline.json` for the UI — this is real new work, not a render gap |
| `risktreemap` | treemap (H5) | **`REN-4`** render-only | the risk **score** per cell is not new math to invent — it is `quality.json hotspots.rows[].risk` (`REG-13`), the exact `churn × complexity × urgent findings` formula the already-shipped `hotspots` one-tap chart discloses (`quality.ts::buildHotspotSpec`); `flame.ts`'s rect-packing engine (`chart-inventory.md:53`) just needs to pack cells sized/colored by that real number instead of churn alone |

### 5.1 `scorecard` — the module scorecard (NEW, answers "is this module safe to touch")

One row per file, the five real metrics a developer actually wants side by side, plus the one that
honestly isn't measured yet:

| Column | Source | `REG-xx` | Real today? |
|---|---|---|---|
| Coverage % | `quality.json coverage_heatmap.files[].percent` | `REG-02` | ✅ (`buildCoverageHeatmapSpec`) |
| Complexity (CC) | `quality.json complexity.files[].avg_cc`/`max_cc` | `REG-14` | ✅ (`buildComplexitySpec`) |
| Duplicates | `quality.json duplication.pairs` naming this file | `REG-15` | ✅ (`buildDuplicationSpec`) |
| Hotspot risk | `quality.json hotspots.rows[].risk` | `REG-13` | ✅ (`buildHotspotSpec`) |
| Security findings | `quality.json security.issues` naming this file | `REG-16` | ✅ (`quality.ts` `security-issues`/`vulnerabilities`) |
| Cohesion / coupling | — | — | ❌ **not yet measured** (`ENG-11`, §7) — the cell renders the literal text "not yet measured" with a `?` info affordance explaining why, never a fabricated number, never silently blank |

Rows sort by `REG-13` (hotspot risk) descending by default — the riskiest module to touch is the first
thing an architect sees. Click a row → `SCOPE(space, module, file)` + `CHIP`, same verb every other
file-identifying element in the Atlas already uses.

**Click choreography (new, composed from the existing closed verb vocabulary — no new verbs):**
- `depgraph` box → `SCOPE(space, container/module)` + `CHIP`; edge → **paired popup** (FROM|TO,
  `atlas-ux-spec.md` §2.2) + `CHIP(edge)`; edge between containers that already call each other →
  `TELL` "these two containers already call each other N times — the boundary is soft" (ux-spec §7,
  unchanged, restated so this PRD doesn't silently drop it).
- `duplication` chord ribbon → `CHIP` + a popup listing the shared functions, each row its own `POPUP`
  (ux-spec §7, unchanged).
- `timeline` marker → `SCOPE(rev, n)` — **not** `RELEASE`, a different axis (ux-spec §3, unchanged).
- `risktreemap` cell → `POPUP` (file head) + `CHIP`.
- `boundedcontexts` box → `SCOPE(space, container)` + `CHIP` (same verb as a C4 container box elsewhere —
  this panel is a different *view* of the same scope concept, not a new scope kind).

## 6. Slice/dice/filter — the architect's cube

*Governed by `atlas-persona-prd-foundations.md` Part E (the universal selection/filter/source contract):
every element selects, filters and double-click-opens by the same `AtlasElementRef` shape; filter state
is a set per dimension (E.2), not a single toggled value. The table below still states which `REG-xx`
dimensions and panels apply here — unchanged by Part E.*

| Field | Value |
|---|---|
| `hero` cube | `dimensions: [{id:"layer", dimension: Field("REG-04")}, {id:"module", dimension: Field("module")}]`, `measures: [{name:"edgeCount", agg:"count"}, {name:"churn", agg:"sum", field:"REG-05"}]`, `layout: Pivot{rows:["module"], columns:["layer"]}` — layering reality vs. intent, as a pivot, is the architect's headline question |
| `facets` | `{panel:"depgraph", dimension:"REG-04"}` (layer), `{panel:"risktreemap", dimension:"REG-13"}` (hotspot risk tier — corrected from churn-only), `{panel:"duplication", dimension:"REG-06"}` (dup tier), `{panel:"scorecard", dimension:"REG-14"}` (complexity tier) |
| `crossFilterDimensions` | `["REG-04", "REG-05", "REG-06", "REG-03", "REG-13", "REG-14", "REG-15", "REG-16"]` — e.g. clicking "Domain" on the layer facet cross-filters the dep-graph to domain-layer edges, the drift board to the same files, and — this is the part an earlier draft left out — the `scorecard` and `duplication` entity tables to the rows whose file carries that layer too; an entity table with no `REG-04` watcher would look unaffected by a filter that in fact applies to its own rows |
| `drillThrough` | `{cell:["REG-04"], entity:"file"}` — drilling a layer cell lists its files; `{cell:["REG-06"], entity:"symbol"}` — drilling a dup-tier cell lists the shared functions (same data `duplication`'s click already opens — the cube's drill-through and the chart's click resolve to the identical query, never two code paths for one answer); `{cell:["REG-13"], entity:"file"}` — drilling a risk tier lists exactly `hotspots.rows` filtered to that tier, the same rows the treemap already colors |

## 7. Engine requirements

- **`ENG-04`** — `cie timeline` CLI route. Reads `.cie/revs/*` via the existing timeline crate; emits
  `timeline.json`: `{ revisions: [{ rev: string, timestamp: string, summary: { conceptsAdded: u32,
  conceptsRemoved: u32, driftDelta: i32 } }] }`, one entry per indexed revision, newest last. No new
  analysis — it is the existing crate's output, exported.
- **`ENG-05`** — `.cie/layers.yaml` (repo-authored, optional) + heuristic fallback for `REG-04` (`layer`):
  ```yaml
  # .cie/layers.yaml — optional, repo-authored, read verbatim as Fact when present
  layers:
    Presentation: ["**/controller/**", "**/web/**"]
    Domain:       ["**/service/**", "**/domain/**"]
    Infrastructure: ["**/repository/**", "**/dao/**", "**/infra/**"]
  ```
  When absent, the engine infers per-language path heuristics (`*/controller/**` → `Presentation`,
  `*/service/**` → `Domain`, `*/repository|dao/**` → `Infrastructure`, compose image-only service →
  `External`), tagged `Good guess`, never `Fact` — consistent with the existing honesty contract. A
  module matching no rule gets `layer: "Unclassified"` (an explicit bucket, never silently dropped from
  the pivot).
- `ENG-03` (drift `alsoEvidence` patch) — **shared** with Reviewer, already defined in foundations Part C;
  needed here only because `driftboard` is reused verbatim.
- **`ENG-11`** — structural cohesion/coupling (the scorecard's one honestly-missing column; `PRD-v1.0.md`
  CAP-1.3, "Coupling/cohesion metrics — CSR + module structure — Core"). Graded precisely, not
  hand-waved: the two primitives it needs — the CSR call-graph relation structure and the module/file
  structure — **already exist** (`cie-analysis` crate; the same PDG/relation-graph work `ENG-02` wires
  into exports). What's missing is the metric itself:
  - **Coupling** (per module): `fan_out` = distinct modules called into; `fan_in` = distinct modules
    calling in; both computed by walking the existing `Calls` relation and mapping each symbol to its
    owning module (already-recorded `parentId` chain). No new relation kind — a reduction over one that
    is already built.
  - **Cohesion** (per module): LCOM-style — among the module's own symbols, the fraction of pairs that
    share a call or a data dependency (`Reads`/`Writes` on the same store), over all possible pairs.
    Also a reduction over existing relations, no new fact collection.
  - Export as `module_metrics.json`: `{ "modules": [{ "module": "string", "fanIn": "u32", "fanOut":
    "u32", "cohesion": "f32 (0.0-1.0)" }] }`.
  - This is real, bounded work (a reduction pass, not a new analysis algorithm) but it is **not render-only**
    — it does not exist in any form today, unlike every other column in `scorecard`. It ships in its own
    phase (§12) and the scorecard's cohesion/coupling cell stays honestly "not yet measured" until then.

## 8. Functional requirements

- **FR-1.** `atlas-presets.ts` architect preset: `hero` changes from `"containers"` to `"depgraph"`;
  `planOrder` becomes `[depgraph, scorecard, boundedcontexts, duplication, driftboard, timeline,
  risktreemap, containers, components, hierarchy, flow, dataflow, sequence]` — `scorecard` leads the
  secondary panels (it answers the single most-asked question fastest), the existing eight demote to a
  condensed `ctx` card (same pattern as the Reviewer PRD §4, reused verbatim, not reinvented).
- **FR-2.** `scorecard` ships **immediately, before any other new panel** (`REN-0` — every column but
  one is already-shipped data; this is pure UI composition, zero engine or render risk). Its sixth column
  shows "not yet measured" until `ENG-11` lands — this is not a blocking dependency, it is the honestly
  incomplete state the column is designed to show. `depgraph`/`boundedcontexts`/`duplication`/
  `risktreemap` ship as soon as `REN-1..4` land; `timeline` ships only after `ENG-04`; `layer` faceting
  ships only after `ENG-05`. These are independently shippable — the grid's `time` area shows "revision
  timeline not yet indexed" until `ENG-04` lands, and `depgraph`'s layer-coloring falls back to
  "Unclassified" for every node until `ENG-05` lands. None of these block each other or `scorecard`.
- **FR-3.** Facts-only stays off by default (unchanged preset default) — inferred structure (layer
  guesses, duplication near-matches) is this persona's explicit value (charter §6), never hidden.

## 9. Edge cases

- **E-1.** `deps.json` has a manifest with zero resolved edges (e.g. a new, empty module) — `depgraph`
  shows the node with "no declared dependencies yet," never omits it.
- **E-2.** `.cie/layers.yaml` present but a path matches two layer globs — first match in file order wins
  (deterministic, documented in the schema comment above — no runtime ambiguity).
- **E-3.** `overlap.json` empty (no duplication found) — `duplication` panel shows "no duplicate pairs
  found above the similarity floor," `REG-06` facet shows `None (100%)`, never an empty chart with no
  explanation.
- **E-4.** `ENG-04` not yet shipped — `timeline` area renders the honest "not yet indexed" state defined
  in FR-2, and `SCOPE(rev,n)` is simply absent from the click vocabulary until then (no dead button).
- **E-5.** A file in `scorecard` has no collected coverage run (`coverage_heatmap` has no row for it) —
  that cell shows "not run," distinct from `0%` (never run and measured-at-zero are different claims,
  the same distinction `quality.ts` already makes for the heatmap's own tiles).
- **E-6.** A file has zero entries in every one of the five real columns (a brand-new, untouched file) —
  the row still renders, all five cells honestly empty/zero, never dropped from the table silently.

## 9a. Why the scorecard changes nothing about ENG-11's honesty

The scorecard is the clearest place in the whole Atlas where the "never fabricate a fact" rule could
quietly slip — a table of numbers invites filling every cell. `ENG-11`'s column is the test: until that
engine work ships, the cell is literal text, never a placeholder number like `0` or `—` that a user could
mistake for "measured and found to be zero." This is normative, not a style note.

## 10. Acceptance criteria

- [ ] Architect hero renders `depgraph`, not `containers`.
- [ ] `layer` pivot (rows=module, columns=layer) renders with real edge counts for a repo carrying
  `.cie/layers.yaml`, and with `Unclassified` fallback for one that doesn't.
- [ ] Churn-bucket facet on `risktreemap` cross-filters `depgraph` edges to the matching files.
- [ ] Clicking a duplication chord ribbon opens the shared-function list; clicking a listed function
  opens its exact source line (unchanged popup contract).
- [ ] `drillThrough` on a `REG-06` (dup-tier) cell returns the same symbol set the chord's own click
  already shows — no divergence between the two code paths.
- [ ] `driftboard` in this persona's grid is pixel-for-pixel the same component instance used in the
  Reviewer preset (verified by shared test fixture, not just "looks the same").
- [ ] `scorecard` renders with real coverage/complexity/duplicate/hotspot/security values for every file
  that has them, sorted by hotspot risk descending, and the cohesion/coupling column reads literally
  "not yet measured" for every row until `ENG-11` ships — never a number.
- [ ] Clicking a `scorecard` row scopes every other panel to that file (`SCOPE(space, module, file)`) —
  the same verb a component box already uses, verified, not a new one.

## 11. Resolved decisions log

| Question | Decision |
|---|---|
| Is `depgraph` a new engine capability? | No — `REN-1`, render-only; `deps.json` already has the edges. |
| What if two layer globs match a path? | First match in `.cie/layers.yaml` file order wins. |
| Does an unclassified module get dropped from the pivot? | No — explicit `"Unclassified"` bucket. |
| Does `timeline` block the other five new panels? | No — each of `REN-1..4` ships independently of `ENG-04`/`ENG-05` (FR-2). |
| Does the scorecard wait for cohesion/coupling before shipping? | No — it ships with five real columns immediately; the sixth reads "not yet measured" until `ENG-11`. |
| Is "not yet measured" ever replaced by a guessed number? | No, never — a missing engine capability is always disclosed as missing, not approximated. |
| Does this PRD restate the generated-code-exclusion / monorepo-scale caveats for `scorecard`/`duplication`? | No — inherited by reference from `atlas-persona-prd-foundations.md` Part H, rows 3/4, governed by Part F.1/F.6/F.7. `scorecard` must virtualize above 2,000 rows and exclude generated/vendored paths before computing `REG-05/06/13/14/15`. |

## 12. Rollout

**Phase 0 (`REN-0`, ships first, same day as the layout change):** `scorecard` with five of its six
columns live — every one of those columns is already a working one-tap chart (`quality.ts`); this phase
is pure composition and carries zero engine or render risk.
Phase 1 (`REN-1..4`, no engine work): `depgraph`, `boundedcontexts`, `duplication`, `risktreemap` plus
the reused `driftboard` — covers the remaining 4 of 6 structural panels. Phase 2 (`ENG-05`): `layer`
dimension goes live, upgrading `depgraph`'s coloring and the layer pivot from "Unclassified" to real
buckets. Phase 3 (`ENG-04`): `timeline` panel goes live. Phase 4 (`ENG-11`): the scorecard's sixth
column goes live, the one phase in this PRD that is genuine new engine work rather than wiring. Each
phase is independently demoable and independently shippable — never a single "architect preset" big-bang
release.
