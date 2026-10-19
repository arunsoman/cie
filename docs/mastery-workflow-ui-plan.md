# UI/UX Plan — Integrating the "LLM-Assisted Codebase Mastery Workflow" into CIE

Status: proposal (no code yet) · Author: main (pi) · 2026-10-19

## 0. Framing

The workflow (Phase 0 Setup → Phase 1 Fast Orientation → Phase 2 Deep Dive into
critical flows → Phase 3 Structure → Phase 4 Validation → Ongoing Use) is
essentially a **guided onboarding journey over an unfamiliar repo**. CIE already
is most of the machinery — engine facts, honest chat, deterministic charts, the
canvas, the source panel, and (just shipped) a clean connect-a-repo flow with
branch picking. What's missing is the scaffolding that turns those pieces into a
**journey**: a progress spine, a living notes document, two new diagram kinds,
the human-verification loop, and the export.

Plain-words name: **"Master this codebase"** — a 5-station journey that CIE
guides, the human verifies, and both remember.

The real-world test bed already exists: `docs/questions-and-answers.md` (55 real
onboarding questions on the `mipay` Gradle platform) is the acceptance matrix —
the journey's Phase-1 and Phase-3 prompts must be able to answer the A–H
question groups in that document, with charts, from the engine's own data.

## 1. Phase → CIE mapping (what exists, what's a gap, what the UI does)

| Workflow phase | In CIE today | Gap | UI treatment |
|---|---|---|---|
| **0 Setup** — open repo, LLM sees it, create NOTES.md | the connect-a-repo dialog just shipped (public repo w/ branch, local folder w/ branch, upload files) | the living notes document | after `Show my project` succeeds, the success card offers **"Start mastering this repo"** → creates an empty Mastery Notes doc for this project (per label, persisted) |
| **1 Fast Orientation** — summary, entry points, C4 containers, top flows | "Start here" 3-step intro, concepts, map, chat digest (summary-ish) | **entry-point extraction** (file:line), **C4 container diagram**, **top-flows list**, prompt **chips** that run the 4 canonical prompts | **Journey view**: 4 one-tap prompt cards; answers land as new chat **answer cards** (see §3) with every claim citing file:line, clickable into the source viewer; the C4 diagram opens on the canvas |
| **2 Deep Dive** — trace 1–2 critical flows, sequence diagrams, verify in debugger | chat can trace call flows; `buildFlowSpec`/`buildUmlSpec` already exist in `draw-anything`; `SummaryInfo` carries reads/writes/effects/exceptions per symbol | a **first-class Trace artifact**: step table + sequence diagram + **human verification loop** | **Trace card**: steps as rows (fn, file:line, reads→writes, failure marks), each row tagged `CONFIRMED` (emerald) or `INFERRED` (amber), each clickable into the source panel at that line; a **"Your turn"** box: "Open your debugger, trigger this flow" → user ticks confirmed / flags wrong; corrections update the notes and the diagram; both flow + diagram pin into Mastery Notes |
| **3 Structural Understanding** — component diagram, fan-in/out, seams, risk | deps view (declared), overlap (shared symbols), cochange (history coupling), concepts (responsibilities) | a **"Where the risk lives"** roll-up (fan-in × fan-out × co-change × size) and a **seams** listing (interfaces/adapters/ports in concepts + code) | journey stops **link to the existing views** (no re-implementation): "Open Dependencies", "Open Overlap", "Open Co-change"; one new canned chart: **risk chart** (table/scatter: hotspots ranked by `cochange times × fan-out × loc`), each row clickable to the module view |
| **4 Validation** — 5 things to know, ARCHITECTURE.md, small safe change | quality, trust views | **"5 things a new dev must know"** card + **ARCHITECTURE.md export** | notes panel assembles the doc from auto + human sections; `Export ARCHITECTURE.md` (and NOTES.md) button; checkpoint: the user answers 2 self-check questions CIE scores against the engine data (optional, keep light) |
| **5 Ongoing** — trace-by-feature, blast radius before changes, feed corrections back | cochange/impact exist engine-side; `/api/impact` route just landed | **Blast-radius UX** surfaced where the coder already looks + notes persistence/resume | on every symbol header (source panel, "What it does" rows, map bubble detail): **"Blast radius"** popover → impact list + "tests to run" suggestion + "trace the flow" chip; corrections from Phase-2 loop get appended to notes automatically so the doc stays current |

