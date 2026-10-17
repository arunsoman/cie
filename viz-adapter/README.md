# cie-viz-engines (viz-adapter/)

The CIE-VIZ-MULTI adapter layer: **CIE emits a semantic `ViewSpec`; a
negotiator picks an engine; every adapter enforces honesty once via
`gate()`; the chart catalog is bounded only by registered extensions.**

```
CIE (Rust, cie-viz)  ──ViewSpec JSON──▶  this package  ──▶  g6 | jointjs | reactflow | mermaid
     producer · gate · hash split            negotiator · adapters · outline · catalog
```

## Layout

| Path | Role |
|---|---|
| `src/types.ts` | ViewSpec v2 + `EngineAdapter`/`CapabilitySet`/`EngineLimits`/`RenderHandle`/`LayoutOutcome` |
| `src/gate.ts` | the ONLY honesty enforcement point (stable codes, shared with Rust) |
| `src/hash.ts` | PRD §3 hash split — `semanticHash` (cache key) / `renderHash` (artifact key) |
| `src/negotiator.ts` | `chooseEngine` (pure) + `renderHeadless` pipeline + disclosure |
| `src/engines/g6.ts` | reference engine: full encoding, plugins, extension surface |
| `src/engines/flame.ts` | flame graph as pure geometry + registration snippet (PRD Part V) |
| `src/engines/{mermaid,reactflow,jointjs}.ts` | alt engines with declared capability limits |
| `src/outline.ts` | A11Y text outline — authoritative when fidelity degrades (VIZ-V-02) |
| `src/catalog.ts` | the 76-chart catalog (data, not branching) |
| `src/disclosure.ts` | engine banner / layout chip / fidelity banner payloads |
| `test/` | 259 vitest cases covering the headless subset of VIZ-01..30 |

## The hash split (PRD Part I §3)

* `semanticHash` — sha256 over `{elements, edges, groups, gaps, footer,
  scope}`. **Cache key**: layout and engine swaps never invalidate it.
  `hidden[]`, `layoutHint`, `preferences` and the hashes are OUTSIDE.
* `renderHash` — sha256 over `{semanticHash, layoutHint, preferences,
  engineId}`. **Render-artifact key**: any layout/preference/engine change
  yields a new artifact. CIE seals the engine-neutral baseline
  (`engineId: ""`); adapters recompute with their own id (VIZ-03; CIE
  never names an engine — VIZ-10).

Both ends (Rust `cie-viz` and this package) produce byte-identical
canonical digests — asserted by `test/cross-language.hash.spec.ts` against
a REAL `cargo run -- viz` output (`fixtures/specs/cie-sample.json`).

## Run

```bash
npm install
npx vitest run        # 259 tests, headless
npx tsc --noEmit      # strict typecheck
```

Produce a fresh spec from a codebase and validate it at the gate:

```bash
cargo run --bin cie -- viz /path/to/repo --out spec.json
cargo run --bin cie -- viz /path/to/repo --validate spec.json [--engine g6]
```

## Demos (browser pages, PRD Part V / III)

| Page | Shows |
|---|---|
| `demos/flame-graph.html` | TC-02: flame graph = ONE custom layout + ONE custom node; fisheye; hypothesis frame dotted |
| `demos/negotiator.html` | engine decision banner (`Preferred: mermaid ✗ … · Rendered: g6`), layout chip, fidelity banner, G6 vs Mermaid side-by-side |

Browser-only ledger items (pixel baselines VIZ-24, keyboard walks VIZ-26,
canvas/SVG parity VIZ-22) run via Playwright against these pages on a
machine with a rendering stack; every headless-checkable item is in
`test/`.

## Fidelity matrix (PRD Part IV)

| Engine | Encoding | Audit returns |
|---|---|---|
| G6 | solid / `[6,4]` / `[2,4]` / hatched+`[1,3]` | `Full` |
| JointJS | strokeDasharray per mode + pattern fill | `Full` |
| ReactFlow | CSS classes on the custom node | `Full` |
| Mermaid | classDef dashes kept; Fog grey fill only | `Degraded` (banner) |
