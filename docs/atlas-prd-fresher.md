# PRD — Atlas for the Fresher (Onboarding)

> **Any chart, anywhere:** every analytical chart sends and receives the shared
> selection. Placement never changes filtering. The [SmsController.java → submitSms() example](atlas-semantic-architecture.md#worked-example-smscontrollerjava--submitsms)
> is mandatory acceptance coverage for this persona too.


> **Implementation boundary:** follow [Atlas semantic architecture](atlas-semantic-architecture.md).
> This persona composes producers + presentation profiles + layout slots. Every
> data element selects/deselects contributors; double-click opens evidence. Legacy
> POPUP-on-click and filtering opt-outs below are superseded. Producer gaps and
> unavailable rendering remain explicit; changing layout never changes meaning.


**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `fresher`
**Reads first:** `atlas-persona-prd-foundations.md` (grid engine `LAYOUT-*`, cube `REG-*`/`ENG-*`)
**Builds on:** `atlas-charter.md` §4, `atlas-ux-spec.md` §5 (choreography — unchanged by this PRD),
`atlas-presets.ts` (`fresher` preset, unchanged by this PRD), `chart-inventory.md` Tier A.

## 1. Problem

The fresher preset is the one persona that is already ~100% Tier A and already matches the charter
(verified `atlas-presets.ts:39-48`: `containers → appentry → svcentry → components → hierarchy → flow →
dataflow`, `defaultScope: "default-container"`, `defaultFactsOnly: false`). The gap is not panels — it is
two things the charter names explicitly and marks un-built: **(a)** no guided-tour narration (the fresher
must notice the reading order themselves), **(b)** no "start here" marker, **(c)** the grid still clips
every panel to the same card height, which hurts exactly the two panels a fresher most needs to *see the
shape of* at a glance — the concept tree (2,000+ nodes, G6 pan/zoom) and the containers map — both
currently boxed into the same 520px card as a six-row entry table.

## 2. Goals

- **G1.** A first-time visitor completes the charter's "first 30 minutes" (§4.1) without needing this
  document explained to them — the UI narrates its own reading order.
- **G2.** The two spatially-dense charts (concept tree, containers) get canvas-sized real estate; the
  six-row tables do not waste it.
- **G3.** Zero behavior regression for the existing fresher choreography (`atlas-ux-spec.md` §5.2) — this
  PRD changes *layout* and *onboarding scaffolding* only, never a click's meaning.

## 3. Non-goals

- No new slice/dice cube depth — a fresher is explicitly **not** the persona that needs facets, pivots,
  or drill-through (charter §4: "Facts-only kept OFF... guided path, not a wall"). §6 below defines the
  smallest cube that earns its place (one dimension), not a full cube UI.
- No new engine facts beyond `ENG-01`/`ENG-02` (shared with Architect, defined once in foundations §C).

## 4. Information architecture — the custom layout

`AtlasLayout.id = "fresher.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx = 110`,
`dock = {defaultOpen: true, widthPx: 280}` (collapsible — foundations §A.6).

**Hero is merged into this grid** (foundations §A.3 invariant 6, corrected from an earlier draft that
gave `containers` its own fixed-width column): `containers` is simply the `isHero` cell, sized generously
(5 of 8 columns, 3 rows) with the concept tree taking the other 3 columns in the *same* rows — nothing
sits empty below either chart, the whole width stays in use at every row.

```
templateAreas (desktop, 8 cols):
"r4   r4   r4   r4   r4   r4   r4   r4  "
"hero hero hero hero hero tree tree tree"
"hero hero hero hero hero tree tree tree"
"hero hero hero hero hero tree tree tree"
"entr entr entr entr rout rout rout rout"
"entr entr entr entr rout rout rout rout"
"comp comp comp comp flow flow data data"
"comp comp comp comp flow flow data data"
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `containers` (`isHero: true`) | `canvas` | 330 | the shape of the whole; 5/8 width leaves real room for the concept tree beside it instead of below an empty column |
| `tree` | `hierarchy` (concept tree, G6) | `canvas` | 330 | beside the hero, not under it — the one pan/zoom chart; a card-clipped tree is illegible at 2,000+ nodes (charter §4.3 #2) |
| `entr` | `appentry` | `table-dense` | 220 | first table a fresher reads after the two hero charts |
| `rout` | `svcentry` | `table-dense` | 220 | beside `entr` — routes follow boot entries in reading order |
| `comp` | `components` | `card` | 220 | wide (4/8) — module lines need horizontal room (charter §4.2 row 4) |
| `flow` | `flow` | `card` | 220 | |
| `data` | `dataflow` | `card` | 220 | |

`templateAreasMd` (768–1279px, 2 cols): `["r4 r4", "hero hero", "tree tree", "entr rout", "comp comp", "flow data"]`.
`templateAreasBase`: linear, `planOrder` order (ux-spec default, unchanged).

## 5. New panel: `sizeintuition` (hbar, Tier A chart, NEW Atlas wiring)

Charter §4.3 ranks "hbar — named things per file" priority #6 ("size intuition: where the mass actually
lives") but it is not in `AtlasPanelId` today, even though the exact chart already renders elsewhere
(`canned-charts.ts` `things-per-file:369`). This PRD adds it to the fresher's plan only (not a shared
panel — other personas don't need it per charter §10's matrix).

- **`AtlasPanelId` gains:** `"sizeintuition"`.
- **Not placed in the main grid** — it renders inside the `components` panel's footer as a collapsible
  strip (`<details>`, closed by default, label "where the mass is") to avoid adding a sixth grid area;
  this is a deliberate space trade-off, not an oversight.
- **Data:** reuses `things-per-file`'s existing query (symbol count per file in the current scope),
  scoped to whatever container is active — no new engine work.
- **Click:** `CHIP(el)` + `ASK(el)` (bar has no single `loc` — today's `pendingAsk` path, ux-spec §3 row
  "element with no location").

## 6. Shared selection with guided controls

The Fresher uses the same Atlas selection algebra as every other persona. Clicking
any data element adds its exact contributor set; clicking again removes it;
double-click opens evidence. Advanced pivot/facet controls may be hidden by the
onboarding layout, but cross-view filtering is never disabled. Guided-tour focus,
visual group expansion and camera movement are presentation actions, not filters.

## 7. Engine requirements

- `ENG-01` (`DecisionFact`) and `ENG-02` (PDG wiring) — **shared** with Architect, defined once in
  `atlas-persona-prd-foundations.md` Part C. This PRD's only dependency on them is the R4 FlowChart's
  decision diamonds (ux-spec §4.1), which the fresher reaches whenever ≥1 chip is active — no
  fresher-specific engine work.
- No other new facts.

## 8. New UI requirements (the actual gap this PRD closes)

- **FR-1.** A "start here" badge renders on the default container's boot-entry row in the `entr` panel
  on first render of the fresher preset (session-scoped: shown once per browser session per project,
  stored in the existing per-persona `sessionStorage` memory key `cie.atlas.persona.fresher`, new field
  `tourSeen: boolean`).
- **FR-2.** A **guided-tour mode**, entered via a button next to the persona label
  ("Walk me through it →"), steps through six stops in this exact order, each a `TELL`-style one-sentence
  caption anchored to the relevant area with a `Next →` / `Skip tour` control:
  1. `tree` area — "This is every concept in the system, from Domain down to Rule. Pan and zoom; nothing here is invented, every box is evidence-backed."
  2. `containers` (R1, hero) — "These are the apps that make up the system. Dashed = couldn't be verified."
  3. `entr`/`rout` — "Execution starts here. Click any row to see the real code."
  4. `comp` — "Inside the app you're scoped to — every file that makes it up."
  5. `flow` — "Who calls whom, from recorded calls only."
  6. `data` — "What data moves where. Dashed arrows are inferred, not measured."
  - Tour completion sets `tourSeen = true`; re-entering the tour is always available from the same
    button (not one-shot-forever) — this resolves the only real ambiguity ("does skipping mean never
    again" → no, it means "not auto-shown again").
- **FR-3.** Facts-only toggle remains forced to the preset default (`false`) with manual override allowed
  (existing `§14.2-3` rule) — unchanged, restated so this PRD does not silently alter it.
- **FR-4.** The R3 grid renders per §4's `AtlasLayout`; the `Maximize2` per-panel expand (ux-spec
  unchanged behavior) continues to work identically on every slot regardless of `chrome` value.

## 9. Edge cases

- **E-1.** Container has <6 files (tiny app) — `comp`/`flow`/`data` panels show their existing honest
  empty states (unchanged); the tour still runs (a tiny app is still worth orienting on).
  Facts-only forced `false`, `defaultScope: "default-container"`.
- **E-2.** No `.cie` concept data indexed yet (fresh scan) — `tree` canvas shows the existing "not indexed"
  empty state; tour step 1's caption appends " — once indexing finishes." (string concatenation, not a
  new component).
- **E-3.** User resizes below 768px mid-tour — tour captions re-anchor to the `templateAreasBase` linear
  order; no stop is skipped, only y-position changes.

## 10. Acceptance criteria

- [ ] New visitor, no prior session: `tourSeen` is false, "start here" badge visible, tour button visible
  and unclicked-state obvious (not auto-started — the user chooses to take it).
- [ ] Taking the tour visits all 6 stops in order, each anchored to the correct grid area at every
  breakpoint (desktop/md/base).
- [ ] `tree` and `containers` render at canvas size (no internal scrollbar clipping a tree under ~50
  nodes; pan/zoom works for ≥2,000 nodes per existing G6 proof).
- [ ] `sizeintuition` strip renders under `components`, closed by default, opens to the existing hbar
  chart scoped to the active container.
- [ ] No existing fresher choreography test (`atlas-ux-spec.md` §5.2 table) regresses — full click table
  re-verified against the new grid.
- [ ] Every Fresher data element participates in shared selection; simplified controls never disable cross-view filtering.

## 11. Resolved decisions log (nothing left open)

| Question | Decision |
|---|---|
| Does skipping the tour hide it forever? | No — always re-enterable from the same button. |
| Does `sizeintuition` get its own grid area? | No — a collapsible strip inside `components`, to avoid a 5th area on an already information-dense screen. |
| Is cross-filtering available to the fresher? | Yes; the selection semantics are universal, while advanced controls may stay hidden. |
| Mobile layout? | Linear `planOrder`, per foundations §A.3 invariant 5 — no custom mobile grid. |

## 12. Rollout

Single phase — this PRD has no engine dependency (`ENG-01`/`ENG-02` land under the Architect PRD's
timeline; the fresher's R4 diamonds simply render "decisions not indexed" honestly until then, which is
already the spec'd behavior). Ship layout + tour + `sizeintuition` together; no feature flag needed since
behavior is strictly additive to an already-shipped preset.
