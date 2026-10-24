# PRD — Atlas for the Performance / SRE Engineer

**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `performance` (6th persona —
added by a later pass closing `atlas-persona-prd-foundations.md` Part D.3's original orphan 1)
**Reads first:** `atlas-persona-prd-foundations.md` (`LAYOUT-*`, `REG-19/30`, Part F, Part G)
**Builds on:** `atlas-charter.md` §10's summary matrix (runtime data has no column there at all — this
PRD adds the column, it does not contradict one), `docs/chart-inventory.md` "Watching it run" group

## 1. Problem

Four real, measured one-tap charts — `runtime-coverage`, `runtime-cpu`, `runtime-race`,
`runtime-overlap` (`canned-charts.ts:465-551`) — have **no Atlas home at all**. A user who wants "where
did CPU time go" or "did the race lose updates" has to leave the Atlas for the one-tap menu; no persona's
job description names this as a need, because no persona in the original five is the person who asks
these questions as their day job. `atlas-persona-prd-foundations.md` Part D.3 named this precisely as
orphan 1 and declined to force it into an existing persona ("Architect `scorecard`'s runtime column" was
considered and rejected as too narrow — runtime data is a distinct job, not a column). This PRD is that
job's screen.

## 2. Goals

- **G1.** Opening "Performance" answers "where did CPU time go, did concurrency actually overlap, did
  the race lose updates, what did the tests really exercise at runtime" on one screen.
- **G2.** Built almost entirely at `REN-0` cost — every panel but the hero reuses an already-shipped
  chart with zero new engine work; this is the cheapest persona in the whole document set to ship.
- **G3.** The project-level classic quality metrics (defect density, MTBF, arrival rate, crash rate,
  MTTR — `quality.ts`'s "Quality metrics" group, Part D.3's original orphan 2) get a home here, since
  this is the one persona already built around whole-project, not per-file, numbers.
- **G4.** A CPU hotspot is cross-linkable to Architect's `scorecard` row for the same file — performance
  risk and structural risk are different lenses on the same file, not two unrelated tools.

## 3. Non-goals

- No live APM/tracing ingestion (OpenTelemetry collection, a live production feed) — this persona reads
  `cie`'s own existing seeded/measured `runtime.json` collection (a profiling run the user triggers
  against their own project), the same data source every other PRD in this set already treats as real.
  RFC-0019's stack-at-ingest work is a separate, unrelated concern.
- No distributed-tracing waterfall across services (`chart-inventory.md`'s F1 "trace waterfall" stays
  Tier C, not built here) — this PRD's scope is the four already-shipped charts plus the project-level
  metric strip, not every conceivable runtime chart.

## 4. Information architecture — the custom layout

`AtlasLayout.id = "performance.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx =
110`, `dock = {defaultOpen: true, widthPx: 280}`. Hero is the flame graph, merged into the one grid as
the `isHero` cell (invariant 6) — `overlap` fills the columns beside it, since thread-overlap is a
complementary *shape* of the same concurrency story, not a duplicate of the hero's CPU-time shape.

