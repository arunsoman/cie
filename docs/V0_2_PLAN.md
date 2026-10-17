# CIE v0.2 Release Plan — Contract Fidelity & Acceptance Completion

**Version:** 1.0 (2026-10-17)
**Baseline:** CIE v0.1.0 (6-crate workspace, 30 tests green, main @ tag `v0.1.0`)
**Input audit:** `CIE_LLM_Instructor_Tasks` v2.0 cross-check — 28 tasks fully delivered, 32 partial, 24 not implemented
**Exit bar:** 30 of 31 acceptance items green · instructor sketches compile as-written for in-scope tasks · CI green on GitHub Actions
**Estimated effort:** 9–11 agent sessions core scope, ~13 with all stretch items

---

## 1. Executive Summary

v0.1.0 delivered a working CIE engine — Python-deep plus four light language adapters, behavior/conformance/security/testing analysis, FCA concepts with rollup, a full document-drift pipeline, an offline-first LLM layer, and a seven-command CLI — as a deliberate 6-crate consolidation of the PRD's 30-crate layout. The cross-check against the 84-task instructor list confirms the engine's behavior is sound where it was built, but also that v0.1.0 diverges from the instructor methodology in two systematic ways. First, the binding validation sketches would not compile against the delivered APIs: there is no `RevisionAllocator`, no `hash()`/`BLAKE3_EMPTY`, no `CieError`, no `CI_TOLERANCE`, identity types are aliases rather than newtypes, and the relation-graph substrate (TASK-007/008) does not exist. Second, nine of the 31 PRD acceptance items are not pinned by tests, even where the underlying functionality already works (LLM items 12–14, drift re-run item 24, LLM-off claims item 25, the FR-18 extension guard item 9).

v0.2 therefore has one theme — **make the delivered engine contract-faithful** — organized into seven work packages. WP-1 aligns the core primitives so the Phase-0/1 sketches compile and pass as-written; WP-2 and WP-3 close the cheapest structural gaps (tombstones, summary reuse, the typed relation graph with its CSR freeze); WP-4 completes the acceptance suite from 22/31 to 30/31 green; WP-5 and WP-6 polish concepts, CLI, and claim storage. Heavy phases that require new substrates before they can pay off — SMT/CEGAR (Phase 6), HNSW/zero-copy/perf-hardening (Phase 9), and five additional language adapters — remain explicitly out of scope and are scheduled on the v0.3/v1.0 roadmap in Section 8. The plan keeps the instructor list's own rules intact: sketches are contracts, metadata is binding, and no existing test may be weakened to make a milestone.

Two decision gates need a call before WP-1 starts. **D-1** asks whether to scaffold the 30-crate workspace now; the recommendation is to defer to v0.3, where the five new adapter crates make the §10 layout pay for itself. **D-2** asks whether to do the mechanical newtype refactor of `SymbolId` and friends; the recommendation is yes — the codebase is ~7k lines, the refactor is bounded, and it unlocks TASK-003 and TASK-006 alignment without a dual-model debt.

## 2. Baseline and Scope

### 2.1 v0.1.0 audit snapshot

The table below is the per-phase verdict from the audit of 2026-10-17. "Full" means the task's deliverable function exists and is demonstrated by a green test or documented design; "partial" means the function exists in adapted form with named gaps; "not" means no implementation.

| Phase | Tasks | Full | Partial | Not | Dominant gaps |
|---|---|---|---|---|---|
| 0 — Workspace & shared core | 4 | 0 | 4 | 0 | 6-crate vs 30-crate layout; no `RevisionAllocator`, `CieError`, `CI_TOLERANCE` |
| 1 — Semantic model & relations | 6 | 1 | 2 | 3 | No petgraph `RelationGraph`, no CSR freeze, no FOW PDG |
| 2 — Language adapters | 12 | 2 | 5 | 5 | Java/C/C++/C#/PHP absent; JS/TS/Go/Rust light-only; no semantics tables |
| 3 — Evidence, storage, revisioning | 6 | 3 | 2 | 1 | JSON store not sled/redb; no latency-gated tables; no explicit tombstones |
| 4 — Engine, summaries, IFDS | 7 | 2 | 3 | 2 | No path conditions; no IFDS tabulation; no checkpoints |
| 5 — Behavior, contracts, FCA | 11 | 6 | 4 | 1 | No Houdini-style protocol mining (TASK-041) |
| 6 — SMT, CEGAR, symbolic | 5 | 0 | 0 | 5 | Entire phase out of scope at kickoff |
| 7 — Recovery & fault injection | 3 | 0 | 1 | 2 | Only exception facts; no obligations or LDFI |
| 8 — Incremental maintenance | 4 | 1 | 2 | 1 | No differential dataflow; file-granularity invalidation only |
| 9 — Vector store & perf | 4 | 0 | 0 | 4 | No HNSW, zero-copy, perf suite, or L3 archive |
| 10 — Native LLM layer | 4 | 4 | 0 | 0 | Implemented; acceptance items 12–14 untested |
| 11 — Folder assimilation & CLI | 4 | 3 | 1 | 0 | No rayon-parallel walk |
| 12 — Documents & claims | 3 | 1 | 2 | 0 | Markdown/text only; no persisted ClaimStore |
| 13 — Drift engine | 3 | 3 | 0 | 0 | Complete (map → verify → report → reconcile → delta) |
| 14 — Acceptance harness | 1 | 0 | 1 | 0 | 22 of 31 items green; no harness constant |
| 15 — Concept DAG (addendum) | 7 | 2 | 5 | 0 | No cycle rejection; storage keys merged; no parent-edge invalidation |
| **Total** | **84** | **28** | **32** | **24** | |

