# RFC-0019: One parse, one store — the stack lens at ingest, and the C4 tiers in the concept hierarchy

**Status:** draft
**Author:** the CIE maintainer (with pi)
**Date:** 2025-10-19
**Applies to:** `cie-ingest`, `cie-stack`, `cie-analysis`, `cie-concepts`, `cie-cli`, the web app
**Related:** FR-42 (every edge carries a reason), FR-43 (rollup), FR-44 (concept tree), US-106 (capability overlap), Tasks 15–18 (the stack pass, the polyglot fixture)

---

## 1. Summary

Today the engine answers "what is this project?" with **two lenses that never meet**:

- the **ingest lens** (`cie ingest` → adapters → `SymbolStore` → behaviour → conformance → FCA → concept
  hierarchy), which **persists** everything it learns into `<repo>/.cie/revs/<N>/` and is shared by every
  later command;
- the **stack lens** (`cie stack`, `cie-stack` crate), which re-walks the tree and re-parses every
  source file with its own tree-sitter grammars — in memory, every run, never persisted.

This RFC proposes to **run the stack lens inside ingest as a stage**, persist its report **as part of the
revision snapshot** (the fourth sibling of `ClaimStoreExt`/`DriftStoreExt`/`LabelStoreExt`), enrich the
docker reading so it can say **which containers are docker-managed and which are not**, and then give
`build_hierarchy` the stack facts so the concept DAG carries the **C4 Container and Component tiers** —
with per-node evidence strength (annotation-recorded vs inferred naming).

The result: one parse per revision, one artifact family, one hierarchy the Atlas (and everything else)
can point at — and the vocabulary for it already sits unused in `ConceptKind`.

---

## 2. Motivation

### 2.1 The two-lens problem (measured on this repo)

The web pipeline (`web/src/lib/analyze-jobs.ts`) invokes the engine nine times per analysis:

| # | step | invocation | parses sources? | persisted? |
|---|---|---|---|---|
| 1 | read | `cie ingest` | **yes — the only adapter pass** | **yes** (`revs/N/`) |
| 2 | map | `cie report` | no (`store.load_snapshot`, `cie-cli/src/main.rs:931`) | — |
| 3 | concepts | `cie concepts` | no — no-op re-ingest + `materialize` load (`cie-ingest/src/lib.rs:539`) | — |
| 4 | deps | `cie deps` | manifests only | — |
| 5 | stack | `cie stack` | **yes — second full tree-sitter pass** | **no** |
| 6 | cochange | `cie cochange` | git mining only | — |
| 7 | overlap | `cie overlap` | no — no-op re-ingest | — |
| 8 | drift | `cie drift` | claims against `.cie/` | yes |

Per analysis, every source file is parsed **twice** (adapters; stack). When a local folder is
re-analyzed later the adapter side is incremental (only changed files re-parse) while the stack side
**re-parses everything, every time**. And the two lenses cannot corroborate each other because they
share no artifact.

### 2.2 The Atlas consistency problem

The atlas Containers view derives "containers" as **the first directory segment of each file path**
(`web/src/lib/mastery.ts:455 buildC4Spec`, mirrored by `web/src/lib/atlas.ts:46` with the comment
*"Same grouping buildC4Spec uses — the two views must never disagree"*). That is a heuristic: correct
only when the repo's directory layout happens to be the deployment layout. The engine *knows* the real
answer — `detect.rs::find_apps` finds build-file roots at any depth — but nothing carries that into the
view. Consistency is enforced **by convention**; this RFC makes it hold **by construction**.

### 2.3 The hierarchy has reserved rooms, built but never occupied

`ConceptKind` (`cie-concepts/src/lib.rs:23`) already declares
`Domain | Subdomain | Module | Service | Entity | Value | Aggregate | UseCase | Rule | Invariant`.
A grep across the engine shows **`Subdomain` and `Service` are never constructed anywhere**. The
container/component tiers were designed in and left empty. Today's built hierarchy
(`cie-analysis/src/hierarchy.rs::build_hierarchy`) is:

