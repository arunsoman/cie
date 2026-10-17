# Changelog

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