### 2.2 Goals

1. **Sketch fidelity for Phases 0–1.** Every instructor sketch for TASK-002, 003, 004, 005, 007, 008 and 010 compiles and passes as-written, up to a single documented import-path mapping (`cie_core::facts` instead of `cie_semantic`, etc.).
2. **Acceptance completion.** Bring the §13 suite from 22/31 to **30/31 green**, with only item 16 (zero-copy snapshot) explicitly deferred to v1.0, and register the locked `ACCEPTANCE_ITEM_COUNT = 26` harness constant plus the authorized TASK-084 extension to 31.
3. **Cheapest structural gaps.** Land tombstones (TASK-028), summary reuse with targeted invalidation (TASK-030), and the typed relation graph with CSR freeze (TASK-007/008) — the three missing pieces that unblock later phases without new substrates.
4. **Release hygiene.** Get CI actually running on GitHub Actions, add the traceability document, and ship the release as tag + zip.

### 2.3 Non-goals (deferred with rationale)

The following stay out of v0.2 not because they are unwanted, but because each needs a substrate that v0.2 does not build, or pays off only at a scale v0.1 does not have. SMT/CEGAR and symbolic execution (TASK-047–051) need the PDG and path conditions first. HNSW (TASK-059), zero-copy snapshots (TASK-060), and the L3 archive (TASK-062) are scale optimizations — the JSON store is not yet the bottleneck. The five missing adapters (TASK-015–018, 021) are each a crate of semantics work best done together in v0.3's multi-language push. Differential dataflow (TASK-057) and deep IFDS (TASK-034/035) presume the graph substrate of WP-3 has landed and stabilized. Deepening JS/TS/Go/Rust (TASK-013/014/019/020) is real work but is v0.3 by the same logic as the new adapters. Sled/redb backend swap (TASK-024) waits until the storage layer's shape is stable under tombstones.

## 3. Work Packages WP-0 through WP-3

Sizing key: **S** ≤ 0.5 agent session, **M** 0.5–1 session, **L** 1–2 sessions. A "session" is one focused agent run that implements a subtask and leaves its validation tests green, per the instructor methodology.

### WP-0 — Release Hygiene (S, ~0.5 session)

Everything here is unglamorous and blocking: CI cannot run until the workflow file lands, and the traceability document is what makes the v0.2 claim auditable.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 0.1 | Commit `.github/workflows/ci.yml`; requires PAT with `workflow` scope (user action; rotate the exposed token first) | — | `.github/` | S |
| 0.2 | Add `docs/V0_2_PLAN.md` + `docs/TRACEABILITY.md` (audit matrix + v0.2 dispositions); bump workspace version to 0.2.0 | — | `docs/`, `Cargo.toml` | S |
| 0.3 | Release mechanics: tag `v0.2.0`, refresh `cie-v0.2.0.zip`, update README status table | — | root | S |

**Validation:** GitHub Actions run green on the CI workflow's own commit; `cargo test --workspace` green at version 0.2.0.

### WP-1 — Core Contract Alignment (M–L, ~2–2.5 sessions)

