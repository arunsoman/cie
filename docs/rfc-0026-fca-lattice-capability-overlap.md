# RFC-0026 — concept-lattice capability overlap: a literature-grounded replacement for the all-pairs matrix

Status: §§1-8 (the overlap/lattice redesign) are literature + design only —
no code changes yet. §§9-10 are implemented: the coalesced/persisted
on-demand build mechanism (§9) and drift's move to fully on-demand (§10).
Scoped to the *capability-overlap* problem (step 12, "Checking for
overlap"); the *syntactic duplicate-code* problem (`cie-polyglot::duplicates`,
step 7's `duplicates` / `duplicate_clusters` sub-stages) is a different
question with a different fix and is out of scope here (see §7).

## 1 · The problem, precisely

Today, "does a capability like this already exist" is answered by
`cie_concepts::overlap_matrix` (`cie-concepts/src/overlap.rs`), called once
at export time from `cie-cli/src/ingestion_job.rs:57-78`. It:

1. Takes every concept node from the FCA/Godin lattice built in step 6
   ("Analyzing and grouping concepts"), each mapped to the set of symbol IDs
   it covers.
2. Compares **every concept against every other concept** — an `O(n²)`
   loop over all concept pairs.
3. For each pair, computes and *fully materializes* `shared`, `only_a`,
   `only_b` symbol-ID sets, then resolves each into a second, parallel
   `Vec<String>` of qualified names.
4. Assembles the whole result as one in-memory JSON value before writing.

For a large repo with many concepts, this is quadratic in concept count and
duplicates memory per surviving pair — which is what produced the
out-of-memory failure that started this thread. It is also the wrong tool
for the capability the user actually wants: *"a new feature needs A+B+C+D —
does something already cover A+B+C, so I only need to add D?"* That query
does not need an all-pairs matrix at all. `cie_concepts::reuse_verdict`
already computes the right *output* (`reusable` = intersection, `adds` =
difference, `duplicate` = adds is empty) — it just has no way to *find*
which existing concept to call it against except by brute force.

The real problem, stated academically: given a set of "objects" (functions)
each tagged with a set of "attributes" (behaviors/operations), build a
structure that lets you ask, for any attribute set `Q`, *"which existing
object's attribute set is the largest subset of `Q`, and what's left over"*
— without comparing every object to every other object, and without
materializing every pairwise comparison in memory.

## 2 · Literature survey

| # | Paper | What it gives us |
|---|---|---|
| 1 | Ganter, B. & Wille, R. (1999). *Formal Concept Analysis: Mathematical Foundations*. Springer. [doi:10.1007/978-3-642-59830-2](https://link.springer.com/doi/10.1007/978-3-642-59830-2) | The foundational theory. A Galois connection between objects (functions) and attributes (behaviors) partitions objects into **concepts**, ordered into a **lattice** by attribute-set inclusion. Subset relationships like "A+B+C ⊆ A+B+C+D" are literal positions in this lattice — a lookup, not a comparison. This is the structure `reuse_verdict` is implicitly assuming already exists. |
| 2 | Godin, R., Missaoui, R., & Alaoui, H. (1995). "Incremental Concept Formation Algorithms Based on Galois (Concept) Lattices." *Computational Intelligence*, 11(2), 246–267. | **Incremental** lattice maintenance — update the lattice as new functions/files are ingested rather than rebuilding it from scratch every revision. This is the exact algorithm our own step-6 label already names ("FCA/Godin concept groups"); it's cited in spirit but not yet acted on for the overlap query. |
| 3 | Valtchev, P., Missaoui, R., & Godin, R. (2002). "Generating Frequent Itemsets Incrementally: Two Novel Approaches Based on Galois Lattice Theory." | Refines incremental maintenance for the itemset-mining view of the same structure — relevant when re-ingesting a changed revision as a delta rather than a full rebuild. |
| 4 | Kuznetsov, S. — **Close-by-One (CbO)**; Andrews, S. (2009). "In-Close, a fast algorithm for computing formal concepts." [SHU repository](https://shura.shu.ac.uk/38); Andrews, S. (2011). "In-Close2, a high performance formal concept miner." | Efficient **enumeration/traversal** algorithms that generate only the concepts that actually exist in the data, without ever materializing the full (worst-case exponential) lattice or all pairwise comparisons. In-Close is ~20× faster than the prior fastest alternative (Krajča et al., 2008) in the authors' benchmarks and needs no matrix pre-processing. This is what the "find the best-covering concept for query `Q`" traversal uses instead of an all-pairs scan. |
| 5 | Stumme, G., Taouil, R., Bastide, Y., Pasquier, N., & Lakhal, L. (2002). "Computing Iceberg Concept Lattices with TITANIC." *Data & Knowledge Engineering*, 42(2), 189–222. | **Minimum-support pruning**: only materialize concepts whose object count clears a threshold. Bounds lattice size on a large repo by dropping one-off, idiosyncratic attribute combinations that appear in a single function — exactly the long tail that made the current all-pairs matrix blow up. Doubles as a condensed representation of frequent closed itemsets, useful later for "what's the common pattern across N functions" queries. |

## 3 · How the papers compose into one design

- **[1]** gives the data structure (the lattice) — build it once from the
  concept→symbol-set mappings already produced in step 6, instead of never
  building it and comparing every pair at query time.
- **[2]+[3]** say *build it incrementally*, during ingestion, not from
  scratch on every export — so the cost is paid once per revision delta,
  not once per `overlap` export call.
- **[5]** says *prune while building* — apply a minimum-support floor so the
  lattice stays bounded regardless of repo size, instead of keeping every
  pairwise combination the way `min_shared = 1` does today.
- **[4]** says *traverse, don't enumerate* — answering "best match for query
  `Q`" is a top-down walk pruned the moment a branch's attribute set stops
  being a subset of `Q`, not a scan of every stored concept.

Net result: build cost is bounded by the number of *real* closed attribute
combinations in the repo (typically far below `n²`), and each query against
it is cheap, instead of the query itself being the `O(n²)` operation.

## 4 · Where this plugs into the pipeline

**Move the build into step 7, not step 12.** Step 7 (`cie-storage::repository::commit_with_progress`,
`cie-storage/src/repository.rs:416-672`) already builds and persists every
other derived view during ingestion — `c4`, `state_machines`, `workflows`,
`duplicates`, `duplicate_clusters` — each as its own `record_stage(...)`
call. The concept lattice belongs in that list as `concept_lattice`,
built once per revision from the concept→symbol mappings step 6 already
produced, using Godin's incremental update when a prior revision's lattice
exists, and Stumme's support floor to bound its size.

Step 12's `overlap` export in `cie-cli/src/ingestion_job.rs:57-78` then
stops *computing* anything — it becomes a cheap read of the already-built,
already-bounded lattice (for the summary artifact) plus the new live-query
path described below. This is the same pattern the pipeline already uses
everywhere else: step 7 builds and persists, later steps only retrieve.

New module sketch (naming only, not implementation):
- `cie-concepts/src/lattice.rs` — builds/updates the Galois lattice
  (Godin-incremental), applies the iceberg support floor, exposes a
  `best_match(query: &BTreeSet<SymbolId>) -> Option<(ConceptId, ReuseVerdict)>`
  traversal (CbO/In-Close-style pruning).
- `cie-concepts/src/overlap.rs` — keeps `reuse_verdict` exactly as is (it's
  already correct); `overlap_matrix` is demoted to a thin, support-floor-bounded
  *view* over the lattice's edges, not an independent all-pairs computation.

## 5 · Exposing the query through MCP

This repo's MCP surface (`cie-server/src/mcp.rs`) deliberately stays at
**two** tools — `cie_describe` and `cie_retrieve` — generated from one
schema, not an open-ended tool list; the test suite explicitly refuses a
legacy third tool (`invalid_protocol_and_legacy_tools_are_refused`, which
rejects `cie_impact`). The right way to expose this capability respects
that constraint instead of adding a third tool:

1. **Static view**: `ArtifactKind::Overlap` already exists
   (`cie-storage/src/retrieval/contract.rs:438,476`, key `"overlap"`,
   already listed in the capabilities blob's `artifact_kinds`,
   `cie-storage/src/retrieval/mod.rs:266`). No new artifact kind is needed —
   it now simply comes from the lattice instead of the brute-force matrix.
   Any MCP client already gets this through `cie_retrieve` with
   `output: "artifact", kind: "overlap"`.
2. **Live query**: an agent proposing a new feature needs to hand in an
   *arbitrary* attribute set (`A+B+C+D`) that was never precomputed. That's
   a `Detail`, the same mechanism `Impact` already uses for on-demand,
   `build_if_missing` computation (`cie-storage/src/retrieval/contract.rs:394-409`).
   Add one variant:
   ```rust
   CapabilityMatch {
       attributes: Vec<String>,         // the behaviors the proposed feature needs
       #[serde(default)]
       availability: AvailabilityPolicy,
   }
   ```
   returning the lattice's `best_match` result (existing concept, `reusable`,
   `adds`). It's discoverable the same way `function_ir`/`impact` already
   are (`details_list` in the capabilities blob), and callable through the
   *same* `cie_retrieve` tool with `detail: "capability_match"` — no new
   tool name, no bespoke query language, consistent with
   `tool_declarations()`'s own comment: "Generated input schemas, never an
   independent MCP query language."

So the MCP tooling list does not grow; the existing two tools become able
to answer this question because the thing they read from (the lattice,
persisted at step 7) now exists.

## 6 · Wire-up (once implementation starts)

| File | Change |
|---|---|
| `cie-concepts/src/lattice.rs` | NEW — Godin-incremental lattice build/update, iceberg support floor, CbO/In-Close-style `best_match` traversal |
| `cie-concepts/src/overlap.rs` | `reuse_verdict` unchanged; `overlap_matrix` becomes a bounded view over the lattice |
| `cie-storage/src/repository.rs` | new `record_stage("concept_lattice", ...)` next to `duplicates`/`c4`/`workflows` in `commit_with_progress` |
| `cie-cli/src/ingestion_job.rs` | `overlap` export reads the persisted lattice instead of recomputing it |
| `cie-storage/src/retrieval/contract.rs` | `Detail::CapabilityMatch { attributes, availability }` |
| `cie-storage/src/retrieval/tools.rs` | wire the new detail into `retrieve_with_cancel`'s dispatch |
| `web/src/lib/analyze-jobs.ts` | step 12 hint text updated to describe a lattice read, not an all-pairs scan |

## 7 · Explicitly out of scope

Syntactic duplicate-code detection (`cie-polyglot::duplicates`, the
`jscpd`-based near-duplicate scan and its own `O(n²)` exclusion-counting
loop) is a *different* problem — text/token similarity for extract-function
refactoring, not attribute-set subsumption for capability reuse. It has its
own literature (winnowing, SourcererCC-style prefix filtering, MinHash/LSH)
and its own fix, covered separately; nothing in this RFC changes
`duplicates.rs`.

## 8 · Open questions

- Iceberg support floor: what minimum object-count should prune a concept
  from the lattice? Too low and idiosyncratic one-off combinations creep
  back in (the original OOM risk); too high and genuinely reusable
  two-function overlaps get hidden. Needs calibration against a real repo,
  not a guess.
- Cross-revision lattice reuse: Godin's algorithm updates a lattice
  incrementally within a growing object set — confirm our revision model
  (new `rev` per ingest) maps cleanly onto "add objects," not "replace the
  whole object set," or the incremental benefit is lost.

## 9 · Implemented: coalesced, persisted on-demand builds (`cie-storage::retrieval`)

Deferring expensive work to first-request only works if two things are both
true: concurrent first-requesters don't each pay the full cost (the
thundering-herd / cache-stampede problem — cf. Nishtala et al., *"Scaling
Memcache at Facebook,"* NSDI 2013, §3.2.1's leases, and the general
"singleflight" / request-coalescing pattern), and paying the cost once means
nobody pays it again. Both are now implemented in
`cie-storage/src/retrieval/deferred.rs`:

- **`DeferredBuilder` trait** — `fn build(&self, root, rev, kind, budget) -> Result<Value, String>`.
  `cie-storage` never depends on whatever crate actually knows how to build
  an artifact (that would invert the workspace's dependency order — see
  below); the host that constructs `Storage`/`StorageMcp` injects a
  concrete implementation as `Arc<dyn DeferredBuilder>` via
  `Storage::with_deferred_builder` / `StorageMcp::with_deferred_builder`.
  Without one injected, behavior is unchanged (`BuildIfMissing` for an
  unsupported kind just reports unavailable, as before).
- **`BuildRegistry`** — a `Mutex<HashMap<(rev, kind), Arc<BuildSlot>>>` where
  each slot is a `Mutex<Slot> + Condvar`. The first miss for a `(rev, kind)`
  spawns exactly one background thread; every concurrent miss for the same
  key joins that thread's slot and waits on its condvar instead of starting
  its own build. On success the result is persisted via
  `RevisionArtifacts::put` *before* the slot is marked done, so a reader
  that later finds the slot evicted goes straight to the persisted file. On
  failure, nothing is persisted and the slot is evicted immediately — a
  transient failure does not permanently wedge future callers out of
  retrying (see `a_failed_build_is_not_persisted_and_does_not_wedge_future_retries`
  in `deferred.rs`'s test module).
- **Liberalized background budget** — the background thread runs under
  `BACKGROUND_BUILD_BUDGET` (30 minutes), independent of whatever budget the
  *triggering* request was under. A waiting caller still only blocks up to
  its own remaining `Execution` deadline (`ex.remaining()`); if that elapses
  first it gets `BuildOutcome::Pending` → a `"BuildInProgress"` error (not a
  hard failure, a "retry shortly" signal), while the build keeps running for
  whoever asks next. This is the "limits are liberalized when called from
  the retrieval system" requirement: the retrieval system's own background
  build is not bound by the MCP transport's `McpBudget`/30s-per-call cap
  (`cie-server/src/mcp.rs`'s `execute()` budget check and `timeout_ms`
  clamp) — only the foreground call that happens to be waiting on it is.
- **`fn artifact()` (`outputs.rs`)** now, for a kind with no value in the
  reader's pinned snapshot: (1) checks `BuildRegistry::read_persisted` —
  covers a build that finished in a different process after this reader
  opened; (2) if `availability == BuildIfMissing` and a builder is
  injected, calls `BuildRegistry::get_or_build`; (3) otherwise falls back to
  today's `"ArtifactUnavailable"`. `Output::Artifact` gained an
  `availability` field (default `BuildIfMissing`, matching `Detail::Impact`'s
  existing convention) so a caller can request `stored_only` explicitly.
- **Dependency direction** — `cie-ingest` already depends on `cie-storage`
  (for `relations_store`, `stack_store`, etc.), so `cie-storage` cannot
  depend back on `cie-ingest` without a cycle. `cie-ingest` therefore
  implements `cie_storage::retrieval::DeferredBuilder` itself
  (`cie_ingest::DriftDeferredBuilder`), and `cie-cli` — which already
  depends on both — injects it at the one place it constructs
  `StorageMcp` (the `mcp` subcommand in `main.rs`). `cie-server` itself
  gained only the one-line setter; it still has no dependency on
  `cie-ingest`, so its "read-only MCP stdio host" framing is unchanged — it
  hosts a capability someone else supplies, it doesn't grow one. The web
  app's native binding (`cie-storage-node`) is **not** wired to a builder
  yet: doing so would add `cie-ingest` (and, transitively, every language
  adapter crate) to a cdylib's dependency tree, which is a real build-time/
  binary-size cost nobody asked to pay yet. Until it's wired, a missing
  `drift` artifact read through the web path reports unavailable exactly as
  it did before this RFC — no regression, just not a new capability there
  yet.
- Only `drift` is wired to a persisted file today (`artifact_file_for`
  maps `ArtifactKind::Drift → Artifact::Drift`). `overlap`'s live query
  (§5's `Detail::CapabilityMatch`) is a different shape of on-demand work —
  a per-query result, not a once-per-revision singleton — and is not yet
  implemented; when it is, it can reuse this same registry (keyed by
  `(rev, "overlap")`) for the lattice's one-time build, while the per-query
  traversal itself stays a cheap synchronous read of the already-built
  lattice, never going through `BuildRegistry` at all.

## 10 · Implemented: drift (step 13) and publish (step 14) stop blocking on drift

`cie-cli/src/ingestion_job.rs`'s `export()` no longer runs the drift pipeline
at all. Step 13 now unconditionally stages `"drift"` as `"skipped": "Deferred:
computed on first request, not at ingest."` and writes a placeholder shaped
like the old timeout-degrade result, but with
`"incomplete_reason":"deferred_on_demand"`. Step 14 ("publish") writes the
bundle immediately after — it was never really coupled to step 13 except by
running after it in the same function, and now there's nothing to wait for.
The terminal sentinel write (the `out` file publish, which is what signals
"this revision is ready" to the web UI and to `cie_describe`) still cannot
be deferred — see §1's reasoning, unchanged — only the *drift field's*
content is now deferred, not the publish step itself.

`fn artifact()` treats the `deferred_on_demand` placeholder as "missing," not
as the answer (a drift value with `value["incomplete_reason"] ==
"deferred_on_demand"` falls through to the on-demand-build path in §9 rather
than being returned as-is) — otherwise the placeholder would permanently
shadow the real on-demand build. Any *other* shape for the artifact (a real
completed-or-failed drift report, written by the on-demand builder itself)
is returned as final.

The doc-discovery walk (bounded to `.md`/`.rst`/`.adoc`, ≤256KiB, depth ≤14,
first 20 by path) moved from a private function in `ingestion_job.rs` to
`cie_ingest::discover_docs`, so both the (now unused at ingest time, but
API-stable) CLI path and `DriftDeferredBuilder::build` share one
implementation instead of two copies. `export()`'s `docs_budget_ms`
parameter is kept (renamed `_docs_budget_ms`) for CLI/API stability but no
longer bounds anything — the on-demand build uses `BACKGROUND_BUILD_BUDGET`
instead, per §9.

Tests: `cie-storage`'s `deferred` module gained three tests (coalescing,
budget-exceeded-while-build-continues, failure-non-poisoning); existing
`cie-cli`/`cie-server` integration tests (`stream_job`, `mcp_storage`,
`task_071_doc_formats`, `task_073_claim_store`) and `cie-storage`'s
`artifact_contract` test all pass unchanged against the new code.
