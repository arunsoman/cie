# CIE Traceability — Instructor Task List ↔ Delivered Code

**Baseline:** CIE v0.1.0 → v0.2.0 (6-crate workspace)
**Input:** `CIE_LLM_Instructor_Tasks` v2.0 — 84 tasks / 16 phases / 31 acceptance items
**Cross-check verdict:** 28 fully delivered, 32 partial, 24 not implemented (out-of-scope substrates).
v0.2 exists to close every **partial** gap; the 24 not-implemented tasks stay scheduled on the
v0.3/v1.0 roadmap (see `docs/V0_2_PLAN.md` §8) because each requires a substrate v0.2 does not build.

## 1. Workspace-shape mapping (TASK-001 deviation)

v0.1.0 consolidated the PRD §10 30-crate layout into 6 crates. The mapping is fixed and stable:

| Delivered crate | Consolidates PRD §10 crates |
|---|---|
| `cie-core` | `cie-time`, `cie-identity`, `cie-semantic` (model), `cie-relations` (from v0.2: `relations.rs`), `cie-concepts`, `cie-drift`, `cie-protocol` |
| `cie-storage` | `cie-store` (KV namespaces, JSON file backend) |
| `cie-langs` | `cie-adapters` + `cie-adapter-python` + JS/TS/Go/Rust adapters |
| `cie-analysis` | `cie-behavior`, `cie-conformance`, `cie-security`, `cie-testing`, `cie-fca`, `cie-drift-engine`, `cie-sarif` |
| `cie-llm` | `cie-llm-api`, `cie-llm-offline`, `cie-llm-cloud`, `cie-llm-cache` |
| `cie-cli` | `cie-cli` + `cie-engine` |

The 30-crate scaffold decision (gate D-1) is deferred to v0.3 where the five new adapter crates
(Java/C/C++/C#/PHP) make the per-adapter layout earn its keep (FR-18).

## 2. Sketch-fidelity mapping (Phase 0–2 validation sketches)

The instructor sketches are the binding validation contract. They compile as-written modulo these
documented, single-point mappings (v0.3 30-crate scaffold additions):

| v0.3 deviation | Reason |
|---|---|
| Flat root layout (member dirs are siblings of the root manifest, not under `crates/`) | the TASK-001 sketch reads `CARGO_MANIFEST_DIR/../Cargo.toml` — the binding path — which only resolves for the flat layout |
| `default_registry()` lives in `cie-ingest` (composition root), not `cie-adapters` | Rust forbids trait-crate → adapter-crate cycles; the one-line registration contract is unchanged |
| `Finding`/`FindingKind` live in `cie-contract` (re-exported by `cie-analysis`) | findings are contract-check outputs; keeps `cie-incremental`/`cie-drift` cycle-free |
| `Evidence`/`Coverage` scalar shapes live in `cie-core` (re-exported by `cie-evidence`) | §9 `AnalysisInput`/`AnalysisOutput` are sketch-pinned to `cie-core` |
| §9 `AnalysisInput<F, C>` / `AnalysisOutput<D>` are generic with `()` defaults | the §9 seam stays in `cie-core` without depending on `cie-semantic` |
| `z3` links as system libz3 via hand-written C FFI (feature `smt-z3`), not the `z3` Rust crate | the crate needs bindgen/libclang, outside the offline-first story; a pure-Rust bit-blast fallback is the default backend (TASK-047) |
| `rustc_hash` sketch spelling = `rustc-hash` manifest key | Cargo hyphenation |

Original v0.1/v0.2 mappings:

| Sketch symbol | Delivered symbol | Note |
|---|---|---|
| `use cie_core::…` / `cie_semantic::…` / `cie_relations::…` / `cie_adapters::…` | `cie_core::…` (incl. `relations` module) / `cie_langs::…` | import-path mapping only |
| `SourceLocation { file, start, end }` byte offsets (§14.1) | `SourceLocation { file, path, start_line, end_line, … }` | rendered line/col + path needed by every consumer; byte-span fields land with §14.2 shapes in v0.3 (TASK-006 alignment) |
| `LanguageId::PYTHON` et al. | `LangId::Python` et al. | naming convention |
| `SymbolFact { id, kind, name, loc, lang, rev }` | rich `SymbolFact` (+ `qual_name`, `parent`, `doc`) | §14.2-exact shape = v0.3 TASK-006 |
| `CallFact { caller, callee, kind, unresolved }` | `CallSite { caller, callee: Callee, … }` | `Callee::Unresolved(reason)` carries the unresolved flag plus its reason (§13 #6) |
| `smallvec` bounded reads/writes | v0.3 (TASK-006, with IFDS shapes) | |

## 3. 84-task disposition matrix

Verdicts: **done** = delivered in v0.1.0 (adapted where noted) · **v0.2** = implemented by the v0.2
work packages · **v0.3** / **v1.0** = scheduled on the roadmap · **stretch** = v0.2 if capacity allows.

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


## 4. v0.2 change log per task

All 32 *partial* dispositions from §3 are implemented in v0.2 (one commit per task group):

| WP | Tasks closed | Where |
|---|---|---|
| WP-0 | 001 (deviation documented), traceability | `docs/TRACEABILITY.md`, workspace 0.2.0 |
| WP-1 | 002, 003, 004, 005, 010 | `hash`/`BLAKE3_EMPTY`/`RevisionAllocator`/`ClockSource`; §14.1 newtypes + `Interner<T>`/`StrInterner`/`InternedStr`; §9 protocol + `CieError` + `CI_TOLERANCE`; `LanguageTable::v1`; `TypedFacts` dedup + integrity — sketch tests `task_002..010` |
| WP-2 | 028, 030, 056 | tombstones + purge; per-file summary reuse; symbol-level dirty worklist (+ FR-4 determinism fix) |
| WP-3 | 007, 008, 009 | `RelKind`/`RelationGraph`; `CsrGraph` + rkyv + 1M-edge gate; FOW `Pdg` + bounded backward slice |
| WP-4 | 011, 027/061, 063/065/066, 072/073/075/076, 077 | semantics tables + FR-18 guard (acc #9); perf suite (acc #15 subset); LLM trio (#12–14); drift #17/#24/#25; harness 26 items |
| WP-5 | 078, 079, 080, 082, 083, 084 | six `concept_node:*` keys; cart fixture + four binding sources (LlmCandidate inert); cycle rejection; `--max-depth`; parent-edge invalidation; 26→31 meta-test |
| WP-6 | 071, 073, 054(subset) | rst/adoc/html ingestion; persisted ClaimStore; IgnoredErrorValue + bare-except recovery gaps |

Runnable source of truth: `crates/cie-cli/tests/acceptance_harness.rs` — base 26 items
(24 green / 2 deferred: #15 perf-subset shipped, #16 zero-copy deferred to v1.0) and the
TASK-084 extension to 31 (29 green / 2 deferred). Perf gates bind on release builds
(`cargo test --release -- --ignored`). Documented sketch adaptations are in §2 above.

Out-of-scope tasks (24) remain scheduled exactly as §3 states: v0.3 (adapters 015–018/021,
deep JS/TS/Go/Rust, path conditions, IFDS, recovery, naming adapters, rayon walk) and
v1.0 (SMT/CEGAR 047–051, HNSW 059, zero-copy 060, dd 057, L3 archive 062, sled/redb 024).
