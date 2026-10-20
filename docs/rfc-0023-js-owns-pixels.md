# RFC-0023: JS owns the pixels — one rendering engine over the sealed ViewSpec

**Status:** draft-for-blessing
**Author:** the CIE maintainer (with pi)
**Date:** 2026-10-23
**Applies to:** `viz-adapter/`, `web/` (`chat-chart.tsx`, `chat-chart.ts`, `atlas.ts`, `concept-tree-g6.tsx`, `analyze-jobs.ts`, `cie-store.ts`), `cie-viz`
**Related:** CIE-VIZ PRD (`docs/VIZ-MULTI.md`, `docs/PRD-v1.0.md`), cie-viz crate doc (`cie-viz/src/lib.rs:1-16`), RFC-0019 (per-scope concept tiers), RFC-0020 (five-claims export)

---

## 1. Summary

The proposal: **the Rust side only ever produces a sealed `ViewSpec`; JavaScript owns all drawing** — G6, mermaid, elk (as a layout), or any other engine, negotiated per chart, with single-chart interactivity *and* multi-chart interactivity. Building this engine first, before porting any Atlas chart to it, is not just acceptable — it is the only order in which nothing regresses.

The central finding of this RFC: **~80% of this engine already exists, tested and idle.**

- The contract half is *fully* built: `viz-adapter/` (renamed `cie-viz-engines`) carries ViewSpec v2 types, the honesty gate, the semantic/render hash split, a pure negotiator, a 76-chart catalog, a11y outline, disclosure, and interaction models — **274 vitest cases, all green in under a second** at `viz-adapter/test/`.
- The host-runtime half has exactly one proof: `web/src/components/cie/concept-tree-g6.tsx` (167 lines) already renders one of the eleven chart kinds with **real `@antv/g6` 5.1.1** (`web/package.json:18`), pan/zoom/click/ask interactivity — the exact pattern needing generalization.
- What is missing is the wiring between them: the adapter's `G6Adapter.render()` **throws** (`viz-adapter/src/engines/g6.ts:298-304` — "requires the `@antv/g6` host runtime"), ten of eleven chart kinds are still painted by the ad-hoc SVG renderer (`chat-chart.tsx`), and nothing wires a scope-driven *conductor* across multiple simultaneous charts.

So this RFC is not a port. It is the completion of a bridge that was designed from day one to be completed (`CIE-VIZ PRD`: "geometry is G6's responsibility, reached through the TypeScript adapter (`viz-adapter/`, the only bridge)").

## 2. Motivation — why the current split costs more every month

Three honesty dialects enforce the same rules today, and two rendering stacks:

| Layer | What it does | What it lacks |
|---|---|---|
| `cie-viz` (Rust) | produces sealed ViewSpec, gate, hash split | produces one whole-repo map; no pixels by law |
| `viz-adapter` (TS, headless) | negotiator, gate (same stable error codes as Rust), hash recompute, catalog, outline, audit | no DOM runtime — `render()` throws in every engine adapter |
| `chat-chart.tsx` (TS, SVG) | the only real renderer; 274 lines of pan/zoom proven; narrative cards, chips, tells | ad-hoc per-kind code; no fisheye, no export, no negotiation, no disclosure, no hash |

The Atlas panels then bypass the adapter for ten kinds, and each new interactivity feature (the Facts-only filter, the max/summary dial of RFC work `26718a2`) is re-implemented against the ad-hoc stack rather than the contract. Every month this persists, the distance between the two stacks grows and the eventual port gets more expensive. Meanwhile the web *already* bundles G6 and runs `cie viz` on every analyze job (`analyze-jobs.ts:828-840` — `viewspec.json` is produced, loaded, consumed only as data) — the substrate for the full path is deployed but dormant.

## 3. What exists vs. what is missing

### 3.1 Exists (with receipts)

