# RFC-0024: The persona Atlas — implementing UX-spec rev 2 (presets, chips, popup, selected-context charts)

**Status:** approved-for-implementation (not yet taken up; nothing else covers this work)
**Author:** the CIE maintainer (with pi)
**Date:** 2026-10-23
**Supersedes for UI execution:** RFC-0023 §5/§11 *stage ordering* (stages 4–6 map onto this RFC's stages 3–5); RFC-0023 remains normative for the engine/contract architecture and its §13.1 three patches are re-stated here as stage 0 — **the two RFCs ship as one program: one owner, coordinated stages.**
**Normative inputs:** `docs/atlas-ux-spec.md` (rev 2 + §14 persona sub-menu — THE spec this implements, section by section), `docs/atlas-charter.md` §3 (PersonaPreset), `docs/chart-inventory.md` (tier receipts), `docs/rfc-0023-js-owns-pixels.md` (contract patches, negotiator, conductor, scoped answering).

---

## 1. What this RFC builds, in one paragraph

The Atlas becomes **persona-aware**: five human presets (Fresher/Reviewer/Architect/Auditor/Consultant) selectable from a top-bar **sub-menu** (ux-spec §14); panels derive from a pure `planSheet(viewSpec, preset)` instead of hand-code; every chart-element click speaks the closed choreography vocabulary (§3 verbs) — **auto-chips** in the Ask-CIE dock, the **code viewer popup** (single + paired variants, ux-spec §2/§2.2) replaces jump-to-source, and the **R4 selected-context slot** draws FlowChart (control+data, decision diamonds, exact-line popups) and SequenceChart over the user's clicked elements. The G1–G6 global rules and the G5 collapse rule are normative. All rendering goes client-G6-canvas over the sealed ViewSpec contract with exactly three additive contract patches.

## 2. Re-stated from RFC-0023 §13.1 — the three contract patches (ship FIRST, everything depends on them)

1. `RenderHandle.onSelection(cb)` + `setSelected(ids, origin)` — selection subscription, loop-guarded.
2. `HiddenCandidate.label` + `.kind` — G1's disclosure rows become clickable.
3. `fields.alsoEvidence?: EvidenceRef[]` + the formalized `fields.bodyLines[]` gate rule — paired popups and board cards; two new gate checks, error codes shared with Rust.

## 3. The file map — every file this RFC touches (`nothing left to guess`)

### 3.1 Rust engine (stage 0.5 — data the UI cannot fake)

| File | Action | Stage | Role |
|---|---|---|---|
| `cie-relations/src/lib.rs` | MOD | 0.5 | export `relation_graph_from_facts` on the production path (RFC-0022 §2.1 wiring) — FlowChart + Impact data source |
| `cie-viz/src/produce.rs` | MOD | 0.5 | flow-chart producer mode over Pdg facts: process/store/external/decision elements, reads/writes/call edges (schema untouched; open kinds) |
| `cie-adapter-java/src/lib.rs`, `cie-adapter-python/src/lib.rs` | MOD | 0.5 | **`DecisionFact {sym, file, line, cond}`** emission from method bodies (both already parse bodies deeply) |
| `cie-viz/src/gate.rs` (or shared) | MOD | 0.5 | mirror the two new TS gate checks (bodyLines evidence, alsoEvidence) — stable codes |
| `cie-cli/src/main.rs` | MOD | 2 | `cie diff A..B --changed-files` → `changed_files.json` (ux-spec §6.1) |
| `cie-cli/src/main.rs` | MOD | 2 | `cie timeline` → `timeline.json` (from `cie-incremental`) — `SCOPE(rev)` |
| `cie-cli/src/main.rs` + `cie-history/` | MOD | 2 | `cochange --authors` (ownership K1) |
| `src/stores` under `J`OBS or `./out/`… — i.e. the analyze-jobs runner (web) | MOD | 2 | run pdg/timeline exports next to `viewspec.json` in the job's scratch; persist optional artifacts in `result.json` |
| `docs/implementation/rfc-0024.md` | NEW | each | the validation record, appended per stage (pattern of `docs/implementation/rfc-0022.md`) |

### 3.2 Contract package `viz-adapter/` (stage 0 — the three patches + kinds)

| File | Action | Stage | Role |
|---|---|---|---|
| `viz-adapter/src/types.ts` | MOD | 0 | patches 1–3: RenderHandle events, HiddenCandidate.{label,kind}, ViewElement.fields.alsoEvidence/bodyLines typing |
| `viz-adapter/src/gate.ts` | MOD | 0 | bodyLines-evidence + alsoEvidence checks; ELEMENT_CAP unchanged |
| `viz-adapter/src/catalog.ts` | MOD | 0 | `component-board` kind entry + per-kind `actions` field (`drill`/`filter` matrix — ux-spec §3/§8) |
| `viz-adapter/src/engines/g6.ts` | MOD | 1 | node kinds map gains `process/decision/store/board-card`; popup-relevant `loc` passthrough |
| `viz-adapter/test/renderhandle.selection.spec.ts`, `test/hidden.disclosure.spec.ts`, `test/gate.bodylines.spec.ts` | NEW | 0 | the additive-patch vitest suites (pattern: existing 14 files) |

### 3.3 Web — viz layer (stages 1–2)

| File | Action | Stage | Role |
|---|---|---|---|
| `web/src/lib/viz/g6-engine.ts` | NEW | 1 | G6 host runtime: real `G6Adapter.render/relayout` bodies (generalize `concept-tree-g6.tsx:110-150`), canvas, pan/zoom/fisheye/export |
| `viz-adapter/src/negotiator.ts` | MOD | 1 | `renderHeadless` gains the host-injected render call (page bundles G6; adapter stays headless-pure) |
| `web/src/components/cie/chat-chart.tsx` | MOD | 1 | render BODIES via the negotiator/G6 for graph kinds; SVG renderer demoted to parity harness (§ stage-0 acceptance of rfc-0023) |
| `web/src/lib/viz/slice.ts` | NEW | 2 | client-side per-scope ViewSpec slicing + re-gate + footer recompute (RFC-0023 §4.3) |
| `web/src/lib/chart-plan.ts` | NEW | 2 | **`planSheet(viewSpec, preset, {scope, viewport, elementSize})`** — pure; actions (drill/filter) per ux-spec tables |
| `web/src/lib/atlas-presets.ts` | NEW | 2 | the five `PersonaPreset`s (charter §3 shape: panels/scope/factsOnly/planOrder) |
| `web/src/components/cie/atlas-persona-menu.tsx` | NEW | 2 | §14 sub-menu (5 entries, per-line purpose text, 1–5 shortcuts, honest agent absence) |

### 3.4 Web — Atlas shell + interactions (stages 3–4)

| File | Action | Stage | Role |
|---|---|---|---|
| `web/src/components/cie/atlas-view.tsx` | MOD | 3 | panels-from-plan (§14.2-1): R1/R3 filled by `planSheet`; G5 collapse/restore; breadcrumb; verb-based click handlers replacing the four ad-hoc `on*Click`; disclosure popups (G1) |
| `web/src/components/cie/code-popup.tsx` | NEW | 3 | the popup (§2 single + §2.2 paired; Esc/Enter; copy; toggle button) |
| `web/src/components/cie/context-chips.tsx` | NEW | 4 | R5a plural chip row (dedupe/cap-24/pulse/✕) |
| `web/src/components/cie/selected-context-panel.tsx` | NEW | 4 | R4: FlowChart (§4.1: process/decision/store/external click semantics) + SequenceChart (§4.2 entry rule + TELL) |
| `web/src/lib/cie-store.ts` | MOD | 4 | `chatContext: ChatContextElement[]`, preset state, per-persona `sessionStorage` memory (§14.2-4), `atlasSelection` shape gains `rev?` |
| `web/src/components/cie/chat-panel.tsx` | MOD | 5 | chips row host; send payload carries the selection scope |
| `web/src/lib/scope-digest.ts` | NEW | 5 | `scopeDigest(...)` — facts sliced to the chips' code space (files ∪ symbols ∪ edges ∪ parentId closure; RFC-0023 §9-R5) |
| `web/src/lib/chat-digest.ts` | MOD | 5 | digest builder accepts the selection; header scope line |
| `web/src/app/api/chat/route.ts` | MOD | 5 | `body.scope` honored: system prompt gains "ONLY the selection's code space" + out-of-scope disclosure rule |
| `web/src/components/cie/topbar.tsx` | MOD | 3/2 | preset menu host + change-context picker (Reviewer/Consultant only) |
| `web/src/lib/cie-types.ts` | MOD | 3/5 | web mirrors of the additive types (`HiddenCandidate.label/kind`, `alsoEvidence`, `ChatContextElement`) |
| `web/src/components/cie/concept-tree-g6.tsx` | MOD | 4 | tree node click → `CHIP`+`SCOPE` (chip-first upgrade of today's ask-only) |

### 3.5 Explicitly NOT this RFC (named so a coder never searches)

G6 bundle optimization, mermaid/elk new engines beyond RFC-0023 stage 5, MCP executor finish (RFC-0021 §6.1), tour mode (fresher §4.2 "guided tour" — a follow-up story).

## 4. Stage plan + acceptance (each stage independently shippable)

| # | Stage | Acceptance (verifiable, no vibes) |
|---|---|---|
| 0 | contract patches + catalog kinds + gate checks | 3 new vitest suites green; existing 274 stay green; Rust gate mirrors compile |
| 0.5 | engine emissions (Pdg wiring, DecisionFact java+python, diff/timeline/authors verbs) | `cie viz` emits flow-chart elements on the polyglot sample; `decision` elems carry file+line; `cie diff` writes changed_files.json |
| 1 | G6 host runtime + render bodies | tree/uml/flow/c4 kinds render canvas-side via negotiator; click→`onSelection` fires; pan/zoom/fisheye live; SVG renderer still available as harness |
| 2 | planSheet + presets + slice + persona sub-menu | Atlas panels render FROM plans for all five presets on shahy + polyglot; switching discloses per §14.2-5; slice re-gates (gate codes printed) |
| 3 | popup + breadcrumb + verb handlers + G5 | the §12 worked example runs click-for-click on shahy-restructure (its 7 clicks = the acceptance list); paired popup on a drift claim |
| 4 | chips + R4 pair | chip row plural/dedupe/cap; R4 activates/collapses per G5; SequenceChart entry TELL |
| 5 | scoped answering | a chips-scoped question answers ONLY from the selection (verified against shahy: ask about account-management chips → no alerting facts in the answer); empty-chip = today's behavior; out-of-scope disclosure + one-tap whole-project chip |
| 6 | retire bypasses | direct paths deleted; catalog 1:1 (RFC-0023 stage 6); implementation record completed |

## 5. Honesty obligations carried from every upstream doc

- No decision diamond without a recorded `DecisionFact` (prints its absence);
- chips/scoped answers print "drawn from N files over M named things";
- the menu never offers the agent a screen (§14.1);
- every `…and N more` opens its ledger list; every click footer-disclosed (G4);
- scope attestation + revision printed on every panel (ScopeAttestation);
- the popup shows context lines styled apart from evidence lines — never fabricated.
## 6. The retirement list — classes deleted when the implementation completes

Everything the new path replaces, named per file, so a coder knows exactly what dies, when, and why — and what survives *demoted* (still executing, never on a bypass path). Deletion stages are the §4 stage numbers; every deletion has a verifying acceptance: **nothing is deleted before the ux-spec §12 worked example runs click-for-click against the canvas path on shahy-restructure plus all five presets render from plans.**

| Class / field | File | Fate | Stage | Why |
|---|---|---|---|---|
| `DiagramSVG` (uml/flow/c4 painter) | `web/src/components/cie/chat-chart.tsx` | **DELETED** | 6 | G6 canvas + engine adapters draw all graph kinds; the SVG body was the bypass |
| `SequenceSVG` | `chat-chart.tsx` | **DELETED** | 6 | SequenceChart draws via the engine over PDG slice |
| `MindmapSVG` | `chat-chart.tsx` | **DELETED** | 6 | mindmap kind routed through the adapter |
| the per-kind dispatch switch (`spec.kind === …` rows `:1039-1047`) | `chat-chart.tsx` | **DELETED** | 6 | negotiator routing replaces hand dispatch; `useAsk`/`ASK_HINT`/`ChartFootnote`/`ElementLoc` live on (host hooks, disclosures) |
| the four ad-hoc Atlas click handlers `onContainersClick` / `onComponentsClick` / `onDataflowClick` / `onFlowClick` | `web/src/components/cie/atlas-view.tsx` | **DELETED** | 3 | verb-composed handlers (§3 vocabulary) own clicks; the old ids survive as `atlasSelection` kinds |
| `concept-tree-g6.tsx` as a standalone bespoke component | `web/src/components/cie/concept-tree-g6.tsx` | **DELETED (pattern generalized first)** | 1→6 | its G6+compactBox wiring moves INTO `web/src/lib/viz/g6-engine.ts`; the tree kind then reads through planSheet like every kind; file dies only after canvas parity confirmed |
| `mapSelected: VizElement \| null` (singular referent) | `web/src/lib/cie-store.ts` + `chat-panel.tsx` chip UI + `sections.tsx` bindings | **DELETED** | 4 | replaced by plural `chatContext: ChatContextElement[]`; `MapInfoCard` keeps working off the context head (the last-picked chip) |
| `openChartOnCanvas`-era ad-hoc chart-on-canvas wiring in chat-panel | `chat-panel.tsx` | **demoted, not deleted** | 2 | chart-on-canvas becomes a planSheet case (an "on canvas" detail preset) — code survives behind the manager |
| statistical-kind painters (`FisheyeHBar`/fisheye math, `ChatDonut`, `ChatStat`, `ChatTable`, `Heatmap`) | `chat-chart.tsx` | **RELOCATED BEHIND `EngineAdapter` — NOT deleted** | 1 | statistical kinds are honest SVG-engines for print/export (per charter boundary: engine = chart engine, not necessarily G6); their code becomes the registered statistical engine's implementation; the fisheye lens contract (US-023) moves with it |
| throwing stubs `render/relayout` in `jointjs/reactflow/mermaid` adapters | `viz-adapter/src/engines/{jointjs,reactflow,mermaid}.ts` | **KEPT (declared-capability engines)** | — | headless until RFC-0023 stage 5; their throws are documented capability statements, not dead code |
| today's jump-to-source-on-click behavior | (behavior, not file) | **REPLACED** | 3 | popup-first (§2); full source panel = popup's `Enter`/button path |

**The promise this list enforces:** after stage 6 there is exactly ONE click path (verbs → conductor → engines → onSelection), exactly ONE chart-plan source (planSheet), and the SVG-in-product question is closed — statistical engines remain as *registered* renderers, while every bespoke bypass hand-route (`DiagramSVG` dispatch, four ad-hoc handlers, standalone tree component, singular referent) is gone from the tree.