The largest package. Its purpose is narrow and non-negotiable: after WP-1, the Phase-0/1 instructor sketches compile against the real codebase and pass with their assertion bodies intact. This is what converts the instructor list from an aspiration into a per-commit checklist.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 1.1 | Export `hash()` (alias of `content_hash`) and `BLAKE3_EMPTY` from `cie-core` | TASK-002 | `types.rs` | S |
| 1.2 | `RevisionAllocator`: `AtomicU64`, `starting_at(n)`/`next()`, strictly monotonic; 8-thread × 100k monotonic test; restart recovery wired to `meta.json` `last_rev` | TASK-002 | `types.rs`, `storage` | M |
| 1.3 | Injectable clock behind `now_ns()` (`ClockSource` trait, system default, test override) | TASK-002 | `types.rs` | S |
| 1.4 | Identity newtypes `SymbolId(u32)`, `OperationId`, `ValueId`, `ResourceId`, `FileId` — `Copy + Hash + Eq + Ord`; `Interner<T>` on `FxHashMap`, `StrInterner`, `InternedStr` | TASK-003 | `types.rs`, `interner.rs`, workspace-wide | M–L |
| 1.5 | §9 protocol: `AnalysisInput`/`AnalysisOutput` fields + `RevisionId` alias; `CieError { Parse, UnsupportedConstruct, Timeout, BudgetExceeded, Storage, DanglingReference }` with `is_timeout()`; `AnalysisOutput::empty_with_evidence()`; `pub const CI_TOLERANCE: f64 = 3.0` documented; `tests/perf_convention.rs` reference `#[ignore]` perf test | TASK-004 | `core` (new `protocol.rs`) | M |
| 1.6 | `LanguageTable::v1()` enumerating exactly the ten §16 languages plus `UNKNOWN` (ids only — no adapters implied) | TASK-005 | `types.rs` | S |
| 1.7 | `TypedFacts` aggregate + deterministic `dedup()` (stable by `(loc, id)`) + `check_integrity() → CieError::DanglingReference` | TASK-010 | `facts.rs` | M |

**Validation contract:** new tests `tests/task_002_time_hash.rs`, `task_003_identity.rs`, `task_004_protocol.rs`, `task_005_languages.rs`, `task_010_typed_facts.rs` replicate the instructor sketch bodies with one import-mapping header (`use cie_core::…`). All 30 existing tests stay green. The 1M-key interner test runs in debug mode per the sketch.

**Decision gate D-2 (at WP-1 start):** the newtype refactor (1.4) touches every crate mechanically. If churn exceeds one session, fall back to keeping aliases, shipping only `Interner`/`StrInterner`, and documenting the deviation per sketch — the sketches for TASK-003 use tuple-constructor syntax, so the fallback also means marking TASK-003 as "adapted" in TRACEABILITY.md.

### WP-2 — Storage & Incremental Hardening (M, ~1–1.5 sessions)

Two behaviors are currently proven only incidentally: deletion correctness (acc7 passes because a re-ingest without the file leaves no stale findings) and summary freshness. WP-2 makes both explicit and cheap.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 2.1 | Tombstones: `file:{path}:tombstone` on deletion; snapshot builder purges symbols/summaries/findings of tombstoned files and drops orphaned call edges; extends acc7 with a direct stale-symbol assertion | TASK-028 | `storage/lib.rs`, `incremental.rs` | M |
| 2.2 | Summary reuse: per-file cache keyed by `(blake3(file), blake3(dep set))`; unchanged file ⇒ reuse stored `FunctionSummary`; changed file ⇒ recompute plus transitive reverse-deps; assert reuse counters in test | TASK-030 | `analysis/behavior.rs`, `cli/engine.rs` | M |
| 2.3 | *(stretch)* Symbol-level dirty worklist: invalidate exactly the transitive reverse-dependency set of changed symbols instead of whole files | TASK-056 | `incremental.rs` | M–L |

**Validation:** new `tests/task_028_tombstones.rs` (delete file → next snapshot has zero facts referencing its symbols); summary-reuse test asserts recompute count == changed-file count on a partial edit; acc7 and acc10 remain green.

### WP-3 — Graph Substrate (M, ~1–1.5 sessions)

The typed relation graph is the missing shared currency that Phase 6 (SMT), Phase 9 (perf), and IFDS all consume. v0.2 builds it and freezes it; it does not yet build the PDG on top unless the stretch lands early.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 3.1 | `RelKind` — exactly ten variants (`Calls, Returns, ControlFlow, DataFlow, Reads, Writes, Throws, Catches, Owns, DependsOn`); `RelationGraph = StableGraph<SymbolId, RelKind, Directed, u32>`; duplicate `(from,to,kind)` collapses, distinct kinds coexist; `CfgEdge`/`DfgEdge` carriers; builder from `LangFacts` (calls→Calls, assignments→DataFlow, ifs/loops→ControlFlow, exceptions→Throws/Catches) | TASK-007 | new `core/relations.rs` (dep: `petgraph`) | M |
| 3.2 | `CsrGraph { offsets, edges }` + `build_from` + `neighbors()` + rkyv-serializable snapshot; `#[ignore]` perf test: scan ≤ 20 ns/edge × `CI_TOLERANCE` on a 1M-edge fixture | TASK-008 | `core/relations.rs` (dep: `rkyv`) | M |
| 3.3 | *(stretch)* FOW PDG: post-dominator tree from `ControlFlow`, data deps from `DataFlow`; `Pdg::build(&RelationGraph)` (pure, provenance-stamped); `backward_slice` + `backward_slice_bounded` | TASK-009 | `core/relations.rs` | L |

