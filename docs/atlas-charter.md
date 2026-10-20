# The Atlas charter — why it exists, whom it serves, and the charts each user type needs

**Date:** 2026-10-23 · **Status:** charter + design inputs to RFC-0023 (`planSheet`)
**Related:** `docs/rfc-0023-js-owns-pixels.md` (viz-manager, ChartPlan), `docs/chart-inventory.md` (the support-today tiers cited below), RFC-0019 (C4 concept tiers), `docs/VIZ-MULTI.md`

---

## 1. The objective — one sentence

> **The one screen where any scope of the project — the whole system, one container, one module — becomes five synchronized, honest maps: what the parts are, how they're built, what calls what, in what order, and who touches which data — every claim chip-proven, every cap honestly grouped, every click narrowing every map at once.**

## 2. Why do we need the Atlas at all?

**Because tables tell you things; the Atlas shows you the shape — and the shape is what humans orient by.**

The engine's knowledge is lists (report rows, concept nodes, findings, routes, call records). Five separate lists answer nothing at a glance, and a developer's real questions are spatial: *how big is this, what are its parts, how do they touch?* Five jobs only the Atlas does:

1. **Mental model in one glance** — before reading a file: containers with build evidence, the components inside them, the calls between them, the data each touches, the order things run.
2. **Cross-view navigation as one thought** — "alerting → its components → who calls them → what data they move" is one click narrowing all panels (verified live: scope chip `scoped: alerting`, 2026-10-23 demo).
3. **Honesty made visible where decisions happen** — proven/good-guess chips, honest empty states, grouping boxes that admit they group, the Facts-only filter, source lines printed under every chart.
4. **The whiteboard you'd draw for someone else** — the onboarding/review/stakeholder picture, every line standing for recorded evidence, each box clicking open to real source.
5. **The spatial index for everything downstream** — the source panel navigates to it, the chat gets asked about it, and (RFC-0023) its clicked elements become the chat's context window. Without Atlas, chat questions are free-floating; with it, the map is the referent.

**The honesty boundary (part of the charter):** the Atlas is the orientation layer, never the authority layer. It draws exactly what the engine's facts and inferences support, marks which is which, and says when something is grouped, missing, or unindexed. The source panel remains the territory; mastery/Trust remain the depth layers.

## 3. Technical meaning of "specialized Atlas for a user type"

One frame, one engine, one honesty contract — **persona presets differ only in the ChartPlan** (RFC-0023 §8): which panels are planned, in what order, at what default scope, with which default preferences. A persona preset is a pure template:

```
PersonaPreset = {
  id, label,
  panels: [{chartKind, scope: "system" | "default-container" | "focused", detail: "summary" | "full"}],
  defaultFactsOnly: boolean,
  planOrder: string[]        // the panels, in reading order
}
```

Everything below cites only charts already in the inventory tiers (A = rendered today, B = one step away, C = data-exists-chart-pending), so each persona's Atlas is honest about what ships now and what lands with which stage.

---

## 4. Persona 1 — the fresher onboarding to this project (the deepest dive)

*Who they are: a developer joining the team (or a student/staff member handed the repo). Knows the language, knows nothing about this system. Afraid of looking lost; needs a safe, ordered way in.*

### 4.1 What their first 30 minutes actually are

| Minute | Question in their head | Atlas panel that answers it |
|---|---|---|
| 0–5 | "What is this thing at all?" | **Containers** (C4, system scope) — the apps, their frameworks/build tools/docker governance, what's external |
| 5–10 | "Which part is alive? Where does execution start?" | **Entry tables** — every app's boot class (`AmsApplication.java:12`), every HTTP route (`POST /ams/api/head-accounts`, `DELETE /{id}`) |
| 10–15 | "What are the parts inside the part I'll touch?" | **Components** (concept tier under one container) — each Service/Subdomain as a box listing its file modules, certainty chips telling them which naming was recorded vs inferred |
| 15–20 | "How is it all organized from the top?" | **Concept tree** (G6, pan/zoom) — Domain → Container → … hierarchy; the one zoomable full-graph view |
| 20–25 | "Who calls whom?" | **Flow** (recorded call graph) inside the same scoped container |
| 25–30 | "What data does it move?" | **Dataflow** — reads/writes, code ↔ stores, dashed = inferred |

The fresher needs this **as a guided path, not a wall**: the fresher preset leads with Containers and Entry tables (orientation before detail), scopes everything to ONE container by default (a 39-app system scrolled to alerting on day one teaches the shape without drowning them), and keeps the Facts-only filter **off** (they need the inferred structure named honestly, not hidden).

