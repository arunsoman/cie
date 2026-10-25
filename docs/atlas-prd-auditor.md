# PRD — Atlas for the Auditor / Security Reviewer

> **Any chart, anywhere:** every analytical chart sends and receives the shared
> selection. Placement never changes filtering. The [SmsController.java → submitSms() example](atlas-semantic-architecture.md#worked-example-smscontrollerjava--submitsms)
> is mandatory acceptance coverage for this persona too.


> **Implementation boundary:** follow [Atlas semantic architecture](atlas-semantic-architecture.md).
> This persona composes producers + presentation profiles + layout slots. Every
> data element selects/deselects contributors; double-click opens evidence. Legacy
> POPUP-on-click and filtering opt-outs below are superseded. Producer gaps and
> unavailable rendering remain explicit; changing layout never changes meaning.


**Date:** 2026-10-24 · **Status:** ready for implementation · **Persona:** `auditor`
**Reads first:** `atlas-persona-prd-foundations.md` (`LAYOUT-*`, `REG-07/08/09`)
**Builds on:** `atlas-charter.md` §7, `atlas-ux-spec.md` §8

## 1. Problem

`atlas-presets.ts` ships `hero: "svcentry"` for the auditor preset, facts-only forced on — the one real
piece of this persona's charter that is already right. But the charter's hero item, the **trust boundary
map (I3)**, and the **policy enforcement matrix (G3)** are both absent from `AtlasPanelId`, and neither
even has engine data yet — `atlas-ux-spec.md` §11 lists both as `ENGINE` gaps with no concrete schema.
Today an auditor gets routes + a generic dataflow panel; the two things that actually define "what faces
the world and what's provably enforced" don't exist.

## 2. Goals

- **G1.** The hero view answers "where does untrusted data enter, and how far does it reach" with a real
  trust-boundary graph, not a container map relabeled.
- **G2.** "Enforced, asserted, or merely claimed" becomes a queryable fact per invariant, not prose in a
  findings list.
- **G3.** Severity-ranked findings, which today live only in the generic cross-filter (`chart-filter.ts`),
  get a dedicated panel in this persona's screen — an auditor should never have to leave "Auditor" to see
  severity-ranked findings.

## 3. Non-goals

- No automated penetration testing or exploit generation — this persona *reads* recorded evidence
  (routes, externals, taint-adjacent facts already in the engine); it never executes anything against the
  target.
- No compliance-framework mapping (SOC2/PCI clause tagging) — `enforcementStatus` (`REG-08`) is a
  code-evidence fact, not a compliance verdict.

## 4. Information architecture — the custom layout

`AtlasLayout.id = "auditor.default"`, `columns = 8` (foundations §A.3 invariant 3), `rowHeightPx = 110`,
`dock = {defaultOpen: true, widthPx: 280}`. Hero changes from `svcentry` to the new `trustboundary` panel
(FR-1) — the charter's own stated hero. `trustboundary`'s diagram is inherently wide (external → route →
core, three columns of nodes), so unlike the other four personas it spans the full 8 columns rather than
sharing a row — the better fit for *this* hero's shape, not a reversion to a wasted column, since the
full width is genuinely in use by the diagram itself (foundations §A.3 invariant 6 forbids an *empty*
column, not a wide one).

```
templateAreas (desktop, 8 cols):
"r4    r4    r4    r4    r4    r4    r4    r4   "
"hero  hero  hero  hero  hero  hero  hero  hero "
"hero  hero  hero  hero  hero  hero  hero  hero "
"hero  hero  hero  hero  hero  hero  hero  hero "
"route route route ext   ext   find  find  find "
"route route route ext   ext   find  find  find "
"route route route pol   pol   pol   pol   pol  "
"data  data  data  data  data  data  data  data "
```

| Area | Panel | `chrome` | minHeightPx | Why this shape |
|---|---|---|---|---|
| `hero` | `trustboundary` (`isHero: true`) | `canvas` | 330 | the attack-surface graph; full width — its own three-column node shape, never clipped |
| `route` | `svcentry`, **system-wide, scope-immune** (§8 FR-2) | `table-dense` | 220 | tall — "every HTTP verb/path the repo exposes" is a long list, not a six-row preview |
| `ext` | `externals` (NEW; a filtered view of `containers` emphasizing image-only services) | `canvas` | 220 | square — compose-service maps read better square than wide |
| `find` | `findings` (NEW; severity-ranked, `REG-09`/`REG-16`) | `table-dense` | 220 | |
| `pol` | `policymatrix` (NEW) | `table-dense` | 110 | wide strip — "enforced / asserted / unenforced / unknown" per invariant is a dense table, not a chart |
| `data` | `dataflow`, **writes-emphasized** (existing panel, `writesEmphasis` prop) | `card` | 110 | full width, short — cross-boundary writes scanned left-to-right |

`templateAreasMd`: `["r4 r4", "hero hero", "route ext", "route find", "route pol", "data data"]`.
`templateAreasBase`: linear `planOrder`.

## 5. New panels

### 5.1 `trustboundary` (NEW, hero)

- **Chart:** graph (I3) — nodes = containers/externals/routes; edges = reachability hops from an
  image-only external to a route, colored by `trustZone` (`REG-07`).
- **Rendering rule (not a click verb, per `atlas-ux-spec.md` §8):** the hero always visually emphasizes
  image-only external services — this is styling derived from `stack.json`, not new data.
- **Click:** external box → `FOCUS` + `POPUP` at the compose YML evidence lines + `CHIP` (ux-spec §8,
  unchanged).

### 5.2 `externals` (NEW; filtered `containers` view)

A `c4`-kind chart (Tier A, already rendered for the main `containers` panel) reused with a server-side
filter: only image-only / compose services, no internal app containers. **Not a new chart kind** — a new
query over the same `stack.json` the existing `containers` panel already reads.

### 5.3 `findings` (NEW; severity-ranked)

- **Chart:** `hbar` by `severity` (`REG-09`) — the exact chart already exists elsewhere
  (`canned-charts.ts` severity/urgency chart) wired into a dedicated Atlas panel for this persona only.
- **Click:** row → `POPUP` the exact evidence line (`quality.json` line numbers) + `CHIP` (ux-spec §8,
  unchanged).

### 5.4 `policymatrix` (NEW)

- **Chart:** `table` — one row per invariant/policy, columns = invariant name, `enforcementStatus`
  (`REG-08`), enforcing/asserting evidence location.
- **Click:** row → `POPUP` the invariant's enforcing line + `TELL` "asserted, not enforced — no passing
  test links to this invariant" (or the matching honest phrasing per status) (ux-spec §8, unchanged
  verb composition, new panel).

