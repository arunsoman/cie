# PRD — Atlas for the Consultant / Tech-Lead on a Foreign Codebase

> **Any chart, anywhere:** every analytical chart sends and receives the shared
> selection. Placement never changes filtering. The [SmsController.java → submitSms() example](atlas-semantic-architecture.md#worked-example-smscontrollerjava--submitsms)
> is mandatory acceptance coverage for this persona too.


> **Implementation boundary:** follow [Atlas semantic architecture](atlas-semantic-architecture.md).
> This persona composes producers + presentation profiles + layout slots. Every
> data element selects/deselects contributors; double-click opens evidence. Legacy
> POPUP-on-click and filtering opt-outs below are superseded. Producer gaps and
> unavailable rendering remain explicit; changing layout never changes meaning.


**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `consultant`
**Reads first:** `atlas-persona-prd-foundations.md` (`LAYOUT-*`, `REG-10/11/12`)
**Reuses:** the condensed-context pattern from `atlas-prd-reviewer.md` §4; the `reviewer.default`
`AtlasLayout` wholesale when a change context is active (§8, FR-3)
**Builds on:** `atlas-charter.md` §9, `atlas-ux-spec.md` §10

## 1. Problem

`atlas-presets.ts` ships the consultant preset as the fresher's six panels auto-focused on the busiest
container (`behavesAsReviewerWithChangeContext: true` exists as a flag but its behavior is unspecified).
The charter's own reason this persona is distinct from the fresher — items 7–10: ownership/bus-factor map,
co-change matrix, coverage heatmap, journey charts — are absent from `AtlasPanelId`. A consultant billing
by the hour on an unfamiliar repo gets the same onboarding screen a new hire gets, with none of "who
knows this part" or "what's safe to touch," which is the actual value this persona needs fastest.

## 2. Goals

- **G1.** The single highest-value fact for this persona — "who knows this part" — is on screen within
  the same first look that auto-focuses the busiest container.
- **G2.** "What's safe to refactor" (coverage) and "what moves together" (co-change) are real panels, not
  implied by staring at file lists.
- **G3.** The unspecified `behavesAsReviewerWithChangeContext` flag gets an exact, testable definition.

## 3. Non-goals

- No automated "safe to refactor" verdict — coverage and co-change are presented as evidence; the
  consultant forms the judgment, same honesty boundary as every other persona.
- No cross-repository ownership (a consultant on repo A doesn't see repo B's authors) — single-repo scope
  only, consistent with the charter's "Out of scope for version 1" (`storage-retrieval-api-spec.md` §15:
  cross-repository federation).

## 4. Information architecture — the custom layout

`AtlasLayout.id = "consultant.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx = 110`,
`dock = {defaultOpen: true, widthPx: 280}`. Hero stays `containers` (auto-focused on the busiest
container, existing behavior, unchanged) — merged into the one grid as the `isHero` cell (foundations
§A.3 invariant 6), with `ownership` filling the columns beside it: "who knows this part" belongs right
next to the auto-focused container, not three scrolls down.