### 4.2 The fresher's Atlas — panel plan (preset)

| # | Panel | Scope | Chart (tier) | Why for a fresher specifically |
|---|---|---|---|---|
| 1 | Containers | system | C4 (A) | the shape of the whole; externals honestly drawn |
| 2 | Entry points + routes | system→default container | tables (A) | where execution begins; the fresher's first click targets |
| 3 | Concept tree | system | tree (A, G6) | the one zoomable hierarchy; trains vocabulary (Domain/Container/Service) |
| 4 | Components | default container | component tier (A) + `component-board` full (B→A) | parts inside one app, each naming its file modules |
| 5 | Flow | default container | flow (A) | calls, with recorded-evidence chips |
| 6 | Dataflow | default container | flow variant (A) | code ↔ data stores |

Fresher-specific overlays (all honest, all present today except two): per-panel captions already say what a box means in plain words; the certainty-chip legend is on every card; **new (B): a "start here" marker on the default container's boot box; new (C): a guided-tour mode that walks panels 1→6 with one sentence each.**

### 4.3 Which charts give a fresher the better picture (priority order)

1. **C4 containers** (A) — the only chart that answers "what exists" before "how it works".
2. **Concept tree** (A) — vocabulary + hierarchy at once; the G6 pan/zoom makes 2,000+ concepts legible.
3. **Component tier with module lines** (A, full detail in expanded view `26718a2`) — the bridge between "concept names" and "actual files".
4. **Entry-point tables** (A) — fresher's first read targets; route tables double as API docs.
5. **Flow** (A) — their first dynamic picture; teaches that arrows are recorded calls, not guesses.
6. **hbar "named things per file"** (A) — size intuition: where the mass actually lives.
7. **Sequence** (A, only when they reach a specific story) — overkill on day one; kept in the mastery journey, not the fresher default.

Explicitly **de-prioritized for freshers**: FMEA/decision tables, trace waterfalls, ownership maps — all later-persona charts; showing them on day one is drowning.

### 4.4 What the fresher preset reuses vs needs

- **Reuses as-is:** panels 1–6 all render today (Tier A); scope dial; expanded full-detail; chips/tells; drill-to-source.
- **Needs RFC-0023 to become a preset:** panels as a planned ChartPlan (stage 2), the persona preset template itself, tour mode (a later story), "start here" marker (B).

---

## 5. Persona 2 — the code reviewer / staff engineer

*What they need: the blast area of a change before reading the diff.*

| # | Panel | Scope | Chart (tier) | Why |
|---|---|---|---|---|
| 1 | **Impact graph** (affected symbols/concepts, minimum test set) | focused | uml of the impact closure (A — `/api/impact` route exists; E-graphs A→B) | "what can this diff break" |
| 2 | Callers of the affected area | focused | flow (A) | who to notify / what else to test |
| 3 | Drift state for the touched docs | focused | drift table/hbar (A data) | "does the diff break a documented promise" |
| 4 | Test confidence for the area | focused | table (A data, G1) | the minimum test set as evidence |
| 5 | Dataflow around touched stores | focused | DFD variant (A/C) | writes are the risk |

Charts that matter for reviewers beyond panels: **trace waterfall** (F1, C — when they ask "did they measure it?"), **counterfactual overlay** (H4, C — "what if this regressed"). Default preference: **Facts-only ON** — reviewers judge on the recorded.

## 6. Persona 3 — the architect / long-term maintainer

*What they need: whether the structure matches the stated architecture, and where it rots.*

| # | Panel | Scope | Chart (tier) | Why |
|---|---|---|---|---|
| 1 | Module/package **dependency graph** | system | E5 graph (C — data in `deps.json`) | layering reality vs intent |
| 2 | Bounded contexts / service map | system | C5/C6 (C — Container tier + stack apps exist) | is the monolith really modular |
| 3 | Duplication candidates | system | overlap table + chord (A data + B) | merge/extract bets |
| 4 | Doc↔code drift board | system | FMEA/hbar by status (A data) | where reality left the docs |
| 5 | Semantic diff timeline | across revisions | I5 (C — `.cie/revs/*` exist) | how the shape changed and when |
| 6 | Change-risk terrain | system | treemap H5 (B — flame-style packing; churn+findings data) | where the next structural failure is most likely |

Default preference: Facts-only OFF (inferred structure is the value), caps expanded (full detail).