- a `Domain` root ("Project"), then
- **one `Module` node per FILE** (`hierarchy.rs:38–52`, edge `PackageContainment`),
- one `Entity`/`Value`/`Aggregate` per class — with `Aggregate` chosen by a **hardcoded money-words
  heuristic** (`charge|pay|refund|transfer`, `hierarchy.rs:57–77`),
- behaviours (tag-labelled, `hierarchy.rs:83–121`),
- FCA-lattice `UseCase` nodes at the root with a `ClusterCohesion` back-edge to the majority module
  (`hierarchy.rs:141+`),
- contract `Rule`/`Invariant` nodes.

On CIE's own shipped polyglot sample this yields **45 nodes: 1 Domain + 24 Module(file paths) +
15 Value + 1 Entity + 4 generic UseCases — and the five services are structurally invisible.**
No container tier, no component tier; `berth-booking` and `billing` are indistinguishable from any
other path prefix.

### 2.4 The docker gap (evidence from the shipped sample)

`ComposeService` is read as `{ name, image, ports }` (`cie-stack/src/types.rs:85`) — **`build:` is not
recorded**, so the one field that ties a compose service to an app directory is lost. The reader also
misses inline-list ports (`ports: ["8081:8081"]`), so the shipped fixture's compose shows
`"ports": []` for all five services. And `AppSite { dir, name, build, framework, evidence }`
(`types.rs:297`) has **no docker linkage** — "governed by compose / by its own Dockerfile / by
nothing" is not expressible today.

---

## 3. Goals

1. **One parse per revision**: the stack lens runs as an ingest stage and its output persists with the
   revision; re-analysis of an unchanged repo re-derives stack from the store, never re-parsing.
2. **C4 tiers in the concept DAG**: `Container` (new kind) and `Service`/`Subdomain` (reserved kinds)
   nodes built from the persisted stack facts, with **per-node certainty** (recorded vs inferred) and
   per-edge reasons per FR-42.
3. **Docker governance**, stated per app: *managed by compose* / *own Dockerfile* / *not containerized
   (as far as the repo says)* — plus compose `image:`-only services surfaced as **C4 Context externals**.
4. **Single source of truth for the Atlas**: containers/components views re-source from the concept DAG;
   the first-path-segment heuristic retires.
5. **Backward compatibility**: old snapshots still load (serde-default on the new field); the `cie stack`
   command keeps working (store-first, compute-if-absent); every existing consumer of `concepts.json`
   keeps working with new kinds as additive values.

## 4. Non-goals (this RFC)

- **Not** rewriting the per-language parsers into the ingest adapters. The stack lens stays
  `cie-stack`, a separate crate; ingest *calls* it (see §6.1).
- **Not** eliminating the literal second parse of source bytes (the stack lens parses for
  routes/flows/DI, the adapters for symbols/summaries). The two lenses keep their extraction
  contracts; only the *duplication of persistence and invocations* ends. Adapter-recorded stack facts
  (the "derive, don't parse" end-state) remain a possible later RFC.
- **Not** removing the per-file `Module` tier under the new components. Consumers assume it
  (`conceptNodeForFile`, `atlas.ts:839`); the file tier becomes a leaf group under components.
- **Not** adding new languages (Go/NestJS lenses are separate increments).

---

## 5. Current-state inventory (with references)