**Validation:** `tests/task_007_relations.rs` and `task_008_csr.rs` follow the sketches (StableGraph survivor-stability, dedup, offsets invariant for empty/1-node/1M-edge graphs, perf gate). If 3.3 lands, `task_009_pdg.rs` reproduces the FOW fixture from the sketch.

## 4. Work Packages WP-4 through WP-6

### WP-4 — Acceptance Completion (M, ~2 sessions)

This package converts "implemented but untested" into "pinned green" and adds the two structural tests the instructor list treats as binding: the FR-18 extension guard and the locked harness constant. After WP-4, the acceptance suite is a single runnable registry rather than a scatter of test files.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 4.1 | Adapter semantics tables as data: `DispatchKind { Static, Dynamic, Hybrid }`, `NumericSemantics { overflow, coercion, precision }`, `&'static [FrameworkRule]` on the trait; Python declares `Dynamic`/`ArbitraryPrecision`. Then the FR-18 guard test: a `TestLangAdapter` defined inside the test file registers with **one line** and resolves via `by_extension("tl")` | TASK-011, acc #9 | `langs/lib.rs`, `langs/python.rs` | M |
| 4.2 | LLM acceptance trio: #12 offline labeling via `OfflineProvider` produces `Label { source: Llm, model_id, prompt_hash }` with no network; #13 cache-hit path measured ≤ 100 ns × `CI_TOLERANCE` on `cache_key(model, prompt)`; #14 budget exceeded → downgrade-and-continue (run completes, rule-based fallback labels, no hard failure) | TASK-063/065/066, acc #12–14 | `llm/` | S |
| 4.3 | Acceptance #24: drift re-run ≤ 5% of first run wall time (second pass over unchanged inputs); if measurement fails the bar, add a claim-mapping cache keyed by `(code hash, doc hash)` and re-measure | TASK-076+069, acc #24 | `analysis/drift.rs`, `cli/engine.rs` | S/M |
| 4.4 | Acceptance #25: LLM fully off → ≥ 60% of explicit claims still extracted from the fixture corpus (rule-based `extract_claims` coverage ratio asserted) | TASK-072, acc #25 | `analysis/drift.rs` | S |
| 4.5 | Acceptance #17: ≥ 90% of explicit claims resolve non-`Unknown` on the fixture corpus (tune `classify_claim`/`verify_claims` thresholds if needed) | TASK-072/073/075, acc #17 | `analysis/drift.rs` | S/M |
| 4.6 | Perf regression suite (TASK-061): encode the §15.4 subset that applies to shipped subsystems — symbol lookup ≤ 50 ns, CSR scan ≤ 20 ns (from 3.2), LLM cache hit ≤ 100 ns — as `#[ignore]`-gated tests comparing against `target × CI_TOLERANCE` | TASK-061, acc #15 | `core/tests/` | M |
| 4.7 | Acceptance harness (TASK-077): `tests/acceptance_harness.rs` registering items 1–26 with `const ACCEPTANCE_ITEM_COUNT: usize = 26`; items 15/16 registered with explicit `deferred` status; per-item entry points dispatch to the existing acc tests | TASK-077, acc #15–16 | `cli/tests/` | M |

**Validation:** harness test asserts the constant, enumerates 26 items, and reports a status table; after WP-4 the suite prints **24 green / 2 deferred** pre-WP-3-dependency and **25 green / 1 deferred** once 3.2 feeds item 15's CSR target. Items 27–31 remain green from v0.1 (WP-5 adds their meta-test).

### WP-5 — Concepts & CLI Polish (S–M, ~1 session)

