# Atlas UX spec — per-user-type layouts and complete click choreography (nothing left to guess)

**Date:** 2026-10-23 · **Status:** build spec · supersedes charter §11 note "what reuses vs needs"
**Related:** `docs/atlas-charter.md` (charter; §12 = today's click behavior), `docs/rfc-0023-js-owns-pixels.md` (conductor, planSheet, chips), `docs/chart-inventory.md` (tier receipts), RFC-0022 §2.1 (the PDG — data source for decision boxes below)

**Reading key for persona sections:** each persona gets (a) a LAYOUT — the regions and what ships in each; (b) DEFAULTS; (c) a complete CLICK CHOREOGRAPHY — for every clickable element, what changes in every region; (d) what is new vs today. Shared choreography verbs are defined once (§3) and referenced by id, so persona sections stay readable without losing precision. Every unmarked behavior exists today; every `NEW` tag must be built; every `ENGINE` tag names the exact data the engine must add. Nothing undocumented.

---

## 1. The canonical layout frame (one shell, all personas)

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ TOP BAR   logo · persona preset [Fresher ▾] · scope breadcrumb                   │
│           [System ▸ alerting ▸ AlertService] · search · Facts only ○             │
│           dock toggle (Ask CIE)                                                  │
├──────────────────────────┬────────────────────────────────────────┬──────────────┤
│  R1 HERO PANEL           │  R3 SECONDARY PANELS (scrolling grid,  │  R5 ASK-CIE  │
│  the persona's main map  │  persona-ordered by planOrder)         │     DOCK     │
│                          │                                        │              │
│                          │                                        │  R5a CONTEXT │
│                          │                                        │  CHIPS row   │
│                          │                                        │  (removable) │
│                          │                                        │  R5b thread  │
│                          │                                        │  R5c composer│
│                          ├────────────────────────────────────────┤              │
│                          │  R4 SELECTED-CONTEXT SLOT              │              │
│                          │  FlowChart(§5.1) + Sequence(§5.2) —    │              │
│                          │  present whenever R5a holds ≥ 1 chip   │              │
├──────────────────────────┴────────────────────────────────────────┴──────────────┤
│ FOOTER  scope attestation (what this slice is) · chip legend (proven/guess/fog)  │
│         indexed count · per-chart honesty tells                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
   POPUP LAYER (§4) floats above all regions; never claims R1–R5 space.
```

Region ownership never changes between personas — only panel contents, order, and defaults. This is deliberate: a user who changes preset keeps their muscle memory; the plan (`PersonaPreset` from charter §3) changes what fills R1/R3/R4.

## 2. The code viewer popup (NEW — user requirement)

**Trigger:** any click that resolves to a file/function/line under today's universal rule ("a fact with a location opens the location"). Replaces today's behavior of opening the full source panel immediately.

**Placement:** anchored card near the clicked element, clamped to the visible panel; if no room, centered modal. Max width 560 px. `Esc` closes; `Enter` opens the full source panel.

**Contents (top → bottom):**
1. Header — file icon + **relative path**, symbol `qual_name` (or the row's natural name), certainty chip (proven/good guess/not sure), element kind in plain words.
2. Snippet — the file around the resolved range: recorded byte range `start–end` **highlighted in the certainty color**, ±8 context lines above/below shown plainly (context lines visually unmarked; evidence lines carry the chip). Line numbers rendered; click a line number → copy `path:line`; hover → per-line evidence tooltip when the engine recorded one (quality findings' `evidence_lines`).
3. Actions row — `Open full source` · `Ask CIE about this` (pre-fills R5 with a question naming this exact symbol/file) · `Copy path:line` · chip state (`Added ✓` / `Add to context`).

**Data source:** identical to the full source panel (`/api/fs` + report/quality locs) — the popup is a clipped, anchored rendering of real file bytes; it invents nothing (honesty rule: context lines are visually distinct from evidence lines).

**What does NOT popup:** scope-level clicks (containers, stores, unresolved buckets) re-slice the display; they have no single location to show — see choreography.

## 3. Choreography verbs (click effects, defined once)

A click composes these; each persona table references them:

| Verb | Effect on the whole display |
|---|---|
| `SCOPE(kind, id)` | breadcrumb appends; R1 re-renders focused; every R3 panel re-slices to the same scope; footer attestation updates; the C4's other boxes dim to outline style. Scope levels: system → container → module (file) → symbol. A second click on the same element **releases** one level (today's rule) |
| `FOCUS(el)` | lights the clicked box's connections across panels; dimming is styling only — never changes data |
| `POPUP(el)` | opens the code viewer popup (§2); the display beneath is untouched |
| `CHIP(el)` | adds element to R5a context chips (dedupe by element id; cap 24 — the 25th click removes the oldest and says so); R4 charts recompute **only** when `CHIP` changes the selected set |
| `ASK(el)` | pre-fills R5c with that element's question; user still presses send |
| `REGATE()` | Facts-only toggle: every panel re-gates; inferred elements fade with their reasons printed; footer recounts facts/inferences/fog |
| `RELEASE()` | breadcrumb steps back; panels re-widen; R4 persists (it is context-driven, not scope-driven) |

**Global rules binding all personas (normative):**
- R1. The grouped-out box (`…and N more`) is never a scope — today it refuses (`atlas-view.tsx:216`); under RFC-0023 it opens a **disclosure popup listing the grouped elements**, each row clickable as its own element. `NEW`
- R2. The unresolved-names bucket is evidence, not a scope (`:246`) — unchanged forever.
- R3. Chip and Scope are independent state: `Release` keeps chips; removing a chip never changes scope. R4 charts follow chips only.
- R4. Every action a click performs is disclosed in the footer (which scope became which; what the chip set holds). Silent UI is forbidden.

## 4. The two selected-context charts (R4 slot) — full spec

Appears whenever R5a holds ≥ 1 chip; shows an honest empty state otherwise: "select chart elements — click anything — and these two charts follow your selection." Both charts recompute within the client gate re-run (RFC-0023 §4.3). Their data source is the **PDG** (`Pdg::build`, `cie-relations` — built, tested, and idle exactly as RFC-0022 §2.1 documents: 10 relation kinds including `ControlFlow`, `DataFlow`, `Reads`, `Writes`, `Calls`, `Returns`) sliced to the selected functions/files.

### 4.1 FlowChart — control + data flow of the selected context (NEW chart)

One graph over the selected functions: **process boxes** (each selected symbol; file+line on the box), **decision diamonds** (each branch point the PDG's control dependence reaches: if/switch/loop conditions, rendered `cond` text truncated, file+line on the diamond), **data edges** (`Reads`/`Writes`, labeled, dashed=inferred), **call edges** (`Calls`, labeled `calls`), **store boxes** (data sinks). Clicking anything opens the popup at its exact line (§2) — **specifically, a decision diamond opens the source line where that decision is made** (user requirement, verbatim). Honesty: a decision is drawn only when the engine recorded its condition — `ENGINE: adapters must emit DecisionFact {sym, file, line, cond}` (start Java + Python; Kotlin/TS follow). Until then the chart draws process + data edges and prints "decisions not indexed in this slice" — never a guessed diamond.

### 4.2 SequenceChart — the selected context, in order (NEW mode of today's sequence)

Today's SequenceSVG recentres on a selection; R4 mode fixes the cast to the selected elements only: lifelines = the selected symbols' owning modules/actors; messages = recorded calls **between** them (arrow → popup at the call site); the first message starts at the selection's entry (the call-least-selected node — disclosed as "starts at the fewest-calls-in node").

## 5. Persona 1 — the onboarding fresher (the full flagship walk)

**Defaults:** preset `Fresher`; scope = system until first click; Facts-only **OFF**; hero = Containers; planOrder: Containers → Entries/Routes → Tree → Components → Flow → Dataflow; R4 hidden until first chip.

### 5.1 Layout they see (d1 screenshot, 2026-10-23 — matches today's build)

```
R1 Containers (hero)      R3 row-1: entry-point table · route table
                          R3 row-2: concept tree · components
   + Ask-CIE dock  R5     R3 row-3: flow · dataflow        (R4 absent — no chips yet)
```

### 5.2 Freshman's first-30-minutes choreography (every element, whole-display effects)

| Click on… | Choreography |
|---|---|
| **Container box** (e.g. `alerting`) | `SCOPE(container, alerting)` + `CHIP` + `FOCUS`: hero re-renders with alerting's internals; entry/route tables filter to boot/route of that app; tree collapses to the alerting branch (others outlined); components → alerting's Service/Subdomain tier with its module lines; flow/dataflow re-slice; footer prints the scope line; dimming of untouched apps |
| **Container box, a second time** | `RELEASE` to system (breadcrumb steps back) |
| **`…and N more` app box** | `NEW` disclosure popup: the grouped apps listed with file-counts; clicking a row runs the container choreography |
| **Boot row** (`AmsApplication`) | `SCOPE(container, that app)` + `POPUP` at `AmsApplication.java:12` (snippet w/ main) + `CHIP` |
| **Route row** (`POST /ams/api/head-accounts`) | `POPUP` at the controller line + `FOCUS` recentres Sequence; `CHIP` (route row, today: exact-line open, never cross-file guess) |
| **Component box** | `SCOPE(module, first module)` + `POPUP` at the module's first symbol line + `CHIP`; R3 flow/dataflow re-slice to the component's files |
| **Module line inside a box** | `POPUP` (file head) + `CHIP` — no scope change (a line is a fact, not a scope) |
| **Symbol box** (module zoom) | `POPUP` exact line + `FOCUS` + `CHIP` |
| **Tree node** | `SCOPE(branch)` + `FOCUS` + `CHIP` (today: only asks — upgraded to chip-first) |
| **Flow box** | `POPUP` + `CHIP` |
| **FlowChart decision diamond** (R4, when context chips exist) | `POPUP` at the **exact condition line** (the user's flagship requirement) |
| **Dataflow store** | `SCOPE(store)` + `CHIP` |
| **Dataflow arrow** | `CHIP(edge)` — today a no-op; NEW |
| **Sequence arrow** | `POPUP` at the call site (today's rule: arrows always open real code, `:1003`) |
| **Facts only ○** | `REGATE` — inferred components/tiers fade with reasons; footer recounts |
| **Ask CIE after chips** | the question is answered **only from the chips' code space** (RFC-0023 §9-R5); the answer's header states the scope; "outside the selection" answers disclose + offer the whole-project chip |

## 6. Persona 2 — reviewer / staff engineer

**Defaults:** preset `Reviewer`; Facts-only **ON**; hero = **Impact graph** (the affected set as boxes: symbols → concepts cascade, min test set side-table); planOrder: Impact → Callers → Drift(state) → Test confidence → Dataflow(writes-first, C when stores exist). Scope default: the container owning the current diff (from the selection/jobs); R4 always visible when chips exist.

Choreography identical to §5.2 plus:
- **Impact-box click** → `POPUP` exact line + `CHIP`; affected-concepts edge lights the concepts branch.
- **Min-test-set row** → `POPUP` (test file/function) + `CHIP`.
- **Drift-status row** → `POPUP` the claim's evidence pair (comment line + code line — two locs; popup shows both, side-scroll) `NEW`.
- **Dataflow write arrow** → `POPUP` at the writing line; the panel prints "this write crosses a container boundary" when the stack facts say so.

## 7. Persona 3 — architect / maintainer

**Defaults:** Facts-only **OFF**; full-detail caps (expanded = full everywhere, `26718a2`); hero = **dependency graph (E5)** — modules/containers as boxes, cross-layer edges; planOrder: DepGraph → Bounded contexts (C5) → Duplication (overlap chord/table) → Drift board (FMEA view) → Timeline (I5) → Risk treemap (H5).

Choreography: §5.2 shared, plus:
- **Dep-graph box** → `SCOPE(container/module)` + `CHIP`; **dep-edge click** → `CHIP(edge)` + popup offers both endpoints' lines (two locs side-scroll) `NEW`.
- **Duplication chord ribbon** → `CHIP` + popup lists the shared functions (each row popups its own line).
- **Timeline marker** → `RELEASE`-style revision jump: panels re-slice to that revision (attested in the footer) `ENGINE: timeline route for the UI`.
- **Treemap cell** → `POPUP` (file head) + `CHIP`.

## 8. Persona 4 — auditor / security reviewer

**Defaults:** Facts-only **ON**; hero = **trust boundary map (I3)** `ENGINE`; planOrder: TrustBoundary → SystemRouteTable (every verb/path of the system, no container filter) → Externals (C4, image-only services emphasized) → Secrets/risk findings → Dataflow (writes across files emphasized) → Policy matrix (G3).

Choreography: §5.2 shared, plus:
- **External box** → `FOCUS` + popup with the compose record (ports/image/evidence) + `CHIP` (a service without build evidence popups the YML lines — recorded evidence, never invented).
- **Route row** — same as fresher, but the table is system-wide (no container filter) and each row carries "crosses from external?" marks.
- **Finding row** → `POPUP` the exact evidence line (`quality.json` line numbers) + `CHIP`.
- **Policy row** → `POPUP` invariant's enforcing line; prints asserted/enforced honestly.

## 9. Persona 5 — AI agent (no layout, by design)

There is deliberately no agent screen: the agent consumes the **same ChartPlans** as structured claims + evidence ids (RFC-0023 §9-R5) over MCP (`US-134`). The Atlas exists as what the human sees of the same answer. The only spec obligation: every element id and evidence loc in this doc must survive into the MCP envelope — the two consumers never fork vocabularies.

## 10. Persona 6 — consultant on a foreign codebase

**Defaults:** preset = Fresher + Knowledge pack; scope = system, then auto-focus "busiest container by file count" (disclosed, §charter §9). planOrder: Fresher's six + **Ownership map (K1)** → **Co-change matrix** → **Journeys (J1/J2)** `ENGINE: per-author export; journey extraction from routes+stories`.

Choreography: §5.2 shared, plus:
- **Ownership cell (author × area)** → `CHIP` + popup lists that author's files in the area.
- **Co-change ribbon (a↔b)** → `CHIP(edge)` + popup lists `supporting_commits`.
- **Journey step** → `POPUP` (the step's code) + route-table highlight.

## 11. The engine-gaps ledger (everything tagged ENGINE, one line each — no guessing for a coder)

| Gap | Needed for | Smallest honest build |
|---|---|---|
| `DecisionFact {sym, file, line, cond}` per adapter | FlowChart diamonds (§4.1) | Java + Python adapters first (both parse bodies deeply today); until then diamonds honestly absent |
| Pdg into production (already built/tested, §RFC-0022 §2.1) | FlowChart + Impact + calls-closure | wire `relation_graph_from_facts` into the analyze job's exports |
| Timeline route for UI | Architect timeline | CLI emits `timeline.json` from `cie-incremental` |
| `cochange --authors` | Ownership map | git log -p author stats into cochange.json |
| Journey extraction (J1/J2) | Consultant | routes + stories → ordered journeys |
| Trust-boundary facts (I3) | Auditor hero | routes + externals → boundary edges |
| Per-claim second loc (doc↔code pair) | Reviewer popup pair | drift.json claims carry both evidence refs |

## 12. Worked example — the fresher, 07 clicks (so reviewers of this spec can run it)

`C4.click(alerting)` → breadcrumb [System ▸ alerting], chip `alerting` — tables filter, tree collapses. `routeRow.click(POST head-accounts)` → popup at `HeadAccountController.java`; chip 2. `Enter` (popup) → full source panel, dock stays. `componentBox.click(alerting service)` → scope module `DynamicRCAEngine`, popup at its first line, chip 3. `flowBox.click(RCAOrchestrator)` → chip 4 → **R4 slot appears**: FlowChart of `DynamicRCAEngine+RCAOrchestrator+…` (process boxes + reads/writes edges; "decisions not indexed" until DecisionFact lands) + SequenceChart of the calls between the two. `decision.click` (once available) → popup at `if (calibrationConfidence < 0.7)` exact line. `chip(2).✕` → R4 reflows to 3 elements, scope unchanged. Footer throughout: every one of these actions stated in plain words, chips counted.

---

## 13. Contract-capability evaluation — can the ViewSpec carry this spec?

Question: does the sealed contract — `ViewSpec` v2 (`viz-adapter/src/types.ts:120-139`), the Rust producer (`cie-viz/src/produce.rs`), and the engine-adapter contracts — have the *shape* to express everything §1–§12 requires, and can the viz-manager render these interactions over it? Verdicts below cite exact fields (`types.ts` line ranges from this session's reads).

**Overall verdict: ~90% of the spec is expressible in the contract as it stands.** Three additive patches carry the rest (`➕`), the interaction vocabulary deliberately stays OUT of the contract (`✅` verdicts of a different kind), and the genuinely missing pieces are producer emissions, not schema (`ENGINE`).

| Spec capability (§ ref) | Contract field today | Verdict |
|---|---|---|
| Element identity for chips + popup + path:line (§2, §3-CHIP/POPUP) | `ViewElement.id` + `evidence {file,start,end}` (`types.ts:31-36, :19-23`) | ✅ exact |
| Drill chains container→module→symbol (§3-SCOPE) | `ViewElement.parentId` (`:42`) | ✅ |
| FlowChart decision diamonds (§4.1) | open `elementKind` vocabulary (`:32`, open map; `g6.ts:31` "adding a kind is adding ONE entry") + `label` carries the condition + `evidence` carries the line | ✅ **no schema change** — needs `ENGINE: DecisionFact` emission only |
| Data edges labeled reads/writes; call edges; stores/externals (§4.1) | open `edgeKind` (`:50`) + open element kinds + `fields` | ✅ |
| Component-board body lines (§4, charter §4.2) | `fields: Record<string, unknown>` (`:41`) | ⚠️ expressible — 🌈 requires gate rule: body-lines must carry evidence too (formalize as `fields.bodyLines` with a gate check) |
| Grouped-box disclosure popup listing grouped elements (§3-R1) | `hidden[]` ledger exists: `HiddenCandidate {id, reason, salienceWeight}` (`:67-71`) | ➕ additive: `label`+`kind` on HiddenCandidate so a disclosure row is clickable |
| Drift claim pairs (doc line + code line, §6) | single `EvidenceRef` per element/edge | ➕ additive: `fields.alsoEvidence?: EvidenceRef[]` |
| Facts-only filter + recount (§3-REGATE) | `Preferences` (`:96-101`) + TS gate + `EpistemicFooter` recount (`:73-80`) | ✅ — the slice re-gate design (RFC-0023 §4.3) already prescribes it |
| Scope attestation per slice (footer) | `ScopeAttestation {revision, environments, coverage}` (`:82-89`) | ✅ |
| Which charts to draw, with which actions (drill/filter) (§8, §9-R3) | **NOT in the ViewSpec — correctly.** `planSheet` (manager) derives actions from chartKind × elementKind | ✅ architectural: the producer stays interaction-blind — the VIZ-A-01 law ("CIE never knows what a chart is") extends naturally: **CIE never knows what a click is either**; the ChartPlan owns actions |
| Selection subscription (chips echo engine↔app, §3-CHIP) | `RenderHandle {engineId, semanticHash, renderHash, layout, dispose}` (`:189-194`) — no events yet | ➕ additive (designed in RFC-0023 §4.1): `onSelection(cb)` + `setSelected(ids, origin)` |
| Chart-list derivation (viz-manager, R2) | negotiator chooses ONE engine per chart (`negotiator.ts:14-22`); catalog is data (`catalog.ts`) | ✅ — `planSheet` is the same pure-function style; no contract change |
| Single-chart interactions: pan/zoom/click/fisheye/export | G6 capabilities declared (`g6.ts` CapabilitySet) + **proven live** (`concept-tree-g6.tsx:115-150`); LOD set (`interaction.ts` US-022) | ✅ — remaining work is the render/relayout BODIES (stage 1), not capability |
| Multi-chart conductor | not implemented anywhere yet | ➕ client-side bus (RFC-0023 stage 4); no contract change |
| Selected-set re-gating + hash stability | `ELEMENT_CAP = 2_000` (`gate.ts:12`) + semantic/render hash split (`:130-138`) + cross-language hash specs pass | ✅ — a selection's FlowChart+Sequence are comfortably inside the cap |
| Producer element emission for the new charts | `produce.rs:211` emits concept-node elements only (`c<id>`) | `ENGINE` — produce gains a flow-chart producer over Pdg facts + DecisionFacts; schema untouched |

### 13.1 The three additive patches, exactly

1. **`RenderHandle.onSelection` / `setSelected`** — the only *behavioral* contract extension; additive, both directions, loop-guarded.
2. **`HiddenCandidate.label` + `.kind`** — makes the `…and N more` disclosure popup real (rows clickable) without weakening the seal (the ledger's ids already resolve to full elements when un-hidden).
3. **`fields.alsoEvidence[]` + a formalized `fields.bodyLines[]` gate rule** — the only multiplicity and narration the spec needs; the gate gains two checks, error codes shared with Rust as usual.

Nothing else changes the wire contract. The producer gains *emissions*, the manager gains *a pure function*, the engines gain *render bodies*, the client gains *a conductor* — four work packages, all inside the law that the producer emits epistemics and never pixels, actions, or clicks.