| Fact | Reference |
|---|---|
| Ingest pipeline stages (walk→hash→plan→extract→analyze→persist) | `cie-ingest/src/lib.rs:158 run_ingest`, `:115 walk_and_hash`, `:410 cie_analysis::analyze`, `:498 store.save_snapshot` |
| No-op fast path ("re-ingest wall time is hashing + plan only") | `cie-ingest/src/lib.rs:190–218` |
| Post-load of persisted outcome (share across commands) | `cie-ingest/src/lib.rs:539 materialize`, `:551 load_outcome` |
| Analysis stage 5 = concepts + hierarchy | `cie-analysis/src/lib.rs:130–131` |
| Hierarchy builder (the whole of §2.3) | `cie-analysis/src/hierarchy.rs:23 build_hierarchy` |
| Extension traits precedent | `cie-ingest/src/lib.rs:21–26 (ClaimStoreExt, DriftStoreExt, LabelStoreExt)` |
| Snapshot tolerant of additive fields | `cie-incremental/src/snapshot.rs:17 RevSnapshot` (`#[serde(default)]` precedent, TASK-030) |
| Composite stack parser + per-app merge | `cie-stack/src/factory.rs:215 CompositeParser` |
| App detection at any depth | `cie-stack/src/detect.rs find_apps` |
| Compose reader (current shape) | `cie-stack/src/docker.rs:92 parse_compose`, `types.rs:85 ComposeService` |
| JVM role classification | `cie-stack/src/spring.rs:870 layer_of_class` |
| Web concept types | `web/src/lib/cie-types.ts:141 ConceptNode/:157 ConceptTree` |
| Atlas heuristic (the one this RFC retires) | `web/src/lib/mastery.ts:455`, `web/src/lib/atlas.ts:46–52` |
| Web cross-container calls (already computed in the client) | `web/src/lib/mastery.ts:455 buildC4Spec (crossCalls)` |

---

## 6. Design

### 6.1 Stage 1 — the stack stage inside `run_ingest` (`StackStoreExt`)

**Slot:** after the rayon extract and immediately **before** `cie_analysis::analyze` (so the hierarchy
can consume it in the same run), inside `run_ingest` between the `extracted` stage and the
`cie_analysis::analyze(&extracted.store, ...)` call (`cie-ingest/src/lib.rs:410`).

```rust
// cie-ingest/src/lib.rs (run_ingest, after extract, before analyze)
let stack = match stack_store::load_for_rev(&store, rev_alloc.peek()) {
    Ok(Some(cached)) => cached,                      // re-analysis, unchanged repo
    _ => match cie_stack::stack_report_stage(&project, &opts, budget_subslice) {
        Ok(r)  => r,
        Err(e) => StackReport::unavailable(format!(
            "stack lens failed for this revision ({e}) — other artifacts are unaffected"
        )),                                  // Amendment 3: degraded, never fatal
    },
};
stack_store::save_for_rev(&store, rev, &stack)?;      // persisted BEFORE save_snapshot
```

**Persistence model** — mirror the label/claim/drift extension traits:

```rust
pub trait StackStoreExt {
    fn save_stack(&self, rev: RevId, report: &StackReport) -> std::io::Result<()>;
    fn load_stack(&self, rev: RevId) -> std::io::Result<Option<StackReport>>;
    /// peek() = the rev the allocator is ABOUT to hand out; lets a no-op
    /// ingest answer from the PRIOR rev's stack without re-deriving.
    fn peek(&self) -> RevId;
}
```

Artifact: `<repo>/.cie/revs/<N>/stack.json` — a full serialized `StackReport` (already
`serde_json::Serializable`, identical to what `cie stack --json` prints today).

**Snapshot integration** (belt and braces — the snapshot stays self-contained for the
`materialize/load_outcome` path):

```rust
// cie-incremental/src/snapshot.rs (additive, serde-default like the TASK-030 fields)
pub struct RevSnapshot {
    ...
    /// The stack lens' reading of this revision (routes, flows, apps, docker,
    /// jvm family facts). Absent on snapshots from before this RFC.
    #[serde(default)]
    pub stack: Option<StackReport>,
}
```

`load_outcome` (`cie-ingest/src/lib.rs:551`) gains nothing — the field rides along.

**CLI compatibility:** `cie stack <dir>` becomes *store-first, compute-if-absent*:

```rust
Command::Stack { path, json } => {
    // if a .cie store exists and the head rev has a stack artifact → print it
    // (identical bytes, honestly stamped "from the revision store");
    // otherwise compute as today (first analysis of a bare folder).
}
```