## 7. Persona 4 — the auditor / security reviewer

*What they need: what faces the world, where data crosses trust boundaries, what's provably enforced.*

| # | Panel | Scope | Chart (tier) | Why |
|---|---|---|---|---|
| 1 | **Externals + containers** | system | C4 (A) — compose image-only services drawn with their evidence | the attack surface as the build files name it |
| 2 | **Route table** (whole system, all controllers) | system | table (A) | every HTTP verb/path the repo exposes |
| 3 | Secrets/risk findings | system | table + hbar by severity (A) | the flagged things, remediations attached |
| 4 | **Trust boundary map** | system | I3 (C — needs the engine's boundary facts from routes/externals) | where untrusted data enters |
| 5 | Dataflow with writes emphasized | system | DFD (A/C) | what external input can reach |
| 6 | Policy enforcement matrix | system | G3 (C — invariants/taint facts exist) | "enforced, asserted, or merely claimed" |

Default preference: **Facts-only ON** (an auditor repeats nothing that was merely inferred); every panel keeps its `loc` links to the evidence lines.

## 8. Persona 5 — the AI agent (MCP / chat)

Not a visual consumer — the honest mirror: the same ViewSpec contract (RFC-0023) serves the agent as structured claims with evidence; the Atlas is what the **human** sees of the same answer. For the agent the "chart list" is irrelevant; what matters is that every chart's underlying elements are addressable by id (`c<n>`), so an agent's claims and the human's map can reference the same nodes. No专属 panels; consumes the engine's MCP tools (`US-134`). This persona exists in the charter to keep ONE contract honest for both audiences.

## 9. Persona 6 — the consultant / tech-lead landing on a foreign codebase

The fresher's preset **plus** what a consultant adds after the first look:

| # | Panel | Chart (tier) | Why |
|---|---|---|---|
| 1–6 | everything from §4.4 | A | same first 30 minutes |
| 7 | **Ownership/knowledge map** | K1/K2 (C — needs `cochange --authors`) | "who knows this part" — the bus-factor picture |
| 8 | Migration/co-change matrix | matrix heatmap (B — `CochangeEdge` exists) | which parts move together (modernization slicing) |
| 9 | Test-coverage heatmap | A | the safe-refactor map |
| 10 | Journey charts (transaction/user) | J1/J2 (C→A for known flows) | "what does this system do for its users" |

Default scope: system first, then the busiest container by file count (computable from the stack facts — disclosed).

## 10. Summary matrix — chart priorities per persona

| Chart | Fresher | Reviewer | Architect | Auditor | Agent | Consultant |
|---|---|---|---|---|---|---|
| C4 containers | ★★★ | ★★ | ★ | ★★★ | (mirror) | ★★★ |
| Concept tree | ★★★ | ★ | ★★ | ★★ | | ★★★ |
| Components + module lines | ★★★ | ★★ | ★★ | ★ | | ★★★ |
| Entry/route tables | ★★★ | ★★ | ★ | ★★★ | | ★★ |
| Flow (call graph) | ★★ | ★★★ | ★★ | ★ | | ★★ |
| Dataflow | ★★ | ★★★ | ★★ | ★★★ | | ★★ |
| Sequence | ★ (later) | ★★ | ★ | ★ | | ★★ |
| Impact graph | | ★★★ | ★★ | ★★ | | ★★ |
| Drift board | | ★★★ | ★★★ | ★★★ | | ★★ |
| Dep graph (E5) | ★ | ★ | ★★★ | ★★ | | ★★★ |
| Overlap/duplication | | ★ | ★★★ | ★★ | | ★★★ |
| Timeline (I5) | | | ★★★ | ★ | | ★★ |
| Ownership (K1) | | ★ | ★★ | ★ | | ★★★ |
| Trust boundary (I3) | | | | ★★★ | | ★★ |
| Treemap risk (H5) | | | ★★★ | ★ | | ★★ |
| Coverage heatmap | | ★★ | ★★ | ★★ | | ★★★ |

## 11. Implementation note — how this feeds RFC-0023

- Persona presets are **ChartPlan templates** consumed by the viz-manager's `planSheet` (RFC-0023 §8): panels per persona, default scope, default preferences, reading order.
- Everything marked **(A)** ships today's renderer unchanged; **(B)** lands with RFC-0023 stage 1–3; **(C)** lists the engine addition needed (one line each, no hand-waving).
- The fresher preset is the **first preset to build** — it is 100% Tier A plus two small overlays, and it doubles as the acceptance harness for the preset machinery.