## 2. New UI surfaces

### 2.1 The Journey view (`view === "mastery"`)

Replaces nothing; it becomes the enriched "Start here". ASCII wireframe:

```
┌ Master this codebase ───────────────────────────── [Reset journey] ┐
│ ①  Orientation    ●●●○○  3/5 prompts run      [ Continue → ]      │
│ ②  Deep dive      ○○○○○   no flows traced    [ Trace a flow ▾ ]  │
│ ③  Structure      ◐       2 of 3 views seen                       │
│ ④  Lock-in        ○       ARCHITECTURE.md not exported            │
│                                                                    │
│ ─ Phase 1: Fast orientation ─────────────────────────────────────  │
│  [ What is this system?        ]  ← one-tap prompt chips            │
│  [ Where does it start from?   ]     (entry points, file:line)      │
│  [ Draw the big picture        ]     (C4 containers → canvas)        │
│  [ The top flows               ]     (pick the 5 most important)    │
│  done → each chip shows a ✓ and a link to its answer card            │
│                                                                    │
│  Checkpoint: "Say it in 2 minutes" — a notes section asking YOU to  │
│  write the summary first; CIE then shows its answer side-by-side.   │
└────────────────────────────────────────────────────────────────────┘
```

- Progress state persisted **per project** (`label@branch`), cleared on switch
  (same discipline as the `sourceRoot`/sourceCache reset).
