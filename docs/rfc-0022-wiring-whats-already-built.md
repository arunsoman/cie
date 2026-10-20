# RFC-0022: Wiring what's already built — call-graph navigation, semantic search, adapter depth parity, and real multi-repo

**Status:** partially implemented — §6.1/6.2/6.3/6.4's CLI/engine pieces landed and are tested; §6.1/6.2's MCP tools are blocked on RFC-0021 §6.1; several deviations and gaps from this text are recorded honestly in §13, added after implementation, not before.
**Author:** the CIE maintainer (with pi)
**Date:** 2026-10-20
**Applies to:** `cie-relations`, `cie-semantic`, `cie-analysis`, `cie-store`, `cie-llm`, `cie-ingest`, `cie-adapter-go`/`-rust`/`-typescript`/`-javascript`, `cie-incremental`, `cie-cli`, `cie-server`
**Related:** RFC-0019 (stack-at-ingest, revision store), RFC-0020 (five unexported claims), RFC-0021 (MCP finish, drift wedge, adapter marketplace), TASK-007/009 (relations/PDG), TASK-059 (HNSW), the competitive-table discussion this session

---

## 1. Summary

A competitive table against kraklabs/cie, Sourcegraph, codebase-memory-mcp, Cursor, and Aider scored CIE across twelve verticals. Four are already best-in-class or already have a landing RFC: Evidence/Provenance, Contract/Conformance, Hierarchical Concepts (RFC-0019), and Document–Code Drift (RFC-0021 §6.2) need no new design here. Two are downstream consequences of the others, not separate engineering: Agent/MCP Readiness improves automatically as RFC-0021 §6.1 lands plus this RFC's two new tools; Maturity/Adoption is a visibility problem, not a code problem (RFC-0021 §12).

That leaves four verticals where CIE is genuinely behind, and all four turn out to share the same root cause: **real, correct substrates exist and are never used in production.**

1. **Call Graph + Impact/Blast Radius** — `cie-relations::RelationGraph` and its `Pdg` (a genuine Ferrante-Ottenstein-Warren program dependence graph, 10 relation kinds, bounded backward slicing) are fully implemented and correct — and built in exactly one place in the entire workspace: a test file. Production code instead maintains three separate, thinner, mutually-unaware "what calls what" structures.
2. **Semantic Search** — `cie-llm`'s `embed()` is implemented three times (offline hashing-trick, Ollama, OpenAI) and `cie-store`'s `HnswRsStore` (a real, persistent, budget-safe HNSW k-NN index) both exist — and are called from production code exactly zero times, combined.
3. **Multi-language Depth** — the gap isn't the adapters' optional trait overrides (every adapter overrides all four); it's that exactly one of fourteen adapters, Python, extracts doc comments into `SymbolFact.doc`. The other thirteen leave it permanently `None` — which is simultaneously most of what "light adapter" means and the reason semantic search would have nothing but bare names to embed for 13 of 14 languages.
4. **Scale (multi-repo)** — `OrgGraph.api_edges`, the field that would carry cross-repo call linkage, is declared and never constructed — the identical "declared, never populated" pattern RFC-0020 found in `DriftStatus` and RFC-0021 found in `blast_radius`. Multi-repo merging today is lowercased-label string matching on concept names; there is no real cross-repo symbol resolution.

This RFC wires all four into production. None of it is new algorithm design — it is the same move as RFC-0020: find the already-correct code, and build the missing connection.

---

## 2. Motivation

### 2.1 Call-graph navigation: one real graph, three thinner stand-ins

`RelationGraph` (`cie-relations/src/lib.rs:87-89`) wraps a `petgraph::stable_graph::StableGraph<SymbolId, RelKind>`. `RelKind` (`:31-42`) has 10 variants: `Calls, Returns, ControlFlow, DataFlow, Reads, Writes, Throws, Catches, Owns, DependsOn`. `edges_from`/`edges_to` (`:239-256`) are real, tested, single-hop lookups. `Pdg::build`/`build_with_rev` (`:395-510`) computes real post-dominator sets for control dependence, combined with data dependence from the `DataFlow`/`Reads`/`Writes` edges; `backward_slice`/`backward_slice_bounded` (`:514-557`) return a budget-bounded `BTreeSet<SymbolId>`.

Its only constructor, `relation_graph_from_facts(facts, store) -> RelationGraph` (`cie-semantic/src/graph.rs:16`), has exactly one call site in the workspace: `cie-relations/tests/task_007_relations.rs:70`.

Meanwhile, three *different* representations of "what calls what" coexist in production, none built from `RelationGraph`:

