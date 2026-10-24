# PRD — Atlas for the Reviewer / Staff Engineer

**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `reviewer`
**Reads first:** `atlas-persona-prd-foundations.md` (`LAYOUT-*`, `REG-01`/`REG-02`/`REG-03`, `API-1`)
**Builds on:** `atlas-charter.md` §5, `atlas-ux-spec.md` §6 (choreography + change-context, §6.1)

## 1. Problem

`atlas-presets.ts` ships a reviewer preset today (`hero: "svcentry"`, facts-only forced on,
`planOrder: [svcentry, appentry, flow, dataflow, components, hierarchy, sequence]`) — but the charter's
own ★★★ items for this persona, **impact graph** and **drift board**, are not in `AtlasPanelId` at all.
A reviewer opens "Reviewer" expecting "what can this diff break before I read it" and instead gets a
reordered version of the fresher's eight panels with facts-only switched on. The one thing this persona
is named for is missing.

## 2. Goals

- **G1.** Opening the Reviewer preset with a change context set answers "what can this diff break" and
  "does it violate a documented promise" on one screen, with evidence, before the diff is opened.
- **G2.** Impact and drift get canvas/dense real estate; the fresher's six panels demote to a single
  collapsible summary, since a reviewer rarely needs them (charter §10 matrix: ★ or blank for this
  persona on containers/tree/components).
- **G3.** The reviewer's slice/dice vocabulary (blast-radius tier, test confidence, drift status) is a
  first-class cube, filterable and drillable, not a static table.

## 3. Non-goals

- No new static-analysis algorithm — blast radius and drift status are **derivations over existing
  facts** (`/api/impact`, `drift.json`), never a new engine capability.
- No cross-revision diff beyond the already-scoped change context (`atlas-ux-spec.md` §6.1) — comparing
  two arbitrary revisions is the Architect persona's `SCOPE(rev,n)` timeline, not this one.

## 4. Information architecture — the custom layout

`AtlasLayout.id = "reviewer.default"`, `columns = 5`, `rowHeightPx = 120`. **Hero changes from
`svcentry` to the new `impact` panel** — this PRD corrects `atlas-presets.ts` accordingly (§8, FR-1).

`AtlasLayout.id = "reviewer.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx = 110`,
`dock = {defaultOpen: true, widthPx: 280}`.

**Second correction, superseding the first:** an earlier draft of this PRD first put `impact` inside the
grid (a bug), then "fixed" that by giving it a separate fixed-width hero column outside the grid — which
foundations §A.3 invariant 6 now identifies as the *bigger* mistake: a column exactly as tall as one
chart, with nothing able to use the empty space below it. The corrected shape puts `impact` back inside
the one grid, as the `isHero` cell, sized to 5 of 8 columns across 3 rows, with `call` (callers) filling
the other 3 columns in those same rows — a natural pairing, since "who to notify" belongs right beside
"what breaks."