Small, well-bounded items that each flip one instructor task from partial to full without new architecture. The concept DAG's data model is already right; what is missing is guard rails and storage-shape fidelity.

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 5.1 | Cycle rejection in `ConceptDag::link_child`: ancestor-chain check returns `Err(CieError::DanglingReference)`-style cycle error; test links A→B then B→A and asserts rejection with `EdgeReason` preserved on legal extra edges | TASK-080, acc #29 | `core/concept.rs` | S |
| 5.2 | Split concept persistence into the six `concept_node:*` keys of TASK-078 (`:node`, `:behaviors`, `:rules`, `:parent`, `:children`, `:rollup`) behind the existing JSON store, with roundtrip test | TASK-078, acc #27 | `storage/lib.rs` | S |
| 5.3 | `cart` fixture (three-level concept hierarchy with mixed sources) + binding-source coverage test: all four sources attach, `LlmCandidate` stays inert (never becomes `Explicit`/`Verified`) | TASK-079, acc #28 | `tests/fixtures/`, `analysis/hierarchy.rs` | S/M |
| 5.4 | `cie concepts --max-depth N` flag (md/json/dot formats and `--with-behaviors/--with-rules` already shipped in v0.1) | TASK-083 | `cli/main.rs` | S |
| 5.5 | *(stretch)* Parent-edge-only invalidation: on re-ingest, invalidate a concept node and propagate only along primary-parent edges; deep children re-rollup lazily | TASK-082, acc #31 | `core/concept.rs`, `cli/engine.rs` | M |
| 5.6 | Harness extension meta-test (TASK-084): asserts count raised 26 → 31 exactly once and items 27–31 all resolve | TASK-084 | `cli/tests/` | S |

### WP-6 — Documents & Claims Gaps (S, ~0.5–1 session)

| # | Subtask | Ref | Files | Size |
|---|---|---|---|---|
| 6.1 | Cheap ingestion formats: `.rst` and `.adoc` line-based extractors, `.html` tag-stripped text (no PDF/OCR — those stay v1.0) | TASK-071 | `cli/engine.rs` | S |
| 6.2 | Persisted `ClaimStore`: claims with `(doc, span, kind, modality)` provenance, dedup key, load/save roundtrip | TASK-073 | `core/drift.rs`, `storage/lib.rs` | S/M |
| 6.3 | *(stretch)* Recovery-lite findings: flag `except: pass` swallow and ignored-error-value patterns from existing exception facts (no obligation solver — that stays v0.3/v1.0) | TASK-054 subset | `analysis/security.rs` | M |

## 5. Dependency DAG and Execution Order

WP-0 first (it is unblocking and tiny). WP-1 next, because the newtypes and the §9 protocol are the vocabulary everything else compiles against. Then three groups run **in parallel** — WP-2 (storage), WP-3 (graph), and the WP-4.1–4.5 test-completion batch plus WP-5.1–5.4 and WP-6 — converging on the harness freeze (WP-4.6/4.7, WP-5.6), which must be last because it registers everyone's results. The DAG below is the execution view; Appendix A carries the per-task traceability.

```
WP-0 (hygiene)
   └─> WP-1 (core contract alignment; gates D-2 inside)
          ├─> WP-2 storage: 2.1 -> 2.2 -> (2.3 stretch)
          ├─> WP-3 graph:   3.1 -> 3.2 -> (3.3 stretch)
          ├─> WP-4 tests:   4.1..4.5 (parallel batch)
          ├─> WP-5 polish:  5.1..5.4 -> (5.5 stretch)
          └─> WP-6 docs:    6.1 -> 6.2 -> (6.3 stretch)
                  all converge
                     └─> FREEZE: 4.6 perf suite -> 4.7 harness (26) -> 5.6 meta-test (31)
                              └─> WP-0.3 release (tag v0.2.0 + zip)
```

**Decision gates.**

- **D-1 — 30-crate scaffold now vs v0.3.** Scaffolding 30 re-export stub crates would satisfy TASK-001's sketch literally, but adds workspace noise while every crate except six is empty. Recommendation: **defer to v0.3**, where the five new adapter crates make the §10 layout earn its keep; record the official deviation + 6↔30 mapping in `docs/TRACEABILITY.md` now.
- **D-2 — newtype refactor vs aliases.** Recommendation: **do the refactor** (1.4) — bounded mechanical work that unlocks TASK-003 verbatim and pre-clears TASK-006's §14.2 shapes for v0.3. Fallback documented under WP-1.

**Session estimates.**

| Package | Core | With stretch |
|---|---|---|
| WP-0 | 0.5 | 0.5 |
| WP-1 | 2.0–2.5 | 2.0–2.5 |
| WP-2 | 1.0 | 1.5–2.0 |
| WP-3 | 1.0–1.5 | 2.0–2.5 |
| WP-4 | 1.5–2.0 | 1.5–2.0 |
| WP-5 | 0.75–1.0 | 1.5 |
| WP-6 | 0.5–0.75 | 1.0–1.25 |
| **Total** | **~7.5–10** | **~10.5–13** |

## 6. Release Definition of Done

v0.2.0 ships only when every line below is checkably true. The exit bar is deliberately stricter than v0.1's: the release's whole point is that the instructor list can be used as a per-commit audit instrument against the repo.