1. `cie-analysis/src/concurrency.rs`'s own `call_graph(facts) -> BTreeMap<SymbolId, Vec<SymbolId>>` (`:186-197`) — Calls-only, filtered to `Callee::Resolved`, rebuilt fresh every time concurrency analysis runs.
2. `reverse_deps: BTreeMap<SymbolId, BTreeSet<SymbolId>>`, built in `cie-analysis/src/behavior.rs:142-384` and re-derived in `cie-ingest/src/lib.rs:86-97` (`rebuild_graphs`) — what `cli:impact` actually traverses, via `cie_analysis::testing::affected_by_change` (`testing.rs:124-132`), which wraps it in `cie_incremental::ReverseDeps::transitive_callers`.
3. `RelationGraph` itself — correct, richer (10 relation kinds vs. 1), unused.

`impact` is already genuinely multi-hop/transitive — confirmed, that part of the brief's "Good (impact command, PDG substrate)" line holds — but it's transitive over the thinnest of the three representations, and the "PDG substrate" credit is aspirational: `Pdg` is unreachable from the production pipeline. kraklabs' `trace_path` and Sourcegraph's SCIP navigation score "Excellent" because multi-relation-kind, multi-hop navigation *is* their product surface; CIE has the better underlying graph and exposes none of it.

### 2.2 Semantic search: both halves exist, never introduced to each other

`VectorStore`/`HnswRsStore` (`cie-store/src/hnsw.rs:64-124`) is real: `insert`/`upsert`/`remove`/`knn(query, k, budget_ns)`, persisted via `attach_kv`/`persist`, dimension pinned per instance, stable record ids so re-embedding a changed symbol is an in-place `upsert`, and `KnnOutcome::Unknown(TimedOut)` on budget expiry rather than a silent empty result (global rule 3). `HnswRsStore::new` is called exactly four times in the workspace — all four in its own test file, `cie-store/tests/task_059_hnsw.rs`.

`embed()` (`cie-llm/src/lib.rs:41-42`, on `LlmProvider`) has three real implementations — `OfflineProvider` (64-dim deterministic hashing-trick, `offline.rs`), `OllamaProvider`, `OpenAiProvider` (real embedding-endpoint calls). `grep -rn "\.embed(" --include=*.rs .` returns **zero hits** anywhere, including tests of its own callers.