## 6. Slice/dice/filter — the auditor's cube

*Governed by `atlas-persona-prd-foundations.md` Part E (the universal selection/filter/source contract):
every element selects, filters and double-click-opens by the same `AtlasElementRef` shape; filter state
is a set per dimension (E.2), not a single toggled value. The table below still states which `REG-xx`
dimensions and panels apply here — unchanged by Part E.*

| Field | Value |
|---|---|
| `hero` cube | `dimensions: [{id:"zone", dimension: Field("REG-07")}]`, `measures: [{name:"routeCount", agg:"count"}, {name:"criticalFindings", agg:"count", filter:"severity=critical"}]`, `layout: Cells` |
| `facets` | `{panel:"trustboundary", dimension:"REG-07"}` (trust zone), `{panel:"findings", dimension:"REG-09"}` (severity), `{panel:"policymatrix", dimension:"REG-08"}` (enforcement status) |
| `crossFilterDimensions` | `["REG-07", "REG-08", "REG-09"]` — e.g. clicking `Public` on the trust-zone facet cross-filters `route`, `findings`, and `dataflow` to routes reachable from an external in ≤1 hop |
| `drillThrough` | `{cell:["REG-07"], entity:"finding"}` — drilling a trust-zone cell lists the findings recorded against that zone's routes; `{cell:["REG-08"], entity:"symbol"}` — drilling an enforcement-status cell lists the invariant-bearing symbols |

## 7. Engine requirements