- "Continue →" deep-links back into the chat with the next unanswered prompt
  pre-filled and the context noted ("you already ran What-is-this; next is
  Where-does-it-start").

### 2.2 Trace card (Phase 2's core artifact)

A new first-class chat chart kind (`kind: "trace"`, rendered as a card + a
sequence diagram on the canvas):

```
┌ Trace: Top-up transaction ─────────────────────── 12 steps · 2 INFERRED ┐
│ #  step                        reads → writes        failures  status  │
│ 1  USSDRouter.route()          req → session                   ✓CONF   │
│ 2  Transactionmgmt.create()    wallet → tx  ⚠ idempotency     ✓CONF   │
│ 3  KYC.check()                 profile                         INF     │   ← amber tag
│ …                                                    (click any row →  │
│                                                        source at line) │
│ ┌ Your turn — verify it yourself ─────────────────────────────────┐    │
│ │ ▸ run the flow in your debugger (or the trace test)              │    │
│ │ [ I confirmed all of it ]  [ Something's wrong in step ▾ ]       │    │
│ └──────────────────────────────────────────────────────────────────┘   │
│ [ Sequence diagram → canvas ]  [ Pin to Mastery Notes ]               │
└───────────────────────────────────────────────────────────────────────┘
```

- `CONFIRMED` here means **evidence-backed by the engine** (a summary exists for
  that symbol, complete=true); `INFERRED` means the path was concluded from the
  call graph / name heuristics — never presented as measured. Same honesty
  contract as the Trust view. The **user's** debugger pass is what upgrades
  INFERRED → "YOU CONFIRMED", which is a third, strongest state (your check).
- "Something's wrong in step N" opens a one-line correction field; the
  correction is appended to the notes doc under *Corrections from verification*
  so the diagram/notes stay honest about what's user-verified vs machine-said.

### 2.3 Sequence / C4 diagram kinds

- `draw-anything.ts` already builds UML and flow specs — extend `ChatChartKind`
  with `sequence`, `c4-container`, `c4-component`, rendered with the existing
  canvas pipeline (camera, fisheye, click-to-chat, "go big" button — all free).
- Every diagram gets the standing caption: **"Derived from CIE's static
  analysis — inferred where marked; base it on evidence, then verify."**

### 2.4 Mastery Notes (the NOTES.md of the workflow)

A dedicated panel (or `view === "notes"`):

```
┌ Mastery notes — mipay@main ● auto-filled, ◐ edited by you ────────────┐
│ ● Summary (CIE)                            [regenerate] [edit]        │
│ ● Entry points (CIE, 14)                   [open list]                │
│ ● Big picture — C4 containers              [open diagram]             │
│ ◐ My summary in my words                   [edit]                     │
│ ◐ Flow: Top-up transaction — trace + seq   [open]  ✓ 3 steps you      │
│                                              confirmed with your own │
│                                              debugger                │
│ ● Corrections from verification (2)        [view]                     │
│ ● Where the risk lives (top 5 hotspots)    [open chart]               │
│ ◐ What every new dev must know (draft)     [edit]                     │
│  [ Export ARCHITECTURE.md ]  [ Export NOTES.md ]   (download, plain md)│
└───────────────────────────────────────────────────────────────────────┘
```

- Sections are **provenance-typed**: `auto` (engine/chip-made, regenerable,
  never user-silently-edited) vs `human` (yours; CIE may suggest, never
  overwrite). Corrections are an append-only log — the honest history of what
  the machine got wrong and what the human verified.
- Export is a pure client-side assembly → markdown download; auto sections cite
  their engine evidence inline (file:line, tool names); diagrams export as
  fenced Mermaid code blocks.

### 2.5 Blast radius (Phase 5's "before changing code")

```
(symbol header in source panel / "What it does" row)
  [ ⚡ Blast radius for TransactionManager.create() ▾ ]
      downstream: 7 symbols, 3 files (clickable)
      coupled-by-history: payments/ + txmgmt/ change together (cochange, 12 commits)
      shared-symbols: overlaps Notifications (overlap, 3 shared) — consider seam
      tests to run: 2 test functions reference this call path (INFERRED if engine
      doesn't link tests — say so)
      [ Trace the flow from here ]  [ Save to notes ]
```

Reuses `/api/impact` + cochange + overlap — pure assembly of existing facts.

## 3. Data model (store additions, no engine protocol break)

```ts
interface NoteSection { id; kind: "auto"|"human"|"correction"; station: PhaseId;
  title; bodyMarkdown; refs: {file?, line?, symbol?}[]; provenance?: string;
  updatedAt }
interface TraceStep { symbol; qualName; file; line; reads; writes; failures[];
  status: "confirmed"|"inferred"; youConfirmed?: boolean; correction?: string }
interface TraceSpec { title; entry; steps: TraceStep[]; sequence: SequenceSpec }
interface MasteryState { projectId: string; progress: Record<PhaseId,{done;at}>;
  traced: TraceSpec[]; pinned: ChatCardRef[]; sections: NoteSection[] }
```

- persisted under `localStorage` key `cie-mastery:<projectId>`; reset on
  project/branch switch exactly like `sourceCache`/`sourceRoot`; export
  regenerates ARCHITECTURE.md deterministically from this state.

## 4. Chat/engine work required (grounded, honest)

1. **Entrypoint facts** — derive from the existing call graph: symbols with
   zero fan-in + side effects (engine has full callee graph), plus name-based
   main/router/`@Controller` heuristics; **tag INFERRED**. (Candidate engine
   command `cie entrypoints`, or a digest-side post-pass first.)
2. **Top flows** — propose from concepts (responsibility words) × fan-in; the
   user picks 1–2 for Phase 2. CIE never claims "these are THE flows" without
   evidence — the card says "candidates, ranked by how central they look".
3. **Trace spec** — path search over the existing call graph between an
   entrypoint and the flow's terminal writes (`SummaryInfo.writes/effects`);
   each step's reads/writes/failures come straight from summaries — this is
   why CIE's trace card will be *evidence-cited*, unlike generic LLM tracing.
4. **Risk roll-up** — join cochange (history) × call-graph fan-out × loc ×
   overlap; purely computed from loaded data, no new engine calls needed for
   v1 of the chart.
5. **Tests-to-run** — v1: name test candidates by symbol-name heuristic
   (`INFERRED`, honestly); v2: engine links test symbols via the call graph.

## 5. Interaction flows (the journey in 4 moments)

1. **Start** — success card after connect → "Start mastering this repo" →
   journey view opens with Phase 1 chips and a notes doc created (empty
   summary section awaiting the user's 2-minute answer).
2. **First trace** — Phase-2 chip "Trace a flow" → flow picker (from top-flows
   cards or typed) → trace card renders step-by-step → sequence diagram on
   canvas → "Your turn" box appears after 2s idle hint → user verifies →
   corrections appended → phase progress updates.
3. **Correction loop** — user flags a step wrong → inline edit field →
   confirmation ("Corrected — your note now says the writes go to
   `txmgmt/LedgerWriter.java:88`, not the router") → notes doc updated +
   diagram node annotated with your correction marker.
4. **Export** — notes panel → "Export ARCHITECTURE.md" → downloads; the doc
   embeds the Mermaid sources and names what was engine-said vs human-said.

## 6. Placement in the existing IA

- Topbar gains nothing new: **"Explore ▾" gets a "Master this codebase"** item;
  the Start view becomes the journey dashboard (its current 3-step explainer
  stays as the empty-state above the journey).
- The chat keeps being the engine: every journey action is a pre-filled prompt
  or a canned chart, so **all honesty rules, the trust story, and the
  click-to-chat behaviors carry over unchanged**.
- Mobile: journey = full-screen sheet with the same rail; trace card scrolls;
  diagrams open to the existing canvas view.

## 7. Build order (each milestone independently shippable)

| M | Scope | Why first |
|---|---|---|
| **M1** — the journey spine | mastery view (progress + chips), Mastery Notes panel with manual edit + pin-from-chat, per-project persistence | smallest slice that makes the workflow survivable day-1; the notes doc alone kills the "scroll chat forever" failure mode |
| **M2** — orientation answers | entry-points table (INFERRED-tagged, clickable), C4 container diagram kind, top-flows candidates | Phase 1 fully mechanized; dogfood against the 55-question doc |
| **M3** — the trace | trace card + sequence-diagram kind + verification loop + corrections log | the workflow's heart; engine summaries make it evidence-cited |
| **M4** — structure & risk | journey links to deps/overlap/cochange + risk roll-up chart + seams listing | mostly assembly of existing views |
| **M5** — lock-in & ongoing | ARCHITECTURE.md export, "5 things to know", blast-radius popover, resume nudges | turns Phase 4–5 into habit |

## 8. Risks & guardrails

- **LLM traces are wrong sometimes** — the product's answer is the
  verification loop: INFERRED/CONFIRMED/YOU-CONFIRMED states, corrections
  logged, never auto-upgraded. (This is also CIE's differentiator vs a plain
  chat window.)
- **Big repos (mipay: 19 modules, some disabled in the build)** — C4 and
  entry-points must be capped and grouped; say what was left out and why.
  (The 55-question doc's Q2/Q3 show exactly this trap: built-vs-deployed
  mismatch. The journey should *name* such mismatches, not hide them.)
- **No new persistence surfaces** — notes live in localStorage only (read-only
  promise intact); export is explicit user action.
- **Don't fork the views** — the journey links to the real views (deps,
  overlap…); if a view can't answer a stop, the stop says so honestly instead
  of duplicating a pretender.
- **Two streams converge here** — the cochange/overlap/drift work just merged
  is the data source for Phase 3/5; sequence/C4 kinds must follow the existing
  `ChatChartSpec` engine+layout disclosure (US-020) so the trust story doesn't
  fork.