1. **The wire contract** — `viz-adapter/src/types.ts`: ViewSpec v2, `EngineAdapter` (`render/relayout/audit`, `:214-217`), `RenderHandle` (`:189-194`), `CapabilitySet`, `EngineLimits`, `LayoutOutcome`.
2. **The gate** — `viz-adapter/src/gate.ts`: the single honesty enforcement point, stable error codes "shared with our TS adapters" (`cie-viz/src/lib.rs:15`); runs on every render path.
3. **The hash split** — `viz-adapter/src/hash.ts`: `semanticHash` (cache key; layout swaps never invalidate) / `renderHash` (artifact key) — Rust computes, TS recomputes and can compare (already proven cross-language: `test/cross-language.hash.spec.ts`).
4. **The negotiator** — `viz-adapter/src/negotiator.ts`: pure `chooseEngine` (capability gate → user preference → size limits → deterministic order `g6 → jointjs → reactflow → mermaid`), every deviation disclosed.
5. **The catalog** — `viz-adapter/src/catalog.ts`: 76 chart kinds as data.
6. **Engine adapters** — `engines/{g6,jointjs,reactflow,mermaid,flame}.ts`: declared capabilities/limits, ViewSpec → engine-shaped data adaptation, config builders that already emit **G6 v5-shaped options** (`engines/g6.ts` `graphOptions` — node/edge style closures match the v5 API `concept-tree-g6.tsx:115-123` uses live).
7. **One real host engine** — `concept-tree-g6.tsx`: real G6 5, `autoFit`, `drag-canvas`/`zoom-canvas` behaviors, click→ask interactivity, ResizeObserver relayout. The template.
8. **The per-job pipeline** — `analyze-jobs.ts` already runs `cie viz` and the store already loads `viewspec.json` (`cie-store.ts:86, :233`).

### 3.2 Missing (this RFC's build)

1. **A real DOM runtime per engine** — the `render(container, spec, options) → RenderHandle` bodies. Today all engines throw.
2. **A `chat-svg` engine** — the existing `chat-chart.tsx` renderer, packaged *as* an `EngineAdapter`, so nothing regresses while G6 lands.
3. **Catalog kinds for Atlas narrative views** — the Atlas "component board" (component cards listing their module lines, grouping box beyond a cap, spec-level title/tell/source) is not expressible as a ViewSpec chart kind. It needs one additive view kind (say `component-board`) carrying per-element body-lines; once in the catalog it is negotiable, disclosed, hashed like every other chart.
4. **Per-scope ViewSpecs.** `cie viz` seals one whole-repo map; Atlas needs a view per scope click (39 containers in shahy-restructure). Decision: **slice client-side, gate client-side** (§4.3) — no server roundtrip per click, honesty re-enforced from the sealed set rather than re-fabricated.
5. **The conductor — multi-chart interactivity.** Engines own single-chart interaction; the Atlas contract is *cross-chart*: click a container → five panels rescope. This is a layer **above** engines: an interaction bus mapping engine-emitted selection events to app scope state (and app scope state → `Preferences`/`spec` re-issue into every engine). `viz-adapter`'s `interaction.ts` already models the single-chart half.

## 4. Design

### 4.1 The contract stays; the bodies get filled

`EngineAdapter` is already the right shape. We fill `render()`/`relayout()` per engine, and extend `RenderHandle` additively with the one thing the conductor needs:

```ts
export interface RenderHandle {
  /* existing: engineId, semanticHash, renderHash, layout, dispose() */
  onSelection(cb: (sel: { ids: string[]; kind: string } | null) => void): void;  // engine → app
  setSelected(ids: string[], origin: "engine" | "app"): void;                    // app → engine, loop-guarded
}
```

Single-chart interactivity then = whatever the engine natively gives (fisheye/zoom/export for G6; free for SVG). Multi-chart interactivity = the conductor listening to one handle and re-issuing specs to the others — reusing the exact state flow `atlas-view.tsx` already runs (`scopeId` → five `body()` routes).

### 4.2 The conductor owns the Facts-only filter and the detail dial

`Preferences` is already negotiated input. `factsOnly: true` and `detail: "full"` become preferences carried on the request, filtered once by the gate/conductor — instead of each chart kind hand-filtering. The RFC-0023 dial of `26718a2` migrates into it; small cards ask for `detail: "summary"`, the expanded panel re-issues `detail: "full"` to the same engine.