**Web pipeline:** the `stack` step (`analyze-jobs.ts:881`) becomes a cached read once `report`
carries the field; when the step is retired, the jobs file keeps the step name and marks it
`"skipped — carried by the report"` so the user-facing wording (step list copy) stays honest.

### 6.2 Stage 2 — stack-input hashing (the invalidation contract)

`walk_and_hash(root, langs)` hashes only *language* files today (`cie-ingest/src/lib.rs:115`).
The stack lens reads inputs that are **not** language files: `pom.xml`, `build.gradle(.kts)`,
`Cargo.toml`, `package.json`, `requirements.txt`, `pyproject.toml`, `go.mod`, `docker-compose*.yml`,
`Dockerfile*`, `application*.yml/properties`.

**Design:** `walk_and_hash` gains a second bounded map (same walk, no second traversal):

```rust
pub struct IngestPlan {
    ...                       // as today (language files)
    pub stack_inputs: BTreeMap<String, Hash>,   // NEW: manifest + docker + config files
}
```

Rules:

- the stack artifact invalidates when EITHER `plan.changed ∪ plan.added` intersects a language file
  (facts may change) **or** the **stack-inputs diff is non-empty**;
- `stack_inputs` changes alone → re-run ONLY the stack stage, then the
  summary-reuse fast path continues to hold for the adapter lens (its inputs didn't change).
- the extensions list lives in `cie-stack` (it owns the inputs), imported by `cie-ingest` —
  no crate cycle (`cie-stack` already depends on nothing but `types`; a small
  `pub const STACK_INPUT_EXTS: &[&str]` is added there).

### 6.3 Stage 3 — docker enrichment + the governance join

**Data model diffs** (all byte-level reads as written; no yaml library, same style as today):

```rust
// cie-stack/src/types.rs
pub struct ComposeService {
    pub name: String,
    pub image: Option<String>,
    pub build: Option<String>,      // NEW — the build context (dir), e.g. "./billing"
    pub dockerfile: Option<String>, // NEW — build.dockerfile when named explicitly
    pub ports: Vec<String>,         // FIXED — inline list `ports: ["8081:8081"]`
                                    //          in addition to block-list rows
}

pub struct AppSite {
    ...
    pub docker: Option<AppGovernance>,   // NEW
}

pub struct AppGovernance {
    pub kind: String,    // "compose" | "own-dockerfile" | "none"
    pub service: Option<String>,  // the compose service name, when compose
    pub ports: Vec<String>,       // exposed ports, read exactly as written
    pub evidence: String,         // compose file/line or the Dockerfile path
}
```

**Join rules** (in the composite stage, after `detect` + `docker`):

1. normalize (`./` prefix, trailing `/`); `app.dir` equals a compose service's `build` context
   → `AppGovernance { kind: "compose", service, ports, evidence: <compose rel:row> }`.
2. else a `Dockerfile*` exists under `app.dir` (the dockerfiles list already carries paths)
   → `{ kind: "own-dockerfile", evidence: Dockerfile rel }`.
3. else `{ kind: "none" }` — and the report gains a note:
   *"berth-booking is not containerized as far as the repo says — no compose row names it and
   no Dockerfile sits under its directory."*

**Externals (C4 Context, L1):** compose services with `image:` and **no** `build:` are not our apps —
they are the named external systems. The report gains:

```rust
pub struct ExternalSystem {
    pub name: String,      // the compose service key
    pub image: Option<String>,   // "postgres:16" as written
    pub ports: Vec<String>,
    pub evidence: String,
}
// StackReport.external_docker: Vec<ExternalSystem>
```

On the polyglot fixture: five managed containers + (in a realistic compose) the database images as
externals — the Context level falls out of files that already exist, read deterministically.

### 6.4 Stage 4 — C4 tiers in the concept hierarchy

`build_hierarchy` gains the stack facts (Option-carrying signature — pre-RFC snapshots still build):

```rust
pub fn build_hierarchy(
    store: &SymbolStore,
    behavior: &BehaviorResult,
    contracts: &[Contract],
    label_fn: LabelFn,
    now: Ns,
    stack: Option<&StackReport>,   // NEW
) -> ConceptDag
```

**Node map** (all new edges carry `EdgeReason` + `provenance` per FR-42):

| Tier | Kind | Built from | Edge to parent | Certainty |
|---|---|---|---|---|
| C4 System (L0) | `Domain` (existing root) | — | — | — |
| App / deployable (L1) | **`Container` (new kind)** | `AppSite` (persisted stack) | `PackageContainment`, provenance `stack:apps` | recorded (the manifest + docker evidence) |
| Framework role / layer (L2) | **`Service` (reserved)** + `Subdomain` for DDD packages (`domain/application/infrastructure`) | jvm layers (`layer_of_class`) for spring; role/suffix naming for rust/ts/py; app-subdirectory fallback otherwise | `ClusterCohesion`, provenance `stack:roles` | **per-node**: `recorded` when an annotation/decorator says so (`@Service`, `@app.route` module…), `inference` when name/structure based |
| Entity/Value/Aggregate (L3, existing) | unchanged kinds | **re-parented** under their Service/Component node instead of the file Module | `CallOwnership` (the file's own containment becomes `PackageContainment` under the component) | unchanged |
| File `Module` nodes | unchanged kind | kept as leaf groups under the component (non-goal 3) | `PackageContainment` | recorded |
| `UseCase` (FCA) | unchanged | lattice attributes **gain stack facts** (`http-route`, `db-write`, `di-injected`, `docker-managed`) → sharper intents | unchanged | unchanged (`0.8` + label provenance) |
| `Rule`/`Invariant` | unchanged | unchanged | unchanged | unchanged |

**Re-parenting of the class nodes.** Today a class node links to its FILE's module
(`hierarchy.rs:57–77`). Under this RFC the class links to its **Service/Component node** (matched by
`sym.loc.file` inside the component's file set), and the file module links under the same component.
Both edges keep reasons; the DAG depth goes Project → Container → Component → (Module → class*).

**The money-words heuristic is replaced.** The `Aggregate` rule (`hierarchy.rs:62–70`) becomes:
- a class reached FROM a route and reaching a repository → DDD Aggregate (recorded: two evidence
  links), else
- framework-role by annotation (existing `layer_of_class` output), else
- keep Entity/Value as today.
The old vocabulary heuristic retires with a note in the report when it fired
(*"classified Aggregate because its methods read like money verbs — name-based, not evidence-based"
*: kept only as the fallback when no stack facts exist).

**Cross-container edges.** Two sources, both honest:
1. **DI/interface bindings** (annotation lens, recorded): `@Bean`/injection types that resolve to a
   component's implementation — recorded as `extra_edges` with `CallOwnership`.
2. **Cross-container HTTP `calls`** (the new, deterministic piece): each app's outbound URL *literals, as
   written* (httpx / reqwest / fetch / `RestTemplate` / `WebClient` argument strings) matched against the
   **other apps' `RouteSite` paths**. Recorded per edge: `from`, `to`, `via: "http"`, the matched path,
   and BOTH files as evidence. Unmatched outbound strings are listed honestly as
   *"calls `<url>` — names an endpoint no CIE-parsed service serves (external or unparsed)."*
   (Client-side cross-container counts already exist in `buildC4Spec`; this stage moves the *authoritative*
   edge list into the engine.)

### 6.5 Stage 5 — consumers re-source

- **`report.json`/`concepts.json`** now carry the stack field and the C4-tiered DAG. Both artifacts
  remain additive for old clients: `ConceptTree.nodes` gains kinds the web already tolerates
  (`cie-types.ts:141` comment lists example kinds, not an exhaustive contract).
- **Atlas** (`web/src/lib/atlas.ts`, `mastery.ts`):
  - `buildC4Spec` reads `stack.apps` when present (dir/name/framework/docker as node metadata with
    evidence chips); falls back to the path heuristic when `stack` is absent (older artifacts) —
    no project regresses.
  - The components panel reads `Service`/`Subdomain` concept nodes — the "two views must never
    disagree" comment becomes true by construction (both views read one DAG).
  - Certainty chips: `recorded`/`inference` from the node confidence — the existing
    `CertaintyChip`/`displayMode` machinery renders it.
- **TS types:** `stack-types.ts` gains `AppSite.docker`, `ExternalSystem`, and the concept kind comment
  update; `ConceptNode` docs list the new kinds.
- **Samples:** banking/spring/polyglot concepts.json + report.json regenerated after the engine lands;
  the polyglot sample is the visual proof (5 containers, per-language components, one externals box
  for postgres-ish images if/when the fixture compose gains a database image-only service — optional
  fixture tweak, cheap and illustrative).

### 6.6 Failure policy (Amendment 3, concretely)

| Failure | Behaviour |
|---|---|
| stack stage errors/times out at ingest | `StackReport.unavailable(reason)` persisted; `analyze` proceeds; note appears in the stack field and the web shows the honest degraded state |
| compose file malformed | per-service skip + note (already the docker crate's style); never fatal |
| a route parse cap trips | existing honest-truncation notes (unchanged, `flow.rs`) |
| old snapshot without `stack` field | `#[serde(default)]` → `None` → hierarchy builds without the C4 tiers (the exact pre-RFC shape) — **old artifacts never produce wrong new claims** |

### 6.7 Compatibility

- **Persisted stores:** additive snapshot field only; rev chain, tombstones, summary reuse untouched.
- **CLI:** `cie stack` gains store-first behaviour; `--force` flag to recompute (default off).
- **Web:** tolerant by design (optional fields + fallback paths named above).
- **Org graph / multi-repo merge (FR-46):** the merged concept roots gain Container nodes at merge
  time — merge keys by node id remain label-independent; no change needed beyond propagating the
  stack field per repo (deferred detail: §10).

---

## 7. Performance & budget

- The stack stage joins the ingest deadline: a **sub-deadline slice** (e.g. `min(remaining/4, 20s)`)
  with the existing honest caps (`flow.rs::MAX_FLOW_NODES/MAX_FLOW_DEPTH`, `detect.rs::MAX_DIRS`).
  The stack lens was measured sub-second on 5 apps / 36 files (the polyglot sample, release build);
  the caps keep the worst case bounded by design.
- No-op re-analysis: walk + hashes only (both maps), stack loaded from disk — the current
  "hashing + plan" wall time barely moves.
- The web pipeline drops one invocation + one full second parse per unchanged re-analysis; for large
  local repos the diff between "re-parse everything" and "re-read a json" is the scaling win.

---

## 8. Test plan

| Level | Gate |
|---|---|
| Unit (cie-stack) | existing `tests/polyglot.rs` (12 routes / 5 mains / 5 flows / 5 apps, jvm dedupe, DDD port) must stay green |
| **NEW** unit | compose: `build:` captured; inline ports parsed; image-only services → externals; malformed rows → skip + note |
| **NEW** unit | governance join: compose-matched / own-Dockerfile / none, on three tiny fixtures |
| **NEW** unit (cie-analysis) | hierarchy with stack: Container nodes 1-per-app, per-component roles with recorded certainty for spring, inference-tagged for naming; file Modules under components; no-stack input builds the pre-RFC shape exactly |
| **NEW** unit (cie-ingest) | stack-input hashing invalidates the stack artifact on pom-only change; language-only change reuses stack; no-op loads stack from disk |
| **NEW** unit (compatibility) | a pre-RFC persisted snapshot (no `stack` field) deserializes and rebuilds the legacy hierarchy |
| E2E (web) | polyglot sample: atlas containers view = 5 app nodes with governance badges; components panel from the DAG; drill-down container → component → module → entity; banking sample unchanged (honest-stub path); spring sample regression |
| Honesty | every new note asserts "as written" / "as far as the repo says" phrasing; no "managed by docker" without evidence |

---

## 9. Alternatives considered

| Alternative | Why not (now) |
|---|---|
| **Web-side graft** (merge concepts + stack at load time in `cie-store`) | fastest, zero engine change — but the CLI consumers (`concepts` dot/mermaid/markbook, org graph, `impact`'s affected concepts, `overlap`) never see the enriched DAG, and the atlas still lacks a single source of truth. Revisit only if the engine stage ever proves too risky. |
| **One unified command** (`cie analyze --all` emitting all artifacts from one in-memory outcome) | removes invocation walk/hash duplication in the pipeline, but still leaves the stack lens unpersisted and the hierarchy blind. Complements, does not replace — worth doing later as the pipeline's single entry point. |
| **Adapters record stack facts** (derive stack from the persisted SymbolStore, no second parse) | the true end-state, but it rewrites the adapter contracts (routes/decorators/DI extraction inside every adapter) — a bigger, riskier change that buys only the residual (second parse), which is the cheapest part of the duplication. Explicitly deferred; this RFC's seam (`StackReport` as `RevSnapshot` field) is exactly what that future RFC would flow through. |
| **Full UAST** (universal intermediate syntax tree across languages) | rejected in RFC review: C4 needs ~10 semantic unit kinds, not universal syntax; CIE's facts-level schema (`StackReport` + `SymbolStore`) IS the right substrate. The MLCPD-style alignment studies concern whole-syntax alignment — out of scope for deterministic, evidence-cited artifacts. |

---

## 10. Open questions

1. **`.cie` inside in-place local projects** — the store lives in the analyzed folder by design today
   (drift already writes there). Keep (consistent, incremental) vs move to a caller-managed cache dir
   (cleaner for read-only-ish use). Default for this RFC: keep, unchanged.
2. **Externals beyond compose** — Kubernetes manifests, terraform? Deferred; compose is the shipped
   read. The `ExternalSystem` shape is deliberately generic enough to extend.
3. **HTTP-call matching precision** — how aggressive should env-var-templa­ted URLs be handled?
   v1: literal strings only + `env("SERVICE_URL")`-style names quoted as unresolved → honest note;
   template interpolation is never evaluated.
4. **Multi-repo org-graph stack merge** — propagate `stack` per repo and merge Container nodes;
   defer the exact merge key policy to the FR-46 owner.

---

## 11. Build order (each stage shippable + revertable)

| # | Stage | Size |
|---|---|---|
| 1 | `StackStoreExt` + stack stage in `run_ingest` + snapshot field + CLI store-first | S-M |
| 2 | stack-input hashing + invalidation rules | S |
| 3 | compose `build:`/ports enrichment + governance join + externals | S |
| 4 | C4 tiers in `build_hierarchy` (+ Aggregate heuristic retirement) | M |
| 5 | consumers: report/concepts/atlas/pipeline + TS types + sample regeneration | M |
| 6 | (later) unified `analyze --all`; (much later) adapter-recorded stack facts; Go/NestJS lenses | — |

Stages 1–3 are independent and small; stage 4 is the first that changes visible artifacts
(concepts.json); stage 5 flips the consumers. Each lands behind its own commit + tests.

---

## 12. Why this is safe for maintainers (the framing that matters)

- **Nothing new is invented.** Persistence = the fourth extension trait; invalidation = the plan's
  existing diff machinery; the hierarchy tiers = reserved enum values; the views = re-pointed
  builders. A maintainer reads `ClaimStoreExt` and already understands `StackStoreExt`.
- **One place to reason about freshness.** "What did CIE see for rev N?" becomes one question with one
  answer (the revision folder), instead of two (the store's snapshot + a stack artifact nobody kept).
- **Degradation is explicit and capped.** Any stack failure ships a persisted honest `unavailable`
  record — the analysis never blocks, and the web says exactly what was and wasn't read.
- **The rollback path is real.** Because every consumer keeps its fallback (no stack → path
  heuristic; no tiers → flat hierarchy; command → compute-if-absent), reverting any stage returns to
  today's behavior without data loss.