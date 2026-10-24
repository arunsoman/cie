# Atlas persona PRDs — shared foundations (grid engine + persona cube)

**Date:** 2026-10-24 · **Status:** PRD, ready for implementation · **Supersedes nothing; extends:**
`docs/atlas-charter.md`, `docs/atlas-ux-spec.md` (rev 2), `docs/storage-retrieval-api-spec.md` §16,
`docs/chart-inventory.md`.

**Read this before any of the six persona PRDs** (`atlas-prd-fresher.md`, `atlas-prd-reviewer.md`,
`atlas-prd-architect.md`, `atlas-prd-auditor.md`, `atlas-prd-consultant.md`, `atlas-prd-performance.md` —
the sixth added by a later pass, closing Part D.3's original orphan 1). Each persona PRD cites this
document by item id (`LAYOUT-*`, `REG-*`, `ENG-*`, `API-*`) instead of re-deriving the mechanism — this
is where the mechanism is defined, once, so the six PRDs cannot drift against each other. The charter
(`atlas-charter.md`) and the top-bar persona sub-menu (`atlas-ux-spec.md` §14) still describe five human
presets — adding Performance as a sixth entry in `atlas-presets.ts`/the sub-menu is explicitly in scope
for whoever implements this PRD, not a documentation-only persona.

## 0. Why this document exists

Two systemic gaps showed up when the five personas were audited against what is actually shipped
(`web/src/lib/atlas-presets.ts`, `web/src/lib/chart-plan.ts`, `web/src/components/cie/atlas-view.tsx`):

1. **The panel grid is uniform.** `atlas-view.tsx:511-549` renders R3 as
   `grid grid-cols-1 lg:grid-cols-2 gap-3` — every panel the same card, same max-height
   (`max-h-[520px] lg:max-h-[46vh]`), reflow is auto-flow left-to-right. A persona preset can reorder
   *which* panel appears first; it cannot make one panel bigger, square, or canvas-shaped while another
   stays a thin table strip. Architecture graphs, trust-boundary maps and ownership matrices do not
   read well at a fixed 2-column card size — they need the asymmetric layouts product teams actually
   draw on whiteboards.
2. **The panel vocabulary is shared.** `AtlasPanelId` has eight members
   (`containers`, `appentry`, `svcentry`, `components`, `hierarchy`, `sequence`, `dataflow`, `flow`) and
   every persona preset is a reordering of the *same eight*. The panels the charter itself ranks ★★★ per
   persona — impact graph, dependency graph, trust boundary map, ownership map, drift board — are not in
   the union at all.

A third gap, caught in review of the first mockup built from this document and corrected here rather
than only in the mockup: **a dedicated hero column is a pixel-budget mistake.** Giving the hero chart a
fixed-width column the full height of the screen wastes everything below the chart's natural height —
that column sits empty while the *rest* of the layout is squeezed into the remaining width. A hero chart
is important, not infinitely tall; it should be the **largest cell in the same grid** as every other
panel, so whatever vertical room it doesn't use is immediately available to the panels beside and below
it, not stranded in a column nothing else can enter. §A.2–A.6 below fold the hero into one unified grid
per persona and make the Ask-CIE dock itself collapsible, so no persona PRD may reintroduce a separate,
permanently-reserved column for either one.

This document fixes gap 1 with a **named-area grid layout engine** (Part A) and gives every persona PRD
a **slice/dice/filter cube extension** (Part B) and the **exact new schemas** three or more personas
would otherwise each invent slightly differently (Part C). Gap 2 — the missing panels themselves — is
closed inside each persona's own PRD, because the panels differ by persona; the *mechanism* for adding
one does not. Gap 3 is closed once, below, the same way gap 1 is.

---

## Part A — The named-area grid layout engine

### A.1 Current mechanism (what changes)

`atlas-view.tsx` renders `AtlasPlan.rows` (from `planSheet`, `web/src/lib/chart-plan.ts`) into a single
`<div className="grid grid-cols-1 lg:grid-cols-2 gap-3">`, mapping each `PlanRow` to a `<PanelCard>` in
array order, with one hand-written escape hatch (`lg:col-span-2` on specific rows, `atlas-view.tsx:549`).
There is no per-panel size, aspect, or canvas-vs-card distinction.

### A.2 New types (`web/src/lib/atlas-layout.ts`, NEW file)

The hero is **not a separate region** in this type — it is `PanelSlot.isHero: true` on one entry in the
same `slots` array as every other panel, placed in `templateAreas` like any other area. There is exactly
one `AtlasLayout` per persona, one grid, one set of rows.

```ts
// A grid-template-areas token: one lowercase word, unique within one AtlasLayout.
export type AreaToken = string;

export interface PanelSlot {
  panel: AtlasPanelId;        // existing or persona-added member of the (extended) union
  area: AreaToken;            // must occur in every row of templateAreas it belongs to
  minHeightPx: number;        // hard floor — never collapses below this, no implicit clipping
  isHero?: true;              // exactly one slot per layout carries this — styling only (bigger label,
                               // emphasized border); it changes NOTHING about grid mechanics, because
                               // there is no second grid for it to be "outside" of (§A.3 invariant 6)
  chrome: "card" | "canvas" | "table-dense";
  //  "card"        — today's PanelCard chrome (header + scroll body), used for tables/lists
  //  "canvas"      — NO internal scroll clipping; the chart owns pan/zoom (G6 trees, dep graphs,
  //                  treemaps) — PanelCard renders chrome only, content fills the slot exactly
  //  "table-dense" — PanelCard chrome, zero vertical padding, built for tall narrow panels
  //                  (route tables, finding lists) that need row density over whitespace
}

export interface AtlasLayout {
  id: string;                      // `${PersonaId}.default` — one per persona, see each persona PRD §4
  columns: 8;                      // fixed for every persona (§A.3 invariant 3) — fine enough granularity
                                    // that a hero can span 4-5 cols and leave 3-4 beside it for real content
  rowHeightPx: number;             // single grid-auto-rows unit; slots span N rows via repeated tokens
  templateAreas: string[];         // ONE grid, hero included, ROW 1 ALWAYS "r4"
  templateAreasMd: string[];       // 768–1279px: declared per layout, 2-column max
  templateAreasBase: string[];     // <768px: single column, one slot per row, in planOrder — no override
  slots: PanelSlot[];
  dock: { defaultOpen: boolean; widthPx: number }; // §A.6 — collapsible, reclaims its track when closed
}
```

### A.3 The six invariants (build-time asserted, not left to code review)

1. **Row 1 is always `"r4 r4 r4 r4 r4 r4 r4 r4"` — full width, 8 tokens.** This is the R4
   selected-context slot (ux-spec §1, §4, G5). It exists in every persona's `templateAreas` whether or
   not a chip is active — the renderer swaps *content* (collapsed caption strip ↔ FlowChart+SequenceChart
   pair), never the grid geometry. "What happens to row heights when R4 toggles" — answer: **nothing**,
   the row is reserved at a fixed height always, collapsed content is vertically centered inside it.
2. **Every `AreaToken` used in `templateAreas`/`templateAreasMd` must have exactly one matching
   `PanelSlot.area`.** A dev build throws `AtlasLayoutError: unmapped area "<token>"` if not — this is a
   `useMemo` assertion in `atlas-view.tsx`, not a lint rule a developer can skip.
3. **`columns` is always `8`.** One fixed grid granularity for every persona removes a whole class of
   "does this persona get 4 or 6 columns" bikeshedding and gives every layout enough resolution that a
   hero spans 4–5 columns (half the screen, roughly) while 3–4 columns of real secondary panels sit
   beside it in the *same rows* — never a hero column with empty space under it and everything else
   squeezed into what's left (§0 gap 3). This is fixed in this document, never computed at runtime.
4. **`chrome: "canvas"` panels never receive `max-h` clipping.** They size exactly to their slot's
   `rowSpan * rowHeightPx` (minus gap) and own their own internal pan/zoom (G6 host runtime, already
   proven at `concept-tree-g6.tsx:115-150`).
5. **`templateAreasBase` (mobile) is always `planOrder` as a 1-column stack, R4 row first, hero first
   after it.** No persona PRD may define a custom mobile layout — mobile is explicitly out of scope for
   the asymmetric grid; it gets the pre-existing linear order, which already ships.
6. **There is one grid, not three.** Earlier drafts of this document (and the first mockup built from
   them) gave the hero its own fixed-width flex column, full viewport height, beside the secondary-panel
   grid. That wastes every pixel below the hero's natural height — nothing else can ever occupy that
   column. Under this invariant, the hero is `area: "hero"` inside the *same* `templateAreas` as every
   other panel, sized generously (it is still the biggest, most prominent cell) but never isolated in a
   column of its own. Each persona PRD's §4 is corrected to this shape.

### A.4 Rendering contract — the exact CSS

```css
.atlas-body {              /* the R1+R3 grid and the R5 dock, side by side */
  display: flex;
  gap: 14px;
  align-items: flex-start;
}
.atlas-grid {               /* ONE grid: hero + every secondary panel, R4 row first */
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: repeat(8, minmax(0, 1fr));
  grid-auto-rows: var(--row-h);
  grid-template-areas: var(--areas);   /* joined templateAreas, one quoted string per row */
  gap: 12px;
}
.atlas-grid > [data-panel] { grid-area: var(--area); }
.atlas-dock { flex: 0 0 var(--dock-w); min-width: 0; }
.atlas-dock[hidden] { display: none; }     /* §A.6 — .atlas-grid's flex:1 fills the reclaimed width for free */
@media (max-width: 1279px) { .atlas-grid { grid-template-areas: var(--areas-md); grid-template-columns: repeat(2,1fr); } }
@media (max-width: 767px)  { .atlas-grid { grid-template-areas: var(--areas-base); grid-template-columns: 1fr; } .atlas-body { flex-direction: column } }
```

`planSheet` gains one field: `AtlasPlan.layout: AtlasLayout` (looked up from the persona's fixed
`AtlasLayout`, not computed — layouts are data, same philosophy as `PersonaPreset`). `atlas-view.tsx`
reads `plan.layout` instead of hand-writing `grid-cols-1 lg:grid-cols-2`, and renders `.atlas-body` as
exactly two flex children: `.atlas-grid` (hero + secondary panels, one grid) and `.atlas-dock` — no third
column.

### A.6 The dock is collapsible — closing is a pixel reclaim, not a preference toggle

The top bar's existing "Dock" button (ux-spec §1) is wired to a real state, not decorative: it toggles
`[hidden]` on `.atlas-dock`. Because `.atlas-grid` is `flex: 1` in the same flex row (§A.4), hiding the
dock costs nothing extra to implement — the grid fills the reclaimed width automatically, no second
layout calculation, no "two layouts to maintain." `AtlasLayout.dock.defaultOpen` sets the persona's
starting state (every persona PRD in this document defaults it `true` — Ask-CIE is first-class, not an
afterthought — but every user can close it to work the grid at full width, and every user can reopen it
without losing scroll position or scope). This is the mechanism that keeps the dock from becoming gap
3's mistake in a different shape: a *permanent* fixed-width column nobody asked to see.

### A.7 Expand-to-fill behavior (unchanged, restated so it isn't re-litigated)

Today's `Maximize2` per-panel expand (`atlas-view.tsx:684-688`) is preserved verbatim: an expanded panel
(hero or secondary, no distinction — there is one grid, §A.3 invariant 6) replaces the whole grid 1:1
regardless of its slot's `area`/`chrome`. This PRD only changes the *unexpanded* grid geometry.

**A note on region naming:** `atlas-ux-spec.md` §1 names the hero column "R1" and the secondary grid
"R3" as two canonical regions. This document keeps that vocabulary for *what the regions mean*
(R1 = the persona's headline answer, R3 = supporting panels) while changing *how they're laid out* —
R1's content is simply the `isHero` cell inside R3's grid now, not a separate CSS region. Nothing in
ux-spec's click choreography (§3, the closed verb vocabulary) changes; only the box each verb's target
lives inside does.

---

## Part B — The persona cube (extends `storage-retrieval-api-spec.md` §16)

### B.1 What exists today

`CubeRequest { dimensions, measures, layout, cell_filter, order, page, totals, missing, drill_through }`
and `DimensionSelection { id, dimension: DimensionRef, level, membership }` are specified
(`storage-retrieval-api-spec.md:473-501`) but **no concrete dimension is defined** beyond the generic
`DimensionRef` enum shape, and the shipped client-side cross-filter (`web/src/lib/chart-filter.ts`) only
knows six dimensions: `severity, kind, file, effect, authority, certainty`, each tied to one of four
"universes" (findings/summaries/contracts/elements).

### B.2 The dimension registry — every new dimension, fully specified

No persona PRD invents a dimension inline. Every dimension a persona needs is `REG-01..REG-17` below,
with its exact derivation rule fixed so two implementers cannot produce different buckets.

**Correction to the first pass of this registry:** `REG-01..REG-12` (below) were specified assuming
several metrics needed new engine work. A direct read of `web/src/lib/quality.ts` and
`web/src/lib/canned-charts.ts` shows that is wrong for five of them — the web layer already ships **16
real, measured "Health" + "Quality metrics" canned charts** (`buildQualityCharts`, `quality.ts:614-820`),
on top of the 21 in `canned-charts.ts` itself: security issues by category and by CWE, duplicate code
blocks (a real clone detector, not FCA overlap), hotspot risk (`churn × complexity × urgent findings`,
formula disclosed), a measured coverage heatmap, cyclomatic complexity per function, code-vs-test drift
and doc-vs-code drift (git-date lag), defect density, MTBF, arrival rate, crash rate and MTTR — all
**Tier A, zero new engine work, wiring only.** `REG-13..REG-17` below expose these to the cube/facet/
cross-filter mechanism; they are "wire an existing chart into a panel," never "build a new metric." The
one metric a developer might expect here that genuinely does **not** exist yet is structural
cohesion/coupling (fan-in/fan-out over the call graph) — graded honestly as `ENG-11` in
`atlas-prd-architect.md` §7, never fabricated as if `REG-13..17` already covered it.

| id | Dimension | Type | Owner persona(s) | Source fact | Derivation rule (exact) | Filters (universe/panel) |
|---|---|---|---|---|---|---|
| REG-01 | `blastRadiusTier` | enum `Low\|Medium\|High\|Critical` | Reviewer | `/api/impact` closure (affected symbol+concept count) | `1-5`→Low, `6-20`→Medium, `21-60`→High, `61+`→Critical | Impact panel, Callers panel |
| REG-02 | `testConfidence` | enum `Covered\|Partial\|Untested\|Unknown` | Reviewer | `quality.json` per-file coverage %, joined to impact's affected files | `≥80%`→Covered, `20-79%`→Partial, `<20%` and executable lines `>0`→Untested, no run recorded→Unknown | Test-confidence panel |
| REG-03 | `driftStatus` | enum `Implemented\|Partial\|Drifted\|Unverified` | Reviewer, Architect | `drift.json` `claim.status` (verbatim, pre-existing field — not derived, just exposed as a cube dimension) | identity | Drift board panel |
| REG-04 | `layer` | open string enum | Architect | NEW `.cie/layers.yaml` (repo-authored) else inferred | see `ENG-05` (defined in `atlas-prd-architect.md` §7 — Architect-only, so specified in that PRD, not here, per §3's "owned by exactly one persona" rule) | Dep-graph, bounded-context panel |
| REG-05 | `churnBucket` | enum `Low\|Medium\|High` | Architect | `cochange.json` per-file commit_count, trailing 90 days | `<5`→Low, `5-20`→Medium, `>20`→High | Risk treemap, dep-edge weight |
| REG-06 | `dupTier` | enum `Exact\|Near\|None` | Architect | `overlap.json` pair similarity — **capability overlap**, FCA-based: two differently-named parts sharing real symbols, not a copy-paste match (`web/src/lib/quality.ts::buildOverlapSpec`) | `≥0.98`→Exact, `0.80-0.97`→Near, no pair→None | "Where capabilities overlap" table |
| REG-07 | `trustZone` | enum `Public\|Internal\|TrustedCore` | Auditor | NEW `trust_boundary.json` (see `ENG-06`) | hop-count from nearest image-only external: `0-1`→Public, `2-3`→Internal, `4+`/unreachable→TrustedCore | Trust boundary map, route table, dataflow |
| REG-08 | `enforcementStatus` | enum `Enforced\|Asserted\|Unenforced\|Unknown` | Auditor | NEW `policy.json` (see `ENG-07`) | linked passing test→Enforced; code comment/annotation only→Asserted; neither, invariant recorded→Unenforced; no invariant for the concept→Unknown | Policy matrix, route table |
| REG-09 | `severity` | enum `Critical\|High\|Medium\|Low` | Auditor (reuse) | `report.findings[].severity` — the core-analysis findings, **already shipped** in `chart-filter.ts` and in the `urgency-donut`/`files-most-problems` canned charts (`canned-charts.ts`); distinct from `REG-16`'s CWE-mapped `quality.json security.issues` | identity | Findings panel |
| REG-10 | `owner` | string (author login) | Consultant | NEW `cochange.json --authors` extension (see `ENG-08`) | top committer by commit count, trailing 180 days, per file | Ownership map |
| REG-11 | `coChangeCluster` | int cluster id | Consultant | `cochange.json` `CochangeEdge.support`, connected components | edge kept when `support ≥ 3`; cluster id = component index after union-find over kept edges | Co-change matrix, dep-graph overlay |
| REG-12 | `journeyId` | string | Consultant | NEW `journeys.json` (see `ENG-09`) | identity (one id per extracted journey) | Journey chart, route table highlight |
| REG-13 | `hotspotRisk` | number 0–100 | Architect, Consultant | `quality.json hotspots.rows[].risk` — **already real**: formula is `hotspots.formula`, disclosed verbatim on every chart (`churn × complexity × urgent findings`) (`quality.ts::buildHotspotSpec`) | `≥50`→High, `25-49`→Medium, `<25`→Low (the same thresholds `buildHotspotSpec`'s bar coloring already uses — reused, not reinvented) | Risk treemap, Module scorecard |
| REG-14 | `complexityScore` | number (cyclomatic CC) | Architect, Reviewer, Consultant | `quality.json complexity.functions[].cc` — **already real**, real AST walk, formula `complexity.formula` (`quality.ts::buildComplexitySpec`) | `≥ complexity.totals.max_cc`→Critical, `≥4`→Elevated, else→Normal (the same thresholds the existing bar coloring uses) | Module scorecard, Test confidence panel |
| REG-15 | `duplicateBlockCount` | integer | Architect, Consultant | `quality.json duplication.pairs` — **already real**, a literal clone detector (`quality.json duplication.tool`), distinct from `REG-06`'s capability overlap (`quality.ts::buildDuplicationSpec`) | identity (count of pairs naming this file) | Duplication panel, Module scorecard |
| REG-16 | `securityFindingCount` | integer, by `REG-09` severity | Auditor, Architect | `quality.json security.issues`, CWE-mapped (`quality.ts::buildQualityCharts`, chart ids `security-issues`/`vulnerabilities`) — **already real**, distinct from `report.findings` (the core-analysis findings `REG-09` originally cited; both are real, this is the CWE-specific one) | identity (count of `security.issues` naming this file) | Findings panel, Module scorecard |
| REG-17 | `testDriftDays` | integer, nullable | Reviewer, Consultant | `quality.json drift.code[].test_lag_days` — **already real**, git-date lag between a file's last change and its tests' last change (`quality.ts::buildCodeDriftSpec`), distinct from `REG-03`'s claim-verification drift | identity | Test confidence panel, Module scorecard |
| REG-18 | `promiseAuthority` | enum `Explicit\|InferredCandidate` | Reviewer | `report.contracts[].authority` — **already real**, the `who-wrote-promises`/`promise-list` canned charts (`canned-charts.ts:193-238`) | identity | Drift board panel (Promises tab, `atlas-prd-reviewer.md` §5.2) |
| REG-19 | `runtimeCoverage` | number %, nullable | Architect, Performance | `runtime.json datasets.coverage.files[].percent` — **already real**, measured, distinct from `REG-02`'s static test-coverage (`canned-charts.ts` chart `runtime-coverage`) | identity; `null` when no runtime collection ran for this project (never defaults to 0) | Module scorecard, optional column; Performance `coverage` panel |

### B.2a Capability-gap registry — REG-20..29 (added after a completeness review against "code intelligence" as a category)

An external review of the first five PRDs against the broader "code intelligence tool" category (search,
supply chain, API contracts, schema safety, dead code, bus factor, doc coverage, secrets) found real,
genuine gaps — not wiring gaps like `REG-13..19`, but capabilities with **no existing data source at
all**. Every row below is graded `ENG-xx` in Part C, never `REN`, because none of this data exists in any
form today; this registry entry is honest about that, matching the discipline the review itself asked
for.

| id | Dimension | Type | Owner persona(s) | Source fact | Derivation rule (exact) | Filters (universe/panel) |
|---|---|---|---|---|---|---|
| REG-20 | `dependencyVulnerability` | enum `Critical\|High\|Medium\|Low\|None` | Auditor, Architect | NEW `sbom.json` (`ENG-12`) | max CVSS score across CVEs matched to the resolved version; no match → `None` (never `Low` — absence of a known CVE is not the same claim as a verified-low-severity one) | Auditor `findings`, Architect `scorecard` |
| REG-21 | `licenseRisk` | enum `Permissive\|WeakCopyleft\|StrongCopyleft\|Unknown` | Auditor | NEW `sbom.json` (`ENG-12`) | SPDX identifier classified against a fixed license-family table; an SPDX id not in that table (not an absent license) → `Unknown`, never defaulted to `Permissive` | Auditor `findings` (new Licenses tab) |
| REG-22 | `dependencyFreshness` | enum `Current\|Behind\|Abandoned` | Architect | `deps.json` + a vendored package-registry snapshot (`ENG-12`) | `0` major versions behind → `Current`; `1-2` → `Behind`; no release in the registry snapshot for 24 months → `Abandoned` | Architect `depgraph` edge styling |
| REG-23 | `apiContractStatus` | enum `Stable\|Deprecated\|BreakingChanged\|Unversioned` | Reviewer, Auditor | NEW `api_contracts.json` (`ENG-13`) | diffed against the previous revision's `api_contracts.json`: a removed/retyped field or parameter → `BreakingChanged`; a spec-declared deprecation tag → `Deprecated`; no version/spec file found for the route at all → `Unversioned` | Reviewer `drift` board, Auditor `route` table |
| REG-24 | `schemaMigrationSafety` | enum `Safe\|Destructive\|Unknown` | Architect, Consultant | NEW `schema.json` (`ENG-14`) | a migration's own DDL contains `DROP TABLE`/`DROP COLUMN`, or narrows a column type that `dataflow`'s existing read records still reference → `Destructive`; no DDL parsed for the migration → `Unknown` (never `Safe` by default) | Architect `scorecard`, Consultant `ownership`/`cochange` cross-link |
| REG-25 | `deadCodeScore` | integer (unreachable symbol count) | Architect, Auditor | `pdg.json` (`ENG-02`, already wired) | count of symbols with zero incoming `Calls`/`Reads`/`Writes`/`Returns` edges, excluding `appentry`-tagged entry points, test files, and symbols named in an exported-API manifest when one exists | Architect `scorecard` (7th column), Auditor `findings` cross-link |
| REG-26 | `busFactor` | integer | Consultant | `cochange.json --authors` (`ENG-08`, already specified) | smallest number of authors, ranked by trailing-180-day commit count descending, whose cumulative share reaches ≥80% of the module's commits | Consultant `ownership` panel (new column, not a replacement for `REG-10`'s top-committer) |
| REG-27 | `docCoverage` | number % | Architect, Reviewer | NEW `doc_index.json` (`ENG-21`) | public symbols with a docstring or an explicit README mention, divided by total public symbols in scope | Architect `scorecard`, Reviewer `driftboard` (Docs tab) |
| REG-28 | `docDriftDays` | integer, nullable | Reviewer, Architect | `quality.json` `drift.document[].drift_days` — **already real** (`quality.ts::buildDocumentDriftSpec`) | identity; `null` when no doc references the symbol/file at all (distinct from `0`, which means the doc is current) | Reviewer `driftboard` (Docs tab), Architect `scorecard` |
| REG-29 | `secretFindingCount` | integer | Auditor | NEW `secrets.json` (`ENG-19`) | identity (count of secret-scan hits naming this file) | Auditor `findings` (new finding kind, same panel — not a separate panel) |
| REG-30 | `cpuHotspotTier` | enum `Low\|Medium\|High` | Performance | `runtime.json datasets.cpu.top_functions[].cum_ms` — **already real** (`canned-charts.ts` chart `runtime-cpu`) | see `atlas-prd-performance.md` §7 (Performance-only, so specified in that PRD, not here, per §3's "owned by exactly one persona" rule) | Performance `hotspots` facet |

No two personas own the same new dimension; `REG-03` (drift) is the only shared one among `REG-01..12`,
and `REG-13..17` are deliberately shared across three or four personas each, because they expose the
*same already-shipped chart* into more than one persona's panel — that is wiring reuse, not a derivation
disagreement, since every reader resolves the identical `quality.json` field. `REG-18` and `REG-19` close
the last two one-tap-menu chart groups ("Promises," "Watching it run") that Part D below found with no
persona owner at all.

### B.3 `PersonaCubeDefault` — the type every persona PRD instantiates

```ts
export interface PersonaCubeDefault {
  personaId: PersonaId;
  hero: CubeRequest;                 // the hero panel's default cube — drawn on load, no user action needed
  facets: { panel: AtlasPanelId; dimension: RegistryId /* REG-xx */ }[]; // which panel's Facet button
                                      // (charter §16 "Facet" op) exposes which dimension, exactly
  crossFilterDimensions: RegistryId[]; // extends chart-filter.ts's FilterDimension union for this persona
  drillThrough: { cell: RegistryId[]; entity: "symbol" | "file" | "concept" | "finding" }[];
}
```

`CubeRequest.dimensions` in `hero` references dimensions by `REG-xx` id via
`DimensionRef::Field(FieldId("REG-xx"))` — the storage layer resolves `FieldId` strings against this
registry table (B.2 is the resolution table; no separate lookup needs building).

### B.4 Drill-through — the one new Storage method every persona PRD needs

```rust
impl RevisionReader {
    /// Charter §16 "Drill through": entities contributing to one cube cell.
    /// `cell` names the exact dimension-value combination the user clicked;
    /// returns the SAME logical population the cube aggregated, never a
    /// re-run of a different query — this is the honesty guarantee the
    /// spec (§15) already requires for all retrieval.
    pub fn drill_through(&self, request: &CubeRequest, cell: CellKey)
        -> StorageResult<Page<EntityRef>>;
}
```

`CellKey` is the tuple of `(dimension_id, value)` pairs identifying one cell — already implied by
`CubeRequest.drill_through: DrillThroughPolicy` in the existing spec; this PRD fixes the method name and
signature so five implementers do not invent five different ones.

### B.5 Cross-filter extension (`web/src/lib/chart-filter.ts`)

`FilterDimension` (today: `"severity" | "kind" | "file" | "effect" | "authority" | "certainty"`) gains
one member per `REG-xx` a persona activates (never all seventeen at once — each persona PRD §6 states
which subset it turns on). The one-active-filter-at-a-time semantics (toggle/replace, XFILTER's existing rule,
`chart-filter.ts:18-21`) is **unchanged** — a persona-specific dimension is just another bucket value in
the same single-slot filter, so the existing banner/disclosure/release UI needs zero new states.

---

## Part C — New export artifacts (schemas, exact)

Three schemas are needed by exactly one persona and are specified in that persona's own PRD
(`changed_files.json` → Reviewer PRD §7; `timeline.json` → Architect PRD §7; `trust_boundary.json` +
`policy.json` → Auditor PRD §7; `cochange.json --authors` + `journeys.json` → Consultant PRD §7). Two are
shared by more than one persona and are fixed **here**, once:

### C.1 `ENG-01` — `DecisionFact` (Fresher + Architect FlowChart, R4 diamonds)

```json
{ "sym": "qual_name:string", "file": "path:string", "line": "u32", "cond": "string (source text, truncated to 120 chars)" }
```
Emitted per adapter into `decisions.json`, one array entry per branch point the PDG's control dependence
reaches. Java and Python adapters first (both already parse full bodies — `atlas-ux-spec.md:201`); other
languages emit nothing for this file until their adapter lands — **never a guessed diamond** (ux-spec
§4.1 is the normative source; this PRD only fixes the file name and wire format).

### C.2 `ENG-02` — PDG production wiring (Fresher + Reviewer + Architect R4/Impact)

No new schema — `relation_graph_from_facts` (RFC-0022 §2.1) already builds the 10-relation PDG in tests;
this item is purely "wire it into the analyze job's export step," producing `pdg.json` (nodes = symbols,
edges = `{kind: ControlFlow|DataFlow|Reads|Writes|Calls|Returns, from, to}`). One task, shared by three
PRDs — built once, consumed three times.

### C.3 `ENG-03` — drift paired-evidence patch (Reviewer + Architect drift board)

`drift.json` claim objects gain `fields.alsoEvidence?: EvidenceRef[]` (ux-spec §13.1 patch 3, already
specified there verbatim) — restated here only so neither persona PRD re-specifies it differently.

### C.4 `ENG-12` — `sbom.json` (Auditor + Architect, `REG-20`/`REG-21`/`REG-22`)

```json
{ "generatedAt": "iso8601", "cveFeed": { "source": "string", "snapshotAt": "iso8601" },
  "packages": [ { "name": "string", "ecosystem": "string", "resolvedVersion": "string",
      "manifestFile": "string", "spdxLicense": "string|null",
      "cves": [ { "id": "string (CVE-...)", "cvss": "f32", "summary": "string" } ],
      "latestKnownVersion": "string|null", "latestReleaseAt": "iso8601|null" } ] }
```
The CVE feed is a **vendored, offline snapshot** (`cveFeed.snapshotAt` discloses its age) — never a live
API call at analyze time, consistent with the "no silent external dependency at scan time" rule every
other `cie` export already follows. `REG-20` (`dependencyVulnerability`), `REG-21` (`licenseRisk`) and
`REG-22` (`dependencyFreshness`) are pure reductions over this one file — one artifact, three dimensions,
same discipline as `quality.json` backing five dimensions already.

### C.5 `ENG-13` — `api_contracts.json` (Reviewer + Auditor, `REG-23`)

```json
{ "revision": "string", "endpoints": [ { "route": "METHOD PATH", "specFile": "string|null",
      "requestShape": "object|null", "responseShape": "object|null",
      "deprecated": "bool", "sourceKind": "openapi|protobuf|graphql" } ] }
```
Parses `openapi.yaml`/`*.proto`/`schema.graphql` when present; an endpoint with no matching spec file
gets `specFile: null` and `REG-23` resolves it to `Unversioned` — never inferred from the route table
alone. `BreakingChanged` is computed by diffing this revision's `requestShape`/`responseShape` against
the same endpoint's shape in the previous indexed revision (the same revision-pinning discipline
`storage-retrieval-api-spec.md` already requires everywhere else).

### C.6 `ENG-14` — `schema.json` (Architect + Consultant, `REG-24`)

```json
{ "tables": [ { "name": "string", "columns": [ { "name": "string", "type": "string", "nullable": "bool" } ],
      "sourceFiles": ["string"] } ],
  "migrations": [ { "file": "string", "appliedOrder": "u32",
      "destructiveOps": [ { "kind": "DropTable|DropColumn|NarrowType", "target": "string" } ] } ] }
```
Parses DDL, migration files and ORM model definitions (language-adapter-specific, same per-adapter
honesty rule as `ENG-01`'s `DecisionFact`: a language with no adapter support emits nothing for this
file, never a guessed schema). `REG-24` joins `destructiveOps` against `dataflow`'s existing read records
(already-wired `Reads` relation) — a migration dropping a column still named in a live read is
`Destructive`; a migration with no parsed DDL at all is `Unknown`, never defaulted to `Safe`.

### C.7 `ENG-15` — `dead_code.json` (Architect + Auditor, `REG-25`)

No new schema beyond the registry's own derivation rule (B.2a) — a pure reduction over `pdg.json`
(`ENG-02`, already wired), exported as `{ "deadSymbols": [ { "symbol": "qual_name", "file": "string",
"line": "u32" } ] }`. Entry points (`appentry`-tagged), test files and symbols named in an
exported-API manifest (when the repo declares one) are excluded before counting — the same exclusion
list a real dead-code tool needs, stated once here rather than re-derived per persona.

### C.8 `ENG-16` — `index_meta.json` (cross-persona, Part G.4)

```json
{ "headSha": "string", "indexedAt": "iso8601", "commitsBehind": "u32",
  "filesTotal": "u32", "filesParsed": "u32", "filesFailed": "u32",
  "languagesIndexed": [ { "lang": "string", "files": "u32", "parsed": "u32" } ] }
```
Already-computable from data every `cie analyze` run already produces (the coverage counters are the
same shape `report.coverage` already uses, `canned-charts.ts` chart `coverage` — this is that same fact,
exported where the dock header can read it without loading the whole report). Not a `REG-xx` dimension —
it drives the always-visible freshness disclosure in Part G.4, never a panel of its own.

### C.9 `ENG-17` — `digest.json` (cross-persona, Part G.2)

```json
{ "since": { "rev": "string", "at": "iso8601" },
  "newFindings": "u32", "resolvedFindings": "u32", "coverageDelta": "f32",
  "newOwners": ["string"], "busFactorDrops": [ { "module": "string", "from": "u32", "to": "u32" } ],
  "newVulnerableDeps": "u32", "driftClaimsNew": "u32" }
```
Graded **`REN-0`**: every field is a diff over artifacts that already exist by the time `REG-20..29`'s
engines have run once (`quality.json`, `sbom.json`, `cochange.json`, `drift.json`) — no new analysis
pass, a comparison job between two already-produced snapshots.

### C.10 `ENG-18` — `deploy_topology.json` (Auditor, extends `ENG-06`)

Same shape `stack.json` already uses for compose services (`apps`/`externals`/`routes`), populated from
`*.tf`, Kubernetes manifests and `Chart.yaml` instead of (or alongside) `docker-compose.yml`. `ENG-06`'s
trust-boundary BFS (foundations-adjacent, specified in `atlas-prd-auditor.md` §7) is written to walk
"every external in the topology source," not "every compose service" specifically — once this artifact
exists, the BFS needs no change, only a wider input set.

### C.11 `ENG-19` — `secrets.json` (Auditor, `REG-29`)

```json
{ "tool": "string (rule-set disclosed, e.g. gitleaks rules)", "findings": [ { "file": "string", "line": "u32",
      "rule": "string", "entropy": "f32|null" } ] }
```
Regex + entropy detection, the same class of heuristic `quality.json`'s own security scan already
discloses as "heuristics with file:line evidence, not a full SAST scanner" — this inherits that honesty
line verbatim rather than overclaiming precision a entropy-based secret scanner does not have.

### C.12 `ENG-20` / `ENG-21` — search and doc coverage (Part G.1; `REG-27`)

`ENG-20` is `QuerySpec`, specified in full in Part G.1 (it is cross-persona infrastructure, not one
persona's artifact, so its schema lives there rather than being duplicated here). `ENG-21` is
`doc_index.json` (`{ "symbols": [ { "symbol": "qual_name", "hasDocstring": "bool", "readmeMentions":
"u32" } ] }`) backing `REG-27` (`docCoverage`) — a straightforward per-adapter parse (docstring presence
is already visible to every language adapter that builds `report.symbols`), with the same "language with
no adapter support emits nothing" rule as every other per-adapter export in this document.

---

## Part D — Consolidation: the one-tap menu's retirement path

### D.1 The north star

**Every fact a user can get today from the one-tap chart menu, the mastery view, or the polyglot
explorer should eventually be reachable from inside the Atlas, through some persona's panel.** The Atlas
is not "one more view alongside the others" — it is meant to become the single screen, and once every
capability below has a persona-panel home, the one-tap menu, the mastery workflow and the standalone
polyglot views become redundant and can be removed. This is a direction for every future persona PRD to
check itself against, not a single task to finish in one pass: a persona PRD that adds a panel should
look here first, and a future audit of this document should shrink the "orphan" list in D.3 toward zero,
never grow it.

### D.2 The mapping — every one-tap chart group, its persona home

| One-tap group (`canned-charts.ts` / `quality.ts`) | Charts in it | Persona home | How |
|---|---|---|---|
| Problems | `files-most-problems`, `urgency-donut`, `problem-kinds`, `fix-right-away`, `clean-files` | Auditor, Architect | Auditor's `findings` panel (`REG-09`); Architect's `scorecard` (`REG-16`) |
| Promises | `who-wrote-promises`, `promise-list` | Reviewer | Drift board's Promises tab (`REG-18`, added this pass) |
| The map | `map-certainty`, `map-kinds`, `uml-diagram`, `call-flow`, `coverage` | Fresher | `hierarchy`, `flow`, `containers` panels (already the fresher's core six) |
| What the code does | `outside-world`, `things-per-file`, `file-does-what` | Fresher, Consultant | Fresher's `sizeintuition` strip; Consultant's `ownership`/condensed `ctx` |
| Dependencies | `deps-ecosystems`, `deps-manifests` | Architect | `depgraph` panel (`deps.json`, same source) |
| Watching it run | `runtime-coverage`, `runtime-cpu`, `runtime-race`, `runtime-overlap` | **Performance** (6th persona, `atlas-prd-performance.md`) | every chart in this group is this persona's panel set, verbatim — see D.3, orphan 1, now closed |
| Health | `concept-hierarchy`, `security-issues`, `vulnerabilities`, `duplications`, `hotspots`, `coverage-heatmap`, `code-drift`, `document-drift`, `most-complex` | Architect, Auditor, Reviewer, Consultant | `REG-13..17` (this pass) place every one of these in `scorecard`, `findings`, or `driftboard` |
| Quality metrics | defect density, MTBF, test coverage, cyclomatic, arrival rate, code churn, crash rate, MTTR | **Performance** | the project-level health strip (`ENG-17 digest.json` relatives; Part G) lives in Performance's hero caption, the one place in the Atlas already built around project-wide, not per-file, numbers |

### D.3 Remaining orphans — status after the capability-gap review

Orphan 1 (CPU hotspots, race/lock, thread overlap, the eight classic quality metrics) is **closed**: the
new Performance / SRE persona (`atlas-prd-performance.md`) owns every chart in both orphaned groups,
built almost entirely at `REN-0` cost since all of it is already-real `runtime.json`/`quality.json` data
with no Atlas panel yet — the same situation Architect's `scorecard` was in before this pass, closed the
same way. What was orphan 2 in the first draft of this document (project-level quality metrics with no
per-file shape to live in) is now simply Performance's hero caption, the one place in the Atlas already
built around whole-project numbers rather than per-file rows.

A broader completeness review (against "code intelligence tool" as a category, not just against the
charter's five named personas) found orphans this pass **does not** close, listed honestly rather than
silently deferred:

1. **Search / navigation** — no panel answers "find this symbol" or "who calls this." Part G.1 specifies
   the primitive (`QuerySpec`, `ENG-20`); it is cross-persona infrastructure, not a panel, and is not
   built in this pass.
2. **Supply chain, API contracts, schema safety, dead code, secrets** — `REG-20..25`, `REG-29` are now
   *registered* (B.2a) with their derivation rules fixed, but the `ENG-12..15`/`ENG-19` artifacts behind
   them are specified (Part C) and not built. A future PRD wires them into Auditor's `findings` and
   Architect's `scorecard` — both panels are already shaped to take new columns/finding-kinds, so this is
   closer to `REG-13..17`'s wiring-only cost than to a from-scratch panel.
3. **Digest, saved views/permalinks, index-freshness** — specified as primitives in Part G, not built.
4. **IaC/deployment topology beyond Docker Compose** — `ENG-18` specified, not built; `ENG-06`'s trust
   boundary BFS is written to consume whichever topology source exists, so this is additive, not a
   rework, once `ENG-18` lands.

Closing these is explicitly in scope for whoever picks up the next persona PRD or infrastructure change;
this document records them so "the Atlas replaces every menu" — and, more broadly, "the Atlas is a
complete code intelligence surface" — stays a tracked, falsifiable claim rather than an aspiration.

---

## Part E — The universal selection / filter / source contract

### E.0 Why this part exists

A review of the first working mockup found the same mistake repeating: a panel got wired into a
persona's cross-filter (a `REG-xx` dimension), and a *different* panel in the same persona, showing the
same kind of fact, was quietly left out — Reviewer's `entries` and `dataflow` panels had no filter
watcher while `driftboard` and `testconfidence` did; Architect's `scorecard` was added without one.
Each gap was individually fixable, but fixing them one at a time is exactly how the next one gets
introduced — a developer adding a new panel six months from now has no reason to know that *every*
panel is supposed to participate, because nothing said so as a rule.

This part states that rule once, as a contract every element in the Atlas satisfies by construction, not
by a developer remembering to wire it: **there is no such thing as an element that only displays a fact
and does not also let the user select it, filter by it, and open it.** Three gestures, one meaning each,
defined for every element kind the Atlas renders — table row, bar, tree row, diagram node, diagram edge,
donut segment, facet control. A facet "chip" button is not a special case under this contract; it is an
ordinary element whose `AtlasElementRef` happens to carry one dimension value. This section **supersedes**
Part B's `facets`/`crossFilterDimensions` framing wherever the two disagree — Part B's registry
(`REG-01..19`, the dimensions themselves and their derivation rules) stays exactly as specified; what
changes is the *mechanism* that reads and writes them, from a one-active-value-per-section toggle to the
richer model below.

### E.1 The three gestures, exactly

| Gesture | Effect | Reverses on |
|---|---|---|
| **Click, element not currently selected** | **Select.** Add a chip to the dock (CHIP, unchanged from ux-spec §2.3). For every `{dim, value}` pair the element's `AtlasElementRef.dims` carries, add `value` to that section's active set for `dim` (E.2) — the element now contributes to slice/dice/filter. | the same element clicked again, or chip-cap eviction (§A.6-adjacent: oldest chip evicted first), or a manual chip `✕` |
| **Click, element currently selected** | **Deselect.** Remove its chip. Withdraw every `{dim, value}` pair it contributed — if no other still-selected element contributes that same `{dim, value}`, the value leaves the active set; if a dimension's active set becomes empty, that dimension stops filtering anything (full release, same end-state the old single-value "click again to release" rule gave, generalized to many simultaneous values). | — (this *is* the reversal) |
| **Double-click** | **Open the source** (POPUP, ux-spec §2) when the element carries a `loc`; pre-fill the composer (ASK) when it doesn't. Never changes selection — see E.3 for why nothing additional has to be built to guarantee that. | — (not a toggle; it is a one-shot action every time) |

An element with no `loc` and no `dims` (decorative text, a section caption) is simply not given an
`AtlasElementRef` at all and is inert — the contract does not require fabricating a dimension value or a
location that doesn't exist; it requires that **every element which has one exposes it**, never that
every pixel must do something.

### E.2 The active-filter model: sets, not a single value

Earlier drafts of this document modeled one persona-section's filter state as a single `{dim, value} |
null` — "one active filter at a time, click again to release." That was correct for a UI where only
designated facet buttons could set a filter. Once every element can, a user may have several different
elements selected at once (chips are already plural, capped at 6 — foundations §A.6-adjacent), possibly
asserting values across several dimensions, or several values of the *same* dimension (two severities
at once, "show me Critical or High"). The model generalizes to:

```ts
// one of these per persona section, held by the conductor (RFC-0023 terminology)
type ActiveFilters = Map<DimensionId, Set<DimensionValue>>;
```

A watcher element (`AtlasElementRef.watches: DimensionId[]` — the dimensions it should dim against,
independent of whether it also contributes a value) is dimmed when, for **every** dimension in its
`watches` list that has a non-empty active set, **none** of its own asserted values for that dimension
fall in that set. Put plainly: **multiple active values within one dimension are OR'd ("Critical or
High"); multiple active dimensions are AND'd ("High severity AND Domain layer")** — this is literally
"slice, then dice": the first selected element's dimension slices the population, a second selected
element on a *different* dimension dices that slice further. An element that carries no value at all for
a watched dimension (e.g., a route row has no `complexityScore`) is **not dimmed by that dimension** — it
is only ever dimmed by a dimension it has an opinion about and disagrees with.

This is strictly a generalization, not a breaking change: with exactly one element ever selected at a
time, it reduces to the original single-value toggle every earlier persona PRD describes.

**A selected element can still end up dimmed.** If the user selects a High-risk file and then separately
selects a Low-risk file, the High-risk file now fails the Low-risk file's own active constraint (and
vice versa) — both stay selected (both still have chips), but each shows dimmed against the other's
contribution. This is not a bug to special-case away: E.2's AND/OR rule applies uniformly, including to
a selected element's relationship to a *different* selected element's contribution, because the
alternative — exempting "my own selections" from dimming — would make the filter state silently lie
about what is actually in view. The dimming is the honest disclosure that the two selections conflict.

### E.3 Double-click needs no debouncing — the browser already serializes it

A naive worry: "if a single click selects, won't a double-click toggle selection twice and then also
open the source, leaving things in a confusing state?" No — and no timer-based double-click
disambiguation needs to be built to prevent it. The DOM dispatches, for a physical double-click, exactly
this synchronous sequence before any repaint: `click`, `click`, `dblclick`. Because `click` toggles
selection, two clicks toggle it **twice** — select→deselect if it started unselected, deselect→reselect
if it started selected — landing back exactly where it started, synchronously, before the browser ever
paints the intermediate state. `dblclick` then fires its own handler (open source / ask), independent of
whatever selection toggling just happened. Net effect, exactly as specified in E.1: **double-click opens
the source and never changes selection**, achieved with zero custom timing code — two ordinary `click`
listeners and one ordinary `dblclick` listener, nothing more. A developer tempted to add a
"was this really a single click? wait 300ms to find out" debounce is solving a problem this contract
does not have; §E.1's table is the complete specification, and this paragraph is the proof it needs no
elaboration.

### E.4 `AtlasElementRef` — the one shape every element emits

```ts
export interface AtlasElementRef {
  id: string;                                  // stable identity — dedupes chips, dedupes dim contributions
  label: string;                                // the chip's visible text
  loc?: { file: string; line?: number };        // present → double-click POPUPs here; absent → double-click ASKs
  dims?: { dim: string /* REG-xx id */; value: string }[]; // values this element's identity asserts — zero, one, or several
  watches?: string[];                           // REG-xx ids this element dims against; usually dims.map(d => d.dim)
                                                  // but can watch a dimension it has no value for (never true for
                                                  // this element, so it is dimmed whenever that dimension is active
                                                  // with no overlapping value — e.g. a row explicitly tagged "no
                                                  // security findings" watches REG-16 and always fails to match)
}
```

A facet button (the old `.fchip[data-xf-val]`) is now just an element whose `dims` is a single pair and
whose `watches` is usually empty (a facet control filters others; it is not itself filtered) — no special
component, no separate code path from a table row.

### E.5 The coverage checklist — every panel, every persona, audited

This is the "never missed" guarantee made checkable rather than asserted. Every panel in every shipped
persona PRD is listed here with whether its elements currently emit `AtlasElementRef`s. A PR that adds a
panel and leaves its row in this table unchecked fails review on that basis alone — this table is the
test, not a status report written after the fact.

| Persona | Panel | Emits refs? | `dims` it asserts | `watches` |
|---|---|---|---|---|
| Fresher | all six + hero | **No, by design** | — | — (crossFilterDimensions `[]`, charter's "guided path, not a wall"; `AtlasElementRef` is still populated for `loc`/chip purposes — double-click still opens source — only `dims`/`watches` are omitted) |
| Reviewer | `impact` (hero) | Yes | `REG-01` | — |
| Reviewer | `call`, `entries`, `dataflow` | Yes | — | `REG-01` |
| Reviewer | `driftboard` | Yes | `REG-03` | `REG-01` |
| Reviewer | `testconfidence` | Yes | `REG-02`, `REG-14`, `REG-17` | `REG-01` |
| Reviewer | `ctx` (condensed) | No (a summary card, not a row list — nothing to select) | — | — |
| Architect | `depgraph` (hero), `boundedcontexts` | Yes | `REG-04` | `REG-04` |
| Architect | `scorecard` | Yes | `REG-04`, `REG-13`, `REG-14`, `REG-15`, `REG-16` | `REG-04`, `REG-13` |
| Architect | `duplication` | Yes | `REG-06`, `REG-04` | `REG-04` |
| Architect | `driftboard` (reused) | Yes | `REG-03` | `REG-04` |
| Architect | `timeline` | Yes (`SCOPE(rev,n)`, a different axis — §3 of ux-spec; carries no `dims`/`watches`, selection there means something else and is exempt by the same "nothing to assert" rule as `ctx`) | — | — |
| Architect | `risktreemap` | Yes | `REG-05`, `REG-13` | `REG-04` |
| Auditor | `trustboundary` (hero), `route`, `externals` | Yes | `REG-07` | `REG-07` |
| Auditor | `findings` | Yes | `REG-09`, `REG-16` | `REG-07` |
| Auditor | `policymatrix` | Yes | `REG-08` | `REG-07` |
| Auditor | `dataflow` | Yes | — | `REG-07` |
| Consultant | `containers` (hero), `ownership` | Yes | `REG-10` | `REG-10` |
| Consultant | `cochange` | Yes | `REG-11` | `REG-10` |
| Consultant | `coverage` | Yes | `REG-02` | — |
| Consultant | `journeys` | Yes | `REG-12` | `REG-10` |
| Consultant | `ctx` (condensed) | No (same reason as Reviewer's) | — | — |

Any new panel in any future persona PRD adds a row here in the same change set. A row that says "No" for
any reason other than "genuinely nothing to select" (a summary card, a free-text caption) is a defect,
not a style choice.

### E.6 What this replaces

- The old two-track mockup (a `.fchip[data-xf-val]` facet button that set one global `{dim,value}`, and
  a separate `data-xf-watch`/`data-xf-match`-only element that could only ever be dimmed, never itself
  select anything) is retired. Every element uses the single `AtlasElementRef` shape (E.4); the
  implementation detail of how that is spelled in markup (`data-ref-id`, `data-ref-dims`, …) belongs to
  the mockup/component layer, not this document — this document fixes the *contract*, not the attribute
  names.
- `chart-filter.ts`'s existing one-active-value rule (`storage-retrieval-api-spec.md` era) generalizes to
  `ActiveFilters` (E.2) without discarding it — a session with exactly one chip selected behaves
  identically to the old rule, byte for byte.
- Every persona PRD's §6 "Slice/dice/filter" table stays correct as written for *which* `REG-xx`
  dimensions exist and *which* panels use them; this part only fixes *how many* elements may set them at
  once and guarantees, via E.5, that no panel is silently excluded.

---

## Part F — Non-functional requirements

Absent from the first five PRDs entirely — a completeness review correctly flagged that "code
intelligence platform" carries NFR obligations none of them stated. These apply across every persona;
a persona PRD that violates one of these is non-compliant with that PRD alone, not with a budget that
only existed in someone's head.

### F.1 Performance budgets

- First panel interactive in **<800ms** on a 100k-file repo (cold load, index already built).
- A cube query (`drill_through`, foundations §B.4) resolves in **<300ms**.
- `scorecard` and any other per-file table **virtualizes above 2,000 rows** — rendering every row of a
  monorepo-scale table is the specific failure mode F.7 names; this is the UI-side fix.

### F.2 Accessibility

- Every panel is keyboard-navigable; the closed verb vocabulary (ux-spec §3) and this document's
  universal contract (Part E) both have keyboard equivalents — `Enter`/`Space` = click (select/open per
  E.1), a distinct key (not a timing-based double-press) triggers the double-click action, since a
  keyboard has no native "double-click" gesture to borrow E.3's free disambiguation from.
- WCAG 2.1 AA contrast, including on `.dimmed` states — a dimmed-but-still-selected element (foundations
  §E.2's documented edge case) must stay distinguishable from a dimmed-and-unselected one for a
  low-vision user, not only by opacity.

### F.3 Internationalization

User-facing strings (panel titles, `why` captions, `TELL` disclosures, empty-state text) are externalized
from day one — every example string in every persona PRD is illustrative content, not literal UI copy to
ship unlocalized. No PRD in this set specifies a translation pipeline; this is named as a gap, not solved
here.

### F.4 Auth / multi-tenancy

Not addressed anywhere in this document set and not solved here either — flagged explicitly rather than
silently assumed. If the Atlas is deployed for more than one team or more than one repository's worth of
people, "who can see which repo, which revision, which persona's view of it" needs its own contract
before `ENG-12..21`'s richer data (CVEs, secrets, schema internals) makes the question sharper than it is
for `REG-01..19`.

### F.5 Privacy

`ENG-08` (`cochange.json --authors`, backing `REG-10` `owner` and `REG-26` `busFactor`) stores raw commit
author logins. That is personal data under GDPR/CCPA in some deployments. Required before `ENG-08` ships
to any multi-tenant or EU-facing deployment:
- a documented retention window,
- an anonymization mode (hash or role-label authors instead of logins) the deployer can switch on,
- a deletion path when an author asks to be removed from historical ownership data.
None of this is built in this pass; it is named here so `ENG-08`'s schema doesn't ship first and get a
privacy contract bolted on after real data is already stored in its un-anonymizable raw-login shape.

### F.6 Git history correctness

Every dimension reading per-file git history (`REG-10` `owner`, `REG-05` `churnBucket`, `REG-11`
`coChangeCluster`, `REG-17`/`REG-28` doc/code drift, `REG-26` `busFactor`) inherits three correctness
risks that no PRD in this set names on its own:
- **Renames**: without `git log --follow`, a `git mv` resets authorship and co-change history as if the
  file were newly created. `ENG-08` and `cochange.json`'s existing collection must use `--follow`
  (or the equivalent rename-tracking the underlying VCS library offers); a collector that doesn't is
  silently wrong, not merely less precise.
- **Squash/rebase merges**: `cochange.support` counts commits; a squash-merge-heavy repo collapses many
  real co-change events into one commit, making genuinely coupled files look unrelated. Consultant's
  `cochange` panel (`atlas-prd-consultant.md` §5.2) needs a `TELL` disclosure when the repo's merge
  strategy is detected as squash-heavy (a high ratio of merge commits with exactly one parent changing
  many files), not a silent undercount.
- **Generated and vendored paths**: `node_modules/`, `target/`, `dist/`, and protobuf/codegen output
  pollute `ownership`, `churnBucket`, `coverage` and `duplication` alike. A fixed, documented exclusion
  list (gitignore-aware, plus a short list of common generated-output directory names) must run before
  any of `REG-05/06/10/11/14/15` are computed — this is normative for `ENG-08` and for the existing
  `quality.json` collectors alike, not optional per-deployment configuration.

### F.7 Scale

- A monorepo at 1M files breaks two things this document otherwise treats as free: `scorecard`'s row
  count (F.1 names the UI-side virtualization fix) and the cube's client-side query cost (`chart-filter.ts`
  scans in memory today — `storage-retrieval-api-spec.md`'s server-side `CubeRequest`/`drill_through`
  path, already specified, is the actual fix; F.1's 300ms budget is not achievable client-side at this
  scale without it).
- Re-indexing must stay incremental (the existing `cie-incremental` content-hash/reverse-dependency
  machinery, already real) — nothing in `ENG-12..21` may require a full re-scan on every commit; each new
  collector follows the same incremental-by-construction discipline `PRD-v1.0.md` already states as a
  project-wide principle.

### F.8 What this part deliberately does not resolve

`REG-xx` type-resolution (parameter/return types) is out of scope — `pdg.json`'s `Calls` edges name
*which* symbols call which, never their resolved types ("what's the parameter type of this function" is
unanswerable today). Multi-repo federation stays out of scope per `storage-retrieval-api-spec.md` §15,
with the same note that document already makes: a future `RepoRef` on every entity is the intended v2
mechanism, named so schemas don't ossify against it, not built here.

---

## Part G — Cross-persona primitives

None of these are a panel inside one persona's grid — each is infrastructure every persona's Atlas sits
on top of. They are specified here, once, so no persona PRD invents its own search box or its own URL
scheme.

### G.1 Search / navigation — `QuerySpec` (`ENG-20`)

The cube (`storage-retrieval-api-spec.md` §16) aggregates; it was never meant to *locate* a single
symbol, and nothing in this document set gives the Atlas a find/go-to-usages primitive — the single
largest gap a completeness review surfaced, since search is the primary interaction in every comparable
tool (an IDE, Sourcegraph, Glean).

```rust
struct QuerySpec {
    text: Option<String>,              // fuzzy symbol/name match
    kind: Option<EntityKind>,          // function | class | module | route | concept | …
    lang: Option<String>,
    path_glob: Option<String>,
    calls: Option<SymbolRef>,          // "all usages of X" — walks the existing Calls relation
    reads_writes: Option<SymbolRef>,   // walks existing Reads/Writes relations
}
impl RevisionReader {
    pub fn search(&self, q: &QuerySpec, page: PageRequest) -> StorageResult<Page<EntityRef>>;
}
```

Results are `EntityRef`s — the same type `drill_through` (foundations §B.4) already returns — so a
search result's click behavior is `SCOPE`/`POPUP`/`CHIP`, the existing closed verb vocabulary, never a
new interaction grammar. Rendered as a search box in the dock header (G.3), present in every persona,
since "find a symbol" is not persona-specific the way a cube dimension is.

### G.2 The digest strip — `ENG-17`

A collapsible strip in the top bar (not inside any persona's grid) summarizing what changed since the
user's last visit: `{ newFindings, resolvedFindings, coverageDelta, newOwners, busFactorDrops,
newVulnerableDeps, driftClaimsNew }` (`ENG-17`'s schema, Part C.9). Each field click-throughs to its
owning panel (a `newVulnerableDeps` count opens Auditor's `findings`, filtered to `REG-20` entries newer
than `since`). Graded `REN-0` — pure diff over artifacts that already exist once `REG-20..29`'s engines
have run at least twice.

### G.3 Saved views and permalinks

Chips are explicitly "the user's, not the persona's" (`atlas-prd-consultant.md` E-3) but nothing lets a
user hand a colleague the exact state they're looking at — the single highest-value feature for the
Consultant persona specifically, which bills by the hour and exists to hand off findings. The URL encodes:

```
{ layoutId: PersonaId, scope: ScopeRef, chips: AtlasElementRef["id"][],
  activeFilters: ActiveFilters (§E.2), expandedPanel: AtlasPanelId | null, dockOpen: boolean }
```

A `Save view` action writes a named snapshot of the same shape into the existing per-persona
`sessionStorage` memory mechanism (`atlas-presets.ts`'s `PersonaMemory`, already real) promoted to a
durable, named, shareable record; `Share` produces the URL directly without saving anything. **No new
engine work** — this is pure serialization of state every persona PRD already has a name for.

### G.4 Index freshness — `ENG-16`

Always visible in the dock header (never a panel a user must think to open): "Indexed 3 commits ago ·
99.2% parsed (Rust: 87%)," sourced from `index_meta.json` (Part C.8). Clicking it `TELL`s the per-language
breakdown. This is the structural fix for the risk the completeness review named precisely: a tool that
silently shows 60%-parsed data as if it were complete is worse than one that refuses — every panel in
every persona already states its own evidence/coverage per the honesty contract (`atlas-charter.md` §2),
but nothing today states *how fresh and how complete the index itself is*, independent of any one panel.

---

## Part H — Correctness caveats in shipped panels (tracked, not silently inherited)

Not missing features — ways an already-specified panel can produce a confidently wrong answer if its
PRD is implemented literally without reading this part. Each line names the governing NFR (Part F) and
the panel(s) it applies to; a persona PRD does not need to restate these, it inherits them by reference.

| # | Risk | Governs | Affected panels |
|---|---|---|---|
| 1 | Git rename/move resets authorship and co-change history without `--follow` | F.6 | Consultant `ownership`, `cochange`; Architect `scorecard`'s churn column |
| 2 | Squash/rebase-merge repos undercount co-change support | F.6 | Consultant `cochange` |
| 3 | Generated/vendored paths pollute ownership, churn, coverage, duplication | F.6 | Architect `scorecard`/`duplication`; Consultant `ownership`/`coverage` |
| 4 | Monorepo scale breaks per-file table rendering and client-side cube cost | F.1, F.7 | Architect `scorecard`; any cube-backed facet across all five personas |
| 5 | Ownership data is personal data (PII) in some jurisdictions | F.5 | Consultant `ownership`; `REG-10`, `REG-26` |
| 6 | No resolved-type information — "what's this parameter's type" is unanswerable | F.8 | Architect `depgraph`; any panel implying type-level analysis |

---

## 1. Naming & versioning conventions

- New export files live beside today's artifacts at the same root the CLI already writes to (same
  directory as `report.json`/`quality.json`/`stack.json`/`drift.json`/`cochange.json`/`overlap.json`) —
  no new directory convention.
- Every new file name is **lowercase, singular-collection, `.json`**: `decisions.json`, `pdg.json`,
  `trust_boundary.json`, `policy.json`, `journeys.json`, `changed_files.json`, `timeline.json`. Where a
  file *extends* an existing artifact (cochange authors) it is a new **field** on the existing file, not
  a new file — avoids a second source of truth for the same entity.
- Every derived (non-verbatim) field carries a certainty tag consistent with the existing honesty
  contract (`Fact` when read from a repo-authored file like `.cie/layers.yaml`; `Good guess` when
  heuristically inferred) — this is not new policy, it is the existing contract (`atlas-charter.md` §2
  item 3) applied to the new fields.

## 2. Cross-persona collision check (resolved, not left open)

- `driftStatus` (REG-03) is read by both Reviewer and Architect from the same field — confirmed no
  divergence possible (B.2).
- `AtlasPanelId` additions across the five persona PRDs are disjoint except where a persona PRD
  explicitly says "reuse `<other persona>`'s panel" (e.g. Consultant reuses Fresher's six panels
  verbatim per charter §9) — each new panel id is defined in exactly one PRD and imported by id,
  never redefined.
- `AtlasLayout.columns` choices (4/5/6) are independent per persona — no shared layout file, no risk of
  one persona's layout change silently resizing another's.

## 3. What a persona PRD may NOT do

- Invent a new grid mechanism (must use `AtlasLayout`/`PanelSlot`, Part A).
- Invent a new dimension without adding it to the `REG-xx` registry here first (a persona PRD may add
  rows to this table in the same change set, but may not define a persona-local dimension type).
- Redefine `drill_through`, `DecisionFact`, or the PDG export — reference them by id.
- Leave a derivation rule as "TBD" or "to be decided by the implementer" — every bucket boundary in this
  document and in each persona PRD is a concrete number, chosen now, changeable later via a follow-up
  PRD, never via silent implementer judgment.
- Invent a second selection/filter mechanism alongside Part E's `AtlasElementRef` contract, or ship a
  panel whose elements don't emit one — Part E.5's coverage checklist is the enforcement point; a new
  panel adds a row there in the same change set, same discipline as the `REG-xx` registry rule above.
- Add a new persona without adding its own row to Part D.2's mapping table and, if it closes a Part D.3
  orphan, updating that orphan's status there — `atlas-prd-performance.md` is the worked example.
