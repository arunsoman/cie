# Changelog

## 1.1.0 — CIE-US partials close (2026-10-18)

All 45 `partial` user-story dispositions implemented and flipped to
`delivered`; the honest registry now reads **110 delivered / 0 partial /
37 planned** (the 37 planned keep their stated reasons). One cluster per
commit, each with deep checks (`cie-stories::checks::s11_closes`) and the
permanent honesty labels:

- **git-history archaeology** — `cie-ingest::history` (SystemGit seam:
  log/numstat/blame porcelain) + `churn` (exclusion ledger with re-ranking
  effects, rank stability, co-change coupling). Closes US-037/086/060.
- **runtime evidence feeds** — `cie-fault::trace` (python/rust/java/
  javascript trace parsers, fingerprinted exceptions inbox, LDFI suspects
  with frame evidence) + `next_steps` (ranked discriminating checks with
  concrete blockers). Closes US-032/039/064/069.
- **static concurrency** — `cie-analysis::concurrency` (lock sites,
  wait-for ordering risks with call paths, interleaving windows citing
  both statements, witness proposals that never claim certificates).
  Closes US-040/079/080.
- **composite analytics** — `cie-analysis::risk` (change-risk terrain with
  disclosed weights and neutral missing data), `cie-drift::causal`
  (chronology vs causal chain, alternatives visible). Closes US-046/071.
- **interface timelines** — `cie-incremental::timeline`. Closes US-089.
- **measurement protocol** — `cie-evidence::workload` (named workloads;
  measured-vs-potential by construction) + `readiness` (external evidence
  verdicts, readiness rollup with waivers-that-disclose, capacity sandbox
  refusing unsupported domains, calibration states). Closes US-074/085/
  100/102/119/120.
- **MCP server seam** — `cie-server::mcp` (JSON-RPC 2.0 over stdio,
  read-only declared-scope tools, budget-exhausted as explicit errors,
  honesty envelope). Closes US-134.
- **what-if simulation** — `cie-incremental::whatif` (remove/split dry
  runs; structural vs inferred consequences). Closes US-099.
- **query + capability layer** — `cie-llm::readings`, `cie-analysis::{
  query, endpoints, alternatives, impact_dims, steps}`. Closes US-009/
  010/024/034/052/059/109.
- **steering + collaboration** — `cie-incremental::steer`, `cie-claims::
  share`, `cie-drift::conflicts`, `cie-incremental::{attrib, diffgen}`.
  Closes US-066/091/107/113/114.
- **overlap + delivery oversight** — `cie-concepts::overlap`,
  `cie-evidence::scope`. Closes US-106/127/128.
- **host interaction layer** — `viz-adapter/src/interaction.ts` (LOD +
  settle, hover lens, gesture→proposal, measured layout quality, keyboard
  parity contract, announcements, never-colour-alone audit, theme tokens;
  15 new vitest cases, 274 total). Closes US-022/023/028/030/140/141/142/143.

Fidelity fixes surfaced by the batch: the miniature incremental world now
ingests repo content before dep recomputation (per its documented two-pass
contract) and the `pending_callees` orphan invariant key/caller semantics
were corrected.


## 1.0.0 — Full Task Matrix (2026-10-17)

All 84 instructor tasks implemented to their validation contracts; the 21 tasks
remaining after v0.3's first half (039/040/041/043, 052/053/054, 058, 067, 027, 071 and
the v1.0 row 024/047–051/057/059/060/062) landed one commit each. The completion
record lives in `docs/TRACEABILITY.md` §5; the completion record supersedes the §3
disposition rows.

### v0.3 closeout (behavior layer)
- **039/040** naming adapter (lexicon verb analysis, FR-10 immutable labels, FR-28
  LLM-off fallback) and Engler belief inference (inconsistent checks, cleanup
  pairing, co-occurrence) combined in a Kremenek factor graph — every output an
  `InferredCandidate` through the guarded registry (§8 rule 4).
- **041/043** protocol mining (k-bounded automata, Houdini refutation with an
  append-only content-addressed log) and bounded predicate checking (requires-DSL,
  path-condition conjunction, Unknown/TimedOut never clean).
- **052/053/054** FR-15 recovery modeler (RefundCalled ≠ Refunded), LDFI fault
  injection (minimal-faults-first, NoLineage → Unknown), Yuan et al. error-handling
  detectors (Safety/Liveness distinct).
- **058/067** deletion-correctness proptests (clean-recompute oracle after every
  step) and the §17.2 parallel folder walk (gitignore, shebang detection, rayon,
  per-file error isolation).

### v1.0 (hardening)
- **047–051** budgeted SMT, CEGAR, SLAM product checking, bounded symbolic
  execution with repro-test emission, and §14.9 counterexample packaging with a
  reachability-validated shrinker.
- **024/027** sled (default) + redb (feature) KV backends behind one `KvStore`
  trait, BE key codec, L0 caches, and the §15.4 latency-gated typed tables
  (50 ns / 30 ns / 5 µs, batched p99 timing).
- **057/059/060/062** differential relation maintenance, HNSW k-NN store,
  zero-copy rkyv+memmap2 snapshots, L3 columnar zstd archive with
  byte-equivalent restore.