```
templateAreas (desktop, 8 cols):
"r4       r4       r4       r4       r4       r4       r4       r4      "
"hero     hero     hero     hero     hero     overlap  overlap  overlap"
"hero     hero     hero     hero     hero     overlap  overlap  overlap"
"hero     hero     hero     hero     hero     overlap  overlap  overlap"
"hotspots hotspots hotspots hotspots race     race     race     race   "
"hotspots hotspots hotspots hotspots race     race     race     race   "
"coverage coverage coverage coverage health   health   health   health "
"coverage coverage coverage coverage health   health   health   health "
"ctx      ctx      ctx      ctx      ctx      ctx      ctx      ctx    "
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `flamegraph` (NEW Atlas wiring) | `canvas` | 330 | the headline shape — where CPU time concentrates, read top-down |
| `overlap` | `threadoverlap` (existing chart `runtime-overlap`, NEW Atlas wiring) | `card` | 330 | beside the hero — the complementary concurrency view, not a duplicate |
| `hotspots` | `cpuhotspots` (existing chart `runtime-cpu`, NEW Atlas wiring) | `table-dense` | 220 | the same data as the flamegraph, ranked — a list for scanning, not replacing the shape |
| `race` | `racelost` (existing chart `runtime-race`, NEW Atlas wiring) | `card` | 220 | |
| `coverage` | `runtimecoverage` (existing chart `runtime-coverage`, NEW Atlas wiring) | `canvas` | 220 | measured coverage, distinct from static `REG-02` |
| `health` | `qualitystrip` (NEW, §5.4) | `card` | 220 | project-level, not per-file — the one place in the whole Atlas these five numbers fit |
| `ctx` | cross-link card to Architect `scorecard` | `card` | 110 | "this file is also high-risk structurally" — one line + a button, not a re-implementation |

`templateAreasMd`: `["r4 r4", "hero hero", "overlap overlap", "hotspots race", "coverage health", "ctx ctx"]`.
`templateAreasBase`: linear `planOrder`.

## 5. New panels — cost-graded

| Panel | Chart kind | Cost grade | What's missing exactly |
|---|---|---|---|
| `flamegraph` | flame graph (F2) | **`REN-1`** render-only (not zero) | engine exists and is headless-tested (`chart-inventory.md:71`, `engines/flame.ts`), `runtime.json cpu.top_functions` already real — missing piece is the G6 host-runtime render (RFC-0023 stage 1), the one item in this PRD that is not pure wiring |
| `cpuhotspots` | `hbar` (existing) | **`REN-0`** already shipped | `runtime-cpu` chart (`canned-charts.ts:490-511`) — placing it in this panel costs nothing |
| `threadoverlap` | `stat` (existing) | **`REN-0`** already shipped | `runtime-overlap` chart (`canned-charts.ts:534-551`) |
| `racelost` | `stat` (existing) | **`REN-0`** already shipped | `runtime-race` chart (`canned-charts.ts:514-531`) |
| `runtimecoverage` | `hbar` (existing) | **`REN-0`** already shipped | `runtime-coverage` chart (`canned-charts.ts:466-487`), the same data `REG-19` already registers |
| `qualitystrip` | `stat` row ×5 | **`REN-0`** already shipped | all five numbers are existing `metricChart` entries in `quality.ts:776-817` (`defect-density`, `mtbf-mtbf`, `arrival-rate`, `crash-rate`, `mttr`) — composed into one strip, not rebuilt |

### 5.1 `flamegraph` (hero)

One flame graph over `runtime.json`'s `cpu.top_functions`, each frame a real measured function, width
proportional to `cum_ms`. Click a frame → `SCOPE(space, module, file)` + `CHIP` (the same verb a
component box already uses); double-click → `POPUP` at the function's source line when the profiler
recorded one, else `ASK` (universal contract, foundations Part E — unchanged, no new verb).

### 5.2 `cpuhotspots`, `threadoverlap`, `racelost`, `runtimecoverage`

Each is the existing chart's data rendered in Atlas panel chrome, with the existing honest-empty states
verbatim ("No collected CPU profile — honest empty," etc., `canned-charts.ts`'s own hint strings) — this
PRD does not rewrite any of those strings, it places the chart.

### 5.3 The Architect cross-link (`ctx`)

A single line: *"3 of this project's 7 CPU hotspots are also flagged High in Architect's scorecard —
<button>Open in Architect →</button>."* Computed by joining `cpuhotspots`' file set against Architect's
`scorecard` rows by file path — no new data, a join over two already-real artifacts (`runtime.json`,
`quality.json`). The button performs a persona switch (`atlas-ux-spec.md` §14.2-1, "switch = re-plan, not
re-page") to Architect, scoped to the same file.

## 6. Slice/dice/filter — the performance persona's cube

| Field | Value |
|---|---|
| `hero` cube | `dimensions: [{id:"fn", dimension: Field("function")}]`, `measures: [{name:"cumMs", agg:"sum"}]`, `layout: Cells` — ranks functions by cumulative CPU time, the same number the flame graph's frame widths already encode |
| `facets` | `{panel:"cpuhotspots", dimension:"REG-30"}` (CPU hotspot tier), `{panel:"runtimecoverage", dimension:"REG-19"}` (measured coverage) |
| `crossFilterDimensions` | `["REG-30", "REG-19"]` — clicking a "High" hotspot tier cross-filters `runtimecoverage` to the same files, surfacing "is the hot path even tested" in one click |
| `drillThrough` | `{cell:["REG-30"], entity:"symbol"}` — drilling a hotspot tier lists exactly the `top_functions` rows counted in that tier |

Per foundations Part E, every panel's elements emit an `AtlasElementRef`: `cpuhotspots` rows assert
`REG-30`; `runtimecoverage` tiles assert `REG-19`; `threadoverlap`/`racelost` are whole-project stat
cards with no per-row identity, so (like Reviewer's `ctx` and Architect's `timeline`) they carry no
`dims`/`watches` — nothing to select, by the same "nothing to assert" rule, not an oversight.

## 7. Engine requirements

- **`REG-30`** (`cpuHotspotTier`, registered centrally in foundations §B.2a, derivation specified here
  per the "owned by exactly one persona" rule): bucketed from `runtime.json datasets.cpu.top_functions[].cum_ms`
  relative to the file's own profiling run — `≥50%` of the single largest `cum_ms` in the run → `High`,
  `20-49%` → `Medium`, else → `Low`. Thresholds are relative to the run's own maximum (not an absolute
  millisecond figure), since "hot" is inherently a within-run ranking, not a fixed number across unrelated
  profiling runs of different workloads.
- No new artifact beyond `REN-1`'s host-runtime render wiring (§5, cost-graded) — every data source this
  PRD reads already exists.

## 8. Functional requirements

- **FR-1.** `atlas-presets.ts` gains a sixth `PersonaPreset` (`id: "performance"`, `label: "Performance /
  SRE"`), and `atlas-ux-spec.md` §14's sub-menu gains a sixth entry — both are in scope for this PRD's
  implementation, not pre-existing infrastructure this PRD can assume.