```
templateAreas (desktop, 8 cols):
"r4    r4    r4    r4    r4    r4    r4    r4   "
"hero  hero  hero  hero  hero  call  call  call "
"hero  hero  hero  hero  hero  call  call  call "
"hero  hero  hero  hero  hero  call  call  call "
"drift drift drift drift test  test  test  test "
"drift drift drift drift test  test  test  test "
"entr  entr  entr  entr  data  data  data  data "
"ctx   ctx   ctx   ctx   ctx   ctx   ctx   ctx  "
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `impact` (`isHero: true`) | `canvas` | 330 | the headline question; 5/8 width, no longer a lone full-height column |
| `call` | `flow` (filtered to affected callers only) | `card` | 330 | beside the hero, not a thin strip under it — "who to notify" read together with "what breaks" |
| `drift` | `driftboard` (NEW) | `card` | 220 | wide — FMEA-style rows need horizontal room |
| `test` | `testconfidence` (NEW) | `table-dense` | 220 | |
| `entr` | `appentry`+`svcentry` combined, compact | `table-dense` | 220 | demoted from hero (charter §10: ★★ not ★★★ for reviewer) to a dense strip |
| `data` | `dataflow`, **writes-first ordering** | `card` | 220 | writes are scanned left-to-right, not drilled deeply here |
| `ctx` | `components`+`hierarchy`+`sequence`, condensed | `card` | 120 | one counts-only card + "View full panel" button that opens the existing Maximize2 expand-to-fill mode (reuses the existing mechanism — no new UI for "expand a condensed card") |

`templateAreasMd`: `["r4 r4", "hero hero", "call call", "drift test", "entr data", "ctx ctx"]`.
`templateAreasBase`: linear `planOrder`.

## 5. New panels

### 5.1 `impact` (replaces `svcentry` as hero)

- **Chart:** `uml` rendering of the impact closure (Tier A/B — `/api/impact` route already exists;
  E-graph layout is Tier B per `chart-inventory.md:98`). Nodes = affected symbols/concepts; a side-table
  lists the minimum test set (charter §5 row 1).
- **Scope source:** the change context (§6, unchanged from `atlas-ux-spec.md` §6.1) when set; else the
  whole-system impact of the currently scoped symbol (empty state when nothing is scoped: "pick a change
  or click a symbol to see its blast radius" — no invented closure).
- **Click:** impact box → `POPUP` exact line + `CHIP`; affected-concept edge → `FOCUS` lights the tree
  branch (ux-spec §6 unchanged); min-test-set row → `POPUP` the test file/function + `CHIP`.

### 5.2 `driftboard` (NEW)

- **Chart:** `hbar`/FMEA-style table by `driftStatus` (`REG-03`) — data is `drift.json`, already Tier A
  data per `chart-inventory.md:34`; only the chart is new.
- **Click:** a row → **paired popup** (`atlas-ux-spec.md` §2.2, DOC|CODE columns) — this is the only
  panel in the whole Atlas that needs the paired popup today; no new UI, the mechanism is already spec'd.

### 5.3 `testconfidence` (NEW)

- **Chart:** `table`, one row per affected file, columns = file, `testConfidence` (`REG-02`), covering
  test names (when `Covered`/`Partial`).
- **Click:** row → `POPUP` the test file + `CHIP`.

## 6. Slice/dice/filter — the reviewer's cube

*The selection/filter mechanism below is governed by `atlas-persona-prd-foundations.md` Part E (the
universal selection/filter/source contract): every element, not only facet buttons, selects, filters and
double-click-opens by the same `AtlasElementRef` shape, and filter state is a set per dimension, not a
single toggled value. The table below still states which `REG-xx` dimensions and panels apply here —
that part is unchanged by Part E.*

| Field | Value |
|---|---|
| `hero` cube | `dimensions: [{id:"blast", dimension: Field("REG-01")}]`, `measures: [{name:"affectedCount", agg:"count"}, {name:"testCount", agg:"count", field:"minimumTestSet"}]`, `layout: Cells` |
| `facets` | `{panel:"impact", dimension:"REG-01"}`, `{panel:"testconfidence", dimension:"REG-02"}`, `{panel:"driftboard", dimension:"REG-03"}` — each panel's header gets a Facet chip row (charter §16 "Facet": available values + counts, e.g. "Critical (3) · High (11) · Medium (22) · Low (40)") |
| `crossFilterDimensions` | `["REG-01", "REG-02", "REG-03"]` — clicking a `blastRadiusTier` bucket on the impact hero cross-filters every panel that names an affected file, not just `driftboard`/`testconfidence`: the `entr` (entries) rows and the `dataflow` write rows watch `REG-01` too, since a file can be entry-point-affected and disk-affected independently of whether it has a drift claim or a test. Omitting those two was the gap a review of the first mockup caught — a panel with no watcher silently looks unaffected by a filter that in fact applies to it (XFILTER, `chart-filter.ts`, one-active-filter-at-a-time semantics, unchanged rule) |
| `drillThrough` | `{cell: ["REG-01"], entity: "symbol"}` — clicking a blast-radius-tier cell calls `drill_through` (`API-1`, foundations §B.4) and opens the row list as the existing filtered-table banner (`chart-filter.ts` pattern, reused verbatim) |

## 7. Engine requirements

- No new facts. `/api/impact` and `drift.json` already exist; `REG-01`/`REG-02` are pure derivations over
  their existing output (bucket thresholds fixed in foundations §B.2 — not re-decided here).
- `ENG-03` (drift `alsoEvidence` patch, foundations Part C) is required for the paired popup in §5.2 —
  shared with Architect, built once.

## 8. Functional requirements

- **FR-1.** `atlas-presets.ts` reviewer preset: `hero` changes from `"svcentry"` to `"impact"`;
  `planOrder` becomes `[impact, flow, driftboard, testconfidence, appentry, svcentry, dataflow,
  components, hierarchy, sequence]` reflecting the new grid's reading order (§4). This is a one-line
  change to an existing file, not a new preset.
- **FR-2.** The change-context picker (`atlas-ux-spec.md` §6.1, unchanged) seeds the `impact` hero's
  scope exactly as today's spec describes — this PRD adds no new seeding path.
- **FR-3.** `driftboard` and `testconfidence` render their honest empty states when no change context and
  no symbol scope is active ("pick a change to scope the review" — ux-spec §6.1's existing empty-state
  text, reused verbatim for both new panels, not reworded per-panel).
- **FR-4.** Cross-filtering (`REG-01/02/03`) obeys the existing XFILTER banner/disclosure/release
  contract (`chart-filter.ts`) with zero new UI states.

## 9. Edge cases

- **E-1.** Change context set, but the diff touches zero symbols the engine indexed (e.g. config-only
  change) — `impact` hero shows "0 affected symbols — this change doesn't touch indexed code," never a
  blank canvas.
- **E-2.** `drift.json` has no claims near the touched files — `driftboard` shows "no documented
  promises found near this change" (distinct from "0 affected," since drift and impact are independent
  facts — never conflate an empty drift board with low risk).
- **E-3.** `blastRadiusTier` facet clicked, then change context cleared — cross-filter clears
  automatically (filters are scoped to the active population; clearing the population clears the filter,
  same rule XFILTER already applies when scope changes elsewhere).

## 10. Acceptance criteria

- [ ] Reviewer hero renders `impact`, not `svcentry`.
- [ ] Setting a change context populates impact/drift/test-confidence from the same diff, verified by
  file-level cross-reference (no two panels disagree on which files are "touched").
- [ ] Drift row opens the paired popup with both DOC and CODE columns when both evidence refs exist;
  single-location fallback (`Unverified` chip) when they don't (ux-spec §2.2 rule, unchanged).
- [ ] Clicking a blast-radius-tier facet value cross-filters drift+test-confidence; clicking it again
  releases the filter (toggle rule, unchanged from XFILTER).
- [ ] The same facet click also dims the `entries` and `dataflow` panels' non-matching rows — every panel
  that carries a `REG-01` watcher responds, not only the two panels named in earlier drafts.
- [ ] `drill_through` on a blast-radius cell returns exactly the symbols counted in that cell — no
  silent re-query drift between the displayed count and the drilled rows.

## 11. Resolved decisions log

| Question | Decision |
|---|---|
| Does the reviewer still see containers/tree/components? | Yes, condensed into one `ctx` card with an expand button — never removed, always demoted (charter §10 ranks them ★–★★, not zero). |
| What happens with no change context? | Honest empty states per panel (§9 E-1/E-2), never a fabricated closure. |
| Shared dimension `REG-03` with Architect — whose derivation wins? | Neither "wins" — it is one field (`drift.json claim.status`), read identically by both, fixed in foundations §B.2. |

## 12. Rollout

Phase 1: layout + hero swap + `driftboard`/`testconfidence` panels over existing data (`drift.json`,
`/api/impact`) — shippable immediately, no engine work. Phase 2: `ENG-03` paired-evidence patch lands
(shared with Architect PRD) — upgrades the drift row's popup from single- to paired-location; Phase 1's
single-location fallback is the correct behavior in the interim, not a placeholder to rip out.