The text to embed already exists per-symbol: `SymbolFact.doc: Option<String>` (`cie-semantic/src/facts.rs:59`), with `qual_name`/`name` as always-available fallbacks. The incremental hook to embed only *changed* symbols already exists too: `SymbolWorklist`/`symbol_dirty_worklist` (`cie-incremental/src/lib.rs:187-196`). Even the budget discipline is already written: `Budget::charge` (`cie-llm/src/budget.rs:27-41`), whose own doc comment states the exact policy an embedding stage should inherit — "on exceed the engine downgrades to the offline provider and continues" (acceptance #14).

Every piece needed for real semantic search exists, tested, in isolation. Nothing connects them.

### 2.3 Multi-language depth: one function, in one adapter, out of fourteen

Going in, the working theory was that "deep" adapters override more of `LanguageAdapter`'s optional trait methods than "light" ones. Checked directly: Python, Go, Rust, and TypeScript **all** override `error_model`/`dispatch_kind`/`numeric_semantics`/`framework_conventions` — that's not the differentiator at all.

The real, singular gap: `docstring_of()` (`cie-adapter-python/src/lib.rs:249`, assigned into `SymbolFact.doc` at `:295-297`) is the **only** doc/comment-extraction function across all fourteen adapters. Direct grep for an equivalent in Go, Rust, TypeScript, JavaScript, C, C++, C#, PHP, Kotlin, Lua, SQL, and Shell: **zero hits in every single one.** Every non-Python symbol's `doc` field is permanently `None`.

This single gap is doing double duty: it's a large part of what "light adapter" means in practice, and — per §2.2 — it means §6.2's semantic search would have nothing but bare `qual_name` strings to embed for every language except Python, which measurably weakens search quality exactly where CIE could otherwise out-hybrid competitors by attaching typed-fact context nobody else has.

(Separately, confirmed Kotlin's and Lua's genuine size — 785 and 856 lines — is real extraction depth: ~260–280 lines of real class/call/taint-sink classification each, not padding or inline test bulk, built independently of any `cie-stack` framework lens, which — also confirmed — exists only for Java/Python/Rust and nowhere else.)

### 2.4 Multi-repo: the cross-repo edge field that nothing writes to

`OrgGraph` (`cie-store/src/lib.rs:62-66`): `{ repos: Vec<String>, merged_concepts: Vec<MergedConcept>, api_edges: Vec<ApiEdge> }`. `ApiEdge` (`:68-74`) has `from_repo`/`to_repo`/`from_symbol`/`to_symbol`/`kind` fields — exactly the shape needed for real cross-repo call linkage. Grep confirms `api_edges` is declared and never constructed anywhere in the workspace — the identical "declared, never populated" pattern RFC-0020 found in `DriftStatus::Drifted`/`Undocumented` and RFC-0021 found in the fictional "blast radius" capability.

`run_org_ingest` (`cie-ingest/src/lib.rs:752-784`) runs `run_ingest` sequentially per repo, then merges concepts by **lowercased label-string equality**, restricted to `UseCase`/`Aggregate`/`Entity` kinds, with a flat 0.9 confidence when ≥2 repos share a label. There is no symbol or import resolution across repo boundaries at all. RFC-0019 §10 open question 4 already named this precisely: *"Multi-repo org-graph stack merge — propagate `stack` per repo and merge Container nodes; defer the exact merge key policy to the FR-46 owner."* No monorepo-scale benchmark exists anywhere in the repository (docs, tests, or comments) — confirmed by grep. `RevSnapshot` (`cie-incremental/src/snapshot.rs:17-32`) is an all-fields-at-once struct with no sharding or lazy-load mechanism, meaning a snapshot is all-or-nothing per repo today, with unknown behavior at true monorepo scale.

Sourcegraph's whole product is built for this; it is CIE's furthest-behind vertical and the one genuine new engineering problem among the four (the other three are wiring jobs).

---

## 3. Goals

1. Bring `RelationGraph`/`Pdg` into the production pipeline, persisted per revision, exposed as a real `cie trace` CLI command and (depending on RFC-0021 §6.1) an MCP tool — multi-hop, multi-relation-kind navigation.
2. Wire `embed()` → `HnswRsStore` into a real, incremental, budget-safe `cie search` CLI command and MCP tool, with hits re-joined to their typed facts (concepts/contracts) — a hybrid search no competitor's plain embedding index can match.
3. Port doc/comment extraction to a priority subset of the thirteen adapters that currently lack it, populating `SymbolFact.doc` broadly — directly improves both the Depth vertical and §2 goal's search quality.
4. Give `OrgGraph.api_edges` a real constructor: resolve each repo's unresolved imports/calls against sibling repos' exported symbols, and ship a monorepo-scale benchmark to find out — not assume — whether the ~3% incremental-reingest number holds at org scale.
5. Backward compatibility: every new field/artifact is additive; absence of a prior revision's relation graph, vector index, or org edges degrades every new consumer to "compute fresh" or "report none," never a wrong claim.

## 4. Non-goals

- **Not** deleting or replacing `reverse_deps` (`cli:impact`) or `concurrency.rs`'s local `call_graph` in this RFC. Both have their own tested, shipping contracts; `RelationGraph` becomes the new *general-purpose* navigation surface for `trace`/`search`, not a forced migration of existing, working, specialized consumers. Consolidation is named as a future option (§10), not committed here.
- **Not** building a distributed or sharded storage engine for monorepo scale. Scope is real cross-repo edge resolution plus an honest benchmark — a storage rewrite is a different, much larger RFC if the benchmark says one is needed.
- **Not** porting doc-extraction to all thirteen remaining adapters in one pass — v1 prioritizes four (§6.3); the rest are a follow-up, ordered by actual usage once there is usage data.
- **Not** switching the default embedding provider away from the offline hashing-trick. Local/Privacy is already an "Excellent" vertical; this RFC must not trade it for search quality.
- **Not** attempting full LSP-style cross-repo project resolution (workspace files, path mapping configs). v1 resolves exactly the signal already available — `Callee::Unresolved(String)` matched against sibling repos' exports — which captures most of the value for a fraction of the engineering.

---

## 5. Current-state inventory (with references)

| Fact | Reference |
|---|---|
| `RelationGraph`/`RelKind` (10 kinds) | `cie-relations/src/lib.rs:31-42,87-89` |
| `edges_from`/`edges_to` — real, single-hop | `cie-relations/src/lib.rs:239-256` |
| `Pdg::build`/`backward_slice[_bounded]` — real PDG, budget-bounded | `cie-relations/src/lib.rs:382-391,395-510,514-557` |
| `relation_graph_from_facts` — the only constructor | `cie-semantic/src/graph.rs:16` |
| Its only call site in the entire workspace | `cie-relations/tests/task_007_relations.rs:70` |
| Parallel representation #1 — `concurrency.rs`'s own local call graph | `cie-analysis/src/concurrency.rs:186-197` |
| Parallel representation #2 — `reverse_deps`, what `impact` actually uses | `cie-analysis/src/behavior.rs:142-384`, `cie-ingest/src/lib.rs:86-97`, consumed via `cie-analysis/src/testing.rs:124-132` |
| `impact` is already real, transitive/multi-hop (just over the thinnest graph) | `cie-cli/src/main.rs:846` → `affected_by_change` → `ReverseDeps::transitive_callers` |
| `VectorStore`/`HnswRsStore` — real, persistent, budget-safe | `cie-store/src/hnsw.rs:64-124` |
| `HnswRsStore::new`'s only 4 call sites, all in one test file | `cie-store/tests/task_059_hnsw.rs` |
| `embed()` — 3 real impls, 0 production call sites | `cie-llm/src/lib.rs:41-42`; `offline.rs`, `ollama.rs`, `openai.rs` |
| `SymbolFact.doc` — the text to embed, already modeled | `cie-semantic/src/facts.rs:52-54,59` |
| `SymbolWorklist`/`symbol_dirty_worklist` — the incremental-embed hook | `cie-incremental/src/lib.rs:187-196` |
| `Budget::charge` — the downgrade-on-exceed policy to reuse | `cie-llm/src/budget.rs:27-41` |
| Python's `docstring_of` — the only doc-extraction function in 14 adapters | `cie-adapter-python/src/lib.rs:249,295-297` |
| Zero doc-extraction in the other 13 adapters | confirmed by direct grep, each |
| `cie-stack` framework lenses exist only for Java/Python/Rust | confirmed, `cie-stack/src/{spring,python,rust}.rs`, nothing else |
| Kotlin/Lua size is real extraction depth, not padding | `cie-adapter-kotlin/src/lib.rs:167-427`, `cie-adapter-lua/src/lib.rs:217-498,104,156` |
| `OrgGraph`/`ApiEdge` | `cie-store/src/lib.rs:62-74` |
| `api_edges` declared, never constructed | confirmed by grep — zero construction sites |
| `run_org_ingest` — sequential per-repo, label-string concept merge only | `cie-ingest/src/lib.rs:752-784` |
| RFC-0019's own open question naming this exact gap | `docs/rfc-0019-stack-at-ingest-and-c4-concept-tiers.md` §10, open question 4 |
| No monorepo-scale benchmark anywhere | confirmed by grep |
| `RevSnapshot` — all-or-nothing, no sharding | `cie-incremental/src/snapshot.rs:17-32` |

---

## 6. Design

### 6.1 Bring `RelationGraph`/`Pdg` into production — `cie trace`

**Insertion point:** inside `cie_ingest::run_ingest`, right after `cie_analysis::analyze` — the same slot RFC-0019 uses for the stack stage and RFC-0020 for the state-model stage:

```rust
// cie-ingest/src/lib.rs, after analyze, before save_snapshot
let relations = cie_semantic::graph::relation_graph_from_facts(&extracted.facts, &store);
store.save_relations(rev, &relations)?;   // new RelationGraphStoreExt, same pattern as StackStoreExt
```

Persistence: a new `RelationGraphStoreExt` trait (`save_relations`/`load_relations`), artifact at `<repo>/.cie/revs/<N>/relations.json` — a serde dump of the edge-tuple shape `edges_from`/`edges_to` already return (source, target, `RelKind`), not petgraph's internal representation.

**New CLI surface:**

```rust
/// Multi-hop call/data/control navigation over the persisted relation
/// graph (TASK-007/009). --kind filters to one RelKind; omitted = Calls
/// only, matching today's informal expectation. --depth bounds hops.
Trace {
    path: PathBuf,
    symbol: String,
    #[arg(long, default_value = "calls")]
    kind: String,
    #[arg(long, default_value_t = 3)]
    depth: usize,
    #[arg(long)]
    reverse: bool,
    #[arg(long)]
    json: bool,
},
```

Implementation: load `relations.json` (store-first, compute-if-absent — same `Stack`/`--force` precedent, `main.rs:74-78`), BFS via repeated `edges_from`/`edges_to` up to `depth` hops filtered by `kind`, each hop citing its originating `(file, line)`.

**MCP tool** (`cie_trace`, depends on RFC-0021 §6.1's `RealExecutor` landing first): routed to the identical BFS function the CLI uses.

**What does not change:** `reverse_deps`/`impact` and `concurrency.rs`'s local `call_graph` stay exactly as they are (§4). `RelationGraph` is additive — a new, more capable navigation surface, not a replacement.

**Failure policy:** no relations persisted yet → compute on demand, same degrade-gracefully pattern as every other store-first command. Budget exhausted mid-BFS → return the partial frontier with an honest `truncated: true` flag (mirrors `flow.rs`'s `MAX_FLOW_NODES` honesty note) — never silently incomplete.

### 6.2 Wire `embed()` → `HnswRsStore` — `cie search`

**New stage**, inside `run_ingest`, after the symbol dirty worklist is computed:

```rust
// cie-ingest/src/lib.rs, new stage after symbol_dirty_worklist
let dim = 64; // matches OfflineProvider's hashing-trick dimension
let mut vs = store.load_vector_index(rev_alloc.peek()).unwrap_or_else(|| HnswRsStore::new(dim));
let texts: Vec<(SymbolId, String)> = worklist.dirty.iter()
    .filter_map(|id| symbols.get(*id).map(|s| (*id, s.doc.clone().unwrap_or_else(|| s.qual_name.clone()))))
    .collect();
if !texts.is_empty() {
    let vecs = llm.embed(&texts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>())?; // budget-gated, auto-downgrades per Budget::charge's existing policy
    for ((id, _), v) in texts.iter().zip(vecs) {
        vs.upsert(id.0 as u64, v)?;
    }
}
store.save_vector_index(rev, &vs)?;
```

Only `worklist.dirty` symbols get (re-)embedded — an unchanged re-ingest costs zero new embedding calls, preserving the ~3% incremental-reingest number (README/TRACEABILITY, acceptance #10) rather than quietly regressing it.

**New CLI surface + MCP tool:**

```rust
/// Hybrid semantic search: vector KNN over symbol text, each hit
/// re-attached to its typed facts (concepts/contracts/evidence) — the
/// hybrid competitors can't do, since none of them have a typed fact
/// graph to re-attach.
Search {
    path: PathBuf,
    query: String,
    #[arg(long, default_value_t = 10)]
    k: usize,
    #[arg(long)]
    json: bool,
},
```

Implementation: `llm.embed(&[query])` → `vs.knn(vec, k, budget_ns)` → for each hit, attach its concept-node label(s) and any `Contract`s covering it (RFC-0020 §6.3's `verified_by`, if landed) — this joined view, not the raw KNN hit list, is the actual differentiator.

**Failure policy:** `KnnOutcome::Unknown(TimedOut)` → report "search timed out, partial/no results," never fabricated hits. LLM budget exhausted mid-embedding → downgrade to `OfflineProvider` per existing policy, continue, never block ingest. Default provider stays `OfflineProvider` (§4 non-goal).

### 6.3 Port doc/comment extraction — Go, Rust, TypeScript, JavaScript first

**v1 scope:** four adapters, chosen for extraction simplicity and expected usage, not all thirteen (§4). Each gets one new function mirroring Python's `docstring_of` shape:

```rust
// cie-adapter-go/src/lib.rs, new function, called where SymbolFact is built
fn doc_comment_of(src: &str, decl_line: u32) -> Option<String> {
    // walk lines immediately above decl_line while they start with "//",
    // collecting and trimming — the same line-scan style this file's own
    // extract_deep already uses for everything else.
}
```

Rust (`///`) and TypeScript/JavaScript (JSDoc `/** ... */` blocks) follow the same shape, adjusted for comment syntax. No schema change — all four populate the existing `SymbolFact.doc` field.

**Failure policy:** no doc comment found → `doc: None`, exactly today's behavior — purely additive, zero risk, since absence stays absence.

### 6.4 Real cross-repo resolution — populate `OrgGraph.api_edges`, then benchmark

**v1 scope:** resolve each repo's unresolved imports/calls against sibling repos' exported symbols, inside `run_org_ingest` after all member repos finish ingesting:

```rust
// cie-ingest/src/lib.rs, inside run_org_ingest, after all repos ingested
for (i, from_repo) in outcomes.iter().enumerate() {
    for unresolved in from_repo.store.unresolved_imports() {
        for (j, to_repo) in outcomes.iter().enumerate() {
            if i == j { continue }
            if let Some(sym) = to_repo.store.find_exported(&unresolved.name) {
                org.api_edges.push(ApiEdge {
                    from_repo: from_repo.name.clone(), to_repo: to_repo.name.clone(),
                    from_symbol: unresolved.caller, to_symbol: sym.id,
                    kind: "import".into(),
                });
            }
        }
    }
}
```

This reuses `Callee::Unresolved(String)` — the exact signal already distinguishing "a call CIE couldn't resolve within this repo" — as the input to a second resolution pass against sibling repos' exports, instead of giving up. No new unresolved-call tracking needs inventing; it already exists per-repo.

**Monorepo benchmark** (addresses the complete absence found in §2.4): a new fixture of N synthetic small repos, extending the polyglot five-service fixture pattern from RFC-0019/0020 to org scale, measuring `run_org_ingest` wall time and confirming — not assuming — whether the existing ~3% incremental-reingest number holds at org scale. If it doesn't, that's a real finding this RFC surfaces, not papers over.

**Failure policy:** no exported symbol matches an unresolved import across any sibling repo → it stays `Unresolved`, exactly as within a single repo today — never a guessed cross-repo edge.

---

## 7. Performance & budget

- §6.1: `RelationGraph` construction is `O(facts)` once per revision, the same cost class as the other per-revision analyses this pipeline already runs; persistence makes repeat `trace` calls cheap reads. BFS is bounded by `--depth` and a budget, same discipline as `backward_slice_bounded`.
- §6.2: the embedding stage only touches `worklist.dirty` — an unchanged re-ingest costs zero new embedding calls. `HnswRsStore`'s KNN is sub-linear by construction.
- §6.3: doc-comment extraction is a per-declaration line scan, same cost class as each adapter's existing extraction — negligible.
- §6.4: the cross-repo resolution pass is `O(unresolved_imports × repos)`, bounded by repo count (an `ingest-org` invocation parameter), not file count.

## 8. Test plan

| Level | Gate |
|---|---|
| **NEW** unit (`cie-ingest`) | `relations.json` persists and round-trips; a no-op re-ingest loads it rather than recomputing |
| **NEW** unit (`cie-cli`) | `cie trace` on a 3-hop synthetic call chain returns exactly the 3 symbols at `--depth 3`, 2 at `--depth 2`; `--kind reads` filters out `Calls` edges |
| **NEW** unit (`cie-ingest`) | the embedding stage embeds only `worklist.dirty` symbols on a changed-one-function re-ingest; zero embed calls on a no-op re-ingest |
| **NEW** unit (`cie-store`) | `cie search` against a 5-symbol fixture returns the semantically nearest match, each hit carrying its concept label |
| **NEW** unit (`cie-llm`) | embedding budget exhaustion downgrades to `OfflineProvider` and completes, never errors the whole ingest |
| **NEW** unit (adapters) | `doc_comment_of` (Go/Rust/TS/JS) extracts a real doc comment above a decl; returns `None` when none exists, never a wrong guess |
| **NEW** unit (`cie-ingest`) | `run_org_ingest` on 2 fixture repos with one real cross-repo import produces exactly 1 `ApiEdge`; an import with no matching export produces zero |
| **NEW** benchmark (`cie-ingest`) | monorepo fixture (§6.4) measures and records `run_org_ingest` wall time at N=20 synthetic repos, pass/fail against the existing ~3% incremental bar |
| Compatibility | every new field/artifact is additive; a pre-RFC snapshot with no `relations.json`/vector index/`api_edges` degrades every consumer to compute-fresh or report-none |
| Honesty | search timeouts and BFS truncation always report their own degraded state; cross-repo edges never appear without an exact exported-symbol-name match |

## 9. Alternatives considered

| Alternative | Why not (now) |
|---|---|
| Replace `reverse_deps`/`concurrency.rs`'s local call graph with `RelationGraph` outright | Risks two already-tested, shipping features in the same change that introduces a new capability. `RelationGraph` ships as an additive navigation surface; consolidation is a separate, assessed decision (§10). |
| Hand-roll a new graph library for `trace` instead of reusing `RelationGraph` | It already exists and is correct — only a production call site was missing. |
| Default to a paid embedding provider for better search quality | Regresses Local/Privacy, already an "Excellent" vertical — not worth trading one strength for another. |
| Full LSP-style cross-repo project resolution (workspace files, path-mapping configs) for §6.4 | Materially bigger scope; v1's unresolved-import-to-sibling-export match captures most of the value for a fraction of the engineering. |

## 10. Open questions

1. **`cie trace`'s default `--kind`** — `Calls`-only (matches today's informal expectation) vs. all 10 `RelKind`s by default (more powerful, more surprising). Leaning Calls-only default, `--kind all` opt-in.
2. **Whether/when to consolidate the three call-graph representations** — named, not committed; needs its own impact assessment against `impact`'s and `concurrency.rs`'s existing tested behavior before any such change.
3. **Embedding dimension policy if a paid provider becomes widely used** — `HnswRsStore` pins dimension per instance; OpenAI's embedding models aren't 64-dim like the offline default, so switching providers after real usage means a re-embed pass, not a config flag. Worth deciding the default-dimension policy before this ships broadly.
4. **Adapter priority order beyond Go/Rust/TS/JS** (§6.3) — the remaining nine, ordered by actual usage once there is usage data, not guessed here.
5. **Monorepo benchmark fixture size** — proposed starting point: 20 synthetic repos / ~2,000 files total, adjusted based on first results.

## 11. Build order (each stage shippable + revertable)

| # | Stage | Size |
|---|---|---|
| 1 | §6.1 — `RelationGraph` persistence + `cie trace` CLI | M |
| 2 | §6.3 — doc-extraction for Go/Rust/TS/JS | S (parallel-safe with everything else) |
| 3 | §6.2 — embedding stage + `cie search` CLI | M (soft-depends on #2 for better text quality, functional without it) |
| 4 | §6.1/§6.2 — `cie_trace`/`cie_search` MCP tools | S (depends on RFC-0021 §6.1's `RealExecutor`) |
| 5 | §6.4 — `api_edges` cross-repo resolution | M |
| 6 | §6.4 — monorepo benchmark fixture + measurement | S (depends on #5) |

---

## 12. Why this is safe for maintainers

- **Every new feature wires already-correct code into production for the first time, not new algorithm design.** `RelationGraph`, `Pdg`, `embed()`, and `HnswRsStore` are tested in isolation today; this RFC's job is the missing call site, not the logic behind it.
- **Nothing existing is deleted or risked.** `reverse_deps`/`impact` and `concurrency.rs`'s local call graph are untouched (§4); doc-extraction is purely additive (absence stays absence); cross-repo resolution never guesses, only confirms exact exported-symbol-name matches.
- **The embedding stage is incremental by construction** — as shipped, via content-addressed carry-forward (§13.2), not the `SymbolWorklist` diff this section originally proposed — it cannot silently regress the ~3% incremental-reingest number because it only ever re-embeds text that actually changed.
- **The one real new engineering risk (monorepo scale) is measured, not assumed.** §6.4 ships a benchmark specifically to find out whether the current story holds at org scale, rather than claiming it does — though at a smaller fixture than proposed (§13.4).
- **The rollback path is real.** Every stage in §11 is independently revertable; a reverted stage returns its consumers to exactly their current "compute fresh" or "report none" behavior.

---

## 13. Implementation status (added after landing §11 stages 1–3, 5–6)

Everything in §6 except the MCP tools (§11 stage 4, blocked on RFC-0021 §6.1's `RealExecutor`, which does not exist yet) has been implemented, compiled, and tested. This section is the honest reconciliation between what §6 specified and what actually shipped — written by re-auditing the diff, not from memory. Where the two disagree, this section is the authoritative account; §6 is left unedited above so the record of what was *designed* stays intact.

### 13.0 What's real, as specified

- `cie-adapters/src/light_engine.rs`: `doc_comment_of` + wiring into `push_light_symbol` — shared by Go/Rust/TypeScript/JavaScript **and Tsx** (a fifth coverage win beyond §6.3's stated scope, since all five already ran through this one function). 5/5 tests.
- `cie-ingest/src/relations_store.rs` (`RelationGraphStoreExt`), `cie-relations/src/lib.rs` (`RelationGraph::find_node`, additive), the `run_ingest` insertion persisting `relations.json` every revision, and `cie trace` in `cie-cli/src/main.rs`. 5/5 CLI integration tests plus a store round-trip test.
- `cie-ingest/src/vector_store.rs` (`EmbeddingStoreExt`, `PersistedVectors`, `update_embeddings`), the `run_ingest` insertion persisting `embeddings.json`, and `cie search` in `cie-cli/src/main.rs`. 4 lib tests (including a real KNN-rebuild round-trip) plus 3 CLI integration tests — verified live against a real fixture: the query "check user permission" ranked `app.authorize` (docstring: "Checks whether the caller has permission to proceed") first by cosine-style distance, ahead of `format_date`/`send_email`, and the hit carried its real `Contract` automatically.
- `cie-ingest/src/lib.rs`: `SymbolStore.unresolved` populated for real (it was declared and never written anywhere — confirmed before this change, the same "declared, never constructed" pattern this RFC's own §2 found in `OrgGraph.api_edges`), and `run_org_ingest`'s cross-repo resolution pass built on top of it. 3/3 tests, including an ambiguity case my own first draft got wrong (it checked ambiguity per-sibling-repo instead of globally) and the test caught before this shipped.
- `cie-ingest/tests/rfc0022_monorepo_bench.rs`, `#[ignore]`-gated: measured 0.6% reingest/full ratio on a real run (§13.4 notes the fixture size).
- Full-workspace `cargo check` is clean; every pre-existing test suite in every crate touched (`cie-adapters`, `cie-relations`, `cie-semantic`, `cie-ingest`, `cie-cli`) still passes.

### 13.1 `cie trace --kind` only exercises 4 of its 10 documented values in production

`relation_graph_from_facts` (`cie-semantic/src/graph.rs:16`) — the only call site wired into `run_ingest` — adds exactly `Calls`, `DataFlow`, `Owns`, and `DependsOn` edges. Its own doc comment says the rest: *"Statement-level relations (`ControlFlow`, `Throws`/`Catches`, `Reads`/`Writes`) are added by consumers that own statement facts (e.g. the PDG fixtures)."* Nothing in the real pipeline is such a consumer. So `cie trace --kind reads|writes|control_flow|throws|catches|returns` parses, filters, and runs without error — and returns **zero hops against any real codebase today**, not because nothing matches but because those six edge kinds are never populated outside test fixtures. §6.1's text did not call this out. A future RFC (or a revision of this one) needs either a statement-fact-driven builder that populates the other six kinds, or `cie trace --kind`'s help text amended to say plainly which four actually carry data today.

### 13.2 No real compute-if-absent fallback for `trace`/`search`

§6's stated failure policy ("no relations persisted yet → compute on demand, same degrade-gracefully pattern as every other store-first command") was not implemented as described. The shipped `Trace`/`Search` commands always call `run_ingest` first — which unconditionally (re)writes `relations.json`/`embeddings.json` every call — then separately re-open the store and read the artifact back. If that read ever comes back `None` (it shouldn't, given the unconditional write, but nothing guarantees it structurally), the CLI hard-errors instead of recomputing inline. This is defensive code, not the graceful fallback §6 promised; it has never been exercised by a failing test.

### 13.3 The budget-exceeded → offline-downgrade path is unreachable from any shipped command

`Trace` and `Search` both construct `IngestOptions::default()`, which hardcodes `no_llm: true` → `run_ingest` always builds `OfflineProvider` regardless of `update_embeddings`'s provider-aware logic. Neither command gained a `--llm-provider`/`--no-llm` flag (unlike `Analyze`/`Ingest`, which already have one). The result: `update_embeddings`'s `budget.exceeded()` check and its `Err(_) => offline.embed(...)` downgrade branch are real, unit-tested in isolation — and structurally dead in the binary as shipped. §8's planned test ("embedding budget exhaustion downgrades to OfflineProvider and completes") was never written, because there is no integration path to exercise it against. Closing this gap needs the CLI flags, not new engine logic.

### 13.4 The monorepo benchmark ran at 1/20th the proposed scale

§10 open question 5 proposed "20 synthetic repos / ~2,000 files total" as the starting point. The shipped fixture (`rfc0022_monorepo_bench.rs`) is 20 repos × 5 files = 100 files. The measured 0.6% reingest/full ratio is a real number from a real run, not an assumption — but it's evidence at roughly 5% of the file count originally proposed, and may not hold at the larger scale (sequential per-repo overhead, memory pressure, and OS-level file-walk costs don't necessarily scale linearly). Re-running at closer to the proposed size is an open follow-up, not done here.

### 13.5 The cross-repo matching algorithm is this implementation's own design, not a transcription of §6.4's pseudocode

§6.4's sketch called `report.routes.iter().find(...)` / `to_repo.store.find_exported(&unresolved.name)` — a function that does not exist. The shipped version matches on the call text's last dot-segment against the real `SymbolStore::by_name` index, with ambiguity checked **globally across every sibling repo**, not per-repo — a stricter rule than the sketch implied, added after a test (`an_ambiguous_match_across_two_repos_resolves_to_neither`) caught the per-repo version producing two guessed edges where it should have produced zero. The behavior is more honest than what was sketched; it is not what was sketched.

### 13.6 One piece of §6.2's pseudocode was corrected, not implemented as written

§6.2 showed `vs.upsert(id.0 as u64, v)` — using the raw `SymbolId` as the vector store's record id. Two things make that wrong: `VecRecordId` is `[u8; 32]`, not `u64`, and `SymbolId`s are **not** stable across revisions (`cie-ingest/src/lib.rs`'s purge/renumber path can reassign them). The shipped code uses `cie_core::types::hash(qual_name.as_bytes())` instead — content-addressed, matching `HnswRsStore`'s own documented "stable record ids" design intent (`cie-store/src/hnsw.rs:4-6`) and incidentally giving the incremental-reembedding logic (§13.0) its mechanism for free. This is flagged here because it is a correction to the written design, made silently during implementation, not a literal build of what §6.2 specified.

### 13.7 Test-plan gaps versus §8's own table

- No test proves a no-op re-ingest *loads* `relations.json` rather than recomputing it (the stack stage, its direct precedent, has exactly this test: `noop_reingest_answers_the_stack_from_the_store_not_a_reparse`; relations never got the equivalent).
- The "unchanged text isn't re-embedded" test (`unchanged_text_is_carried_forward_not_reembedded`) proves the *output* is byte-identical across two runs; it does not prove `embed()` was literally never called a second time — there is no call counter or mock in the test.
- No test for §13.3's downgrade path, for the reason given there.
- No dedicated JavaScript doc-extraction test (only Go/Rust/TypeScript) — the code path is shared with TypeScript's test and should behave identically, but it was never asserted directly.
- No compatibility test proving an old, pre-this-RFC `.cie` store upgrades cleanly when re-ingested with the new binary (`stack_store.rs` has exactly this test, `pre_rfc_snapshot_without_a_stack_field_still_loads`; relations/embeddings never got the equivalent). In practice this is likely fine, because `run_ingest` now writes both artifacts unconditionally on every call — but "likely fine" is an assumption, not a passing test.

### 13.8 Scope actually covered

Only this RFC (0022) was implemented. RFC-0019's stack stage turned out to already be real in the codebase — discovered during this work, not built by it. RFC-0020 and RFC-0021 remain entirely unimplemented design documents; nothing from either has been coded. §11 stage 4 (MCP tool exposure of `cie trace`/`cie search`) stays blocked on RFC-0021 §6.1 landing first, exactly as this RFC's build order said it would.