| # | Criterion | Evidence |
|---|---|---|
| 1 | `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check` all green | CI run on release commit |
| 2 | Phase-0/1 sketches (TASK-002/003/004/005/007/008/010/011) compile and pass as-written, modulo the one documented import mapping | `tests/task_00X_*.rs` |
| 3 | Acceptance suite: **30/31 items green**; only #16 (zero-copy snapshot ≤ 10 µs @ 1M syms) deferred to v1.0 | `acceptance_harness.rs` status table |
| 4 | `ACCEPTANCE_ITEM_COUNT = 26` locked, TASK-084 extension to 31 authorized and meta-tested | harness + meta-test |
| 5 | `#[perf]` convention live: `CI_TOLERANCE = 3.0`, reference + §15.4-subset tests pass under `cargo test -- --ignored` | perf suite |
| 6 | Tombstones, summary reuse, relation graph, CSR freeze: each with its own contract test beyond the acceptance items | WP-2/3 tests |
| 7 | GitHub Actions green including clippy gate; `ci.yml` committed (PAT scope prerequisite) | Actions run |
| 8 | `docs/TRACEABILITY.md` maps all 84 tasks → verdict → v0.2 WP or deferred release | doc in repo |
| 9 | Tag `v0.2.0` + refreshed zip; README updated | release artifacts |

**Acceptance exit matrix (summary):** items 1–8, 10, 11, 18–23, 26 already green in v0.1 (17 items); items 9, 12, 13, 14, 17, 24, 25 newly pinned by WP-4 (7 items); item 15 goes green via the perf suite's shipped-subsystem subset (1 item); items 27–31 stay green with the new meta-test (5 items); item 16 alone defers. Appendix B carries the full 31-row matrix.

## 7. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Newtype refactor (1.4) churns beyond estimate | Medium | Schedule slip | Timebox to one session; D-2 fallback keeps aliases and marks TASK-003 "adapted"; refactor is mechanical (compiler-driven) with rustfmt/clippy as guard rails |
| Perf gates flaky on shared CI runners | Medium | False red builds | `CI_TOLERANCE = 3.0` multiplier per the instructor convention; `#[ignore]`-gated so per-commit runs stay fast; nightly job for perf, blocking CI for correctness only |
| Acceptance #24 (drift re-run ≤ 5%) misses the bar on first measurement | Medium | Rework in WP-4 | Pre-planned claim/code-mapping cache keyed by `(code hash, doc hash)`; v0.1 already showed unchanged re-ingest at ~3%, so the drift path is likely within budget |
| PAT lacks `workflow` scope → CI file still uncommitted | High (known) | CI not running | WP-0.1 is the first action; user rotates the exposed token and grants `workflow` scope; until then CI runs are local and the release ships without Actions green (documented exception) |
| PDG stretch (3.3) overruns | Medium | Minor | Pre-declared slip path to v0.3 with IFDS; 3.3 is not on the DoD critical path |
| Sketch divergence discovered mid-WP (sketch contradicts delivered design) | Low | Design rework | Instructor rule applies: fix the design, not the test; where genuinely impossible, the deviation is recorded in TRACEABILITY.md with rationale rather than silently weakened |
| Dual fact-model debt if §14.2 shapes are half-adopted | Low | v0.3 friction | Explicit non-goal: v0.2 adds no duplicate fact structs; TASK-006 alignment deferred wholesale to v0.3 |

## 8. Roadmap: v0.3 and v1.0