- **071** PDF text-layer extraction and the honest OCR seam.

## 0.2.0 — Contract Fidelity (2026-10-17)

Every instructor task marked *partial* by the 84-task cross-check is implemented to its
validation contract. One commit per task; the disposition matrix lives in
`docs/TRACEABILITY.md`, the plan in `docs/V0_2_PLAN.md`.

### Core (TASK-002..005, 007..010)
- `hash()` / `BLAKE3_EMPTY` exports; lock-free `RevisionAllocator` with `meta.json`
  restart recovery (`Store::allocator`/`commit_rev`); injectable `ClockSource`.
- §14.1 identity newtypes `SymbolId/OperationId/ValueId/ResourceId/FileId` (serde-transparent)
  and the FxHashMap `Interner<T>` / `StrInterner` / `InternedStr`.
- §9 `AnalysisInput`/`AnalysisOutput` protocol, `CieError` taxonomy (+`Cycle`),
  `CI_TOLERANCE = 3.0`, `#[perf]` convention reference test.
- `LangId` closes over the ten §16 v1 languages; `LanguageTable::v1()`.
- Typed relation substrate: `RelKind` (10 kinds), `RelationGraph` (petgraph StableGraph,
  (from,to,kind) dedup), `CsrGraph` freeze with rkyv snapshots + 1M-edge perf gate,
  Ferrante–Ottenstein–Warren `Pdg` with bounded backward slicing.
- `TypedFacts` aggregate with deterministic idempotent `dedup()` and
  `check_integrity() → CieError::DanglingReference`.

### Storage & incremental (TASK-028, 030, 056, 073, 078)
- Tombstones (`file:{path}:tombstone`) with explicit snapshot purge (symbols, summaries,
  findings, orphaned call edges); re-add clears.
- Per-file summary reuse keyed `(blake3(file), blake3(dep set))` with persisted
  `summary_cache`; symbol-level dirty worklist invalidates exactly the transitive
  caller set (semantic local-fact hashes). Includes the FR-4 fix: adapter AST
  traversal no longer sorts by unstable `node.id()` (byte-stable evidence).
- Persisted `ClaimStore` (`claims.json`) with (doc, span, kind, modality) provenance
  and blake3 dedup keys.
- Concept persistence split into the six `concept_node:{id}:*` keys with
  reason-carrying children pairs and a `:rollup` aggregate view.

### Languages (TASK-011)
- Semantics tables as adapter data: `DispatchKind`, `NumericSemantics`
  (`OverflowPolicy`/`CoercionPolicy`/`Precision`), `FrameworkRule`; Python vouches
  Dynamic + ArbitraryPrecision. `LanguageRegistry::by_extension` + `new()`;
  FR-18 one-line extension guard pinned (acceptance #9).

### Drift & documents (TASK-071..077)
- `.rst`/`.adoc` line-based and `.html` tag-strip ingestion via `run_drift`.
- Drift re-run cache keyed (file-index code hash, docs hash) — pipeline re-run ≤ 5%
  of cold (acceptance #24, release-gated); acceptance #17 (≥90% non-Unknown) and
  #25 (≥60% LLM-off) pinned.
- Runnable acceptance harness: locked `ACCEPTANCE_ITEM_COUNT = 26` (24 green / 2
  deferred) with the authorized TASK-084 extension to 31 (29 green / 2 deferred).
- §15.4 perf regression suite for shipped subsystems (symbol lookup, CSR scan,
  interner lookup; release-gated × `CI_TOLERANCE`).

### Concepts & CLI (TASK-079..084)
- `ConceptDag::try_link_child` with ancestor-chain cycle rejection (`CieError::Cycle`).
- Four binding sources (`Explicit`/`InferredCandidate`/`Verified`/`LlmCandidate`);
  `LlmCandidate` inert by construction (`promote_to` refuses decisive targets).
- `cart` fixture (three-level hierarchy) + binding-source coverage tests.
- `cie concepts --max-depth N` (depth-limited view across md/json/dot/mermaid).
- Parent-edge-only invalidation with lazy `rollup_dirty` (dirty rollup == clean
  recompute, acceptance #31).

### Recovery (TASK-054 subset)
- `IgnoredErrorValue` findings (throwaway-assigned call results) and bare-except-
  without-re-raise recovery gaps (`sec/SEC-004`/`SEC-005`).

### Known deviations
- 6-crate workspace instead of the §10 30-crate scaffold (gate D-1 → v0.3), documented
  mapping in `docs/TRACEABILITY.md`.
- `SourceLocation` keeps rendered line/col + path (byte-span fields arrive with the
  §14.2 shapes in v0.3); `RevId` stays the §14.1 alias per TASK-002's explicit constraint.
- Perf gates bind on release builds only.

## 0.1.0 — Foundation (2026-10-16)

Initial release: 6-crate workspace, Python-deep + JS/TS/Go/Rust adapters, behavior/
conformance/security/testing analysis, FCA concepts, drift pipeline, offline-first LLM
layer, seven-command CLI, 30 tests green.
