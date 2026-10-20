# Chart inventory — what the platform can support TODAY (2026-10-23)

Grounded, tiered list of every chart the product can draw **today**, every generic
statistical chart **one small step away**, and every engineering chart whose data
**already exists but has no chart yet**. Every row carries its data source receipt.
Feeds the RFC-0023 `planSheet` (viz-manager) and the catalog: the "supports today"
set is Tier A + Tier B; the 76-entry catalog (`viz-adapter/src/catalog.ts`) is the
destination, not the present.

## Tier A — rendered and wired today (11 kinds, live paths)

The renderer's full kind dispatch (`chat-chart.tsx:1039-1047` + `hbar`): every kind
below has at least one live production route (one-tap menu, Atlas panel, chat
canvas, or mastery card).

| Kind | What it draws | Live routes (receipts) |
|---|---|---|
| `hbar` | horizontal bars, fisheye lens (US-023) | 22 one-tap charts (`canned-charts.ts` — e.g. `files-most-problems` :77, `things-per-file` :369); digest-charts from Ask-CIE answers (`api/chat/route.ts:59`) |
| `donut` | pie/donut share of a counted whole | `urgency-donut` :100, `problem-kinds` :131, `map-certainty` :252 |
| `stat` | single-number KPI cards (with provenance chips) | chat digest facts; mastery summaries |
| `table` | rows × columns with per-cell certainty | `who-wrote-promises` :202, `promise-list` :225, quality tables, Controller-route table (Atlas svcEntry) |
| `uml` | boxes + typed arrows (class/component/module graphs), hover lights connections | Atlas Components (concept tier or module fallback), `uml-diagram` :291, inside-module symbol zoom |
| `flow` | recorded call flow, who-calls-whom over the call graph | `call-flow` :304, Atlas Flow panel |
| `c4` | containers/externals with build evidence, cert chips | Atlas Containers (RFC-0019 stack apps), `outside-world` :345 |
| `tree` | concept hierarchy — **drawn by real @antv/g6 canvas** + compactBox (`concept-tree-g6.tsx:115`), pan/zoom/click | Explore/Charts concept tree |
| `mindmap` | radial concept map | canned concepts view; mindmapSVG |
| `heatmap` | tiled grid of per-file measured values (e.g. test coverage) | `coverage` :318 (quality.json tiles) |
| `sequence` | actor→actor call sequence over recorded routes | Atlas Sequence panel; mastery traces |

Data sources behind Tier A: `report.json` (symbols/findings/summaries/exports),
`concepts.json` (Domain/Container/Service/Module/UseCase/Rule/Invariant tiers),
`deps.json` (ecosystems/manifests), `runtime.json` (cpu top_functions, concurrency
overlap), `cochange.json` (git churn pairs), `quality.json` (findings + coverage),
`drift.json` (claims by status), `overlap.json` (shared-symbol pairs), `stack.json`
(apps/externals/routes), `viewspec.json` (sealed semantic map).

## Tier B — generic statistical charts, supportable now (data exists, small renderer work)

Every numbered artifact carries counted buckets, so each of these has real rows the
day the layout lands. With the RFC-0023 G6 host runtime, the marked G6 kinds are
free engine features (registered plugins/layouts), zero bespoke SVG.

| Chart | Triviality | Data today (receipt) |
|---|---|---|
| **Vertical bar** | ~rotate of `hbar` | same buckets as all canned charts |
| **Pie** (solid) | = `donut` with hole 0 | same buckets |
| **Stacked bar** | hbar + series field | findings by file × severity (`canned-charts.ts:77-130` pattern) |
| **Line / area** (trend) | polyline renderer | cochange weekly commit counts (`CieCochange`, `cie-types.ts:200-213`); runtime makespan series |
| **Scatter** | two-numeric per file | complexity vs coverage per file (quality.json + report) |
| **Radar** | per-entity metric profile | per-file: symbols × findings × coverage |
| **Gauge** | one score dial | quality score, drift implemented-% |
| **Sparklines in tables** | inline mini-line | any per-row series |
| **Treemap** (rect-packing) | G6/flame geometry reuse — `flame.ts` packs rect-in-rect already | findings-weighted file sizes; change-risk terrain (H5) |
| **Sunburst** | G6 built-in | concept hierarchy rings (RFC-0019 tiers) |
| **Sankey** | G6/adapter layout | imports → modules → symbols (exports/import graph, E8) |
| **Chord / arc** | G6 built-in | cochange pairs (`CochangeEdge.support`) |
| **Matrix heatmap** (files × files) | heatmap variant | cochange co-touch matrix; test-confidence matrix (G1) |
| **Box / distribution** | five-number over bucket | complexity distributions |
| **KPI + delta rows** | `stat` extension | any two-scan comparison (drift over revisions) |

Honestly not supportable today (no data): geo maps, real-time streaming charts,
calendar heatmaps need time-bucket exports the engine does not emit yet.

## Tier C — engineering charts the data already supports, chart not built yet

| Catalog id | Chart | Data exists? | Missing |
|---|---|---|---|
| E5 / A3 | Module/package **dependency graph** | ✅ `deps.json` ecosystems/manifest edges | graph layout over deps (today: table only, :421-447) |
| D2 | Data-flow **DFD** (code ↔ stores) | ✅ report summaries reads/writes + sql stores (Atlas Dataflow draws a variant) | store-centric DFD layout |
| D1 | **ER diagram** | ✅ sql schema files indexed in concepts (tables seen in shahy) | SQL DDL column/FK parser (engine-side) |
| F2 | **Flame graph** | ✅ engine exists + headless tested (`engines/flame.ts`); runtime.json cpu top_functions | host runtime render (RFC-0023 stage 1) |
| F1 | **Trace waterfall** | ✅ runtime concurrency task windows | waterfall layout |
| F5 | Lock-ordering graph | ✅ runtime concurrency (observed_overlap) | needs per-lock records from the collector |
| H1–H4 | Decision tables / FMEA / metrics map / counterfactual | ✅ findings + claims + invariants (566 for shahy) | table+graph variants of existing facts |
| I5 | **Semantic diff timeline** | ✅ `.cie/revs/*` revisions + timeline crate | revision-diff chart + route |
| I4 | Archaeology chain | ✅ cochange supporting_commits | chain layout |
| K1 | Ownership map | ⚠️ cochange has no per-author export | `cie cochange --authors` engine addition |
| E6 / E7 | Dead-code reachability / hot path | ✅ call graph reachability + runtime cpu | overlay marks on the uml engine |
| C5/C6 | Bounded-contexts / microservices graph | ✅ stack.json apps + concepts Container tier | cross-container service graph (Atlas shows per-container; system map is C4) |

## Totals

- **Ready (Tier A):** 11 kinds, every one with a live route.
- **One step away (Tier B):** 15 additions, ~10 of them free under the G6 host runtime.
- **Data ready, chart pending (Tier C):** ~12 engineering charts, ~8 of them needing only
  rendering, ~4 needing small engine additions (ER columns, authors, lock records).
- **Catalog alignment:** 76 listed; ~26 supportable now, +~27 with stage 1–3 of RFC-0023.