```
templateAreas (desktop, 8 cols):
"r4    r4    r4    r4    r4    r4    r4    r4   "
"hero  hero  hero  hero  hero  own   own   own  "
"hero  hero  hero  hero  hero  own   own   own  "
"hero  hero  hero  hero  hero  own   own   own  "
"cochg cochg cochg cochg cover cover cover cover"
"cochg cochg cochg cochg cover cover cover cover"
"jrny  jrny  jrny  jrny  jrny  jrny  jrny  jrny "
"ctx   ctx   ctx   ctx   ctx   ctx   ctx   ctx  "
"ctx   ctx   ctx   ctx   ctx   ctx   ctx   ctx  "
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `containers` (`isHero: true`), auto-focused | `canvas` | 330 | unchanged behavior, corrected shape — 5/8 width, not a lone column |
| `own` | `ownership` (NEW, K1) | `card` | 330 | beside the hero — "who knows this part" read together with the auto-focus |
| `cochg` | `cochange` (NEW) | `canvas` | 220 | square-ish — a matrix heatmap |
| `cover` | `coverage` (existing Tier A `heatmap` chart, NEW Atlas wiring only) | `canvas` | 220 | square-ish — tile grid |
| `jrny` | `journeys` (NEW) | `card` | 110 | full width, short — journey steps read left-to-right |
| `ctx` | fresher's six panels, condensed | `card` | 220 | the fresher's full toolkit, demoted to one expandable card (reuses the identical condensed-card pattern from `atlas-prd-reviewer.md` §4 — same component, not reinvented) |

`templateAreasMd`: `["r4 r4", "hero hero", "own own", "cochg cover", "jrny jrny", "ctx ctx"]`.
`templateAreasBase`: linear `planOrder`.

## 5. New panels

### 5.1 `ownership` (NEW, K1)

- **Chart:** `table`/matrix — author × area, cell = file count owned.
- **Click:** cell → `CHIP` + a list popup of that author's files in that area (ux-spec §10, "`POPUP`-list
  variant NEW" — a popup whose body is a scrollable file list rather than a code snippet; same popup
  shell as §2, body variant only).

### 5.2 `cochange` (NEW)

- **Chart:** matrix heatmap (Tier B per `chart-inventory.md:57`) — files × files, cell intensity =
  `CochangeEdge.support`; colored/grouped by `coChangeCluster` (`REG-11`).
- **Click:** ribbon/cell → `CHIP(edge)` + a popup listing `supporting_commits` (ux-spec §10, unchanged).

### 5.3 `coverage` (existing Tier A chart, NEW Atlas wiring)

The `heatmap` chart kind already renders coverage tiles elsewhere (`chart-inventory.md:27`,
`canned-charts.ts` `coverage:318`, `quality.json` tiles) — this PRD only wires it into a dedicated
Consultant panel; **no new chart, no new data.**

### 5.4 `journeys` (NEW)

- **Chart:** ordered step chart (J1/J2) — one lane per extracted journey, steps = route + story pairs in
  call order.
- **Click:** step → `POPUP` the step's code + `FOCUS` (the route-table row inside the condensed `ctx`
  card lights, when expanded) (ux-spec §10, unchanged).

## 6. Slice/dice/filter — the consultant's cube

*Governed by `atlas-persona-prd-foundations.md` Part E (the universal selection/filter/source contract):
every element selects, filters and double-click-opens by the same `AtlasElementRef` shape; filter state
is a set per dimension (E.2), not a single toggled value. The table below still states which `REG-xx`
dimensions and panels apply here — unchanged by Part E.*

| Field | Value |
|---|---|
| `hero` cube | `dimensions: [{id:"owner", dimension: Field("REG-10")}]`, `measures: [{name:"fileCount", agg:"count"}]`, `layout: Cells` — "who owns what, by file count" is the fastest orientation fact this persona needs |
| `facets` | `{panel:"ownership", dimension:"REG-10"}`, `{panel:"cochange", dimension:"REG-11"}`, `{panel:"coverage", dimension:"REG-02"}` (test confidence, reused from the Reviewer registry entry — one field, two personas, per foundations §2's collision rule) |
| `crossFilterDimensions` | `["REG-10", "REG-11"]` — clicking an owner cross-filters `cochange` to that owner's files; clicking a co-change cluster cross-filters `ownership` to the cluster's files. `ownership` itself keeps a per-file entity list (not only the donut-style share chart) specifically so it has rows to watch `REG-10` with — a chart with no underlying row-level table has nothing for a filter to dim |
| `drillThrough` | `{cell:["REG-10"], entity:"file"}` — drilling an owner cell lists their files (identical query to the §5.1 popup — one code path) |

## 7. Engine requirements

- **`ENG-08`** — `cochange.json --authors` extension (the gap already named in `atlas-charter.md:147`,
  `atlas-ux-spec.md` §11 "`cochange --authors`"). Adds, per file, the per-author commit counts over the
  trailing 180 days:
  ```json
  { "files": [
      { "path": "string", "authors": [ { "login": "string", "commits": "u32" } ] }
  ] }
  ```
  `REG-10` (`owner`) = the entry with the highest `commits` per file (tie-break: most recent commit
  wins — deterministic, stated here so two implementers can't pick differently).
- **`REG-11`** (`coChangeCluster`) needs no new engine data — it is a pure derivation over the existing
  `CochangeEdge.support` field (connected components at `support ≥ 3`, fixed in foundations §B.2).
- **`ENG-09`** — `journeys.json`. Derived from existing routes (`stack.json`) + stories (`USER-STORIES.md`
  lineage data, already indexed per the engine's story extraction) into ordered steps:
  ```json
  { "journeys": [
      { "id": "string", "label": "string",
        "steps": [ { "route": "METHOD PATH", "symbol": "qual_name", "loc": {"file":"string","line":"u32"} } ] }
  ] }
  ```
  Build rule: a journey is one user story's call chain, ordered by the recorded call graph from the
  story's entry route to its last recorded call; a story with no resolvable entry route is excluded (not
  guessed) — the journey list is shorter than the story list when evidence is incomplete, and that gap is
  disclosed in the panel footer ("N of M stories resolved into journeys").

## 8. Functional requirements

- **FR-1.** `atlas-presets.ts` consultant preset: `planOrder` gains `[ownership, cochange, coverage,
  journeys]` ahead of the fresher's six (now condensed into `ctx`, same pattern as Reviewer §4).
- **FR-2.** Auto-focus on the busiest container (existing behavior, `TELL`-disclosed on the hero per
  `atlas-ux-spec.md` §10) is unchanged by this PRD.
- **FR-3.** **`behavesAsReviewerWithChangeContext`, defined exactly:** when the change-context picker
  (visible in this preset per `atlas-ux-spec.md` §1 top-bar note) has both `base` and `head` populated,
  the Atlas swaps `AtlasLayout` from `consultant.default` to `reviewer.default` wholesale (§4 of this PRD
  is replaced by `atlas-prd-reviewer.md` §4 verbatim, same component tree, same cube) for the duration the
  context stays set; clearing the context restores `consultant.default`. This is the one unambiguous
  reading of the existing flag: **not** a blend of the two layouts, a **full swap**, footer-disclosed
  ("Atlas re-planned for: Reviewer — change context active") exactly as persona switches already disclose
  (`atlas-ux-spec.md` §14.2-5).

## 9. Edge cases

- **E-1.** Repo has no git history deep enough for 180-day author stats (shallow clone / young repo) —
  `ownership` shows "not enough commit history to attribute ownership (N days available)," never a
  false single-author attribution from too little data.
- **E-2.** A story's entry route can't be resolved to a symbol — excluded from `journeys.json` per the
  build rule above; the footer discloses the gap (ENG-09).
- **E-3.** Change context cleared mid-session while chips are active — chips survive the layout swap in
  both directions (G3, unchanged: "chips are the user's, not the persona's").

## 10. Acceptance criteria

- [ ] Hero auto-focuses the busiest container with the existing `TELL` disclosure, unchanged.
- [ ] `ownership` cell count matches `ENG-08`'s per-file author with the most commits, tie broken by
  recency — verified against a fixture with a deliberate tie.
- [ ] Setting a change context swaps the entire grid to `reviewer.default` and the footer discloses the
  swap; clearing it swaps back, chips preserved across both transitions.
- [ ] `journeys` panel footer accurately reports "N of M stories resolved" and never fabricates a step
  for an unresolved story.
- [ ] `coChangeCluster` cross-filter on `cochange` narrows `ownership` to exactly that cluster's files.

## 11. Resolved decisions log

| Question | Decision |
|---|---|
| Does "behaves as Reviewer" mean a blended layout? | No — a full `AtlasLayout` swap, footer-disclosed, chips preserved. |
| Ownership tie-break? | Most recent commit wins. |
| What happens to an unresolvable story? | Excluded from `journeys.json`, gap disclosed in the panel footer — never guessed. |
| Cross-repo ownership? | Out of scope — single repo only (§3). |
| Does this PRD restate the git-rename/squash-merge/PII caveats for `ownership`/`cochange`? | No — inherited by reference from `atlas-persona-prd-foundations.md` Part H, rows 1/2/5, governed by Part F.5/F.6. `ENG-08`'s collector must use rename-following history and the deployer must have an anonymization option before this ships to a multi-tenant deployment. |

## 12. Rollout

Phase 1: `coverage` ships immediately (existing Tier A chart, wiring only). Phase 2: `cochange` +
`REG-11` ship (pure derivation over existing `cochange.json`, no new facts). Phase 3: `ENG-08` ships
`ownership` + `REG-10`. Phase 4: `ENG-09` ships `journeys`. Phase 5: the change-context layout swap
(FR-3) — independent of all four, can ship whenever the Reviewer PRD's `reviewer.default` layout exists,
including before any of Phases 1–4 land.