- **FR-2.** `cpuhotspots`/`threadoverlap`/`racelost`/`runtimecoverage` ship immediately (`REN-0` — no
  engine dependency whatsoever); `flamegraph` ships once `REN-1`'s G6 host-runtime render lands. Until
  then the `hero` area renders `cpuhotspots`' existing ranked-bar chart as a temporary hero (an honest
  substitution, not a placeholder) — the panel plan's `hero` field simply points at a different panel id
  until `REN-1` lands, which `planSheet` already supports (it reads the preset's data, not a hardcoded
  chart).
- **FR-3.** When a project has no `runtime.json` at all (no profiling run was ever triggered), every
  panel in this persona shows its existing honest-empty hint ("No collected CPU profile — honest empty")
  and the hero shows the same — this persona is the one place in the Atlas where an entirely-empty
  screen on first load is the *correct*, not a broken, state, since runtime collection is opt-in and most
  projects will not have triggered it.

## 9. Edge cases

- **E-1.** `runtime.json` exists but only `coverage` was collected (no CPU/race/overlap run) —
  `flamegraph`/`cpuhotspots`/`threadoverlap`/`racelost` each show their own honest-empty state
  independently; `runtimecoverage` renders normally. Partial collection is common and must never make
  unrelated panels look broken.
- **E-2.** A CPU hotspot's file no longer exists in the current revision (the profiling run is older than
  the latest index) — the row renders with a `TELL`: "profiled against an earlier revision — this file
  may have changed since," never silently matched to a different file of the same name.
- **E-3.** The Architect cross-link's join finds zero overlapping files — the `ctx` card states "no CPU
  hotspots currently flagged High in Architect's scorecard" rather than hiding the card.

## 10. Acceptance criteria

- [ ] Performance appears as a sixth entry in the persona sub-menu, keyboard-reachable with a `6` shortcut
  (`atlas-ux-spec.md` §14.2-7's existing 1–5 pattern, extended).
- [ ] `cpuhotspots`, `threadoverlap`, `racelost`, `runtimecoverage` render real data from an existing
  `runtime.json` fixture with zero new engine calls.
- [ ] With no `runtime.json` present, every panel including the hero shows an honest-empty state — the
  screen is not blank/broken, it is legibly "nothing collected yet."
  Clicking a `REG-30` "High" facet value cross-filters `runtimecoverage` to the same files.
- [ ] The Architect cross-link opens Architect, scoped to the clicked file, verified by checking the
  resulting scope matches the file that was clicked, not a stale or default scope.

## 11. Resolved decisions log

| Question | Decision |
|---|---|
| Does this persona need a live telemetry pipeline? | No — it reads the existing seeded/measured `runtime.json`, same as every other PRD's runtime data. |
| Is the flamegraph render genuinely free? | No — `REN-1`, the one real (if small) render cost in this PRD; everything else is `REN-0`. |
| What shows as hero before the flamegraph ships? | `cpuhotspots`' existing ranked-bar chart, an honest substitution, not a placeholder. |
| Is an all-empty screen on first load a bug? | No — explicitly the correct state when no profiling run has ever been triggered (FR-3). |
| Are the CPU-tier thresholds absolute milliseconds? | No — relative to the single run's own maximum `cum_ms`, since "hot" only means something within one profiling run. |

## 12. Rollout

Phase 1 (`REN-0`, ships first): `cpuhotspots`, `threadoverlap`, `racelost`, `runtimecoverage`, and
`qualitystrip` — five panels, zero engine work, the cheapest shippable persona in this entire document
set. Phase 2: the Architect cross-link (a join over two already-real artifacts — cheap, but sequenced
after Phase 1 so it has real data on both sides to join). Phase 3 (`REN-1`): the flame graph replaces
`cpuhotspots` as hero once the G6 host-runtime render lands; `cpuhotspots` does not disappear, it demotes
back to its own panel slot in the grid the same way every other persona's "temporary hero" substitution
resolves.