**v0.3 — Deep Semantics and Multi-Language.** The organizing move is executing decision gate D-1: scaffold the §10 30-crate workspace so each new adapter gets its own crate per FR-18. On top of that land the five missing adapters (TASK-015 Java, 016 C, 017 C++, 018 C#, 021 PHP), the deepening of JS/TS/Go/Rust (013/014/019/020), and the cross-language semantics guard (TASK-022) that proves FR-3 across all ten. WP-3's relation graph becomes load-bearing here: §14.2 fact shapes (TASK-006), path conditions (TASK-032), the FOW PDG (TASK-009, if the stretch slipped), and the IFDS framework with its two instances (TASK-034/035) all consume it. Naming-adapter rule inference (TASK-039–041), the full recovery engine (TASK-052–054), rayon-parallel ingestion (TASK-067), and the deletion-equivalence property harness (TASK-058) round out the release. Optional if storage friction appears: the sled/redb backend behind TASK-024's unchanged API.

**v1.0 — Full PRD Parity.** The remaining block is exactly the audited "not implemented" set that needs heavy substrates: budgeted Z3 encoding, CEGAR refinement, SLAM-style protocol checking, symbolic path exploration, and counterexample packaging (TASK-047–051); the HNSW embedding store (TASK-059); rkyv+memmap zero-copy snapshots (TASK-060); the L3 zstd+parquet cold archive (TASK-062); and differential-dataflow maintenance (TASK-057). The exit criterion is the instructor list's own: 84/84 tasks full, 31/31 acceptance items green, and the harness constant guarded by TASK-084's meta-test.

---

## Appendix A — 84-Task Disposition

Verdicts: **done** = delivered in v0.1.0 (adapted where noted) · **v0.2** = targeted by a WP above (ref in parentheses) · **v0.3** / **v1.0** = scheduled on the roadmap · **stretch** = v0.2 if capacity allows.

| Task | Title (short) | Disposition |
|---|---|---|
| 001 | 30-crate workspace scaffold | v0.3 (D-1; deviation documented in v0.2) |
| 002 | Ns/RevId/Hash + RevisionAllocator | v0.2 (1.1–1.3) |
| 003 | Identity newtypes + Interner | v0.2 (1.4) |
| 004 | §9 protocol + CI_TOLERANCE | v0.2 (1.5) |
| 005 | SymbolFact + 10-lang table | v0.2 (1.6; fact shape done v0.1) |
| 006 | Four fact families §14.2 shapes | v0.3 (facts exist adapted; shapes w/ IFDS) |
| 007 | RelationGraph + RelKind | v0.2 (3.1) |
| 008 | CsrGraph freeze + perf | v0.2 (3.2) |
| 009 | FOW PDG + backward slice | stretch (3.3) → else v0.3 |
| 010 | TypedFacts dedup + integrity | v0.2 (1.7) |
| 011 | Adapter trait + semantics tables | v0.2 (4.1; trait/registry done v0.1) |
| 012 | Python deep adapter | done v0.1 (adapted) |
| 013 | JavaScript deep adapter | v0.3 (light shipped) |
| 014 | TypeScript deep adapter | v0.3 (light shipped) |
| 015 | Java adapter | v0.3 |
| 016 | C adapter | v0.3 |
| 017 | C++ adapter | v0.3 |
| 018 | C# adapter | v0.3 |
| 019 | Go adapter | v0.3 (light shipped) |
| 020 | Rust adapter | v0.3 (light shipped) |
| 021 | PHP adapter | v0.3 |
| 022 | Cross-language FR-3 guard | v0.3 (shadowing gate shipped v0.1) |
| 023 | Evidence/coverage substrate | done v0.1 (adapted) |
| 024 | sled KV + redb flag | v1.0 (JSON store documented as drop-in) |
| 025 | Revision allocator + meta | done v0.1 (+1.2 restart link) |
| 026 | Time-travel queries | done v0.1 (acc #11) |
| 027 | Latency-gated tables | v0.2 partial (4.6 gates) / v1.0 full |
| 028 | Tombstones + orphan purge | v0.2 (2.1) |
| 029 | Engine budgets/cancellation | v0.2 partial (1.5) / checkpoints v1.0 |
| 030 | Summary store + invalidation | v0.2 (2.2) |
| 031 | Intra-procedural effects | done v0.1 |
| 032 | Path conditions | v0.3 |
| 033 | Interprocedural fixpoint | done v0.1 |
| 034 | IFDS framework | v0.3 |
| 035 | IFDS instances | v0.3 (unresolved/taint marks shipped v0.1) |
| 036 | Stage-1 behavior detectors | done v0.1 (acc #1–3) |
| 037 | Result-protocol record store | v0.2 partial (1.5) |
| 038 | Contract registry + authority | done v0.1 (adapted; +1.5 alignment) |
| 039 | Naming adapter (full) | v0.3 (name-based subset shipped) |
| 040 | Candidate rule inference (full) | v0.3 (InferredCandidate shipped) |
| 041 | Houdini-style protocol mining | v0.3 |
| 042 | Conformance check | done v0.1 (acc #4–5) |
| 043 | Predicates + bounded checks | v0.3 (budget/deadline shipped) |
| 044 | FCA lattice core | done v0.1 |
| 045 | Godin incremental FCA | done v0.1 |
| 046 | Clustering + hierarchy | done v0.1 |
| 047 | Z3 budgeted encoding | v1.0 |
| 048 | CEGAR refinement | v1.0 |
| 049 | SLAM-style protocol checks | v1.0 |
| 050 | Symbolic execution + repro tests | v1.0 |
| 051 | Counterexample packaging | v1.0 |
| 052 | Recovery obligations | v0.3 |
| 053 | Fault injection planning | v0.3 |
| 054 | Error-handling failures | stretch (6.3) → v0.3 |
| 055 | blake3 revision diff | done v0.1 |
| 056 | Reverse-dep dirty worklist | stretch (2.3) |
| 057 | Differential dataflow | v1.0 |
| 058 | Deletion-correctness proof | v0.3 (behavioral acc #7 shipped) |
| 059 | HNSW k-NN store | v1.0 |
| 060 | Zero-copy rkyv+memmap | v1.0 (acceptance #16 defers with it) |
| 061 | §15.4 perf regression suite | v0.2 (4.6) |
| 062 | L3 zstd+parquet archive | v1.0 |
| 063 | LlmProvider + offline default | done v0.1 (tests in 4.2) |
| 064 | Cloud providers opt-in | done v0.1 (tests in 4.2) |
| 065 | Content-addressed LLM cache | done v0.1 (gate test in 4.2) |
| 066 | Budget downgrade-continue | done v0.1 (gate test in 4.2) |
| 067 | Parallel folder walk (rayon) | v0.3 |
| 068 | End-to-end assimilation | done v0.1 (acc #8) |
| 069 | Incremental re-ingestion | done v0.1 (acc #10, ~3%) |
| 070 | `cie ingest` CLI | done v0.1 |
| 071 | Document ingestion formats | v0.2 partial (6.1; PDF/OCR v1.0) |
| 072 | Typed claim extraction | done v0.1 (ratio tests in 4.4/4.5) |
| 073 | Persisted ClaimStore | v0.2 (6.2) |
| 074 | Claim–code mapping | done v0.1 |
| 075 | Per-claim verification | done v0.1 (acc #18–20, 26) |
| 076 | Drift reports/reverse/reconcile/delta | done v0.1 (acc #21–24 in 4.3) |
| 077 | Acceptance harness (26 items) | v0.2 (4.7) |
| 078 | ConceptNode + six storage keys | v0.2 (5.2; model done v0.1) |
| 079 | Four binding sources + cart | v0.2 (5.3) |
| 080 | DAG merge + cycle rejection | v0.2 (5.1; DAG done v0.1) |
| 081 | Rollup semantics | done v0.1 (acc #30) |
| 082 | Parent-edge invalidation | stretch (5.5) |
| 083 | `cie concepts` CLI flags | v0.2 (5.4; md/json/dot done v0.1) |
| 084 | Harness 26 → 31 meta-test | v0.2 (5.6) |

## Appendix B — Acceptance Exit Matrix

| # | Expected outcome (short) | v0.1 status | v0.2 target |
|---|---|---|---|
| 1 | Print-only authorization detected | green | green |
| 2 | Unconditional success detected | green | green |
| 3 | Legitimate delegation recognized | green | green |
| 4 | Rejected authorization ignored | green | green |
| 5 | Exceptions converted to success | green | green |
| 6 | Unresolved external flagged | green | green |
| 7 | Deletion invalidates finding | green | green (+2.1 tombstone test) |
| 8 | Polyglot repo end-to-end | green | green |
| 9 | New adapter: 1 crate + 1 line | by design | **pinned (4.1 guard test)** |
| 10 | Re-ingest unchanged ≤ 5% | green (~3%) | green |
| 11 | Time-travel rev N and N−1 | green | green |
| 12 | LLM offline via Ollama | implemented | **pinned (4.2)** |
| 13 | LLM cache hit ≤ 100 ns | implemented | **pinned (4.2)** |
| 14 | Budget exceeded → downgrade | implemented | **pinned (4.2)** |
| 15 | §15.4 latency targets | n/a | **green, shipped subset (4.6)** |
| 16 | Zero-copy 1M syms ≤ 10 µs | n/a | deferred → v1.0 (TASK-060) |
| 17 | ≥ 90% claims non-Unknown | implemented | **pinned (4.5)** |
| 18 | Double-charge → Contradicting | green | green |
| 19 | No-match → Missing | green | green |
| 20 | p99 claim → Unverifiable | green | green |
| 21 | Undocumented code in reverse drift | green | green |
| 22 | Conflicting docs cite both | green | green |
| 23 | --from-rev/--to-rev delta only | green | green |
| 24 | Drift re-run ≤ 5% | implemented | **pinned (4.3)** |
| 25 | LLM off → ≥ 60% claims | implemented | **pinned (4.4)** |
| 26 | Timeout → Unknown, never clean | green | green |
| 27 | Concept nodes persisted + evidence | green | green (+5.2 key split) |
| 28 | Four sources; LlmCandidate inert | green | green (+5.3 cart fixture) |
| 29 | Merge → DAG; cycles rejected | green (no cycle test) | green (+5.1 cycle test) |
| 30 | Rollup == clean recompute | green | green |
| 31 | Invalidation along parent edges | partial | **pinned (5.5 stretch)** |