- **`ENG-06`** — `trust_boundary.json`. Derived (no new parsing, no new analysis pass — a join over two
  existing artifacts):
  ```json
  { "edges": [
      { "from": "external:<compose-service-name>", "to": "route:<METHOD PATH>", "hops": 1 }
  ] }
  ```
  Build rule: BFS from every image-only external (`stack.json`) over the recorded call graph to every
  route (`svcentry`'s existing data); `hops` = shortest path length. `REG-07` (`trustZone`) buckets this
  verbatim per foundations §B.2 (`0-1`→`Public`, `2-3`→`Internal`, `4+`/unreachable→`TrustedCore`). A
  route with no path from any external is `TrustedCore` by construction — never silently omitted from
  the map (it renders as an isolated node, honestly labeled "no external reaches this route").
- **`ENG-07`** — `policy.json`. Derived from existing invariant/finding facts (`quality.json`), no new
  static analysis:
  ```json
  { "policies": [
      { "invariant": "string", "status": "Enforced|Asserted|Unenforced|Unknown",
        "evidenceLoc": {"file":"string","line":"u32"}, "linkedTestLoc": {"file":"string","line":"u32"}|null }
  ] }
  ```
  Build rule (fixed, per foundations §B.2 `REG-08`): an invariant with a `linkedTestLoc` →
  `Enforced`; an invariant with only `evidenceLoc` (a comment/annotation) → `Asserted`; an invariant
  recorded with neither → `Unenforced`; no invariant recorded for a concept that plausibly has one
  (e.g. a route with no auth check found) → `Unknown` — never collapsed into `Unenforced`, since
  "nothing recorded" and "recorded as absent" are different claims (the existing honesty contract,
  applied here).

## 8. Functional requirements

- **FR-1.** `atlas-presets.ts` auditor preset: `hero` changes from `"svcentry"` to `"trustboundary"`;
  `planOrder` becomes `[trustboundary, svcentry, externals, findings, policymatrix, dataflow, flow,
  components, hierarchy, containers]`.
- **FR-2.** `svcentry` in this persona's grid is **scope-immune**: unlike every other persona, clicking a
  container box elsewhere in the Atlas does **not** filter this panel's rows — the auditor's route table
  is always system-wide (charter §7, "Route table (whole system, all controllers)" — explicitly not
  scoped). This is a one-line exception in the existing `SCOPE` choreography (`atlas-ux-spec.md` §3):
  `svcentry` is removed from the re-slice list specifically for the `auditor` preset.
- **FR-3.** Facts-only stays forced ON with no manual override shown in the UI for this preset — this is
  the one preset where the spec already says the toggle isn't optional (`atlas-presets.ts` already sets
  `defaultFactsOnly: true`; this PRD adds hiding the toggle control entirely for `auditor`, since an
  auditor repeating an inferred fact is a process failure, not a preference — FR, not a default).
- **FR-4.** `externals` always renders even when `trust_boundary.json` (`ENG-06`) hasn't landed yet — it
  only needs `stack.json`, which exists today; it does not block on the hero.

## 9. Edge cases

- **E-1.** No image-only externals recorded (a fully internal system, e.g. a CLI tool) — `trustboundary`
  hero shows "no external entry points recorded — every route is `TrustedCore` by default," and `zone`
  facet shows only `TrustedCore (N)`, never an empty graph.
- **E-2.** A route recorded twice under two controllers (duplicate route mapping) — `ENG-06`'s BFS takes
  the shortest hop count across both; the route table row itself is unaffected (existing dedup rule,
  unchanged).
- **E-3.** `policy.json` has zero invariants for the whole repo — `policymatrix` shows "no invariants
  indexed for this repository" (distinct from `Unknown` per-row, which requires at least a plausible
  concept to be absent-from — a whole-repo zero is a coverage statement, not a per-row fact).

## 10. Acceptance criteria

- [ ] Auditor hero renders `trustboundary`, not `svcentry`.
- [ ] `svcentry` row count is identical regardless of what container is scoped elsewhere in the session
  (scope-immunity verified by an explicit regression test, not visual inspection).
- [ ] Facts-only toggle control is absent (not merely disabled) from the auditor top bar.
- [ ] `trustZone` facet counts sum to the total route count (no route silently excluded from every zone).
- [ ] `enforcementStatus` never reports `Unenforced` for a concept with zero recorded invariants —
  that case is `Unknown` (explicit negative test).
- [ ] Drilling a trust-zone cell returns exactly the findings recorded against that zone's routes — cube
  count and drill-through row count match.

## 11. Resolved decisions log

| Question | Decision |
|---|---|
| Is a route unreachable from any external a bug in the map? | No — it's `TrustedCore` by construction, rendered as an isolated, honestly labeled node. |
| Can the auditor turn facts-only off? | No — the control is removed for this preset, not merely defaulted. |
| Does "no invariants recorded" mean "unenforced"? | No — distinct buckets (`Unknown` vs. `Unenforced`); conflating them is the one failure mode this PRD explicitly rules out. |
| Does scoping a container elsewhere affect the auditor's route table? | No — `svcentry` is scope-immune in this preset only (FR-2). |

## 12. Rollout

Phase 1: `externals`, `findings` ship immediately (`stack.json`/`quality.json` already exist — zero
engine work). Phase 2: `ENG-06` (`trust_boundary.json`) ships `trustboundary` hero + `REG-07` faceting.
Phase 3: `ENG-07` (`policy.json`) ships `policymatrix` + `REG-08` faceting. Each phase is independently
demoable; the hero shows "trust boundary data not yet indexed" honestly between phase 1 and phase 2,
never a placeholder graph.
