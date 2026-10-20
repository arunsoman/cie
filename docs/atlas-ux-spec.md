# Atlas UX spec — per-user-type layouts and complete click choreography (nothing left to guess)

**Date:** 2026-10-23 (rev 2 — builder audit applied: global rules renamed G1–G6; chip/popup semantics resolved; paired popup defined; verb vocabulary closed; R3/R4 exclusivity rule added; Reviewer scope made implementable)
**Status:** build spec
**Related:** `docs/atlas-charter.md` (charter; §12 = today's click behavior), `docs/rfc-0023-js-owns-pixels.md` (conductor, planSheet, chips; §9 = feasibility ids "RFC-0023 §9-Rn"), `docs/chart-inventory.md` (tier receipts), RFC-0022 §2.1 (the PDG — data source for decision boxes below)

**Reading key:** each persona gets (a) a LAYOUT — regions and what ships in them; (b) DEFAULTS; (c) a complete CLICK CHOREOGRAPHY — for every clickable element, what changes in every region; (d) what is new vs today. The choreography vocabulary is **§3 and it is CLOSED** — §3 lists every effect a click can produce; persona sections compose only these verbs. After this revision, any effect appearing in persona tables that is not a §3 verb is a bug in this spec. Every unmarked behavior exists today; every `NEW` tag must be built; every `ENGINE` tag names the exact data the engine must add. **Region ids (R1–R5, §1) and global-rule ids (G1–G6, §3) are the only two id namespaces in this doc; "RFC-0023 §9-Rn" is a third, always prefixed.**

## 1. The canonical layout frame (one shell, all personas)

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ TOP BAR   logo · persona preset [Fresher ▾] · scope breadcrumb                   │
│           [System ▸ alerting ▸ AlertService] · search · Facts only ○             │
│           change-context picker (Reviewer/Consultant presets only) · dock toggle │
├──────────────────────────┬────────────────────────────────────────┬──────────────┤
│  R1 HERO PANEL           │  R3 SECONDARY PANELS (scrolling grid,  │  R5 ASK-CIE  │
│  the persona's main map  │  persona-ordered by planOrder)         │     DOCK     │
│                          │                                        │              │
│                          │  R4 SELECTED-CONTEXT SLOT sits FIRST   │  R5a CONTEXT │
│                          │  in the R3 grid when it is active (G5) │  CHIPS row   │
│                          │                                        │  (removable) │
│                          │                                        │  R5b thread  │
│                          │                                        │  R5c composer│
├──────────────────────────┴────────────────────────────────────────┴──────────────┤
│ FOOTER  scope attestation · chip legend · indexed count · per-click disclosure   │
└──────────────────────────────────────────────────────────────────────────────────┘
   POPUP LAYER (§2) floats above all regions; never claims R1–R5 space.
```

Region ownership never changes between personas — only panel contents, order, and defaults. A user who changes preset keeps their muscle memory; the plan (`PersonaPreset`, charter §3) changes what fills R1/R3/R4.

**R4 is not a separate column — it is a position in the R3 grid (first row, spanning full width), rendered as the pair FlowChart + SequenceChart. Its activation rule is G5.**

## 2. The code viewer popup (NEW — user requirement)

**Trigger:** any click that resolves to a file/function/line (POPUP verb, §3). Replaces today's jump to the full source panel.

**Placement:** anchored card near the clicked element, clamped inside its panel's visible bounds; no room → centered modal. Max width 560 px. `Esc` closes; `Enter` opens the full source panel (the popup is a preview, the panel is the territory).

**Contents (single-location popup):**
1. Header — file icon + relative path, symbol `qual_name` (or the row's natural name), certainty chip (proven/good guess/not sure), element kind in plain words.
2. Snippet — the file around the resolved range; the **recorded byte range `start–end` highlighted in the certainty color**, ±8 context lines plainly styled (visually distinct from evidence lines; context is never fabricated fact). Line numbers rendered; click → copy `path:line`; hover → per-line evidence tooltip when the engine recorded one (`quality.json` `evidence_lines`).
3. Actions row —
   - **`In context ✓ (click to remove)` / `Add to context`** — one toggle button, honestly reflecting chip state (§3-CHIP is automatic on the click; this button is the popup's chip REMOVAL affordance — see §2.3).
   - **`Ask CIE about this`** — `ASK(el)`: pre-fills R5c with a question naming this exact symbol/file; user still sends.
   - `Open full source` — the panel path.
   - `Copy path:line`.

**Data source:** identical to the full source panel (`/api/fs` + report/quality locs). The popup invents nothing.

### 2.2 The paired popup (two evidence refs — NEW)

For elements that carry **two** recorded locations (drift claim: doc line ↔ code line, §6; dependency edge: from-line ↔ to-line, §7):

- Layout: two columns side-by-side (561–1080 px); **stacked** under 560 px effective width. Each column = its own header (`DOC` / `CODE` for drift; `FROM` / `TO` for dep edges), own line numbers, own copy control.
- Certainty: **one chip per column** — the doc-side chip states the claim's extraction certainty (`Fact` — the doc says so), the code side states the verification status (`Implemented/Partial/Drifted/…` from drift.json). For dep edges both columns carry the edge's own `displayMode`.
- `Esc` closes both; `Enter` opens the **primary** column in the full source (drift = the code side; dep edge = target side); a dedicated footer button `Open both in source` opens each column in its own source-panel tab.
- A paired popup is only drawn when BOTH refs exist in the artifact; a claim with no code-side match is a single-location popup showing the doc line with the status chip `Unverified` — never two blank columns.

### 2.3 Chip semantics (the rule that removes §2's ambiguity)

> **Every click that resolves an element to an identity auto-adds its chip.** The popup, when it opens, merely *displays* that a chip was added and offers removal. Therefore:
> - First click on an element = `POPUP` (if located) + `CHIP` — automatic.
> - Repeating a click on an already-chipped element = no new chip (dedupe), the existing chip pulses once in R5a.
> - The popup's toggle button, chip-row `✕` buttons, and cap eviction are the ONLY removal paths. Clicking an element never removes its chip.

## 3. Choreography verbs — the COMPLETE, CLOSED vocabulary

Every click composes these; persona sections reference them by id only.

| Verb | Effect on the whole display |
|---|---|
| `SCOPE(space, kind, id)` | breadcrumb appends; R1 re-renders focused; **every R3 panel re-slices to the same scope** (entry/route tables filter to that app; tree collapses to the branch, others outlined; flow/dataflow re-slice; secondary scopes: `container`, `module`, `symbol`, `store`); footer attestation updates; non-scoped C4 boxes dim. A second identical click `RELEASE`s one level |
| `SCOPE(rev, n)` | **time scope**: same as SCOPE(space) but on the revision axis — R3 panels re-render AT revision n; the footer attests the revision the slice is drawn from. Today's panels show head; `ENGINE` timeline route (§11) |
| `RELEASE()` | one spatial level back (breadcrumb steps back); panels re-widen; chips/R4 persist (G3) |
| `FOCUS(el)` | lights the element and its connections across all panels (arrows lit, counterparts highlighted); dimming is styling only — never changes data or scope |
| `TELL(text)` | prints a plain-words, evidence-derived fact banner in the clicked panel's telling line ("this write crosses a container boundary" — computed from the stack facts, cited). Never actionable, always sourced; the anti-silent-UI half of G4 |
| `POPUP(el)` | opens the code viewer popup (§2 / §2.2); display beneath untouched |
| `CHIP(el)` | adds the element to R5a (dedupe by element id, cap 24 — the 25th click evicts the oldest and says so); **automatic on every element-resolving click (§2.3)**; pulses on repeat clicks; R4 recomputes only when membership changes |
| `ASK(el)` | pre-fills R5c with that element's question (user still sends); exercised from the popup button (always available) and directly by elements WITHOUT a location (today's `pendingAsk` path stays for unlocatable bars/rows) |
| `REGATE()` | Facts-only toggle: every panel re-gates; inferred elements fade with reasons printed; footer recounts facts/inferences/fog |

### Global rules (normative, G1–G6)

- **G1.** The grouped-out box (`…and N more`) is never a scope. Under this spec it opens a **disclosure popup listing the grouped elements** (rows built from the `hidden[]` ledger — `HiddenCandidate` gains `label`+`kind`, §13 patch 2), each row acting as its own element. `NEW`
- **G2.** The unresolved-names bucket is evidence, not a scope (`atlas-view.tsx:246`). Unchanged forever.
- **G3.** Chip state and scope state are independent: `RELEASE` keeps chips; removing a chip never changes scope; the R4 slot follows chips only.
- **G4.** Every click's performed actions are disclosed in the footer (which scope became which; what the chip set holds; what a `TELL` is based on). Silent UI is forbidden.
- **G5. Exclusivity of subject — "why two flow charts on screen?" answered by construction:** the flow/sequence subject is **either the scope (R3 panels) or the selection (R4)**, never both visible at once. When R4 activates (first chip), R3's Flow/Dataflow/Sequence panels **collapse** into caption rows with counts ("flow — showing selection-scoped below"); expanding a collapsed panel re-opens it in place and pushes R4 below it in grid order (all panels coexist — G5 stops fighting, never deletes). When chips drop to zero, R4 disappears and the collapsed panels restore automatically. `NEW`
- **G6.** A click resolves to exactly one primary verb chain; every other visible change is styling, dimming, or `TELL` disclosure. No click performs two scope-changing acts.

## 4. The two selected-context charts (R4) — full spec

Present whenever R5a holds ≥ 1 chip (G5). Otherwise an honest empty: "click chart elements — these two follow your selection." Both recompute within the client gate re-run (RFC-0023 §4.3). Data source: the **PDG** (`Pdg::build` — built, tested, and idle exactly as RFC-0022 §2.1 documents: 10 relation kinds including `ControlFlow`, `DataFlow`, `Reads`, `Writes`, `Calls`, `Returns`) sliced to the selected functions/files.

### 4.1 FlowChart — control + data flow of the selected context (NEW chart)

One graph over the selected functions:
- **process boxes** — each selected symbol, its `file` + `line` printed on the box. CLICK → `POPUP` + `CHIP`;
- **decision diamonds** (`NEW`, `ENGINE`) — each branch point the PDG's control dependence reaches; diamond text = the condition (truncated), `file`+`line` printed. CLICK → `POPUP` at the **exact line where that condition is written** (the user's flagship requirement). A decision is drawn ONLY when the engine recorded a `DecisionFact {sym, file, line, cond}` — until a slice has none, the chart prints "decisions not indexed in this slice" (honest absence, never a guessed diamond);
- **data edges** — `Reads`/`Writes`, labeled, dashed = inferred. CLICK → `CHIP(edge)`;
- **call edges** — `Calls`, labeled `calls`. CLICK → `CHIP(edge)`;
- **store boxes** — data sinks inside the selection (sql files/`db`): they have **no single code line, so no popup — CLICK → `CHIP` + `FOCUS`** (same honesty as the Dataflow panel's stores; the carve-out is normative);
- **external boxes** — compose services reached in the flow: they DO popup — their recorded evidence is the compose YML lines (`STACK` artifact) — CLICK → `POPUP` at those YML lines + `CHIP`.

### 4.2 SequenceChart — the selected context, in order (NEW mode of today's sequence)

Lifelines = the selected symbols' owning modules/actors; messages = recorded calls **between** selected elements only (arrow → `POPUP` at the call site); the first message starts at the selection's **entry = the selected node with the fewest incoming calls from others in the selection** (computed, then `TELL`-disclosed: "starts at RCAOrchestrator — nothing selected calls it first"). No invented messages; gaps in the story print as the honest `…unrecorded hop` connector to the ledger.

## 5. Persona 1 — the onboarding fresher (the full flagship walk)

**Defaults:** preset `Fresher`; scope = system until first click; Facts-only **OFF**; hero = Containers; planOrder ≡ the R3 grid in reading order: **[R4 when active] → Entries/Routes → Tree → Components → Flow → Dataflow**; R4 inert until first chip.

### 5.1 Layout they see (d1 screenshot 2026-10-23 matches today's build)

```
R1 Containers (hero)      R3 [R4 when active] → entry table · route table →
   + Ask-CIE dock  R5        concept tree · components → flow · dataflow
   (footer as §1)
```

### 5.2 Freshman's complete choreography (every element, whole-display effects, verbs only)

| Click on… | Choreography |
|---|---|
| **Container box** (`alerting`) | `SCOPE(space, container, alerting)` + `POPUP`-no (scope-level; no single location, R2-class rule) + `CHIP` + `FOCUS`; R3 re-slices per §3-SCOPE list; untouched apps dim |
| **Container box again** | `RELEASE` to system |
| **`…and N more` app box** | `NEW` disclosure popup (G1): the grouped apps with file-counts; a row click = the container choreography |
| **Boot row** (`AmsApplication`) | `SCOPE(space, container, app)` + `POPUP` at `AmsApplication.java:12` + `CHIP` (auto, §2.3) |
| **Route row** (`POST /ams/api/head-accounts`) | `POPUP` at the controller line + `FOCUS` (recentres collapsed-R3 sequence when re-expanded) + `CHIP` |
| **Component box** | `SCOPE(space, module, first module)` + `POPUP` at its first symbol line + `CHIP` |
| **Module line inside a box** | `POPUP` (file head) + `CHIP` — no scope change (a line is a fact, not a scope) |
| **Symbol box** (module zoom) | `POPUP` exact line + `FOCUS` + `CHIP` |
| **Tree node** | `SCOPE(space, module-level, id)` + `FOCUS` + `CHIP` (upgrade of today's ask-only) |
| **Flow box** | `POPUP` + `CHIP` (+ `TELL` when the box owns recorded route/behavior facts worth telling) |
| **FlowChart decision diamond** (R4) | `POPUP` at the exact condition line |
| **Dataflow store** | `SCOPE(space, store, id)` + `CHIP` + `FOCUS` |
| **Dataflow arrow** | `CHIP(edge)` (+ `TELL` "crosses a container boundary" when the stack facts say so) |
| **Sequence arrow** | `POPUP` at the call site ("arrows always open real code", `:1003`) |
| **`Ask CIE about this` (popup button)** | `ASK(el)` |
| **Facts only ○** | `REGATE()` |
| **Element with no location (some bars/rows)** | `ASK(el)` directly (today's pendingAsk path) |

## 6. Persona 2 — reviewer / staff engineer

**Defaults:** Facts-only **ON**; hero = **Impact graph** (affected symbols → concepts cascade + min-test-set side-table); planOrder: **[R4] → Impact → Callers → Drift → Test confidence → Dataflow(writes-first)**; scope default = **system with the change-context picker empty** — see §6.1 (implementable definition).

Choreography = §5.2 shared verbs plus:
- **Impact-box click** → `POPUP` exact line + `CHIP`; the affected-concepts edge lights the tree branch (`FOCUS`).
- **Min-test-set row** → `POPUP` (the test file/function) + `CHIP`.
- **Drift-status row** → **paired popup** (§2.2: DOC|CODE columns; per-column chips; `Open both in source`).
- **Dataflow write arrow** → `POPUP` at the writing line + `TELL` "this write crosses a container boundary" when the stack facts say so.

### 6.1 The change context — implementable definition (replaces "from the selection/jobs")

The Reviewer preset's scoping source is a concrete artifact flow, not a mystery:

1. Top bar gains **`Change context [base…] [head…]`** (visible only in Reviewer/Consultant presets).
2. Choosing branches/populating it issues `ENGINE: cie diff A..B --changed-files → changed_files.json` (git branch support already exists in scan; the diff verb is the one addition, §11).
3. With a change context active: hero = impact of the changed files' symbols; every R3 panel scopes to the **owning containers of the changed files** (the same SCOPE choreography, seeded by data instead of a click); unchanged files dim; the footer attests the diff.
4. Without a context: hero = system impact (empty state: "pick the change to scope the review"), panels system-wide, nothing invented.

## 7. Persona 3 — architect / maintainer

**Defaults:** Facts-only **OFF**; full-detail caps everywhere (`26718a2`); hero = **dependency graph (E5)**; planOrder: **[R4] → DepGraph → Bounded contexts (C5) → Duplication (overlap chord/table) → Drift board (FMEA) → Timeline (I5) → Risk treemap (H5)**.

Choreography = shared verbs plus:
- **Dep-graph box** → `SCOPE(space, container/module)` + `CHIP`; **dep-edge click** → **paired popup** (§2.2 FROM|TO) + `CHIP(edge)`; edge between containers → `TELL` "these two containers already call each other N times — the boundary is soft" when the call facts say so.
- **Duplication chord ribbon** → `CHIP` + popup listing the shared functions (each row = its own `POPUP`).
- **Timeline marker** → `SCOPE(rev, n)` (NOT `RELEASE` — different axis; §3).
- **Treemap cell** → `POPUP` (file head) + `CHIP`.

## 8. Persona 4 — auditor / security reviewer

**Defaults:** Facts-only **ON**; hero = **trust boundary map (I3)** `ENGINE`; planOrder: **[R4] → TrustBoundary → SystemRouteTable (system-wide, no container filter) → Externals → Findings (secrets/risk) → Dataflow (cross-boundary writes) → Policy matrix (G3)**.

Rendering rule (not a click verb): **the hero always emphasizes image-only external services** (style, from the stack facts). Choreography = shared verbs plus:
- **External box** → `FOCUS` + `POPUP` at the compose YML evidence lines + `CHIP`.
- **System route row** → fresher's route choreography; each row `TELL`s "external entry" when its route is the first hop after an external.
- **Finding row** → `POPUP` the exact evidence line (`quality.json` line numbers) + `CHIP`.
- **Policy row** → `POPUP` the invariant's enforcing line + `TELL` asserted/enforced honestly.

## 9. Persona 5 — AI agent (no layout, by design)

No agent screen exists, deliberately: the agent consumes the **same ChartPlans** as structured claims + evidence ids (RFC-0023 §9-R5) over MCP (`US-134`). The Atlas is what the human sees of the same answer. Spec obligation: every element id and evidence loc named in this doc must survive into the MCP envelope — the two consumers never fork vocabularies.

## 10. Persona 6 — consultant on a foreign codebase

**Defaults:** preset = Fresher + Knowledge pack; scope = system, then auto-focus the **busiest container by file count** (computed from stack facts, `TELL`-disclosed on the hero: "auto-focused: 40 files"). planOrder: fresher's six + **Ownership (K1)** → **Co-change matrix** → **Journeys (J1/J2)** `ENGINE`. With a change context, consultant behaves as Reviewer.

Choreography = shared verbs plus:
- **Ownership cell (author × area)** → `CHIP` + popup listing that author's files in the area (`POPUP`-list variant `NEW`).
- **Co-change ribbon (a↔b)** → `CHIP(edge)` + popup listing `supporting_commits`.
- **Journey step** → `POPUP` (the step's code) + `FOCUS` (route-table highlight; the step's route row lights).

## 11. The engine-gaps ledger (every ENGINE tag, one line each)

| Gap | Needed for | Smallest honest build |
|---|---|---|
| `DecisionFact {sym, file, line, cond}` per adapter | FlowChart diamonds | Java + Python first (both parse bodies today); until then honest absence printed |
| Pdg into production (RFC-0022 §2.1) | FlowChart + Impact + selection closures | wire `relation_graph_from_facts` into the analyze job's exports |
| `cie diff A..B --changed-files` | §6.1 change context | git-aware diff export → `changed_files.json` |
| `cie timeline` route→UI | `SCOPE(rev, n)` | CLI emits `timeline.json` from `cie-incremental` |
| `cochange --authors` | Ownership (K1) | git log -p author stats into cochange.json |
| Journey extraction (J1/J2) | Consultant | routes + stories → ordered journeys |
| Trust-boundary facts (I3) | Auditor hero | routes + externals → boundary edges |
| Per-claim second doc loc pair | paired popup | drift.json claims carry both evidence refs |

## 12. Worked example — the fresher, 07 clicks (runnable against this spec)

1. `C4.click(alerting)` — `SCOPE(container)` + `CHIP` + `FOCUS`: breadcrumb [System ▸ alerting]; tables filter; tree collapses; flow/dataflow re-slice; footer attests.
2. `routeRow.click(POST /ams/api/head-accounts)` — popup `HeadAccountController.java` (highlighted range = the route declaration); `CHIP` (auto); `FOCUS` recentres.
3. Popup `Enter` — full source panel opens; dock, chips, scope unchanged.
4. `componentBox.click(alerting service)` — scope → module `DynamicRCAEngine`; popup at first line; chip 3; **R4 activates (G5)**: R3 Flow/Dataflow/Sequence collapse to caption rows; FlowChart (process+data edges, honest "decisions not indexed") + SequenceChart (the two elements' calls) render; the sequence `TELL`s its entry node.
5. `flowBox.click(RCAOrchestrator)` — chip 4; the R4 charts re-flow around the grown selection.
6. `decision.click(freshness check)` — once `DecisionFact` lands: popup at that exact condition line.
7. `chipRow.✕(2)` — R4 reflows to 3 elements; scope unchanged (G3); footer discloses everything performed.

## 13. Contract-capability evaluation — can the ViewSpec carry this spec?

**Overall verdict updated for rev 2: ~90% expressible as-is; the rev-2 features added NO new schema needs beyond the three patches below** — the paired popup rides `alsoEvidence`, the disclosure popup rides the hidden-ledger patch, and `SCOPE(rev)` rides the existing timeline data requirement (already in §11's ledger).

| Spec capability (rev-2 cite) | Contract field today | Verdict |
|---|---|---|
| Element identity → chips/popup/`path:line` (§2, §3) | `id` + `evidence {file,start,end}` (`types.ts:31-36, :19-23`) | ✅ exact |
| Drill chains container→module→symbol (§3-SCOPE) | `parentId` (`:42`) | ✅ |
| Decision diamonds (§4.1) | open `elementKind` + `label`=condition + `evidence` line | ✅ no schema change — `ENGINE: DecisionFact` only |
| Reads/writes/call edges, stores, externals (§4.1) | open `edgeKind` + open kinds + `fields` | ✅ |
| Component-board body lines | `fields` bag | ⚠️ expressible; gate rule to formalize (`fields.bodyLines` must carry evidence) |
| Grouped-box disclosure rows (G1) | `hidden[]`: `HiddenCandidate {id, reason, salienceWeight}` (`:67-71`) | ➕ patch 2: `label`+`kind` |
| Two-location elements (paired popup, §2.2) | single `EvidenceRef` | ➕ patch 3: `fields.alsoEvidence?: EvidenceRef[]` |
| Facts-only + re-gate + recount (§3-REGATE) | `Preferences` (`:96-101`) + TS gate + `EpistemicFooter` | ✅ |
| Scope attestation (§ footer) | `ScopeAttestation {revision, environments, coverage}` (`:82-89`) | ✅ |
| Which charts + which actions | NOT in ViewSpec — correctly: `planSheet` derives them | ✅ architectural: the producer stays interaction-blind (VIZ-A-01 extended: CIE never knows what a click is); ChartPlan owns actions |
| Selection subscription (§2.3 auto-chip echo) | `RenderHandle {engineId, semanticHash, renderHash, layout, dispose}` (`:189-194`) | ➕ patch 1: `onSelection(cb)` + `setSelected(ids, origin)` (RFC-0023 §4.1) |
| planSheet / chart list | negotiator = one chart (`negotiator.ts:14-22`); catalog = data | ✅ same pure-function style; no contract change |
| Single-chart interactions | G6 CapabilitySet + proven live (`concept-tree-g6.tsx:115-150`) | ✅ bodies remain stage-1 work |
| Multi-chart conductor + G5 collapse rule | none yet | ➕ client bus (RFC-0023 stage 4); no contract change |
| Selected-set re-gating + hash stability | `ELEMENT_CAP = 2_000` (`gate.ts:12`) + hash split (`:130-138`) | ✅ |
| Producer emissions for new charts | `produce.rs:211` emits concept-node elements only | `ENGINE` (flow producer over Pdg + DecisionFact), schema untouched |

### 13.1 The three additive patches, exactly

1. **`RenderHandle.onSelection` / `setSelected`** — behavioral, additive, loop-guarded.
2. **`HiddenCandidate.label` + `.kind`** — G1's disclosure rows become clickable.
3. **`fields.alsoEvidence[]` + the formalized `fields.bodyLines[]` gate rule** — the only multiplicity/narration the spec needs; two new gate checks, error codes shared with Rust.

The producer stays epistemics-only; the manager owns charts+actions; engines own pixels+events; the conductor owns sync — four work packages, all inside the law.