### 4.3 Per-scope slicing is a *client-side gate re-run*, not a server re-emit

Server emits the sealed whole-repo ViewSpec once per analysis (already happening). A scope click slices it:

1. partition elements/edges by container/scope tag (the stack facts already tag them),
2. recompute the footer + `hidden[]`/`gaps[]` ledger for the slice (the ViewSpec's own ledger discipline),
3. re-run `gate()` on the slice (TS gate, stable codes shared with Rust by design),
4. negotiate → render.

This keeps scope switching instant and keeps the *sealed* property honest: every drawn pixel descends from the sealed set, and any slice that fails the gate draws nothing rather than something dishonest. (Server per-scope emit stays available as an offline alternative; it is strictly the same code in Rust and needs no new route.)

### 4.4 elk is a layout engine axis, not a chart engine

The negotiator already carries `LayoutHint`/`supportedLayouts` as a first-class axis. `elkjs` registers as one more layout (best-in-class wide-graph/layered layouts for dependency graphs G6's built-ins crowd); mermaid stays a text-chart/export engine. Neither requires touching the adapter contract.

## 5. Stages

| # | Stage | Acceptance |
|---|---|---|
| 0 | **Contract freeze**: `RenderHandle` selection hooks; `component-board` view kind (+ body-lines element field) lands additively; existing 274 tests still green | types compile in both consumers; no behavior change |
| 1 | **`chat-svg` engine**: `chat-chart.tsx`'s renderer behind `EngineAdapter`; Atlas panels route through `negotiator.render()` behind a feature flag | zero pixel diffs on the current demos (screenshot parity); negotiator/dislosure become real |
| 2 | **G6 host runtime**: implement `G6Adapter.render/relayout` in `web/`, generalizing `concept-tree-g6.tsx`'s pattern; register `compactBox` and dagre layouts | tree + uml + flow kinds render under G6 with pan/zoom/click; `renderHash` recomputes |
| 3 | **Per-scope slicing + client gate re-run** on scope click | every Atlas panel draws from a gated slice; slice of a container shows that container's tier (RFC-0019) |
| 4 | **Conductor**: selection events bus; Facts-only + detail dial as preferences; expanded panels re-issue full detail | click a container → all panels rescope; expanded = full detail through the same path as `26718a2` |
| 5 | **elk layout + mermaid export + engine choice surfaced in the disclosure banner** | the banner names engine and layout honestly per chart |
| 6 | **Retire the direct paths**: `concept-tree-g6.tsx` and flagged-out bypasses deleted; `chat-chart.ts` kinds map 1:1 onto the catalog | the two stacks collapse into one |

Each stage is independently shippable; stages 0–2 alone already deliver "single chart, one contract". Stage 4 is what delivers "multi-chart interactivity".

## 6. Risks and honest limits

1. **SSR/pixel parity.** G6 canvas is client-only (`"use client"`; dynamic import). The existing SVG engine remains the SSR/print/export engine — two *engines*, still one *contract*. Server-rendered charts stay possible via `chat-svg`.
2. **Bundle.** G6 is already in `web/package.json` and shipped once; the marginal cost is ~zero for kind 2+. elk adds one more dep — justified only when wide graphs actually need it.
3. **G6 v5 API churn.** The engine adapters' config builders (`engines/g6.ts`) are hand-written against v5's shape and untested against the live library — that is precisely what stage 2's tests close, and `concept-tree-g6.tsx:110-150` is the working reference.
4. **Two gates, one law.** Rust and TS gate implementations share stable codes; the port makes divergence a testable property (cross-language hash spec pattern, now applied to gate outputs) rather than a silent drift.
5. **Caps.** The Atlas renderer guard (`FULL_VIEW_CAP = 400`) and the adapter's engine limits (`G6_LIMITS.maxNodes = 10_000`) are different numbers serving different layers — the negotiator now *chooses* at negotiation time which picture the size demands, and every deviation is disclosed, instead of silently grouping.
6. **The catalog extension must not lie.** `component-board` cards name their modules because the engine's concept DAG carries them (RFC-0019 facts), not because the renderer invented prose — the gate's evidence requirement applies to body-lines like to everything else.

## 7. Answer to "is it ok to build the engine first?"

Yes — and it is the *required* order. Stages 0–2 build the engine without moving any chart off its current renderer, so the product cannot regress while the substrate lands. The ViewSpec port (stages 3–4) then happens chart-kind by chart-kind against a working engine, with the screenshot-parity harness from stage 1 as the regression net. Building charts first would mean porting onto a negotiator that cannot yet draw — the one order guaranteed to break the product.

---

# Amendment (v2): the viz-manager, click-to-chat context, and scoped answering

**Added 2026-10-23** after the maintainer's requirements review. The product contract changes:

1. **SVG is retired** — the web renders client-side only (canvas/G6 and engines of their kind), with the a11y text outline as the non-canvas surface.
2. A **viz-manager** takes one sealed ViewSpec and *plans the sheet*: which charts to display, with which engine, and what action each supports (drill-down vs filter).
3. **Every element clicked — in a single chart or across the multi-chart sheet — becomes a chip in the chat's context window**, removable by the user.
4. **A question is answered only from the code space covered by the clicked elements** — a scoped digest, disclosed over the answer.

## 8. The amended architecture

```
cie-viz (Rust) ──sealed ViewSpec──▶ viz-manager (TS) ──ChartPlan[]──▶ negotiator ──▶ engines (g6 canvas, …)
                                         │
                                         └── ChartPlan.actions (drill | filter) — drives the conductor
selection (any chart) ──▶ chat context (plural chips, removable) ──▶ scoped digest ──▶ answer
```

The manager is the missing third step: today the system can produce a ViewSpec (Rust) and negotiate one engine per chart (adapter), but nothing *plans which charts to show*. `ChatPlan = planSheet(viewSpec, {scope, preferences, viewport, elementSize})` — a pure function over the catalog (76 entries, data-driven), engine limits, and the slice's element/edge kinds, returning `{chartKind, engineId, elementSubset, actions, why}` rows, sorted by relevance. This also subsumes Atlas's hand-maintained seven-branch `body()` (`atlas-view.tsx:315-380`): the panels stop being hard-coded and start being *planned*, scope-dependently.

## 9. Feasibility verdicts — every requirement, receipt in hand

| # | Requirement | Verdict | Receipts | Gap to build |
|---|---|---|---|---|
| R1 | All rendering client-side; no SVG outputs | **Feasible** | charts already paint only after client-side data loads (`chat-panel.tsx:101-106` digest `useMemo`; nothing user-visible is SSR-painted today); real G6 v5 canvas proven live in `concept-tree-g6.tsx:115-150` (autoFit, drag/zoom behaviors, click→ask); the text outline is already modeled as the authoritative non-canvas surface (`interaction.ts` header, US-141); canvas export is a declared G6 capability (`g6.ts` `Capabilities.canvasExport`) | drop the SVG hand-renderer from the drawing path (keep as parity harness + print/SSR fallback only); G6 render/relayout bodies for the remaining graph kinds (stage 2 work from §5); print/export switches to canvas→PNG |
| R2 | viz-manager plans the chart list from one ViewSpec | **Feasible** (~60% new) | the negotiator already solves this for ONE chart (`negotiator.ts:14-22` ladder); the catalog is pure data with per-kind input expectations (`catalog.ts:1-3`) — a `planSheet` over the same inputs is the same style of pure function; ViewSpec already carries the raw material (`elements/edges/groups` with `parentId`, `types.ts:31-59`) | the `planSheet` function + its vitest suite; Atlas wiring behind a flag |
| R3 | Action per chart type: drill-down or filter | **Feasible** (small contract addition) | `ViewElement.parentId` exists (`types.ts:42`) — drill-down = walk to parent/scope+1, the conductor re-slices; filter = set-intersection among current elements, re-gated client-side; `interaction.ts` already models proposals/disclosure so the action vocabulary lands in one place | add `actions` to catalog entries (or infer from `parentId`/edge presence); conductor handlers; footer disclosure of the action taken |
| R4 | Clicks of any chart → removable context chips | **Feasible (80% exists)** | a working prototype ships today: single `mapSelected: VizElement \| null` (`cie-store.ts:113`, `:161`) renders as a referent chip above the composer with an X-remove button ("picked elements become the 'these' in your question", `chat-panel.tsx:466-486`); the wiring point for engines is the `RenderHandle.onSelection` hook (§4.1) | pluralize: `chatContext: ChatContextElement[]` (dedupe by element id, small cap, chip row scrollable, per-chip X + clear-all); both Atlas and the chat canvas feed the same store field |
| R5 | Answers use only the clicked elements' code space | **Feasible** | every ViewElement carries `evidence: {file, start, end}` (`types.ts:19-23`) and stable ids (`c<concept-node-id>`, `cie-viz/src/produce.rs:211`, mirrored in the web's `VizElement`, `cie-types.ts:104-113`); the code space is therefore *computable*: the union of the selected elements' files + the symbols inside them + the edges among them (the call graph `calleesOf` is already loaded in the web) + `parentId` closure when the user wants context upward | `buildDigest` gains a scope parameter (`chat-digest.ts:15-21` today builds the whole-project digest and never sees `mapSelected` — verified: its signature has no selection input; the selection currently shapes only companion-chart choice, `quality.ts:65`/`:1284`) — a `scopeDigest(report/concepts/deps, selection)` slices the fact buckets to the selection's files; the answer's honesty header states the scope plainly ("drawn from N files over M named things, covered by your selections") and the system prompt forbids leaving it silently |

**Two honest paths for R5, same mechanism** — Built-in AI (deterministic) slices the facts before answering, so an out-of-scope question literally computes over nothing else; the LLM path narrows its facts JSON to the same scoped digest, which makes prompts smaller and answers sharper, and the honesty line above the answer is identical for both. When the selection is empty the behavior is exactly today's; when a question clearly needs facts outside the selection, the answer says so and offers a one-tap "answer from the whole project" chip — the scope is never a silent filter.

## 10. Risks specific to this amendment

1. **Canvas ends screenshot-SSR parity as a regression net** — stage 1's vitest/diff harness works for SVG; canvas needs Playwright-level runs (already part of this loop) instead. Accepted: the tree chart has proven canvas behavior in production long enough.
2. **Element-id stability is now load-bearing** — chips, scoping, and drill-downs all key on `c<id>` ids and evidence files. The Rust producer already emits them deterministically from node ids (checked in `produce.rs`); the amendment adds a shared id map in the manager, and the cross-language hash spec pattern extends to ids so Rust/TS can never drift.
3. **Element-to-file mapping covers graph kinds, not stat/table kinds** — non-graph charts (bars, tables) carry element locs too (the web's `ElementLoc` exists for exactly this, `chat-chart.ts:12-16`), so the code-space union treats every chart the same. The only honest boundary: chart kinds with no file evidence (system-level counts) contribute no code space, and the scope header says so.
4. **Scope + 400-cap interplay** — a drilled-down scope is always *smaller* than the sealed whole-repo slice, so the client gate re-run (§4.3) plus the render cap compose; the footer states which slice is drawn.

## 11. Revised stage order (supersedes §5 ordering, not its criteria)

| # | Stage | Change from §5 |
|---|---|---|
| 0 | Contract freeze + `RenderHandle` selection hooks + `component-board` kind | unchanged |
| 1 | G6 host runtime first (canvas from day one — no SVG engine on the drawing path at any stage) | the `chat-svg` engine becomes the parity *harness only*; stage order swapped to avoid building a renderer we then retire |
| 2 | `planSheet` (the viz-manager) behind a flag → Atlas panels drawn from plans | new — was implicit |
| 3 | Per-scope slicing + client gate re-run | unchanged |
| 4 | Conductor: actions (drill/filter) + plural chat-context chips from every chart | absorbs R3/R4 |
| 5 | `scopeDigest` + scoped answering + honesty scope header | new — R5 |
| 6 | Retire direct paths; catalog coverage 1:1 | unchanged |