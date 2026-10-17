# CIE-VIZ-MULTI — Multi-Engine Visualization Platform

Implementation record for the CIE-VIZ-MULTI PRD (supersedes the
single-engine G6 PRD). Rust side in `cie-viz/` + the `cie viz` CLI
subcommand; TypeScript side in `viz-adapter/`.

## 1. What shipped

| Layer | Artifact | Tests |
|---|---|---|
| Rust producer | `cie-viz` crate: ViewSpec v2 model, gate, §3 hash split, `RevSnapshot` → sealed spec | 23 unit + 1 doctest |
| Rust CLI | `cie viz <path> [--out f] [--max-elements N] [--layout s]` / `--validate spec.json [--engine id]` | roundtrip via CLI |
| TS negotiator | `chooseEngine` (pure, deterministic g6 → jointjs → reactflow → mermaid), `renderHeadless` (VIZ-09), disclosure payloads | 255+ vitest |
| TS engines | `G6Adapter` (full), `JointJSAdapter` (full), `ReactFlowAdapter` (full via custom node), `MermaidAdapter` (Degraded, disclosed) | per-engine specs |
| TS extras | flame-graph pure geometry, A11Y text outline, 76-chart catalog, UI banner/chip payloads | suite |
| Demos | `demos/flame-graph.html` (TC-02), `demos/negotiator.html` (VIZ-28/29/30) | browser |

**Totals: Rust 222 workspace tests green (incl. cie-viz), TypeScript 259
vitest + strict tsc green.**

## 2. The hash split (PRD Part I §3) — the load-bearing decision

```
semanticHash = sha256({elements, edges, groups, gaps, footer, scope})
renderHash   = sha256({semanticHash, layoutHint, preferences, engineId})
```

* `semanticHash` is the **cache key** — a layout or engine swap never
  invalidates semantic content (VIZ-02). `hidden[]` is a disclosure
  ledger, NOT semantic content: it is outside the domain.
* `renderHash` is the **render-artifact key** — any layout/preference/
  engine change yields a new artifact (VIZ-03). CIE seals the
  engine-neutral baseline `engineId: ""`; adapters recompute with their
  own id. CIE output names NO engine (VIZ-10) — verified by tests on both
  sides (`cie-viz/src/produce.rs::produced_spec_names_no_engine`,
  `viz-adapter/test/schema.no-engine.spec.ts`).
* Cross-language identity: canonical JSON = wire text → reparsed Value →
  JS-style integral-float normalization → sorted keys → sha256. f32
  fields hash through their ryu shortest repr so Rust `sha2` and Node
  `crypto` agree byte-for-byte on REAL produced specs.

## 3. Negotiation (PRD Part I §2, Part II §6)

Ladder: capability gate (hard) → user preference → size limits →
deterministic order. `chooseEngine(request, registry)` is pure; the same
`(request, registry, graphSize)` always yields the same adapter (VIZ-04).
`renderHeadless` is the only pipeline: negotiate → gate → adapt — no
adapter is ever invoked without a negotiator decision, and a negotiation
failure is a typed `NegotiationError` with zero adapter invocations
(VIZ-09). Deviations are disclosed: `Preferred: mermaid ✗ (capability
fisheye unavailable) · Rendered: g6` (VIZ-06/28).

## 4. Honesty (unchanged from the single-engine PRD)

`gate()` remains the single enforcement point; every adapter's `adapt`
runs it (asserted for all four engines). Stable codes:
`ELEMENT_WITHOUT_EVIDENCE`, `NON_FACT_WITHOUT_CLAIM`,
`HYPOTHESIS_WITHOUT_COUNTER`, `FOG_WITHOUT_REASON`,
`EDGE_WITHOUT_EVIDENCE`, plus `EDGE_UNKNOWN_ENDPOINT`,
`DUPLICATE_ELEMENT_ID`, `ELEMENT_CAP_EXCEEDED` (cap 2 000, producer-side
salience bounding with a `hidden[]`/`gaps[]` ledger — the adapter refuses
over-cap specs, never silently truncates).

## 5. Encoding fidelity (PRD Part IV)

| Engine | Fact | Inference | Hypothesis | Fog | audit().encodingFidelity |
|---|---|---|---|---|---|
| G6 | solid | `[6,4]` | `[2,4]` | hatch+`[1,3]`+`?` badge | `Full` |
| JointJS | solid | dasharray `6 4` | `2 4` | pattern fill | `Full` |
| ReactFlow | solid border | dashed CSS | dotted CSS | hatched bg | `Full` |
| Mermaid | solid | classDef dash | classDef dash | grey fill only | `Degraded` |

Degraded fidelity is surfaced by `fidelityBanner()` and the text outline
(`outline.ts`, generated from the ViewSpec, never from the canvas) is
authoritative whenever fidelity ≠ Full (VIZ-V-02/19).

## 6. The flame graph proof (PRD Part V)

`engines/flame.ts` holds the pure rect-in-rect packing
(`computeFlameGeometry`) with the PRD §16 assertions tested headless:
root width = canvas, widths ∝ samples, siblings never overlap, depth → y
monotone, hypothesis frame keeps `[2,4]`/`#f85149`, determinism. The G6
host page binds it via `register(ExtensionCategory.LAYOUT, 'cie-flame')`
+ `register(ExtensionCategory.NODE, 'cie-flame-frame')` — zero schema
change, zero adapter branching.

## 7. Catalog (PRD Part VI)

`catalog.ts` encodes all 76 charts as DATA with the per-category counts
of the PRD's tables. The PRD's summary row (68/4/4) contradicts its own
per-category tables (A: 3 custom, F: 1 custom ⇒ 69 native); we resolve to
the internally consistent set the PRD names in Part IX:
**69 native / 4 custom (A1 UML class, A6 ports, A7 actor, F2 flame) /
3 alt-engine (B3 Mermaid, B6 + F1 custom SVG)**. VIZ-25 runs the
provenance audit over a generated spec for every one of the 76 entries.

## 8. Ledger coverage

Headless (this repo, CI-green): VIZ-01..10, 12 (geometry core), 13 (text
output), 16, 17, 18, 19, 21, 23 (structural + audit), 25, 27, 28, 29, 30
— **259 named vitest cases** + 24 Rust cases.

Browser-bound (Playwright on a rendering host, pages provided):
VIZ-11/14/15 (canvas renders), 20/22 (fisheye interaction + renderer
parity pixels), 24 (screenshot baselines), 26 (keyboard walks).

## 9. Deviation ledger

| Deviation | Why |
|---|---|
| `renderHash` sealed engine-neutral by CIE | VIZ-10 forbids CIE naming an engine; adapters rebind (documented in `cie-viz/src/hash.rs`) |
| `hidden[]` outside `semanticHash` | PRD §3's enumerated domain lists elements/edges/groups/gaps/footer/scope only |
| Catalog counts 69/4/3 | PRD summary row (68/4/4) is internally inconsistent; Part IX's explicit four + non-G6 note resolve to 69/4/3 |
| Browser-bound ledger items not executed here | headless environment; pages + harness provided under `viz-adapter/demos/` |
| ReactFlow `layered` fallback layout | dagre/ELK are host-side; headless suite uses the deterministic built-in and DISCLOSES `Substituted` |
