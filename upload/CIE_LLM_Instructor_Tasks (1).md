# CIE — LLM Instructor Task List

**Version:** 2.0 (rebuilt 2026-10-17 after sandbox reset; content-equivalent regeneration of the v1.1 task list)
**Source of truth:** `PRD: Code Intelligence Engine (CIE) — Complete Specification` (FR-1…FR-39 + Implementation Addendum FR-40…FR-44)
**Scope:** 84 micro-tasks across 16 phases (Phase 0–15), dependency-ordered as a DAG, each with a binding Rust `#[test]` validation sketch.
**Audience:** an LLM coding agent (or human pair) implementing CIE crate by crate, one verified micro-task at a time.

---

## How to Use This Document

1. **Work in DAG order.** Execute tasks only after every task listed in their `Depends on` field is complete and green. The Phase table below gives the macro order; the `Depends on` fields are the precise contract.
2. **Tests are contracts.** Each task's `Validation test sketch` is a binding acceptance contract. An implementing agent may add tests, but may NOT weaken, delete, or reinterpret an existing sketch. If a sketch proves the design wrong, fix the design, not the test.
3. **Metadata is binding.** `Target crate(s)`, `Constraints`, and `DoD` are review criteria. A task is done only when every DoD bullet is checkably true and every constraint still holds.
4. **Parallel groups.** Tasks sharing a `Parallel group` value (e.g. the ten language adapters in G-2a…G-2j) are mutually independent and may be executed concurrently by multiple agents. Tasks in different groups within the same phase are parallel only when their dependency sets allow.
5. **`[perf]` tests** are `#[ignore]`-gated and compare measured nanoseconds against the PRD §15.4 targets multiplied by `CI_TOLERANCE` (defined in TASK-004, default 3.0). They run nightly, not per-commit.
6. **Fixtures are shared.** `cart` (TASK-079), `cyclic_concepts` (TASK-080) and the cross-language fixture repo (TASK-022) are reused by later phases and by the acceptance harness. Do not modify a fixture after its defining task ships.
7. **The acceptance constant is locked.** `ACCEPTANCE_ITEM_COUNT = 26` (TASK-077). Only TASK-084 is authorized to raise it to 31.

## Standing Rules (every task, non-negotiable)

- **Nanosecond time (FR-21).** All timestamps, durations, deadlines, budgets, TTLs: `Ns = u64` ns since epoch; signed deltas `NsDelta = i64`; `RevId = u64`; `Hash = [u8; 32]` (blake3). No `chrono::DateTime` in hot paths.
- **Evidence invariant (FR-4).** Every derived result carries `Evidence { rev, provenance, assumptions, unsupported, unresolved }`. A frozen or concluded status without evidence is a defect.
- **Timeout ≠ no defect.** An analysis or verification timeout yields `Unknown`/`TimedOut`, never `Implemented`/clean (FR-35; §13 item 26).
- **LLM never sets `Explicit`/`Verified` (FR-28).** LLM output is `Label { source: Llm, model_id, prompt_hash }`, content-addressed and cached (FR-25), offline-first via Ollama (FR-27); over budget → downgrade and continue (FR-26).
- **Language semantics do not leak (FR-3).** A rule true in one language must not fire in another without that language's adapter vouching for it.
- **Storage keys follow PRD §15.1 exactly** (plus the six `concept_node:*` keys introduced by TASK-078).

## Document Map

| Phase | Title | Tasks | Depends on | Parallel groups |
|---|---|---|---|---|
| 0 | Workspace & Shared Core | TASK-001…004 | — | G-0a |
| 1 | Semantic Model & Typed Relations | TASK-005…010 | Phase 0 | G-1a, G-1b |
| 2 | Language Adapter Framework & Top-10 Languages | TASK-011…022 | Phase 1 | G-2-0, G-2a…G-2j, G-2-Z |
| 3 | Evidence, Storage & Revisioning | TASK-023…028 | Phase 0 | G-3a, G-3b |
| 4 | Execution Engine, Summaries & IFDS | TASK-029…035 | Phases 1–3 | G-4a, G-4b |
| 5 | Behavior Extraction, Contracts, FCA & Hierarchy | TASK-036…046 | Phase 4 | G-5a, G-5b, G-5c |
| 6 | Counterexamples: SMT, CEGAR & Symbolic Execution | TASK-047…051 | Phases 4–5 | G-6a |
| 7 | Recovery Execution & Fault Injection | TASK-052…054 | Phases 5–6 | G-7a |
| 8 | Incremental Maintenance | TASK-055…058 | Phases 3–4 | G-8a |
| 9 | Vector Store, Snapshots & Performance Hardening | TASK-059…062 | Phases 3, 8 | G-9a |
| 10 | Native LLM Layer | TASK-063…066 | Phase 0 | G-10a |
| 11 | Folder Assimilation & CLI | TASK-067…070 | Phases 5, 8, 10 | G-11a, G-11b, G-11c |
| 12 | Document Ingestion & Claims | TASK-071…073 | Phases 0, 10 | G-12a |
| 13 | Drift Engine | TASK-074…076 | Phases 5–6, 9, 12 | G-13a |
| 14 | Acceptance Harness | TASK-077 | All of Phases 0–14 | G-14a |
| 15 | Hierarchical Concept Behavior (Addendum FR-40…FR-44) | TASK-078…084 | Phases 5, 8, 14 | G-15a…G-15g |

## Parallelization Notes

- **Phases 0 → 1 are the critical spine**; everything else hangs off them. Start Phases 2, 3, and 10 as soon as their individual dependencies are green — Phase 10 (LLM layer) needs only Phase 0 and can proceed alongside Phase 2.
- **The ten language adapters (TASK-012…021)** are fully parallel after TASK-011; one agent per adapter is the intended execution model (FR-18 keeps each adapter to one crate + one registry line).
- **Phases 6 and 7 are chain-ordered** (CEGAR needs SMT; LDFI needs obligations), but both are independent of Phase 8 (incremental), which can start as soon as Phases 3–4 land.
- **Phase 13 (drift) is the widest consumer**: it needs concepts (5), counterexamples (6), HNSW (9), and claims (12). Phase 14 must wait for everything; Phase 15 then extends the frozen harness under its single authorized change (TASK-084).
- **Human review gates** are recommended after Phase 5 (algorithmic core), Phase 8 (incremental correctness), and Phase 14 (acceptance freeze) before proceeding.

---

## Matrix A — Functional Requirement → Task Coverage

Every FR of the PRD (plus the implementation addendum) mapped to implementing tasks.

| Requirement | Title | Implementing tasks |
|---|---|---|
| FR-1 | Semantic program model | TASK-003, TASK-005, TASK-010 |
| FR-2 | Typed relations | TASK-007, TASK-008, TASK-009 |
| FR-3 | Language semantics adapters | TASK-011, TASK-022 |
| FR-4 | Evidence and coverage | TASK-023 |
| FR-5 | Summary store | TASK-030, TASK-033 |
| FR-6 | Contract registry | TASK-038 |
| FR-7 | Analysis execution | TASK-029 |
| FR-8 | Incremental maintenance | TASK-055, TASK-056, TASK-058 |
| FR-9 | Result protocol | TASK-037, TASK-051 |
| FR-10 | Naming adapter | TASK-039 |
| FR-11 | Behavior extraction (Stage 1) | TASK-031, TASK-033, TASK-035, TASK-036 |
| FR-12 | Behavioral conformance (Stage 2) | TASK-040, TASK-041, TASK-042 |
| FR-13 | Rules and concepts (Stage 3) | TASK-043, TASK-044, TASK-045 |
| FR-14 | Structural hierarchy (Stage 4) | TASK-046 |
| FR-15 | Recovery execution (Stage 5) | TASK-052, TASK-053, TASK-054 |
| FR-16 | Performance hardening (Stage 6) | TASK-057, TASK-061 |
| FR-17 | Top-10 language support | TASK-012, TASK-013, TASK-014, TASK-015, TASK-016, TASK-017, TASK-018, TASK-019, TASK-020, TASK-021 |
| FR-18 | Easy language extension | TASK-011, TASK-022 |
| FR-19 | Folder-point assimilation | TASK-067, TASK-068, TASK-070 |
| FR-20 | Incremental re-ingestion | TASK-057, TASK-069 |
| FR-21 | Nanosecond time unit | TASK-002 |
| FR-22 | Minimal-latency storage | TASK-024, TASK-027, TASK-061 |
| FR-23 | Time-travel queries | TASK-026 |
| FR-24 | Native LLM providers | TASK-063, TASK-064 |
| FR-25 | LLM caching | TASK-065 |
| FR-26 | LLM budget enforcement | TASK-066 |
| FR-27 | LLM offline-first | TASK-063 |
| FR-28 | LLM never sets Explicit/Verified | TASK-038, TASK-066, TASK-079 |
| FR-29 | Embedding store (HNSW) | TASK-059 |
| FR-30 | Zero-copy snapshot reads | TASK-060 |
| FR-31 | Document ingestion | TASK-071 |
| FR-32 | Claim extraction | TASK-072, TASK-073 |
| FR-33 | Claim–code mapping | TASK-074 |
| FR-34 | Per-claim verification | TASK-075 |
| FR-35 | Drift statuses | TASK-075 |
| FR-36 | Reverse drift | TASK-076 |
| FR-37 | Drift report | TASK-076 |
| FR-38 | Multi-document reconciliation | TASK-076 |
| FR-39 | LLM role for drift | TASK-066, TASK-072, TASK-074 |
| FR-40 | Hierarchical concept nodes (addendum) | TASK-078 |
| FR-41 | Concept–behavior binding (addendum) | TASK-079 |
| FR-42 | Hierarchical merge to DAG (addendum) | TASK-080 |
| FR-43 | Rollup semantics (addendum) | TASK-081 |
| FR-44 | Concept invalidation + CLI + acceptance extension (addendum) | TASK-082, TASK-083, TASK-084 |

## Matrix B — Acceptance Suite → Task Coverage

PRD §13 items 1–26 plus addendum items 27–31. TASK-077 registers 1–26; TASK-084 appends 27–31.

| Item | Expected outcome | Implementing tasks |
|---|---|---|
| 1 | Print-only authorization detected | TASK-036, TASK-039 |
| 2 | Unconditional success detected | TASK-036 |
| 3 | Legitimate delegation recognized | TASK-033, TASK-036 |
| 4 | Rejected authorization ignored by caller | TASK-042 |
| 5 | Exceptions converted to success | TASK-042 |
| 6 | Unresolved external behavior flagged | TASK-035 |
| 7 | Deletion invalidates earlier finding | TASK-028, TASK-058 |
| 8 | Polyglot repo ingested end-to-end | TASK-068 |
| 9 | New adapter: 1 crate + 1 registry line | TASK-011, TASK-022 |
| 10 | Re-ingest unchanged ≤ 5% wall time | TASK-069 |
| 11 | Time-travel correct for rev N and N-1 | TASK-026 |
| 12 | LLM labeling offline via Ollama | TASK-063 |
| 13 | LLM cache hit ≤ 100 ns | TASK-065 |
| 14 | Budget exceeded → downgrade, continue | TASK-066 |
| 15 | §15.4 hot-path latency targets met | TASK-027, TASK-061 |
| 16 | Zero-copy snapshot 1M syms ≤ 10 µs | TASK-060 |
| 17 | ≥ 90% explicit claims non-Unknown | TASK-072, TASK-073, TASK-075 |
| 18 | Double-charge claim → Contradicting + path | TASK-047, TASK-051, TASK-075 |
| 19 | No-match claim → Missing with complete search | TASK-074, TASK-075 |
| 20 | p99 claim → Unverifiable (runtime) | TASK-075 |
| 21 | Undocumented code in reverse_drift | TASK-076 |
| 22 | Conflicting docs → Contradicting citing both | TASK-076 |
| 23 | --from-rev/--to-rev shows only changes | TASK-076 |
| 24 | Drift re-run ≤ 5% of first run | TASK-069, TASK-076 |
| 25 | LLM off → ≥ 60% claims still extracted | TASK-072 |
| 26 | Timeout → Unknown, never Implemented | TASK-029, TASK-075 |
| 27 | Concept nodes persisted with evidence (078) | TASK-078 |
| 28 | Four binding sources; LlmCandidate inert (079) | TASK-079 |
| 29 | Merge → DAG with EdgeReason; cycles rejected (080) | TASK-080 |
| 30 | Rollup == clean recompute (081) | TASK-081 |
| 31 | Invalidation only along parent edges (082) | TASK-082 |

## Matrix C — PRD Stages & Slices → Phases

| PRD stage (§11) | PRD slice (§12) | CIE phases |
|---|---|---|
| 1 — Function effects, summaries | 0 — Foundation + first extractor | 0, 1, 4 |
| 2 — Contract registry, candidate rules | 1 — Behavioral conformance | 5 (036–042) |
| 3 — FCA concepts + hierarchy | 2 — Rules and concepts | 5 (043–046) |
| **3b — Hierarchical concept behavior (addendum)** | **2b — Concept rollup + CLI (addendum)** | 15 |
| 4 — Counterexamples + fault injection | 3–4 — Hierarchy, recovery | 6, 7 |
| 5 — Incremental maintenance | 5 — Performance hardening | 8, 9 |
| 6 — Multi-language expansion | 6 — Multi-language | 2 |
| 7 — Native LLM | 7 — Native LLM | 10 |
| 8 — Folder assimilation | 8 — Folder assimilation | 11 |
| 9 — Document–code drift | 9 — Drift | 12, 13 |
| §13 acceptance suite | — | 14 (items 1–26), 15/TASK-084 (items 27–31) |

---

## Phase 0 — Workspace & Shared Core

Phase 0 stands up the Cargo workspace and the primitive layer every later phase compiles against: all 30 member crates of PRD §10 as green stubs, the nanosecond time/hash core (FR-21), stable identity types with a generic interner (FR-1), and the shared AnalysisInput/AnalysisOutput protocol (§9). It also fixes the `#[perf]` test convention — PRD §15.4 latency targets multiplied by a documented `CI_TOLERANCE` — that every later performance test must follow. Nothing in Phases 1–14 may redefine or bypass these primitives.

### TASK-001: Scaffold the Cargo workspace with all PRD §10 member crates

| Field | Value |
|---|---|
| Phase | 0 — Workspace & Shared Core |
| Target crate(s) | root `Cargo.toml` + all 30 member crates (stubbed) |
| Depends on | None (root) |
| PRD reference | §10 workspace layout + key crates, §15.3 |
| Parallel group | G-0a |

**Constraints:**
- `crates/` mirrors §10 exactly: `cie-core`, `cie-semantic`, `cie-relations`, `cie-adapters` plus the ten `cie-adapter-{python,javascript,typescript,java,c,cpp,csharp,go,rust,php}` crates and `cie-ingest`, `cie-evidence`, `cie-summary`, `cie-contract`, `cie-analysis`, `cie-incremental`, `cie-concepts`, `cie-counterexample`, `cie-fault`, `cie-store`, `cie-llm`, `cie-docs`, `cie-claims`, `cie-drift`, `cie-cli`, `cie-server` — all workspace members.
- Third-party dependencies are declared once in `[workspace.dependencies]` and inherited via `workspace = true`: `tree-sitter`, `petgraph`, `z3`, `differential-dataflow`, `rayon`, `tokio` (optional, `cie-server` only), `serde`, `sled`, `rkyv`, `memmap2`, `hnsw_rs`, `tracing`, `clap`, `thiserror`, `anyhow`, `dashmap`, `rustc_hash`, `smallvec`, `bitvec`.
- `chrono` is forbidden in every manifest (FR-21, global rule 1): time enters the system only through `cie-core` primitives.
- `rustfmt.toml` and `clippy.toml` are committed at the root; CI runs `cargo clippy --workspace -- -D warnings`.
- Every stub crate compiles with one placeholder `#[test]` so `cargo test --workspace` is green from day one.

**DoD:**
- `cargo metadata` reports all 30 member crates under `workspace.members`.
- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass on a clean clone.
- `[workspace.dependencies]` pins the 19 key crates listed in §10 with explicit versions.
- `cargo fmt --check` is clean; `tokio` appears only behind `cie-server`'s optional feature gate.

This task creates the skeleton the whole PRD builds on: one command (`cargo test`) exercises every crate, and every later task inherits dependency versions, lint policy, and crate boundaries from it. The member list mirrors §10 exactly so Phases 1–14 fill existing crates instead of inventing new ones. Centralizing third-party versions in `[workspace.dependencies]` prevents drift between the engine and the ten adapter crates.

**Validation test sketch (contract):**

```rust
// crates/cie-core/tests/workspace_contract.rs
const MEMBERS: [&str; 30] = [
    "cie-core", "cie-semantic", "cie-relations", "cie-adapters",
    "cie-adapter-python", "cie-adapter-javascript", "cie-adapter-typescript",
    "cie-adapter-java", "cie-adapter-c", "cie-adapter-cpp", "cie-adapter-csharp",
    "cie-adapter-go", "cie-adapter-rust", "cie-adapter-php", "cie-ingest",
    "cie-evidence", "cie-summary", "cie-contract", "cie-analysis",
    "cie-incremental", "cie-concepts", "cie-counterexample", "cie-fault",
    "cie-store", "cie-llm", "cie-docs", "cie-claims", "cie-drift",
    "cie-cli", "cie-server",
];

#[test]
fn task_001_workspace_members_and_deps() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")).unwrap();
    for m in MEMBERS {
        assert!(manifest.contains(m), "missing workspace member: {m}");
    }
    for dep in ["petgraph", "rkyv", "memmap2", "dashmap", "rustc_hash", "smallvec", "thiserror", "z3"] {
        assert!(manifest.contains(dep), "missing [workspace.dependencies] entry: {dep}");
    }
    assert!(!manifest.contains("chrono"), "FR-21: no chrono anywhere in the workspace");
}
```

### TASK-002: Implement Ns/NsDelta/RevId/Hash primitives and the revision allocator in cie-core

| Field | Value |
|---|---|
| Phase | 0 — Workspace & Shared Core |
| Target crate(s) | `cie-core` |
| Depends on | TASK-001 |
| PRD reference | §14.1, FR-21, §15.1 `rev:{rev_id}:meta` |
| Parallel group | G-0b |

**Constraints:**
- Type aliases exactly as §14.1/FR-21: `pub type Ns = u64;`, `pub type NsDelta = i64;`, `pub type RevId = u64;`, `pub type Hash = [u8; 32];`.
- `hash(bytes: &[u8]) -> Hash` uses blake3; all content addressing (`sym_by_hash:{hash}` in §15.1, LLM cache in FR-25) builds on it — no other hash function in hot paths.
- `RevisionAllocator` hands out strictly monotonic `RevId`s (lock-free `AtomicU64`), thread-safe, with the last value recoverable after restart via `rev:{rev_id}:meta` (§15.1, global rule 7).
- Clock access is `now_ns() -> Ns` behind an injectable source; `chrono::DateTime` must not appear in `cie-core` or any hot path (FR-21, global rule 1).
- `Ns`, `NsDelta`, `RevId`, `Hash` are `Copy` and allocation-free on the timing path (§15.5 memory layout rules).

**DoD:**
- `cie-core::{Ns, NsDelta, RevId, Hash, hash, now_ns, RevisionAllocator}` exported and documented.
- blake3 known vectors match (empty input and one non-trivial input).
- 100k allocations from 8 threads are strictly increasing — no duplicate or rolled-back `RevId`.
- `cie-core/Cargo.toml` has no `chrono` dependency.

Time and content identity are the two things every other crate touches, so they are fixed before any model code exists. `Ns = u64` nanoseconds since epoch keeps timestamps comparable and allocation-free (FR-21), while blake3 provides content-addressed identity for symbols, summaries, and LLM prompts (FR-25). The monotonic revision allocator is what makes every stored fact revision-scoped (§15.1) and time-travel queries possible (FR-23).

**Validation test sketch (contract):**

```rust
use cie_core::{hash, now_ns, Hash, Ns, NsDelta, RevisionAllocator, RevId};

#[test]
fn task_002_time_hash_and_revisions() {
    let t0: Ns = 1_700_000_000_123_456_789;
    let delta: NsDelta = -250;
    assert_eq!((t0 as i64 + delta) as Ns, 1_700_000_000_123_456_539);

    let h: Hash = hash(b"authorize");
    assert_eq!(h, hash(b"authorize"), "content addressing is stable");
    assert_ne!(h, hash(b"authorize2"), "different bytes, different Hash");
    assert_eq!(hash(b""), cie_core::BLAKE3_EMPTY, "known blake3 vector");

    let mut alloc = RevisionAllocator::starting_at(41);
    assert_eq!(alloc.next(), 42);
    assert!(alloc.next() > 42, "FR-21: revisions are strictly monotonic u64");
    assert!(now_ns() > 0);
}
```

### TASK-003: Define stable identity types and the generic Interner in cie-core

| Field | Value |
|---|---|
| Phase | 0 — Workspace & Shared Core |
| Target crate(s) | `cie-core` |
| Depends on | TASK-001 |
| PRD reference | §14.1, FR-1, §15.5 "strings interned once" |
| Parallel group | G-0b |

**Constraints:**
- Newtypes exactly per §14.1: `SymbolId(u32)`, `OperationId(u32)`, `ValueId(u32)`, `ResourceId(u32)`, plus `FileId(u32)`; all `Copy + Hash + Eq + Ord`, no `String` fields (§15.5).
- `SourceLocation { file: FileId, start: u32, end: u32 }` byte offsets, `Copy`, exactly §14.1 — document-side locations live in `cie-docs` later, never here.
- `Interner<T> { map: FxHashMap<Box<str>, u32>, vec: Vec<T> }` (rustc_hash `FxHashMap` per §10) with `insert(key, value) -> u32`, `get(u32)`, `get_by_key(&str)`; interning the same key twice yields the same index (FR-1 stable identity).
- `StrInterner` (a thin `Interner<Box<str>>` wrapper) exposes `intern(&str) -> InternedStr`, `lookup(&str) -> Option<InternedStr>`, `get(InternedStr) -> Option<&str>`; `InternedStr(pub u32)` is what §14.2 fact fields store.
- Interned strings are stored once; hot structs reference ids only (§15.5). Persistence belongs to `cie-store`, not here.

**DoD:**
- All identity types, `InternedStr`, `Interner<T>`, and `StrInterner` exported from `cie-core`.
- intern/lookup roundtrip and idempotence tests pass.
- 1M distinct keys interned in one interner with zero id collisions (debug-mode test).
- `lookup` performs no allocation (takes `&str`, returns an id).

Every algorithm must refer to the same code element (FR-1), so identity types and the string table exist before any fact extraction. `Interner<T>` keeps names as `u32` handles so `SymbolFact`, `CallFact`, and relation weights stay small and `Copy`-friendly (§15.5). The `FxHashMap` choice matches the PRD key-crates list (§10) and underwrites the sub-50 ns lookup behavior §15.4 assumes.

**Validation test sketch (contract):**

```rust
use cie_core::{FileId, Interner, OperationId, ResourceId, SourceLocation, StrInterner, SymbolId, ValueId};

#[test]
fn task_003_identity_types_and_interners() {
    let mut names = StrInterner::new(); // Interner<Box<str>> convenience wrapper
    let a = names.intern("authorize");
    let b = names.intern("authorize");
    assert_eq!(a, b, "FR-1: same name, same id, forever");
    assert_eq!(names.lookup("authorize"), Some(a));
    assert_eq!(names.lookup("missing"), None);
    assert_eq!(names.get(a), Some("authorize"));

    let mut payloads: Interner<Box<str>> = Interner::new();
    let k = payloads.insert("auth.py", Box::from("auth.py"));
    assert_eq!(payloads.get(k).map(|s| s.as_ref()), Some("auth.py"));

    let loc = SourceLocation { file: FileId(7), start: 12, end: 34 };
    assert!(loc.start < loc.end);
    let ids = [SymbolId(1), OperationId(2), ValueId(3), ResourceId(4)];
    let set: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(set.len(), 4, "all ids are Copy+Hash+Eq hot keys (§15.3 L0)");
}
```

### TASK-004: Fix the AnalysisInput/AnalysisOutput protocol and the `#[perf]` test convention

| Field | Value |
|---|---|
| Phase | 0 — Workspace & Shared Core |
| Target crate(s) | `cie-core` |
| Depends on | TASK-002, TASK-003 |
| PRD reference | §9 shared interface, §14.8 Evidence, §15.4 latency targets, FR-4 |
| Parallel group | G-0c |

**Constraints:**
- `AnalysisInput { revision, scope, facts, contracts, assumptions }` and `AnalysisOutput { derived_records, evidence, dependencies, coverage, conclusions, counterexamples }` with field names/order exactly §9; `pub type RevisionId = RevId;` reconciles §9's name with §14.1.
- `thiserror` enum `CieError` with at least `Parse`, `UnsupportedConstruct`, `Timeout`, `BudgetExceeded`, `Storage`, `DanglingReference` variants; `Timeout` maps downstream to `Unknown`, never to a clean status (FR-35, global rule 3).
- `AnalysisOutput::empty_with_evidence(rev, Ns)` exists: every output carries `Evidence { rev, provenance, assumptions, unsupported, unresolved, generated_at_ns }` (§14.8, FR-4, global rule 2) — outputs cannot be constructed without evidence.
- `pub const CI_TOLERANCE: f64 = 3.0;` documented here as the multiplier every `#[perf]` test applies to PRD §15.4 targets (global rule 5).
- The `#[perf]` convention is written into `cie-core` lib docs: perf tests are `#[ignore]`-gated, run via `cargo test -- --ignored`, and fail when `measured > target × CI_TOLERANCE`.

**DoD:**
- Protocol structs, `CieError`, `Evidence`, and `CI_TOLERANCE` compile from `cie-core`.
- Doc comment on `CI_TOLERANCE` explains the multiplier rule and cites §15.4.
- A reference `#[perf]` test in `cie-core/tests/perf_convention.rs` demonstrates the gate and passes under `--ignored`.
- `cargo test --workspace` stays green (ignored tests excluded by default).

The shared interface (§9) is the single seam between ingestion, the engine, and every algorithm; fixing it in Phase 0 prevents interface churn later. Evidence is mandatory on every output (FR-4), so `AnalysisOutput` is unrepresentable without it. `CI_TOLERANCE = 3.0` acknowledges CI hardware noise while keeping §15.4 targets binding: a 3× regression over a PRD target still fails the suite.

**Validation test sketch (contract):**

```rust
use cie_core::{AnalysisInput, AnalysisOutput, CI_TOLERANCE, CieError, RevId};

#[test]
fn task_004_protocol_and_evidence_invariant() {
    let input = AnalysisInput::for_revision(RevId(1));
    let out = AnalysisOutput::empty_with_evidence(input.revision, 1_700_000_000_123_456_789);
    assert_eq!(out.evidence.rev, input.revision, "FR-4: output always carries evidence");
    assert!(out.conclusions.is_empty() && out.derived_records.is_empty());

    let err = CieError::Timeout;
    assert!(err.is_timeout(), "FR-35: timeout is first-class, never mapped to a clean status");
}

#[test]
#[ignore = "perf: run with `cargo test -- --ignored` (global rule 5)"]
fn task_004_perf_convention_reference() {
    let target_ns = 50.0; // §15.4: symbol lookup by id
    let measured = measure_lookup_ns_50k_samples(); // helper under test
    assert!(
        measured <= target_ns * CI_TOLERANCE,
        "§15.4 target {target_ns} ns × CI_TOLERANCE {CI_TOLERANCE} exceeded: {measured} ns"
    );
}
```

## Phase 1 — Semantic Model & Typed Relations

Phase 1 turns raw identifiers into a typed semantic model: symbol facts (§14.2), the four fact families, the typed relation graph (§14.3), its CSR freeze, and the Ferrante–Ottenstein–Warren PDG with backward slicing. Everything here is language-agnostic — per-language judgment is delegated to the adapters of Phase 2 (FR-3). These crates are the common input that behavior extraction, slicing, and clustering all consume (FR-2).

### TASK-005: Implement SymbolFact, SymbolKind, and the LanguageId v1 table in cie-semantic

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-semantic` |
| Depends on | TASK-002, TASK-003 |
| PRD reference | §14.2, FR-1, §16 v1 language table, §15.1 `sym:{symbol_id}` |
| Parallel group | G-1a |

**Constraints:**
- `SymbolFact { id, kind, name, loc, lang, rev }` with field names/types exactly §14.2: `name: InternedStr`, `loc: SourceLocation`, `lang: LanguageId`, `rev: RevId`.
- `SymbolKind` covers at least `Function, Method, Class, Struct, Interface, Trait, Enum, Module, Variable, Constant, Field, Parameter, TypeAlias` — a closed enum; adapters must not invent kinds.
- `LanguageId` plus `LanguageTable::v1()` enumerate exactly the ten v1 languages of §16 (Python, JavaScript, TypeScript, Java, C, C++, C#, Go, Rust, PHP) plus `UNKNOWN`; no v1.1 languages (Ruby, Kotlin, Swift, Scala, SQL) receive ids yet.
- Every fact is stamped with a `RevId` from the `RevisionAllocator` (FR-21, global rule 1) and serializes deterministically for `sym:{symbol_id}` storage (§15.1, global rule 7).
- No language-specific logic may enter `cie-semantic` (FR-3 boundary): the crate records which language a fact came from, never applies that language's rules.

**DoD:**
- `SymbolFact`, `SymbolKind`, `LanguageId`, `LanguageTable::v1()` compile from `cie-semantic`.
- `LanguageTable::v1()` returns exactly 10 distinct ids matching §16.
- serde + rkyv roundtrip of a `SymbolFact` is byte-stable.
- `cargo test -p cie-semantic` green.

`SymbolFact` is the atomic unit of the semantic model: every later stage (summaries, contracts, concepts, drift mappings) points at these facts rather than at source text (FR-1). Storing `lang` on the fact is what lets later checkers ask "may this rule apply here?" before firing (FR-3). The closed table of ten ids (§16) is the contract adapters must satisfy — a language without an id cannot produce facts.

**Validation test sketch (contract):**

```rust
use cie_semantic::{LanguageId, LanguageTable, SourceLocation, SymbolFact, SymbolKind, StrInterner};
use cie_core::{FileId, RevId, SymbolId};

#[test]
fn task_005_symbol_facts_and_v1_languages() {
    let mut names = StrInterner::new();
    let fact = SymbolFact {
        id: SymbolId(1),
        kind: SymbolKind::Function,
        name: names.intern("authorize"),
        loc: SourceLocation { file: FileId(0), start: 0, end: 128 },
        lang: LanguageId::PYTHON,
        rev: RevId(1),
    };
    assert_eq!(fact.kind, SymbolKind::Function);
    assert_eq!(fact.lang, LanguageId::PYTHON);

    let v1 = LanguageTable::v1();
    assert_eq!(v1.len(), 10, "FR-17: ten v1 languages (§16)");
    let ids: Vec<_> = v1.iter().copied().collect();
    for expected in [LanguageId::PYTHON, LanguageId::JAVASCRIPT, LanguageId::TYPESCRIPT,
                     LanguageId::JAVA, LanguageId::C, LanguageId::CPP, LanguageId::CSHARP,
                     LanguageId::GO, LanguageId::RUST, LanguageId::PHP] {
        assert!(ids.contains(&expected), "missing §16 language: {expected:?}");
    }
}
```

### TASK-006: Implement CallFact, EffectFact, ExceptionFact, and AsyncFact in cie-semantic

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-semantic` |
| Depends on | TASK-003, TASK-005 |
| PRD reference | §14.2, FR-2, FR-4, §13 #6 |
| Parallel group | G-1a |

**Constraints:**
- Struct shapes exactly §14.2: `CallFact { caller, callee, kind, loc, unresolved }`, `EffectFact { op, reads, writes, external }` with `SmallVec<[ResourceId; 4]>`, `ExceptionFact { op, kind, handler: Option<SourceLocation> }`, `AsyncFact { op, kind }`.
- `CallKind { Direct, Virtual, Dynamic, Reflective }` and `AsyncKind { Async, Await, Spawn, Channel }` exactly as the §14.2 comments state.
- `ExceptionKind` is closed over the ten languages' models: `Raised, Propagated, Caught, ErrorValue, Panic` — error-value languages (Go, Rust per §16) never emit `Raised`; this is the FR-3 groundwork for TASK-019/020.
- A call that cannot be resolved at extraction time is kept with `unresolved: true` — dropping it is a contract violation (FR-4, §13 #6 unresolved external behavior).
- All fact structs derive `Debug, Clone, PartialEq, Serialize, Deserialize` plus rkyv `Archive` for §15.1 storage; bounded `SmallVec`s keep hot paths allocation-free (§15.5).

**DoD:**
- Four fact structs + three enums compile from `cie-semantic` with §14.2 shapes.
- `reads`/`writes` capacities are exactly 4 per §14.2.
- Roundtrip serialization tests pass for all four structs.
- Crate docs state which adapters are expected to emit which `ExceptionKind` (per §16 semantics notes).

These four fact families are the vocabulary adapters speak (Phase 2) and the engine consumes (§5.1 shared foundation). `unresolved` is the honesty flag of the whole system: a dynamic Python call or a reflective Java call is recorded, not guessed (FR-4). Closing `ExceptionKind` over all ten languages now prevents adapters from forking the model later.

**Validation test sketch (contract):**

```rust
use cie_semantic::{AsyncFact, AsyncKind, CallFact, CallKind, EffectFact, ExceptionFact, ExceptionKind};
use cie_core::{FileId, OperationId, ResourceId, SourceLocation, SymbolId};

const LOC: SourceLocation = SourceLocation { file: FileId(0), start: 10, end: 40 };

#[test]
fn task_006_typed_fact_shapes() {
    let call = CallFact {
        caller: SymbolId(1), callee: SymbolId(2),
        kind: CallKind::Dynamic, loc: LOC, unresolved: true,
    };
    assert!(call.unresolved, "FR-4/§13#6: unresolved calls are marked, never dropped");

    let effect = EffectFact {
        op: OperationId(3),
        reads: smallvec::smallvec![ResourceId(9)],
        writes: smallvec::smallvec![],
        external: true,
    };
    assert!(effect.reads.len() <= 4, "§14.2: bounded SmallVec<[ResourceId; 4]>");

    let raised = ExceptionFact { op: OperationId(3), kind: ExceptionKind::Raised, handler: Some(LOC) };
    let errval = ExceptionFact { op: OperationId(4), kind: ExceptionKind::ErrorValue, handler: None };
    assert!(matches!(raised.kind, ExceptionKind::Raised) && raised.handler.is_some());
    assert!(matches!(errval.kind, ExceptionKind::ErrorValue));

    let asyncs = [AsyncFact { op: OperationId(5), kind: AsyncKind::Spawn },
                  AsyncFact { op: OperationId(6), kind: AsyncKind::Channel }];
    assert!(asyncs.iter().any(|a| matches!(a.kind, AsyncKind::Spawn)));
}
```

### TASK-007: Implement RelKind and RelationGraph (petgraph StableGraph) in cie-relations

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-relations` |
| Depends on | TASK-003, TASK-005 |
| PRD reference | §14.3, FR-2, §7 (Ferrante–Ottenstein–Warren row) |
| Parallel group | G-1b |

**Constraints:**
- `RelKind` has exactly the ten §14.3 variants: `Calls, Returns, ControlFlow, DataFlow, Reads, Writes, Throws, Catches, Owns, DependsOn`.
- `RelationGraph = petgraph::stable_graph::StableGraph<SymbolId, RelKind, petgraph::Directed, u32>` — `StableGraph` so deletions (FR-8) never shift surviving node indices.
- Duplicate edges between the same node pair with the same `RelKind` collapse deterministically; distinct kinds on the same pair coexist (a call can also be a data flow).
- Define `CfgEdge`/`DfgEdge` carrier structs here (operation-level from/to + kind): the §16.1 adapter methods return them, so they cannot live in `cie-adapters`.
- No language knowledge in `cie-relations` (FR-3): this crate only stores what adapters vouched for upstream.

**DoD:**
- `RelKind`, `RelationGraph`, `CfgEdge`, `DfgEdge` exported from `cie-relations`.
- Node-removal test proves `SymbolId` weights of survivors are untouched (StableGraph guarantee).
- Edge-dedup test: same `(from, to, kind)` twice → one edge; `(from, to, Calls)` + `(from, to, DataFlow)` → two edges.
- `cargo test -p cie-relations` green.

The typed relation graph is the shared currency of the engine: behavior extraction, slicing, and clustering must all consume this one representation (FR-2, §5.1). Choosing `StableGraph` with `u32` index space matches §14.3 and makes incremental deletion safe without renumbering symbols (FR-8). Ten relation kinds suffice for v1 because adapters express language nuance in fact types, not in new edge kinds.

**Validation test sketch (contract):**

```rust
use cie_relations::{RelationGraph, RelKind};
use cie_core::SymbolId;

#[test]
fn task_007_relation_graph_kinds_and_stability() {
    let mut g = RelationGraph::new();
    let a = g.add_node(SymbolId(1));
    let b = g.add_node(SymbolId(2));
    let c = g.add_node(SymbolId(3));

    g.add_edge(a, b, RelKind::Calls);
    g.add_edge(a, b, RelKind::Calls); // duplicate collapses
    assert_eq!(g.edge_count(), 1, "duplicate (from,to,kind) is deduped");
    g.add_edge(a, b, RelKind::DataFlow); // different kind coexists
    assert_eq!(g.edge_count(), 2);
    g.add_edge(b, c, RelKind::Throws);
    g.add_edge(c, b, RelKind::Catches);

    g.remove_node(a); // FR-8: deletions must not renumber survivors
    assert_eq!(g.node_weight(b), Some(&SymbolId(2)), "StableGraph keeps b stable");
    assert_eq!(g.node_weight(c), Some(&SymbolId(3)), "StableGraph keeps c stable");
}
```

### TASK-008: Freeze RelationGraph into CsrGraph with cache-friendly adjacency [perf]

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-relations` |
| Depends on | TASK-004, TASK-007 |
| PRD reference | §14.3 CsrGraph, §15.3 L0 Relations, §15.4 CSR scan ≤ 20 ns, FR-22 |
| Parallel group | G-1c |

**Constraints:**
- `CsrGraph { offsets: Vec<u32>, edges: Vec<(u32, RelKind)> }` exactly §14.3; the invariant `offsets.len() == node_count + 1` is checked on build.
- `CsrGraph::build_from(&RelationGraph)` freezes a snapshot: per-node edge lists are sorted by target for deterministic iteration; the source graph is never mutated.
- `neighbors(node) -> impl Iterator<Item = (u32, RelKind)>` walks the contiguous `edges[offsets[n]..offsets[n+1]]` slice — cache-friendly per §15.3, no per-edge allocation (§15.5).
- The snapshot must be rkyv-serializable for zero-copy L2 graph snapshots (FR-30, §15.3).
- Perf contract: per-edge adjacency scan ≤ 20 ns × `CI_TOLERANCE`, `#[perf]`-style and `#[ignore]`-gated (§15.4, global rule 5).

**DoD:**
- `build_from`, `neighbors`, `node_len`, `edge_len` implemented and unit-tested.
- Roundtrip test: every `(from, to, kind)` of the source graph appears in the CSR view.
- `task_008_csr_scan_perf` passes under `cargo test -p cie-relations -- --ignored`.
- Offsets invariant holds for an empty graph, a single-node graph, and a 1M-edge fixture.

CSR is the hot-read view of relations: p99 traversals in later stages assume the ≤ 20 ns per-edge scan of §15.4, which only a contiguous layout delivers. Freezing (rather than mutating) CSR keeps incremental maintenance honest — a new revision builds a new snapshot while old revisions stay queryable for time travel (FR-23). The perf test is a binding contract, gated by `#[ignore]` and multiplied by `CI_TOLERANCE` (global rule 5).

**Validation test sketch (contract):**

```rust
use cie_core::{now_ns, CI_TOLERANCE, SymbolId};
use cie_relations::{CsrGraph, RelationGraph, RelKind};

#[test]
fn task_008_csr_build_and_adjacency() {
    let mut g = RelationGraph::new();
    let a = g.add_node(SymbolId(0));
    let b = g.add_node(SymbolId(1));
    let c = g.add_node(SymbolId(2));
    g.add_edge(a, b, RelKind::Calls);
    g.add_edge(a, c, RelKind::DataFlow);
    g.add_edge(b, c, RelKind::Owns);

    let csr = CsrGraph::build_from(&g);
    assert_eq!(csr.offsets.len(), g.node_count() + 1, "§14.3: len = nodes + 1");
    let mut nbrs: Vec<_> = csr.neighbors(0).collect();
    nbrs.sort_by_key(|(t, _)| *t);
    assert_eq!(nbrs, vec![(1, RelKind::Calls), (2, RelKind::DataFlow)]);
    assert_eq!(csr.edge_len(), 3);
}

#[test]
#[ignore = "perf: cargo test -- --ignored"]
fn task_008_csr_scan_perf() {
    let csr = fixture_csr(1_000_000); // ~1M edges, built in-test
    let t0 = now_ns();
    let mut seen: u64 = 0;
    for n in 0..csr.node_len() {
        for _ in csr.neighbors(n) { seen += 1; }
    }
    let per_edge = (now_ns() - t0) as f64 / seen as f64;
    assert!(per_edge <= 20.0 * CI_TOLERANCE,
            "§15.4: ≤ 20 ns/edge × {CI_TOLERANCE}, measured {per_edge:.1} ns");
}
```

### TASK-009: Build the PDG (Ferrante–Ottenstein–Warren) with backward slicing

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-relations` |
| Depends on | TASK-006, TASK-007 |
| PRD reference | §7 Ferrante/Ottenstein/Warren row → `cie-relations`, §14.3, FR-2 |
| Parallel group | G-1c |

**Constraints:**
- Control-dependence edges are derived from `RelKind::ControlFlow` relations via the post-dominator tree (FOW 1987); data-dependence edges from `DataFlow`/`Reads`/`Writes` relations — def-use, not name equality alone.
- The PDG is strictly derived: `Pdg::build(&RelationGraph) -> Pdg` never mutates its input and records the input `rev` as provenance (FR-4, global rule 2).
- `backward_slice(SymbolId) -> BTreeSet<SymbolId>` is deterministic (sorted set) and a `backward_slice_bounded(sym, budget)` variant exists so slicing cannot loop forever (FR-7 worklists/budgets).
- Statements not dominated by any conditional still receive their (trivial) control dependence to the virtual entry node, per FOW.
- Node identity is `SymbolId` end to end — no re-mapping inside the PDG (FR-1).

**DoD:**
- `Pdg`, `Pdg::build`, `backward_slice`, `backward_slice_bounded` exported from `cie-relations`.
- Fixture reproduces the classic FOW shape: a use inside an `if` depends on both the defining assignment (data) and the condition (control).
- Slice of an unconditional statement contains only data-dependence ancestors.
- Bounded slice with budget 1 returns exactly the seed node.

The PDG connects actions to the conditions that guard them and the values they consume — that is what lets Stage 1/2 checkers explain *why* a finding exists (§7 algorithm mapping). Deriving it from the shared relation graph, rather than a private parse, keeps slicing consistent with clustering and behavior extraction (FR-2). Determinism and budget bounds make slices usable inside evidence: the same revision always yields the same slice (FR-4).

**Validation test sketch (contract):**

```rust
use cie_core::SymbolId;
use cie_relations::{Pdg, RelKind, RelationGraph};

fn node(n: u32) -> SymbolId { SymbolId(n) }

fn fow_fixture() -> RelationGraph {
    // 1: entry | 2: cond (x > 0) | 3: y := f(x), then-branch | 4: use(y) after join
    let mut g = RelationGraph::new();
    g.add_edge(node(1), node(2), RelKind::ControlFlow);
    g.add_edge(node(2), node(3), RelKind::ControlFlow);
    g.add_edge(node(3), node(4), RelKind::DataFlow);
    g.add_edge(node(2), node(4), RelKind::ControlFlow);
    g
}

#[test]
fn task_009_pdg_backward_slice_fow() {
    let pdg = Pdg::build(&fow_fixture());
    let slice = pdg.backward_slice(node(4));
    assert!(slice.contains(&node(3)), "data dependence: value flows 3 → 4");
    assert!(slice.contains(&node(2)), "control dependence: 4 guarded by condition 2");

    let bounded = pdg.backward_slice_bounded(node(4), 1);
    assert_eq!(bounded.len(), 1, "FR-7: budget caps slice work");
    assert!(bounded.contains(&node(4)));
}
```

### TASK-010: Assemble TypedFacts with dedup, integrity checks, and barrel re-exports

| Field | Value |
|---|---|
| Phase | 1 — Semantic Model & Typed Relations |
| Target crate(s) | `cie-semantic` (+ re-exports of `cie-relations`) |
| Depends on | TASK-005, TASK-006, TASK-007 |
| PRD reference | §9 `AnalysisInput.facts`, §14.2–14.3, FR-2, FR-3 |
| Parallel group | G-1d |

**Constraints:**
- `TypedFacts { symbols, calls, effects, exceptions, asyncs, relations }` aggregates every §14.2 fact family plus the `RelationGraph` — it is the exact `facts` field type of §9 `AnalysisInput`.
- `dedup()` collapses structurally identical facts deterministically (stable order by `(loc, id)`) and is idempotent.
- `check_integrity()` rejects facts referencing `SymbolId`s/`OperationId`s absent from `symbols`; integrity problems are `CieError::DanglingReference`, never silent edges (global rule 3 spirit).
- Barrel: `cie_semantic::prelude` re-exports `cie-core` identity/time types and the `cie-relations` graph/PDG types so Phase 2 adapters import one namespace.
- No language-specific types may appear in `TypedFacts` (FR-3): per-language judgment stays in adapters.

**DoD:**
- `TypedFacts` with push/build/dedup/integrity APIs compiles and is the type used by `AnalysisInput.facts`.
- Dedup test: duplicate `SymbolFact`/`CallFact` collapse; order stable across runs.
- Integrity test: dangling `callee` `SymbolId` → `Err(CieError::DanglingReference { .. })`.
- `use cie_semantic::prelude::*;` alone compiles a test touching facts + graph + PDG.

`TypedFacts` is the payload of every analysis run (§9), so its dedup and integrity rules determine whether downstream stages can trust their inputs (FR-2). Deterministic ordering makes evidence reproducible: same revision in, same facts out, byte for byte (FR-4). The prelude keeps the adapter contract of TASK-011 to a single import line.

**Validation test sketch (contract):**

```rust
use cie_semantic::prelude::*;
use cie_core::CieError;

#[test]
fn task_010_typed_facts_dedup_and_integrity() {
    let mut facts = TypedFacts::default();
    facts.push_symbol(sym_fact(1, SymbolKind::Function));
    facts.push_symbol(sym_fact(1, SymbolKind::Function)); // exact duplicate
    facts.push_call(call_fact(1, 2, CallKind::Dynamic, true));
    facts.push_call(call_fact(1, 2, CallKind::Dynamic, true)); // duplicate

    facts.dedup();
    assert_eq!(facts.symbols.len(), 1, "deterministic dedup");
    assert_eq!(facts.calls.len(), 1);
    facts.dedup();
    assert_eq!(facts.symbols.len(), 1, "dedup is idempotent");

    facts.push_call(call_fact(1, 99, CallKind::Direct, false));
    assert!(matches!(facts.check_integrity(), Err(CieError::DanglingReference { .. })),
            "dangling ids are errors, not silent edges");

    let snapshot = facts.clone();
    facts.dedup();
    assert_eq!(facts, snapshot, "same revision ⇒ byte-stable facts (FR-4)");
}
```

## Phase 2 — Language Adapter Framework & Top-10 Languages

Phase 2 delivers the LanguageAdapter framework (§16.1–16.2) and one adapter per v1 language (FR-17). Every adapter crate follows one contract — tree-sitter grammar, the eight §16.1 `extract_*` methods, explicit semantics tables (dispatch, numeric, exception, async, framework conventions), and unresolved-call marking — so the engine cannot tell adapters apart except through their vouched semantics (FR-3). The ten adapter tasks are mutually parallel (groups G-2a…G-2j); TASK-022 closes the phase with a cross-language guard proving a rule cannot fire where a language's semantics vouch against it.

### TASK-011: Define the LanguageAdapter trait, semantics tables, and LanguageRegistry

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapters` |
| Depends on | TASK-010 |
| PRD reference | §16.1, §16.2, FR-3, FR-17, FR-18 |
| Parallel group | G-2-0 |

**Constraints:**
- The trait exposes exactly the §16.1 surface: `id`, `extensions`, `tree_sitter_language`, `extract_symbols/calls/control_flow/data_flow/reads_writes/exceptions/async`, `dispatch_kind`, `numeric_semantics`, `framework_conventions`; a provided `parse(&[u8]) -> Tree` default wraps the grammar.
- Semantics tables are data on the adapter: `DispatchKind { Static, Dynamic, Hybrid }`, `NumericSemantics { overflow, coercion, precision }` (§16.1 comment), `&'static [FrameworkRule]` — rules consult these tables instead of hardcoding language knowledge (FR-3, global rule 6).
- `LanguageRegistry::register(Box<dyn LanguageAdapter>)` is the single registration line (§16.2); lookup helpers `all()`, `by_id()`, `by_extension()`; adapters are `Send + Sync` for rayon-parallel ingestion (§10).
- Extension detection maps file suffixes → adapter deterministically; an unknown suffix is reported, never silently ignored (FR-4).
- Adding a language must cost ≤ 1 new crate + ≤ 1 `register()` line + 0 changes to `cie-core`/`cie-semantic`/`cie-relations` (FR-18) — enforced by the contract test below.

**DoD:**
- `cie-adapters` compiles with the trait, `DispatchKind`, `NumericSemantics`, `OverflowPolicy`, `CoercionPolicy`, `Precision`, `FrameworkRule`, `LanguageRegistry`.
- `task_011_registry_extension_cost_fr18` passes: a `TestLangAdapter` defined only inside the test file registers with one line and no core edits.
- `by_extension("tl")` resolves the test adapter after registration.
- Trait docs enumerate the ten v1 adapters that must implement it (TASK-012…021).

This task is the hinge of the multi-language story: the engine upstream and the checkers downstream see only this trait, so a new language can never require touching them (FR-18). Making semantics tables explicit adapter data is the mechanism behind FR-3 — the TASK-022 guard reads these tables to refuse rules a language's semantics vouch against. The registry is shared across rayon workers, hence the `Send + Sync` bound.

**Validation test sketch (contract):**

```rust
use cie_adapters::{DispatchKind, LanguageAdapter, LanguageId, LanguageRegistry, NumericSemantics};

struct TestLangAdapter; // a hypothetical 11th language, defined ONLY in this test file (FR-18)

impl LanguageAdapter for TestLangAdapter {
    fn id(&self) -> LanguageId { LanguageId::UNKNOWN }
    fn extensions(&self) -> &'static [&'static str] { &["tl"] }
    fn tree_sitter_language(&self) -> tree_sitter::Language { unimplemented!("11th grammar") }
    // extract_symbols/calls/control_flow/data_flow/reads_writes/exceptions/async:
    // the seven §16.1 methods, each a stub returning Vec::new() for this contract test.
    fn dispatch_kind(&self) -> DispatchKind { DispatchKind::Hybrid }
    fn numeric_semantics(&self) -> NumericSemantics { NumericSemantics::default() }
    fn framework_conventions(&self) -> &'static [cie_adapters::FrameworkRule] { &[] }
}

#[test]
fn task_011_registry_extension_cost_fr18() {
    let mut registry = LanguageRegistry::new();
    registry.register(Box::new(TestLangAdapter)); // THE one registration line (§16.2)
    assert_eq!(registry.by_extension("tl").map(|a| a.id()), Some(LanguageId::UNKNOWN));
    assert_eq!(registry.all().len(), 1, "≤ 1 crate + ≤ 1 registry line + 0 core changes (FR-18)");
    // No imports from cie-core/cie-semantic/cie-relations beyond the prelude: 0 core edits.
}
```

### TASK-012: Implement the Python adapter (dynamic dispatch, async/await, bare-except exceptions)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-python` |
| Depends on | TASK-011 |
| PRD reference | §16 #1, §16.1, FR-17, FR-3 |
| Parallel group | G-2a |

**Constraints:**
- Grammar: `tree-sitter-python` (workspace dep); pipeline is parse → the eight §16.1 `extract_*` methods feeding `TypedFacts` via `cie_semantic::prelude`.
- Semantics tables per §16 #1: `dispatch_kind() = Dynamic` (dynamic dispatch, `getattr`, monkey-patching); numeric = arbitrary-precision `int` (no overflow), IEEE-754 f64 floats; exceptions = `raise`/`try`/`except`/`finally`, bare `except:` swallows; async = `async def`/`await`, `asyncio.create_task` → `AsyncKind::Spawn`, queues → `AsyncKind::Channel`; conventions = decorators, context managers, dunder protocols.
- Every cross-module, attribute-based, or `*`-imported call is emitted with `unresolved: true` and `CallKind::Dynamic` — never dropped, never guessed (FR-4, global rule 6).
- Calls resolvable to a symbol in the same tree may be `CallKind::Direct` with `unresolved: false`; the adapter only vouches for what it can prove in-tree.
- Constructs the adapter cannot model (metaclasses, C extensions) go to `unsupported` in evidence, not silent omission (FR-4); fixture tests live under `fixtures/*.py`.

**DoD:**
- `PythonAdapter` implements all §16.1 methods; registers via one `registry.register(PythonAdapter::new())` line.
- Fixture: `auth.authorize(u, t)` yields a dynamic, unresolved call fact.
- Fixture: bare `except: pass` yields an `ExceptionFact` with `handler: None`.
- Fixture: `async def … await g()` yields `AsyncKind::{Async, Await}` facts.
- `numeric_semantics().overflow == OverflowPolicy::ArbitraryPrecision`.

Python is the first adapter and the reference implementation of the contract: its dynamic dispatch means almost every interesting call is `unresolved: true`, which forces the honesty the rest of the system depends on (FR-4). Its semantics tables — arbitrary-precision ints, swallow-friendly exception model — are exactly what TASK-022's guard reads to refuse rules that assume fixed-width or non-swallowing behavior (FR-3). Extracting generators and `async` as first-class facts gives Stage 5 liveness checks visible suspension points (§16 #1).

**Validation test sketch (contract):**

```rust
use cie_adapter_python::PythonAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_semantic::prelude::*;

// `callee_name()` below: test-support accessor resolving a CallFact's callee via the StrInterner.
#[test]
fn task_012_python_adapter_facts() {
    let adapter = PythonAdapter::new();
    assert_eq!(adapter.id(), LanguageId::PYTHON);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #1");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::ArbitraryPrecision));
    assert!(adapter.extensions().contains(&"py"));

    let src = b"\
import auth
def login(user, token):
    return auth.authorize(user, token)   # dynamic, cross-module
def broken(user):
    try:
        return login(user, None)
    except:                               # bare except swallows
        pass
async def refresh(user):
    await login(user, 't')
";
    let tree = adapter.parse(src);
    assert!(adapter.extract_symbols(&tree, src).iter()
        .any(|s| matches!(s.kind, SymbolKind::Function)));
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("authorize")).unwrap();
    assert!(authorize.unresolved && matches!(authorize.kind, CallKind::Dynamic));
    assert!(adapter.extract_exceptions(&tree, src).into_iter()
        .any(|e| e.handler.is_none()), "bare except yields an unhandled ExceptionFact");
    assert!(adapter.extract_async(&tree, src).iter()
        .any(|a| matches!(a.kind, AsyncKind::Await)));
}
```

### TASK-013: Implement the JavaScript adapter (prototype chain, promises, closures, this-binding)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-javascript` |
| Depends on | TASK-011 |
| PRD reference | §16 #2, §16.1, FR-17, FR-3 |
| Parallel group | G-2b |

**Constraints:**
- Grammar: `tree-sitter-javascript`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #2: `dispatch_kind() = Dynamic` (prototype chain, `this` binding); numeric = all numbers are IEEE-754 doubles (`Precision::F64Only`) with implicit numeric coercion; exceptions = `throw`/`try`/`catch`/`finally` plus unhandled promise rejections; async = `Promise`/`then` → `AsyncKind::Async`, `await`, timer/queue callbacks → `Spawn`/`Channel`; conventions = closures, prototype methods, CommonJS/ESM imports.
- Method calls through objects, prototypes, or dynamic property access are `CallKind::Dynamic` + `unresolved: true` (FR-4); `this`-based calls are never resolved statically.
- Rejections without a `.catch`/`catch` handler produce exception facts with `handler: None` — swallow patterns stay visible to Stage 2 (FR-12 groundwork, global rule 6).
- `eval`, `with`, and dynamic `import()` land in `unsupported`/unresolved evidence, not silent omission (FR-4).

**DoD:**
- `JavaScriptAdapter` implements all §16.1 methods; one-line registration.
- Fixture: prototype method call yields `CallKind::Dynamic` + `unresolved: true`.
- Fixture: `try/catch` around `await` yields a handled `ExceptionFact` and an `AsyncKind::Await` fact.
- `numeric_semantics().precision == Precision::F64Only` (§16 #2: every number is a double).

JavaScript is the purest stress test of unresolved marking: prototype dispatch means the callee is rarely knowable at parse time, so the adapter must vouch only for the call site existing, not for what it calls (FR-4). Its all-doubles numeric table is what lets the TASK-022 guard refuse integer-overflow rules here — there is no integer overflow to refuse (FR-3). Promise rejections extracted as exception facts make "exceptions converted to success" (§13 #5) detectable in JS codebases.

**Validation test sketch (contract):**

```rust
use cie_adapter_javascript::JavaScriptAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, Precision};
use cie_semantic::prelude::*;

#[test]
fn task_013_javascript_adapter_facts() {
    let adapter = JavaScriptAdapter::new();
    assert_eq!(adapter.id(), LanguageId::JAVASCRIPT);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #2: prototype chain");
    assert!(matches!(adapter.numeric_semantics().precision, Precision::F64Only));

    let src = b"\
class Auth { authorize(u) { return this.check(u) && fetch('/ok').then(r => r.ok); } }
const a = new Auth(); a.authorize('u');
async function login(u) { try { return await a.authorize(u); } catch (e) { return true; } }
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    assert!(calls.iter().any(|c| c.unresolved && matches!(c.kind, CallKind::Dynamic)),
            "prototype/this dispatch is dynamic and unresolved (FR-4)");
    assert!(adapter.extract_exceptions(&tree, src).iter().any(|e| e.handler.is_some()));
    assert!(adapter.extract_async(&tree, src).iter()
        .any(|a| matches!(a.kind, AsyncKind::Await)));
}
```

### TASK-014: Implement the TypeScript adapter (structural typing, generics, decorators)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-typescript` |
| Depends on | TASK-011 |
| PRD reference | §16 #3, §16.1, FR-17, FR-3 |
| Parallel group | G-2c |

**Constraints:**
- Grammar: `tree-sitter-typescript`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #3: `dispatch_kind() = Hybrid` — the static type system exists, but types are erased at runtime, so interface/virtual calls stay dynamic; numeric = f64 plus `BigInt` as a distinct fact-worthy type; exceptions and async inherit the JavaScript model (§16 #2) but are re-declared in this crate (crate boundary, no cross-crate leakage of FR-3 tables).
- Calls through interface-typed or generic values are `CallKind::Virtual` + `unresolved: true`: the runtime target is unknown after erasure (FR-4).
- Decorators, `implements`, and declaration merging are `FrameworkRule` entries so framework behavior is vouched data, not folklore (FR-3).
- `any`, type assertions, and `@ts-ignore` sites are recorded as assumptions in evidence (FR-4).

**DoD:**
- `TypeScriptAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `interface Authorizer` is a `SymbolKind::Interface` symbol.
- Fixture: call via an interface-typed parameter yields `CallKind::Virtual` + `unresolved: true`.
- `framework_conventions()` is non-empty and includes a decorator rule (§16 #3).

TypeScript shows why dispatch is a table and not a global constant: the language is statically typed yet erasure makes runtime dispatch dynamic, so the adapter vouches for `Hybrid` and marks interface calls unresolved (FR-4). Re-declaring the exception/async tables in this crate keeps FR-3's isolation mechanical — sharing a table would be a hidden core change (FR-18). Decorators as framework rules let later stages reason about injected behavior without parsing transpiled output.

**Validation test sketch (contract):**

```rust
use cie_adapter_typescript::TypeScriptAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter};
use cie_semantic::prelude::*;

#[test]
fn task_014_typescript_adapter_facts() {
    let adapter = TypeScriptAdapter::new();
    assert_eq!(adapter.id(), LanguageId::TYPESCRIPT);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Hybrid, "§16 #3: types erased at runtime");

    let src = b"\
interface Authorizer { authorize(u: string): boolean; }
function login(a: Authorizer, u: string): boolean { return a.authorize(u); }
@(log) class SsoAuth implements Authorizer { authorize(u: string) { return true; } }
";
    let tree = adapter.parse(src);
    assert!(adapter.extract_symbols(&tree, src).iter()
        .any(|s| matches!(s.kind, SymbolKind::Interface)), "interfaces are symbols");
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("authorize")).unwrap();
    assert!(authorize.unresolved && matches!(authorize.kind, CallKind::Virtual),
            "interface-typed call is virtual & unresolved: runtime type unknown after erasure");
    assert!(!adapter.framework_conventions().is_empty(), "decorators are framework rules");
}
```

### TASK-015: Implement the Java adapter (JVM dispatch, checked exceptions, annotations)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-java` |
| Depends on | TASK-011 |
| PRD reference | §16 #4, §16.1, FR-17, FR-3 |
| Parallel group | G-2d |

**Constraints:**
- Grammar: `tree-sitter-java`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #4: `dispatch_kind() = Dynamic` — JVM virtual dispatch for overridable instance methods (`CallKind::Virtual`), while `static`/`final`/`private` calls are `CallKind::Direct`; numeric = fixed-width two's-complement wraparound for `int`/`long` (`OverflowPolicy::WrapAround`), f32/f64 floats; exceptions = `throw`/`try`/`catch`/`finally` with `throws` clauses extracted as `ExceptionKind::Raised` facts; async = `Thread`/`ExecutorService`/`CompletableFuture` → `Spawn`; conventions = annotations as `FrameworkRule`s.
- Reflection (`Method.invoke`, `Class.forName`) is `CallKind::Reflective` + `unresolved: true` (FR-4); the adapter never pretends reflection is resolvable.
- Checked exceptions from `throws` clauses propagate into `ExceptionFact` even when no call site throws them, so interface contracts stay visible downstream (FR-2).
- Annotations (`@Override`, `@Transactional`, …) are recorded as framework conventions, never as facts about behavior (FR-3).

**DoD:**
- `JavaAdapter` implements all §16.1 methods; one-line registration.
- Fixture: call on an overridable instance method yields `CallKind::Virtual`.
- Fixture: `throws IOException` + `catch` yields `ExceptionKind::{Raised, Caught}` facts.
- `numeric_semantics().overflow == OverflowPolicy::WrapAround` (JLS-defined wrap).

Java's value to the suite is the contrast it forces: a language with a real virtual-dispatch machine and a checked-exception contract. Marking overridable calls `Virtual` (resolvable signature, unknown runtime target) refines the naive dynamic/everything model and feeds Stage 2's "rejected authorization ignored" checks (§13 #4). Extracting `throws` clauses as raised-exception facts keeps contract information that exists only in signatures, not bodies (FR-2).

**Validation test sketch (contract):**

```rust
use cie_adapter_java::JavaAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_semantic::prelude::*;

#[test]
fn task_015_java_adapter_facts() {
    let adapter = JavaAdapter::new();
    assert_eq!(adapter.id(), LanguageId::JAVA);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #4: JVM virtual dispatch");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::WrapAround));

    let src = b"\
class Auth { boolean authorize(User u) throws IOException { return u != null; } }
class Login { void run(Auth auth, User u) { try { auth.authorize(u); } catch (IOException e) {} } }
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("authorize")).unwrap();
    assert!(matches!(authorize.kind, CallKind::Virtual), "overridable instance method ⇒ Virtual");

    let excs = adapter.extract_exceptions(&tree, src);
    assert!(excs.iter().any(|e| matches!(e.kind, ExceptionKind::Caught) && e.handler.is_some()),
            "checked exceptions appear as ExceptionFacts with handlers");
    assert!(adapter.framework_conventions().iter().any(|r| r.applies_to_annotations()),
            "@annotations are framework conventions (§16 #4)");
}
```

### TASK-016: Implement the C adapter (pointers, UB, manual memory, preprocessor)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-c` |
| Depends on | TASK-011 |
| PRD reference | §16 #5, §16.1, FR-17, FR-3 |
| Parallel group | G-2e |

**Constraints:**
- Grammar: `tree-sitter-c`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #5: `dispatch_kind() = Static` for direct calls, function-pointer calls are `CallKind::Dynamic`; numeric = fixed-width with `OverflowPolicy::UndefinedBehavior` for signed overflow plus integer promotions — UB sites are recorded as assumptions, never silently assumed away (FR-4, §3 non-goals).
- Exceptions: C has none — error-code conventions (`-1`/`NULL`/`errno`) are `FrameworkRule`s and produce `ExceptionKind::ErrorValue` facts only where the convention is vouched by the table; never `ExceptionKind::Raised` (FR-3).
- `malloc`/`free`/`realloc` produce `Owns` relations and external `EffectFact`s so leak/double-free reasoning has raw material (§14.3, FR-2).
- Preprocessor: macro expansions record expansion boundaries as assumptions; `#include`-ed behavior stays `unsupported` in evidence (FR-4).

**DoD:**
- `CAdapter` implements all §16.1 methods; one-line registration.
- Fixture: direct call yields `CallKind::Direct`; function-pointer call yields `CallKind::Dynamic` + `unresolved: true`.
- Fixture: `malloc` yields an external effect + `Owns` relation input.
- No `ExceptionKind::Raised` is ever emitted; `numeric_semantics().overflow == OverflowPolicy::UndefinedBehavior`.

C is the adapter that punishes optimistic rules hardest: UB semantics mean the engine must record assumptions instead of trusting arithmetic (FR-4, global rule 2). Error codes modeled as table-vouched `ErrorValue` facts — never `Raised` — are the canonical FR-3 case the TASK-022 guard checks: a rule about thrown exceptions must not fire here. Making `malloc`/`free` first-class `Owns` relations feeds the recovery/failure stages with real ownership structure (§14.3).

**Validation test sketch (contract):**

```rust
use cie_adapter_c::CAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_relations::RelKind;
use cie_semantic::prelude::*;

#[test]
fn task_016_c_adapter_facts() {
    let adapter = CAdapter::new();
    assert_eq!(adapter.id(), LanguageId::C);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Static, "§16 #5: direct calls are static");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::UndefinedBehavior));

    let src = b"\
int db_check(int user);
int auth(int user) { return db_check(user); }
int wrap(void) { int (*handler)(int) = &auth; return handler(7); }
void *alloc_buf(void) { return malloc(64); }
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    assert!(calls.iter().any(|c| matches!(c.kind, CallKind::Direct)));
    assert!(calls.iter().any(|c| matches!(c.kind, CallKind::Dynamic) && c.unresolved),
            "function-pointer call ⇒ Dynamic + unresolved (FR-4)");
    assert!(adapter.extract_reads_writes(&tree, src).iter().any(|e| e.external),
            "malloc is an external effect");
    assert!(!adapter.extract_exceptions(&tree, src).iter()
        .any(|e| matches!(e.kind, ExceptionKind::Raised)),
        "C never raises — error codes only, table-vouched (FR-3)");
    assert!(adapter.control_flow_relations(&tree, src).iter().any(|r| *r == RelKind::Owns)
        || adapter.extract_reads_writes(&tree, src).iter().count() > 0);
}
```

### TASK-017: Implement the C++ adapter (templates, RAII, overload resolution, exceptions)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-cpp` |
| Depends on | TASK-011 |
| PRD reference | §16 #6, §16.1, FR-17, FR-3 |
| Parallel group | G-2f |

**Constraints:**
- Grammar: `tree-sitter-cpp`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #6: `dispatch_kind() = Hybrid` — overload resolution is static, `virtual` calls through base references/pointers are `CallKind::Dynamic`; numeric = fixed-width with `OverflowPolicy::UndefinedBehavior` (as C) plus user-defined operators recorded as convention-dependent; exceptions = `throw`/`try`/`catch`, with `noexcept` recorded as an assumption; async = `std::thread`/`std::async` → `AsyncKind::Spawn`.
- Templates: calls with dependent names stay `unresolved: true` until instantiation; uninstantiated templates are recorded as assumptions, not guesses (FR-4).
- RAII: destructors produce implicit-call facts and `Owns` relations so cleanup paths are visible to recovery checks (§14.3, §16 #6).
- UB (signed overflow, invalid casts) is recorded as assumptions per FR-4; the adapter never vouches for UB-defined behavior (global rule 2).

**DoD:**
- `CppAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `virtual` call through a base reference yields `CallKind::Dynamic` + `unresolved: true`.
- Fixture: template-dependent call stays unresolved.
- `numeric_semantics().overflow == OverflowPolicy::UndefinedBehavior`; no assumption-free UB facts.

C++ extends C's honesty requirements with two new hard cases: dependent template names and implicit destructor calls. Treating destructors as real facts (with `Owns` backing) is what later lets the engine see that "cleanup ran" — the §13 #7 deletion/invalidation family needs this. The Hybrid dispatch table — static overloads, dynamic virtuals — is a per-language vouch that a universal dispatcher would get wrong (FR-3).

**Validation test sketch (contract):**

```rust
use cie_adapter_cpp::CppAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_semantic::prelude::*;

#[test]
fn task_017_cpp_adapter_facts() {
    let adapter = CppAdapter::new();
    assert_eq!(adapter.id(), LanguageId::CPP);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Hybrid, "§16 #6: overloads static, virtuals dynamic");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::UndefinedBehavior));

    let src = b"\
struct Base { virtual bool authorize(User& u) = 0; virtual ~Base(); };
template <class T> bool gate(T u) { return T::authorize(u); }
bool check(Base& b, User& u) { return b.authorize(u); }
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    assert!(calls.iter().any(|c| matches!(c.kind, CallKind::Dynamic) && c.unresolved),
            "virtual through base reference ⇒ Dynamic + unresolved (§16 #6)");
    assert!(calls.iter().filter(|c| c.unresolved).count() >= 2,
            "template-dependent calls stay unresolved until instantiation (FR-4)");
    assert!(adapter.extract_symbols(&tree, src).iter()
        .any(|s| matches!(s.kind, SymbolKind::Method) || matches!(s.kind, SymbolKind::Struct)));
}
```

### TASK-018: Implement the C# adapter (CLR dispatch, LINQ, async/await, attributes)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-csharp` |
| Depends on | TASK-011 |
| PRD reference | §16 #7, §16.1, FR-17, FR-3 |
| Parallel group | G-2g |

**Constraints:**
- Grammar: `tree-sitter-c-sharp`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #7: `dispatch_kind() = Dynamic` — CLR virtual/interface dispatch (`CallKind::Virtual` for interface-typed and `virtual` calls, `Direct` for sealed/static); numeric = fixed-width with context-dependent overflow — default unchecked wrap, `checked{}` blocks throw `OverflowException` and therefore emit `ExceptionFact`s; exceptions = `throw`/`try`/`catch`/`finally`; async = `async`/`await` → `AsyncKind::{Async, Await}`, `Task.Run` → `Spawn`, `Channel<T>` → `Channel`; conventions = attributes and LINQ providers as `FrameworkRule`s.
- LINQ deferred execution is recorded as a framework-rule assumption: a query expression is not an executed effect (FR-4).
- Catch-and-return patterns (`catch { return true; }`) produce handled facts with visible handlers so Stage 2's swallowed-exception checks fire (§13 #5, FR-12).
- `dynamic`-typed calls are `CallKind::Reflective` + `unresolved: true` (FR-4).

**DoD:**
- `CSharpAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `await auth.AuthorizeAsync(u)` on an interface-typed value yields `CallKind::Virtual` + `unresolved: true` and an `AsyncKind::Await` fact.
- Fixture: `catch (Exception) { return true; }` yields a handled exception fact.
- Semantics tables expose `Channel<T>` → `AsyncKind::Channel` and attribute-based framework rules.

C# matters to the suite because its async model and its overflow model are both context-dependent: `checked{}` changes arithmetic into an exception, which only a per-language table can express (FR-3). Extracting LINQ as deferred, vouched-by-convention behavior prevents "query built" from being misread as "query executed" (FR-4). Channel/Task facts give Stage 5's recovery checks the same vocabulary Go's goroutines use (§16 #7 vs #8).

**Validation test sketch (contract):**

```rust
use cie_adapter_csharp::CSharpAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter};
use cie_semantic::prelude::*;

#[test]
fn task_018_csharp_adapter_facts() {
    let adapter = CSharpAdapter::new();
    assert_eq!(adapter.id(), LanguageId::CSHARP);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #7: CLR dispatch");

    let src = b"\
async Task<bool> Login(IAuthorizer auth, User u) {
    try { return await auth.AuthorizeAsync(u); }
    catch (Exception) { return true; }   // swallowed
}
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("AuthorizeAsync")).unwrap();
    assert!(matches!(authorize.kind, CallKind::Virtual) && authorize.unresolved,
            "interface-typed call: CLR dispatch, runtime target unknown");

    assert!(adapter.extract_async(&tree, src).iter()
        .any(|a| matches!(a.kind, AsyncKind::Await)));
    assert!(adapter.extract_exceptions(&tree, src).into_iter()
        .any(|e| matches!(e.kind, ExceptionKind::Caught) && e.handler.is_some()),
        "catch-and-return-true is visible for Stage 2 (FR-12)");
    assert!(adapter.framework_conventions().iter()
        .any(|r| r.covers_linq_or_attributes()), "attributes/LINQ are conventions (§16 #7)");
}
```

### TASK-019: Implement the Go adapter (goroutines, channels, interfaces, error values)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-go` |
| Depends on | TASK-011 |
| PRD reference | §16 #8, §16.1, FR-17, FR-3 |
| Parallel group | G-2h |

**Constraints:**
- Grammar: `tree-sitter-go`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #8: `dispatch_kind() = Dynamic` — interface-typed calls dispatch via runtime itables (`CallKind::Dynamic`), concrete-type calls are `Direct`; numeric = fixed-width with **defined** wraparound (`OverflowPolicy::WrapAround`, never UB); errors are values: `if err != nil` patterns produce `ExceptionKind::ErrorValue` facts, `panic`/`defer`/`recover` produce `ExceptionKind::Panic` facts; async = `go f()` → `AsyncKind::Spawn`, `chan`/`select` → `AsyncKind::Channel`.
- The adapter never emits `ExceptionKind::Raised` — Go has no throw; conflating error values with exceptions would leak foreign semantics (FR-3, global rule 6).
- Interface satisfaction is implicit, so calls on interface values stay `unresolved: true` until a type is known (FR-4).
- A returned `error` with no checked `err != nil` at the call site is still just an `ErrorValue` fact — the adapter reports structure, downstream stages judge handling (FR-2).

**DoD:**
- `GoAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `a.Authorize(u)` on an interface-typed value yields `CallKind::Dynamic` + `unresolved: true`.
- Fixture: `go refresh(ch)` yields `AsyncKind::Spawn`; `ch <- v`/`select` yields `AsyncKind::Channel`.
- `numeric_semantics().overflow == OverflowPolicy::WrapAround`; zero `ExceptionKind::Raised` facts.

Go is the canonical refutation case for FR-3: a call that "looks like" the Python/Java `authorize()` pattern returns an error value instead of throwing, so a rule claiming *call ⇒ success established* must be refused here. That refusal — driven by the adapter's tables, not by hardcoded language lists — is exactly what TASK-022's guard test asserts. Defined wraparound (vs C's UB) is the second table value the guard can distinguish (FR-3, §16 #8).

**Validation test sketch (contract):**

```rust
use cie_adapter_go::GoAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_semantic::prelude::*;

#[test]
fn task_019_go_adapter_facts() {
    let adapter = GoAdapter::new();
    assert_eq!(adapter.id(), LanguageId::GO);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #8: interface itable dispatch");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::WrapAround));

    let src = b"\
func login(a Authorizer, u User, ch chan Token) error {
    if err := a.Authorize(u); err != nil { return err }   // error value, not throw
    go refresh(ch)                                        // goroutine
    return nil
}
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("Authorize")).unwrap();
    assert!(matches!(authorize.kind, CallKind::Dynamic) && authorize.unresolved,
            "interface-typed call ⇒ itable dispatch, unresolved at extraction");

    assert!(adapter.extract_exceptions(&tree, src).iter()
        .any(|e| matches!(e.kind, ExceptionKind::ErrorValue)),
        "Go errors are values (§16 #8), never ExceptionKind::Raised (FR-3)");
    let asyncs = adapter.extract_async(&tree, src);
    assert!(asyncs.iter().any(|a| matches!(a.kind, AsyncKind::Spawn)));
    assert!(asyncs.iter().any(|a| matches!(a.kind, AsyncKind::Channel)));
}
```

### TASK-020: Implement the Rust adapter (ownership, traits, Result, async, macros)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-rust` |
| Depends on | TASK-011 |
| PRD reference | §16 #9, §16.1, FR-17, FR-3 |
| Parallel group | G-2i |

**Constraints:**
- Grammar: `tree-sitter-rust`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #9: `dispatch_kind() = Hybrid` — inherent/impl calls are `CallKind::Direct`, `dyn Trait` and generic-bound calls are `CallKind::Dynamic`; numeric = `OverflowPolicy::PanicInDebug` (debug builds panic on overflow; release wrap is recorded as an assumption, FR-4); errors are values: `Result`/`?` propagation → `ExceptionKind::ErrorValue`, `panic!`/`unwrap` → `ExceptionKind::Panic`; async = `async fn`/`await` → `AsyncKind::{Async, Await}`, `tokio::spawn` → `Spawn`, `mpsc` → `Channel`; ownership/borrows feed `Owns`, `Reads`, `Writes` relations.
- The adapter never emits `ExceptionKind::Raised` — Rust has no exceptions; `?` is propagation of a value (FR-3, global rule 6).
- Proc-macro output and `unsafe` blocks are recorded as assumptions/`unsupported` in evidence — the adapter never invents macro-expanded behavior (FR-4, global rule 2).
- Trait-object calls stay `unresolved: true` unless the concrete type is visible in-tree (FR-4).

**DoD:**
- `RustAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `a.authorize(u)?` on `&dyn Authorizer` yields `CallKind::Dynamic` + `unresolved: true` plus an `ErrorValue` fact for `?`.
- Fixture: ownership move produces an `Owns` relation input; borrows produce read effects.
- `numeric_semantics().overflow == OverflowPolicy::PanicInDebug`; framework conventions cover macros.

Rust mirrors Go as a refutation case but through a different mechanism: `?` propagation means the seed call's failure path is a value in the type system, so "call ⇒ established" cannot fire without checking the `Result` (FR-3). Ownership gives the relation graph something no other adapter has — precise `Owns` structure — which the recovery stages use to distinguish "refund called" from "money returned" (FR-15 caution, §5.1). Recording macro boundaries as assumptions keeps the engine honest about generated code (FR-4).

**Validation test sketch (contract):**

```rust
use cie_adapter_rust::RustAdapter;
use cie_adapters::{DispatchKind, LanguageAdapter, OverflowPolicy};
use cie_semantic::prelude::*;

#[test]
fn task_020_rust_adapter_facts() {
    let adapter = RustAdapter::new();
    assert_eq!(adapter.id(), LanguageId::RUST);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Hybrid, "§16 #9: impl static, dyn Trait dynamic");
    assert!(matches!(adapter.numeric_semantics().overflow, OverflowPolicy::PanicInDebug));

    let src = b"\
fn login(a: &dyn Authorizer, u: User) -> Result<Token, Error> {
    let t = a.authorize(u)?;   // Result propagation, not exception
    Ok(t)
}
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("authorize")).unwrap();
    assert!(matches!(authorize.kind, CallKind::Dynamic) && authorize.unresolved,
            "&dyn Trait call ⇒ Dynamic + unresolved (§16 #9)");
    assert!(adapter.extract_exceptions(&tree, src).iter()
        .any(|e| matches!(e.kind, ExceptionKind::ErrorValue)),
        "`?` on Result is ErrorValue propagation (FR-3)");
    assert!(!adapter.extract_exceptions(&tree, src).iter()
        .any(|e| matches!(e.kind, ExceptionKind::Raised)), "Rust has no exceptions");
    assert!(adapter.framework_conventions().iter().any(|r| r.covers_macros()),
        "proc-macro output stays unsupported/unresolved, never invented (FR-4)");
}
```

### TASK-021: Implement the PHP adapter (dynamic types, magic methods, exceptions)

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapter-php` |
| Depends on | TASK-011 |
| PRD reference | §16 #10, §16.1, FR-17, FR-3 |
| Parallel group | G-2j |

**Constraints:**
- Grammar: `tree-sitter-php`; same parse → `extract_*` pipeline contract as TASK-012 (§16.1).
- Semantics tables per §16 #10: `dispatch_kind() = Dynamic` — dynamic types, trait reuse, and property-based method calls; numeric = `CoercionPolicy::IntToFloatOnOverflow` (ints promote to f64 on overflow) with f64 precision; exceptions = `throw`/`try`/`catch`/`finally` with the `Error` vs `Exception` hierarchy as two distinct raised families; async = no native v1 model — async extension calls are recorded as `unsupported` assumptions, never fabricated `AsyncFact`s (FR-4, §3 non-goals).
- Magic methods (`__call`, `__get`, `__invoke`) and `call_user_func` are `CallKind::Reflective` + `unresolved: true` (FR-4).
- Property-based method calls (`$auth->authorize($u)`) are `CallKind::Dynamic` + `unresolved: true` unless the class is visible in-tree (FR-4, global rule 6).
- Variable variables (`$$x`) and `extract()`-style dynamic scoping land in `unsupported` evidence, not silent omission (FR-4).

**DoD:**
- `PhpAdapter` implements all §16.1 methods; one-line registration.
- Fixture: `$auth->authorize($u)` yields `CallKind::Dynamic` + `unresolved: true`.
- Fixture: `__call`-based dispatch yields `CallKind::Reflective`.
- Fixture: `try { } catch (Exception $e) { return true; }` yields a handled `ExceptionKind::Caught` fact.
- `numeric_semantics().coercion == CoercionPolicy::IntToFloatOnOverflow`; no fabricated async facts.

PHP completes the ten-language matrix with the loosest type system, making it the final stress test of unresolved marking: nearly every interesting call is dynamic or reflective, and the adapter must record rather than resolve (FR-4). Its int→float overflow coercion is the fourth distinct numeric model in the suite, and the TASK-022 guard uses it to refuse fixed-width overflow rules here (FR-3). Refusing to fabricate async facts for an async-less v1 model demonstrates the `unsupported` path of the evidence invariant (FR-4).

**Validation test sketch (contract):**

```rust
use cie_adapter_php::PhpAdapter;
use cie_adapters::{CoercionPolicy, DispatchKind, LanguageAdapter};
use cie_semantic::prelude::*;

#[test]
fn task_021_php_adapter_facts() {
    let adapter = PhpAdapter::new();
    assert_eq!(adapter.id(), LanguageId::PHP);
    assert_eq!(adapter.dispatch_kind(), DispatchKind::Dynamic, "§16 #10: dynamic types, magic methods");
    assert!(matches!(adapter.numeric_semantics().coercion, CoercionPolicy::IntToFloatOnOverflow));

    let src = b"\
<?php
class Gateway {
    public function __call($name, $args) { return true; }   // magic dispatch
}
function login($auth, $u) {
    try { return $auth->authorize($u); }
    catch (Exception $e) { return true; }                    // swallowed
}
";
    let tree = adapter.parse(src);
    let calls = adapter.extract_calls(&tree, src);
    let authorize = calls.iter().find(|c| c.callee_name() == Some("authorize")).unwrap();
    assert!(authorize.unresolved && matches!(authorize.kind, CallKind::Dynamic),
            "property-based method call is dynamic + unresolved (FR-4)");
    assert!(calls.iter().any(|c| matches!(c.kind, CallKind::Reflective)),
            "__call / call_user_func ⇒ Reflective (§16 #10)");
    assert!(adapter.extract_exceptions(&tree, src).iter()
        .any(|e| matches!(e.kind, ExceptionKind::Caught) && e.handler.is_some()));
}
```

### TASK-022: Add the cross-language semantics guard proving FR-3 over all ten adapters

| Field | Value |
|---|---|
| Phase | 2 — Language Adapter Framework & Top-10 Languages |
| Target crate(s) | `cie-adapters` (+ integration test across the ten adapter crates) |
| Depends on | TASK-011–TASK-021 (framework + all ten adapters) |
| PRD reference | FR-3, §16, §5.1 (semantics adapters row), FR-4 |
| Parallel group | G-2-Z |

**Constraints:**
- `SemanticsGuard::evaluate(rule, &LanguageRegistry, &FixtureRepo) -> Vec<Admission>` is **matrix-driven**: it iterates `registry.all()` — no hardcoded language list — so adding an adapter automatically extends the matrix (FR-18).
- A candidate rule fires for a language only if that language's adapter tables vouch for it (dispatch model, numeric semantics, exception model, async model — §16.1); otherwise the admission records why with evidence (FR-3, global rule 6).
- Every refusal is materialized in the admission's `Evidence { assumptions, unresolved, … }` (§14.8, FR-4): refusals are recorded, never silent, and never converted into a clean verdict (global rule 3).
- The guard's fixture repo contains ten tiny files — one per language, all shaped like the PRD's `authorize()` example (§1) — so the same rule is evaluated over identical intent in ten semantic worlds.
- The guard lives in `cie-adapters` (framework level); it must not modify any adapter crate or core crate (FR-18 boundary).

**DoD:**
- `SemanticsGuard`, `Admission { lang, fired, evidence }` exported from `cie-adapters`.
- `task_022_cross_language_semantics_guard` passes: 10 admissions, one per registered adapter.
- The `call ⇒ authorization established` rule does NOT fire for Go (`ErrorValue` semantics) or Rust (`Result`/`?` semantics); every refusal carries evidence.
- Fixture repo with 10 files committed under `cie-adapters/tests/fixtures/`.
- `cargo test --workspace` green with this integration test included.

This task converts FR-3 from a slogan into a mechanical check: the same candidate rule meets the same intent in ten languages, and the guard admits or refuses strictly on adapter-vouched semantics. Refusing in Go and Rust — where error values and `Result` propagation mean a bare call establishes nothing — is the acceptance evidence that language semantics do not leak across languages (FR-3, §5.1). Because the matrix is registry-driven, the guard keeps working unchanged when v1.1 languages (Ruby, Kotlin, …) arrive (FR-18).

**Validation test sketch (contract):**

```rust
use cie_adapters::{LanguageId, LanguageRegistry, SemanticsGuard};
use cie_semantic::prelude::*;

fn v1_registry() -> LanguageRegistry {
    let mut r = LanguageRegistry::new();
    r.register(Box::new(cie_adapter_python::PythonAdapter::new()));
    r.register(Box::new(cie_adapter_go::GoAdapter::new()));
    r.register(Box::new(cie_adapter_rust::RustAdapter::new()));
    // … the remaining seven adapters, one registration line each (§16.2)
    r
}

#[test]
fn task_022_cross_language_semantics_guard() {
    let registry = v1_registry();
    assert_eq!(registry.all().len(), 10, "FR-17: the guard runs over all ten adapters");
    let repo = fixture_repo_10_files(); // one tiny `authorize`-shaped file per language

    // Candidate rule (Houdini-style, cf. §7): "a call to authorize establishes authorization".
    let rule = candidate_rule_authorize_established();
    let admissions = SemanticsGuard::evaluate(&rule, &registry, &repo);
    assert_eq!(admissions.len(), 10, "matrix-driven: one admission per registered adapter");

    let fired: Vec<_> = admissions.iter().filter(|a| a.fired).map(|a| a.lang).collect();
    assert!(!fired.contains(&LanguageId::GO),
            "FR-3: Go returns error values — a bare call does not establish success");
    assert!(!fired.contains(&LanguageId::RUST),
            "FR-3: Result/? semantics vouch against firing");
    for a in admissions.iter().filter(|a| !a.fired) {
        assert!(!a.evidence.assumptions.is_empty() || !a.evidence.unresolved.is_empty(),
                "FR-4: every refusal is recorded with evidence ({:?})", a.lang);
    }
}
```

## Phase 3 — Evidence, Storage & Revisioning

This phase delivers the provenance and persistence backbone that every later algorithm trusts. `cie-evidence` makes the FR-4 invariant — every derived result carries `Evidence{rev, provenance, assumptions, unsupported, unresolved, generated_at_ns}` (§14.8) — mechanically enforceable, while `cie-store` implements the §15.1 key namespaces, §15.2 time-indexed tables, and the §15.3 latency layers (L0 DashMap hot facts, L1 sled/redb KV) with blake3 content hashing and monotonic `RevId` allocation. Deletion tombstones and reverse-dependency cleanup close the loop so incremental maintenance (FR-8) can never serve stale facts.

---

### TASK-023: Build the evidence and coverage substrate

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-evidence` (+ `cie-core`) |
| Depends on | TASK-002, TASK-004 |
| PRD reference | FR-4 (§6); §14.8 Evidence & Coverage; §9 Shared Interface |
| Parallel group | G-3a |

**Constraints:**
- Define `Evidence { rev: RevId, provenance: SmallVec<[ProvenanceRef; 8]>, assumptions: SmallVec<[Assumption; 4]>, unsupported: SmallVec<[Unsupported; 4]>, unresolved: SmallVec<[SymbolId; 8]>, generated_at_ns: Ns }` field-for-field per §14.8, plus a `Coverage` type recording what was actually analyzed per `Scope`.
- FR-4 is enforced mechanically, not by convention: the only path to a concluded/frozen record is a typestate builder (`RecordBuilder<Open>` → `seal(evidence) -> ConcludedRecord`) or an equivalent runtime assert — constructing a concluded record without evidence must be impossible.
- `generated_at_ns: Ns` is u64 ns since epoch and durations are `NsDelta = i64`; no `chrono::DateTime` anywhere in this crate (FR-21).
- `ProvenanceRef` cites the producing analysis stage plus the input `Hash`es, so any finding can state exactly what was analyzed and what was not (FR-4 acceptance).
- Types are serde- and rkyv-serializable, `SmallVec`-bounded per §15.5, and land under the `evidence:{record_id}` namespace (§15.1).

**DoD:**
- `Evidence`, `Coverage`, `ProvenanceRef`, `Assumption`, `Unsupported` compile and round-trip through serde and rkyv.
- A test proves the no-evidence construction path does not exist (compile-fail case documented or runtime panic asserted).
- `Coverage { analyzed, total, scope }` embeds cleanly in the `DerivedRecord` shape consumed by §9/§14.9.
- `cargo test -p cie-evidence` green; `chrono` absent from the crate's dependency tree.

`cie-evidence` is the crate the rest of the engine leans on for honesty: FR-4 requires every result to carry revision, provenance, assumptions, unsupported constructs, and unresolved calls, and the §9 `AnalysisOutput.evidence` field bakes it into the shared interface. This task defines those types and, critically, the construction discipline — a builder that cannot be sealed without evidence removes the entire class of "clean answer with no receipts". It is a Phase-3 root with no algorithmic dependencies beyond core IDs and hashing, so it starts immediately.

**Validation test sketch (contract):**

```rust
#[test]
fn task_023_concluded_record_requires_evidence() {
    let rev: RevId = 41;
    let now: Ns = 1_700_000_000_000_000_000;

    // arrange: pending builder with provenance, no evidence yet
    let builder = DerivedRecordBuilder::open(RecordKind::Finding)
        .with_provenance(ProvenanceRef { stage: Stage::Effects,
                                         inputs: smallvec![blake3_hash(b"auth.py")] });

    // act + assert: sealing without evidence is unconstructable (FR-4)
    let attempt = std::panic::catch_unwind(|| builder.clone().seal_unchecked());
    assert!(attempt.is_err(), "concluded record without evidence must be impossible");

    // act: seal with full §14.8 evidence
    let evidence = Evidence {
        rev,
        provenance: builder.provenance().clone(),
        assumptions: smallvec![],
        unsupported: smallvec![Unsupported::Construct("dynamic dispatch")],
        unresolved: smallvec![SymbolId(9)],
        generated_at_ns: now,
    };
    let record = builder.seal(evidence);

    // assert: sealed record carries evidence + sane coverage
    assert_eq!(record.evidence().rev, rev);
    assert_eq!(record.evidence().generated_at_ns, now);
    assert!(record.coverage().analyzed <= record.coverage().total);
}
```

---

### TASK-024: Implement the cie-store KV layer with sled and a redb feature flag

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-store` |
| Depends on | TASK-002 |
| PRD reference | §15.1 Key Namespaces; §15.3 Storage Layers; FR-22 |
| Parallel group | G-3a |

**Constraints:**
- One `KvStore` trait with `sled` as the default backend and `redb` behind the `backend-redb` cargo feature; both implement identical semantics so the backend swap needs zero call-site changes (§10 lists sled/redb as the KV layer).
- Tree management follows §15.1 exactly — one tree per namespace group over `rev:`, `file:`, `sym:`, `sym_by_hash:`, `rel:csr:`, `sum:`, `contract:`, `concept:`, `hier:`, `label:`, `vec:`, `evidence:`, `record:`, `deps:`, `rev_deps:`, `doc:`, `claim:`, `claim_by_doc:`, `map:`, `verify:`, `drift:`, `drift_delta:`, `reverse:` (global rule 7).
- Numeric key components (`RevId`, `SymbolId`, `FileId`, `RecordId`) are big-endian encoded so lexicographic order equals numeric order; `Hash` keys are raw 32-byte blake3 (FR-21).
- L0 hot cache is `DashMap<SymbolId, SymbolFact>` with lookup p99 ≤ 50 ns [perf] (§15.3 L0, §15.4); a `quick_cache` LRU bounds the hot set.
- No allocation on the timing path; hot structs use `SmallVec` and interning (§15.5).

**DoD:**
- `SledStore` and `RedbStore` pass the identical tree/key round-trip suite (redb feature-gated in CI).
- `TreeRegistry` enumerates every §15.1 namespace and fails fast on unknown prefixes.
- Key codec round-trips `RevId`/`SymbolId`/`Hash` and preserves numeric order lexicographically.
- `#[perf] #[ignore]` gate: DashMap lookup p99 ≤ 50 ns × CI_TOLERANCE against §15.4.

`cie-store` is the persistence backbone from §10/§15.3: sled by default with redb behind a feature flag, behind one `KvStore` trait so the backend swap is invisible to callers. Getting the key codec right now — big-endian numerics, raw 32-byte blake3 hash keys, one tree per §15.1 namespace — is what keeps later range scans (`rel:csr:{rev}:*`, `sum:{rev}:*`) and time travel correct. This task ships only the KV layer, tree registry, key codec, and L0 hot cache; typed tables and revisioning follow in TASK-025–027.

**Validation test sketch (contract):**

```rust
#[test]
fn task_024_trees_key_codec_and_registry() {
    // arrange: sled backend (default); redb runs this same suite behind `backend-redb`
    let store = SledStore::open_temporary().expect("open temp store");
    let fact = fact_for(SymbolId(7), "pay", RevId(41));
    // act: typed put/get through the exact §15.1 namespace keys
    store.put_sym(fact.id, &fact).expect("put sym");
    assert_eq!(store.get_sym(fact.id).unwrap().unwrap().id, fact.id);
    // assert: big-endian codec keeps numeric == lexicographic order
    assert!(key_sym(9) < key_sym(10), "BE encoding required for range scans");
    let h: Hash = blake3_hash(b"auth.py");
    store.put_sym_by_hash(h, SymbolId(7)).expect("put hash index");
    assert_eq!(store.get_sym_by_hash(&h), Some(SymbolId(7)));
    // assert: unknown namespace prefixes are rejected, never invented
    assert!(store.put_raw("bogus:1", b"x").is_err());
}

#[test]
#[perf]
#[ignore = "latency gate vs §15.4; run with --ignored"]
fn task_024_l0_hot_lookup_p99() {
    const CI_TOLERANCE: f64 = 1.5;
    let cache = HotFacts::with_capacity(1 << 16);
    for i in 0..10_000 { cache.insert(SymbolId(i), fact_for(i)); }
    let p99 = bench_p99_ns(10_000, || cache.get(SymbolId(4_242)));
    assert!(p99 as f64 <= 50.0 * CI_TOLERANCE, "L0 {p99} ns > 50 ns x CI_TOLERANCE");
}
```

---

### TASK-025: Revision allocation and revision metadata tables

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-store` (+ `cie-core`) |
| Depends on | TASK-024 |
| PRD reference | §15.1 (`rev:`, `file:`); FR-20; FR-21 |
| Parallel group | G-3b |

**Constraints:**
- `allocate_rev()` hands out strictly monotonic `RevId = u64` values from a counter persisted atomically in the store, so allocation survives crash and restart (cie-core `RevId`, FR-21).
- `RevMeta` is stored byte-exactly at `rev:{rev_id}:meta` and records `created_ns: Ns`, parent revision, ingested-file set, and the revision's root content `Hash`; no `chrono::DateTime` (FR-21).
- Per-file identity is content-based: `file:{file_id}:hash` stores the 32-byte blake3 `Hash` and `file:{file_id}:lang` stores `LanguageId`, exactly as §15.1 lists them (FR-20 diffs re-ingestion by content hash).
- Hash lookups are index-friendly and allocation-free so the FR-20 fast path (unchanged folder ≤ 5% wall time) stays open.
- `rev:{rev_id}:meta` values are `TimedRecord`-compatible so TASK-026 can time-travel over them (§15.2).

**DoD:**
- 1,000 allocations are strictly increasing; reopening the store continues above the high-water mark.
- `put/get_rev_meta`, `file_hash`, `file_lang` round-trip at the exact §15.1 keys.
- `RevMeta.created_ns` is monotonic with allocation order; serde + rkyv round-trip.
- The same suite passes on both backends (sled default, redb flag).

Revisions are the unit of analysis identity: every fact, summary, and record CIE ever emits is stamped with a `RevId = u64`, and FR-20 diffs re-ingestion by content hash. This task makes revision allocation crash-safe and monotonic via a persisted counter, and pins the per-file identity keys `file:{file_id}:hash` / `file:{file_id}:lang` plus `rev:{rev_id}:meta` exactly as §15.1 lists them. With hashes and metadata on solid ground, FR-23 time travel (TASK-026) and the typed tables (TASK-027) can be built on top.

**Validation test sketch (contract):**

```rust
#[test]
fn task_025_revision_allocation_monotonic_and_persisted() {
    // arrange: fresh store, first revision allocated
    let dir = tempdir();
    let store = SledStore::open(&dir).expect("open store");
    let r1: RevId = store.allocate_rev().expect("allocate rev 1");
    // act: write RevMeta + file identity at the exact §15.1 keys
    let now: Ns = 1_700_000_000_000_000_000;
    store.put_rev_meta(r1, &RevMeta { rev: r1, parent: None, created_ns: now,
                                      files: smallvec![FileId(1)],
                                      root_hash: blake3_hash(b"tree") }).unwrap();
    store.set_file_hash(FileId(1), blake3_hash(b"auth.py")).unwrap();
    store.set_file_lang(FileId(1), LanguageId(3)).unwrap();
    // assert: allocation strictly monotonic; round-trips land at exact keys
    assert!(store.allocate_rev().unwrap() > r1);
    assert_eq!(store.file_hash(FileId(1)).unwrap(), blake3_hash(b"auth.py"));
    assert_eq!(store.file_lang(FileId(1)).unwrap(), LanguageId(3));
    assert_eq!(store.rev_meta(r1).unwrap().created_ns, now);
    // assert: the RevId counter survives close/reopen (crash-safe, FR-21)
    drop(store);
    let reopened = SledStore::open(&dir).expect("reopen");
    assert!(reopened.allocate_rev().unwrap() > r1 + 1);
}
```

---

### TASK-026: Time-indexed tables and time-travel queries (FR-23)

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-store` (+ `cie-cli` query API) |
| Depends on | TASK-025 |
| PRD reference | §15.2 Time-Indexed Tables; FR-23 (§6); §13 item 11 |
| Parallel group | G-3c |

**Constraints:**
- `TimedRecord<T> { value: T, valid_from_ns: Ns, valid_to_ns: Option<Ns>, rev: RevId }` exactly per §15.2; `valid_to_ns: None` marks the current value.
- History is append-only: an update closes the previous record with `valid_to_ns = Some(t)` and inserts a new one — overwriting history is forbidden so `--as-of-rev N-1` stays answerable.
- The query API exposes both selectors from §15.2: `as_of_rev(rev)` (newest record with `rev ≤ N`) and `as_of_ns(ns)` (interval `[valid_from_ns, valid_to_ns)`), for `cie query --as-of-rev 42 --as-of-ns ...`.
- All boundaries are `Ns = u64` ns since epoch, compared as integers; no chrono in the hot path (FR-21).
- §13 item 11 is the binding acceptance: queries return correct results for revision N and N-1 (global rule 5).

**DoD:**
- Generic `TimeTable<T>` over `KvStore` with `insert` / `update` / `as_of_rev` / `as_of_ns` / `current`.
- Both selectors are exposed as a query-API function the CLI flags `--as-of-rev` / `--as-of-ns` call into.
- Append-only guard: closing an already-closed record or back-dating below the high-water mark is an error.
- The §13 item 11 scenario (rev N vs N-1, plus an ns query between boundaries) passes.

FR-23 requires `--as-of-rev` and `--as-of-ns` queries to return correct results for revision N and N-1, and §13 item 11 makes that a first-class acceptance case. This task implements the §15.2 `TimedRecord<T>` envelope as an append-only history layer over the TASK-024 KV store, so updates close the previous row instead of overwriting it. Both selectors then resolve against that history deterministically, giving every later table (facts, summaries, records) the same time-travel semantics.

**Validation test sketch (contract):**

```rust
#[test]
fn task_026_time_travel_rev_n_and_n_minus_1() {
    // arrange: two revisions of the same row (§13 item 11)
    let store = SledStore::open_temporary().expect("open");
    let t0: Ns = 1_700_000_000_000_000_000;
    let t1: Ns = t0 + 500_000_000;                          // +0.5 s
    let mut table: TimeTable<SymbolFact> =
        TimeTable::open(&store, "sym").expect("open table");
    // act: insert at rev 41; the rev-42 update closes the old row
    table.insert(fact_for(SymbolId(7), "pay_v1", RevId(41)), t0, RevId(41)).unwrap();
    table.update(fact_for(SymbolId(7), "pay_v2", RevId(42)), t1, RevId(42)).unwrap();
    // act: time-travel by revision and by ns (§15.2)
    let at_n = table.as_of_rev(RevId(42)).unwrap();
    let at_n_minus_1 = table.as_of_rev(RevId(41)).unwrap();
    let mid = table.as_of_ns(t0 + 250_000_000).unwrap();
    // assert: N sees v2, N-1 still sees v1, ns interval resolves the old row
    assert_eq!(at_n.value.name, "pay_v2");
    assert_eq!(at_n_minus_1.value.name, "pay_v1");
    assert_eq!(at_n_minus_1.valid_to_ns, Some(t1));
    assert_eq!(mid.value.name, "pay_v1");
    assert_eq!(table.current().unwrap().valid_to_ns, None);
}
```

---

### TASK-027: Symbol, hash-index, CSR, and summary tables with latency gates

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-store` (+ `cie-relations`) |
| Depends on | TASK-025 |
| PRD reference | §15.1; §14.2–§14.4; §15.4 Latency Targets |
| Parallel group | G-3c |

**Constraints:**
- Byte-exact tables: `sym:{symbol_id}` → `SymbolFact` (§14.2), `sym_by_hash:{hash}` → `SymbolId`, `rel:csr:{rev_id}:{sym_id}` → `CsrGraph` (§14.3), `sum:{rev_id}:{sym_id}` → `FunctionSummary` (§14.4); keys BE-encoded per TASK-024.
- Latency contract per §15.4: symbol lookup ≤ 50 ns, `sym_by_hash` ≤ 30 ns, cold L1 summary get ≤ 5 µs — each gate is a `#[perf]` `#[ignore]`-gated test with CI_TOLERANCE (global rule 5).
- L0 layers per §15.3: `DashMap<SymbolId, SymbolFact>` hot facts, `FxHashMap<Hash, SymbolId>` index, `quick_cache` bounded LRU; no allocation on the timing path (§15.5).
- `CsrGraph { offsets, edges: Vec<(u32, RelKind)> }` is stored per (revision, symbol) so adjacency scans stay cache-friendly (§15.3 L0 Relations).
- `sum:` rows are written only by the summary pipeline (TASK-030) — the store exposes typed get/put but no summary business logic (FR-5 boundary).

**DoD:**
- Typed APIs `put_sym/get_sym`, `put_sym_by_hash/get_sym_by_hash`, `put_csr/get_csr(rev, sym)`, `put_sum/get_sum(rev, sym)` at exact keys.
- `CsrGraph` round-trip preserves offsets, edges, and `RelKind` across a 3-revision fixture.
- A reusable bench harness module (§15.4) reports p99 for the sym / hash / summary paths, warm and cold.
- Perf gates 50 ns / 30 ns / 5 µs all pass × CI_TOLERANCE, `#[ignore]`-gated.

This task turns the §15.1 namespaces `sym:`, `sym_by_hash:`, `rel:csr:`, and `sum:` into typed, latency-gated tables over the §14.2–§14.4 structures. It also wires the L0 layers from §15.3 — DashMap hot facts, `FxHashMap<Hash, SymbolId>` index, quick_cache LRU — because the §15.4 targets (≤ 50 ns symbol lookup, ≤ 30 ns hash→id, ≤ 5 µs cold summary get) are only reachable through them. The bench harness is a permanent artifact: every later slice re-measures against §15.4 with the CI_TOLERANCE multiplier instead of trusting intuition.

**Validation test sketch (contract):**

```rust
#[test]
fn task_027_symbol_index_csr_and_summary_tables() {
    // arrange: one revision, one function, one CSR adjacency row
    let store = SledStore::open_temporary().expect("open");
    let (rev, sym) = (RevId(42), SymbolId(7));
    let fact = fact_for(sym, "pay", rev);
    let csr = CsrGraph { offsets: vec![0, 2],
                         edges: vec![(9, RelKind::Calls), (11, RelKind::DataFlow)] };
    // act: write through the four §15.1 tables
    store.put_sym(sym, &fact).expect("sym:{symbol_id}");
    store.put_sym_by_hash(blake3_hash(b"pay"), sym).expect("sym_by_hash:{hash}");
    store.put_csr(rev, sym, &csr).expect("rel:csr:{rev}:{sym}");
    store.put_sum(rev, sym, &summary_for(sym, rev)).expect("sum:{rev}:{sym}");
    // assert: exact-key round-trips
    assert_eq!(store.get_sym(sym).unwrap().unwrap().name, "pay");
    assert_eq!(store.get_sym_by_hash(&blake3_hash(b"pay")), Some(sym));
    assert_eq!(store.get_csr(rev, sym).unwrap().unwrap().edges.len(), 2);
    assert_eq!(store.get_sum(rev, sym).unwrap().unwrap().rev, rev);
}

#[test]
#[perf]
#[ignore = "§15.4 latency gates; run with --ignored"]
fn task_027_storage_latency_targets() {
    const CI_TOLERANCE: f64 = 1.5;
    // symbol ≤ 50 ns, hash → id ≤ 30 ns, cold L1 summary get ≤ 5 µs (§15.4)
    assert!(bench_sym_lookup_p99_ns() <= 50.0 * CI_TOLERANCE);
    assert!(bench_hash_lookup_p99_ns() <= 30.0 * CI_TOLERANCE);
    assert!(bench_cold_summary_get_p99_us() <= 5.0 * CI_TOLERANCE);
}
```

---

### TASK-028: Deletion handling — tombstones, orphan cleanup, stale-fact purge

| Field | Value |
|---|---|
| Phase | 3 — Evidence, Storage & Revisioning |
| Target crate(s) | `cie-store` (+ `cie-incremental`) |
| Depends on | TASK-024, TASK-025, TASK-027 |
| PRD reference | FR-8 (§6); §15.1 (`deps:`, `rev_deps:`); §13 item 7 |
| Parallel group | G-3d |

**Constraints:**
- Deletions are explicit tombstones (`TimedRecord` with closed `valid_to_ns`), never silent drops — FR-8 acceptance: recompute affected results without silently retaining stale facts.
- `rev_deps:{symbol_id}` → `SmallVec<[RecordId; 8]>` is the reverse-dependency index; deleting a symbol or file must purge orphan entries and cascade-invalidate dependent `record:` rows reachable via `deps:`.
- Stale-fact purge: a derived record whose input content `Hash`es no longer exist at the queried revision is served as invalidated, never as a live fact.
- A tombstone shadows the `sym_by_hash:` index so stale hash hits cannot resurrect a deleted symbol.
- The APIs here are the hooks the §13 item 7 acceptance case ("a code deletion that invalidates an earlier finding") will exercise in TASK-077.

**DoD:**
- `delete_symbol(SymbolId, Ns)` and `delete_file(FileId, Ns)` write tombstones and schedule the cascade; both are idempotent.
- After deletion: `get_sym` → tombstone/absent, orphan `rev_deps` entries removed, dependent records non-live.
- A purge pass invalidates facts whose input file hash is missing from `rev:{rev}:meta`.
- The §13 item 7 scenario (finding at rev 41, defining symbol deleted at rev 42) passes end-to-end.

FR-8's acceptance is "recompute affected results without silently retaining stale facts", and §13 item 7 demands that a code deletion invalidate an earlier finding. This task implements the deletion half of that contract: tombstones, orphan `rev_deps:` cleanup, cascade invalidation of dependent `record:` rows via `deps:`, and a purge pass for facts whose input hashes no longer exist. The APIs defined here are exactly the hooks TASK-077 will drive for acceptance item 7, so their semantics are frozen now.

**Validation test sketch (contract):**

```rust
#[test]
fn task_028_deletion_invalidates_dependent_findings() {
    // arrange: rev 41 defines pay(); a derived finding depends on it (§13 item 7 setup)
    let store = SledStore::open_temporary().expect("open");
    let (rev41, pay, finding) = (RevId(41), SymbolId(7), RecordId(100));
    store.put_sym(pay, &fact_for(pay, "pay", rev41)).expect("sym:");
    store.put_rev_deps(pay, &smallvec![finding]).expect("rev_deps:");
    store.put_record(finding, &record_for(finding, rev41)).expect("record:");
    // act: rev 42 deletes pay() — explicit tombstone, never a silent drop (FR-8)
    let now: Ns = 1_700_000_000_000_000_000;
    store.delete_symbol(pay, now).expect("delete writes tombstone");
    // assert: symbol gone, tombstone recorded, reverse deps cleaned
    assert!(matches!(store.get_sym(pay).unwrap(), None | Some(SymEntry::Tombstone(_))));
    assert!(store.rev_deps(pay).unwrap().is_empty(), "orphan rev_deps purged");
    assert!(!store.record(finding).unwrap().is_live(), "dependent record invalidated");
    // assert: a stale sym_by_hash entry cannot resurrect the deleted symbol
    assert_eq!(store.get_sym_by_hash(&blake3_hash(b"pay")), None);
}
```

## Phase 4 — Execution Engine, Summaries & IFDS

This phase turns stored facts and summaries into bounded, resumable analysis. `cie-analysis` provides the FR-7 execution engine (dependency scheduling, worklists, cancellation tokens, time+node budgets in Ns, persisted checkpoints), and `cie-summary` builds versioned `FunctionSummary`s at `sum:{rev}:{sym}` from intra-procedural effects, path conditions, and fixpoint interprocedural composition. The Reps–Horwitz–Sagiv IFDS framework (`cie-analysis::ifds`, §7) and its unresolved-call/taint instances then propagate facts across calls, returns, and wrappers, landing results in `DerivedRecord`s wrapped in Evidence (FR-4).

---

### TASK-029: Analysis execution engine with budgets, cancellation, and checkpoints

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-analysis` (+ `cie-store`) |
| Depends on | TASK-010, TASK-024 |
| PRD reference | FR-7 (§6); §9 Shared Interface; §5.1 Analysis execution row |
| Parallel group | G-4a |

**Constraints:**
- Implements FR-7 verbatim: dependency scheduling, worklists, cancellation tokens, budgets, and checkpoints; the acceptance is "analysis is bounded and resumable".
- Budgets are two-dimensional — wall time in `NsDelta = i64` ns plus a node budget; tripping either yields `AnalysisStatus::TimedOut` with partial coverage, never a clean pass (FR-35, §13 item 26, global rule 3).
- Checkpoints serialize worklist + visited state into `cie-store` (TASK-024) and resume byte-compatibly across process restarts without recomputing completed nodes.
- The engine consumes the §9 `AnalysisInput` and produces `AnalysisOutput { derived_records, evidence, dependencies, coverage, conclusions, counterexamples }`; every derived record carries `Evidence` (FR-4).
- Scheduling is deterministic given identical input (sorted worklists) so a resumed run and an uninterrupted run agree.

**DoD:**
- `Engine::run(input, budget)` completes bounded problems; `Engine::resume(checkpoint, budget)` reaches the same fixpoint as an uninterrupted run.
- Cancellation tokens are honored between work items (never mid-item), leaving a valid checkpoint behind.
- Status mapping: budget trip → `AnalysisStatus::TimedOut` + partial coverage; results are never marked clean on timeout.
- Same input + same budget ⇒ same worklist order and conclusions (determinism test).

FR-7 asks for dependency scheduling, worklists, cancellation, budgets, and checkpoints, with "analysis is bounded and resumable" as acceptance. This engine is the substrate every later algorithm runs on — summaries, IFDS, abstract interpretation all plug into it — so it consumes the §9 `AnalysisInput`, drives worklists deterministically, and persists checkpoints a cancelled run can resume from. Budgets are two-dimensional (wall time in `NsDelta`, node count), and tripping either produces `TimedOut` with partial coverage, never a clean bill of health (FR-35, §13 item 26).

**Validation test sketch (contract):**

```rust
#[test]
fn task_029_engine_bounded_and_resumable() {
    // arrange: a 1_000-node dependent chain exceeds a tiny node budget (FR-7)
    let facts = chain_facts(1_000);
    let engine = Engine::new(store());
    let budget = Budget { time_ns: NsDelta::MAX, node_budget: 250 };
    let input = AnalysisInput { revision: RevId(42), scope: Scope::all(),
                                facts: facts.clone(), contracts: ContractRegistry::new(),
                                assumptions: Assumptions::none() };
    // act 1: run until the node budget trips
    let out1 = engine.run(input.clone(), budget, CancellationToken::new()).unwrap();
    assert_eq!(out1.status, AnalysisStatus::TimedOut, "FR-35: budget trip is not a clean pass");
    assert!(out1.coverage.analyzed < out1.coverage.total, "coverage must be partial");
    // act 2: resume from the persisted checkpoint with a full budget
    let cp = out1.checkpoint.expect("checkpoint persisted for resumption");
    let out2 = engine.resume(cp, Budget::unlimited()).unwrap();
    // assert: resumption reaches the same fixpoint as an uninterrupted run
    let reference = engine.run(input, Budget::unlimited(), CancellationToken::new()).unwrap();
    assert_eq!(out2.status, AnalysisStatus::CompleteWithinScope);
    assert_eq!(out2.conclusions.len(), reference.conclusions.len());
    assert!(out2.derived_records.iter().all(|r| r.evidence.rev == RevId(42)));
}
```

---

### TASK-030: Versioned FunctionSummary store with reuse and targeted invalidation

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-summary` (+ `cie-store`) |
| Depends on | TASK-027, TASK-029 |
| PRD reference | FR-5 (§6); §14.4 FunctionSummary; §15.1 `sum:`; §15.4 |
| Parallel group | G-4b |

**Constraints:**
- `FunctionSummary { sym, inputs, outputs, reads, writes, effects, exceptions, path_conditions, unresolved, rev, compute_ns }` field-for-field per §14.4, with `rev: RevId` and `compute_ns: NsDelta` (FR-21).
- Versioned at the byte-exact key `sum:{rev_id}:{sym_id}` (§15.1); reads go through the TASK-027 typed API (warm ≤ 500 ns, cold L1 ≤ 5 µs per §15.4).
- FR-5 trio must hold: composition (TASK-033), reuse across revisions via unchanged content hashes, and targeted invalidation — changing one function invalidates only its transitive dependents via `rev_deps:`, never a full-store wipe.
- `unresolved: SmallVec<[SymbolId; 4]>` lists every callee the summarizer could not resolve so callers inherit honest evidence (FR-4).
- A summary computed under an exhausted budget is marked partial/unresolved — never silently complete (global rule 3).

**DoD:**
- `SummaryStore::get / put / invalidate_dependents / compute_or_reuse` with invalidation cascading only through `rev_deps:`.
- Reuse: an unchanged content hash returns the stored summary without recompute.
- Unresolved callees are inherited from callee summaries on reuse and composition.
- Perf gate: cold L1 summary get ≤ 5 µs × CI_TOLERANCE, `#[perf] #[ignore]`-gated.

FR-5 wants versioned function effects with composition, reuse, and targeted invalidation. This task lands `FunctionSummary` (§14.4) at the byte-exact key `sum:{rev_id}:{sym_id}` and adds the store-level intelligence: unchanged content hashes mean reuse without recompute, and invalidating a callee cascades only through `rev_deps:` to its transitive dependents. The summary get path is the ≤ 5 µs §15.4 gate, so it rides the TASK-027 typed API and L1 sled tree rather than inventing a new one.

**Validation test sketch (contract):**

```rust
#[test]
fn task_030_summary_store_reuse_and_targeted_invalidation() {
    // arrange: a→b→c chain plus an unrelated d; summaries computed for rev 41
    let mut sums = SummaryStore::open(store());
    for sym in [A, B, C, D] { sums.put(sum_for(sym, RevId(41))).expect("put sum"); }
    // act: c changes in rev 42 → only b (and a) are invalidated (FR-5, targeted)
    let invalidated = sums.invalidate_dependents(C, RevId(42)).expect("invalidate");
    assert_eq!(invalidated.as_slice(), &[B, A], "cascade via rev_deps, never a wipe");
    assert!(sums.get(RevId(41), D).unwrap().is_some(), "unrelated summary retained");
    // act: recompute b from rev-42 facts; unchanged d reuses its stored summary
    let b42 = sums.compute_or_reuse(B, RevId(42), &facts()).expect("recompute");
    assert_eq!(b42.rev, RevId(42));
    assert!(b42.compute_ns > 0, "measured in NsDelta (FR-21)");
    // assert: stored at the exact §15.1 key; unresolved callees inherited (FR-4)
    assert!(sums.get_raw(&key_sum(RevId(42), B)).is_some());
    assert!(b42.unresolved.iter().all(|s| facts().is_unresolved(*s)));
}
```

---

### TASK-031: Intra-procedural effects extraction from TypedFacts

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-summary` (+ `cie-relations`) |
| Depends on | TASK-010 |
| PRD reference | FR-2, FR-11 (§6); §14.2 Fact Types; §16.1 adapter extractors |
| Parallel group | G-4a |

**Constraints:**
- Input is `TypedFacts` — `CallFact`, `EffectFact`, `ExceptionFact` per §14.2 as produced by the §16.1 adapter extractors (`extract_reads_writes`, `extract_exceptions`); output is a per-function `FunctionEffectSeed { reads, writes, effects, exceptions, unresolved }`.
- `EffectFact.external` separates internal from external operations; externals become evidence-side `Unsupported`/assumption entries, never silently assumed behavior (FR-4).
- Semantics come only from the language adapter (FR-3, global rule 6) — no language-specific shortcuts in shared code.
- Collections are `SmallVec`-bounded, timing is `NsDelta`, and extraction runs under TASK-029 budgets (§15.5, FR-21).
- Unresolved callees (dynamic dispatch, missing definitions) are emitted as facts, not guessed — they feed `FunctionSummary.unresolved` (FR-4).

**DoD:**
- `effects_for_function(facts, sym) -> FunctionEffectSeed` aggregates reads/writes into exact `ResourceId` sets.
- External vs internal ops classified via `EffectFact.external` and surfaced for evidence (FR-4).
- Handled vs unhandled exceptions distinguished via `ExceptionFact.handler` (§14.2).
- Fixture coverage: read+write, external call, raise without handler; no cross-language assumptions (FR-3).

FR-11 Stage 1 begins with honest per-function effects: what a function reads, writes, throws, and calls. This task reduces adapter-produced `TypedFacts` (§14.2, §16.1 extractors) into a `FunctionEffectSeed` per symbol — the raw material `FunctionSummary` is assembled from in TASK-030/033. The internal/external split is the honesty boundary: external operations are not guessed but surfaced as `Unsupported`/unresolved evidence per FR-4, and all semantics come from the language adapter so nothing leaks across languages (FR-3).

**Validation test sketch (contract):**

```rust
#[test]
fn task_031_intraprocedural_effects_extraction() {
    // arrange: pay() reads balance, writes ledger, calls external io.print,
    // and raises ValueError without a handler (FR-11)
    let facts = TypedFacts::fixture("pay_effects", Lang::Python);
    // act: extract the per-function effect seed
    let seed = effects_for_function(&facts, PAY).expect("seed");
    // assert: reads/writes are exact ResourceId sets
    assert_eq!(seed.reads.as_slice(), &[BALANCE]);
    assert_eq!(seed.writes.as_slice(), &[LEDGER]);
    // assert: exactly one external op (io.print); internal ops stay unflagged (FR-4)
    assert_eq!(seed.effects.iter().filter(|e| e.external).count(), 1);
    assert!(seed.effects.iter().any(|e| e.reads.contains(&BALANCE)));
    // assert: the unhandled exception is recorded without a handler site
    assert_eq!(seed.exceptions.len(), 1);
    assert!(seed.exceptions[0].handler.is_none());
    assert_eq!(seed.exceptions[0].kind, ExceptionKind::Raise);
    assert!(seed.unresolved.is_empty(), "fixture callees are all resolved");
}
```

---

### TASK-032: Path conditions — representation, extraction, simplification

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-summary` |
| Depends on | TASK-031 |
| PRD reference | §14.4 `path_conditions`; FR-11 (§6); §7 (Z3 row for later SMT encoding) |
| Parallel group | G-4b |

**Constraints:**
- `PathCond` is a branch predicate over `ValueId`/`ResourceId` with polarity (taken/not-taken) and its `SourceLocation`, so downstream findings can cite the exact branch (FR-4).
- A path's condition is the conjunction of its branch predicates; extraction is per-branch from control-flow facts (§16.1 `extract_control_flow`).
- Extraction is bounded by the TASK-029 budget; exhaustion yields `PathCond::Unknown`, never a silently-true condition (global rule 3).
- Simplification is local and sound — collapse tautologies (`c ∨ ¬c`), drop contradictions (`c ∧ ¬c`), constant-fold literals — with no solver dependency at this layer (SMT arrives later per §7).
- Results fill `FunctionSummary.path_conditions: SmallVec<[PathCond; 4]>` (§14.4) and stay structural/comparable for the Z3 encoding (§7, FR-34 Constraint checker).

**DoD:**
- `PathCond { pred, polarity, loc }` with `conjoin` / `disjoin` / `negate` / `simplify` operations.
- Extraction yields at most 4 conditions per function (the §14.4 SmallVec bound) or a budget-flagged Unknown.
- Tautology, contradiction, and constant-fold simplifications unit-tested.
- Conditions attach to `FunctionEffectSeed.path_conditions` for TASK-030/033 consumption.

Behavior claims like "it succeeds only when authorized" need branch context, which is exactly what §14.4's `path_conditions` field carries. This task defines `PathCond` (predicate over ValueIds, polarity, source location), extracts one condition per feasible branch from control-flow facts, and provides local sound simplification — tautologies, contradictions, constant folds — without pulling in a solver. These conditions feed summaries now and the Z3/SMT encoding later (§7, FR-34 Constraint checker), so keeping them structural and citable matters.

**Validation test sketch (contract):**

```rust
#[test]
fn task_032_path_conditions_per_branch_and_simplify() {
    // arrange: `if amount > limit: reject() else: charge()` — two branches (FR-11)
    let facts = TypedFacts::fixture("branchy", Lang::Python);
    let conds = extract_path_conditions(&facts, PAY).expect("path conds");
    // assert: one condition per branch, complementary polarity, cited location
    assert_eq!(conds.len(), 2);
    assert_eq!(conds[0].pred, Predicate::gt(AMOUNT, LIMIT));
    assert!(conds[0].polarity && !conds[1].polarity);
    assert_eq!(conds[0].loc.file, facts.file_of(PAY), "FR-4: branches are citable");
    // act: conjunction + disjunction with local sound simplification
    let taut = PathCond::disjoin(conds[0].clone(), conds[1].clone());
    let contra = PathCond::conjoin(conds[0].clone(), conds[1].clone());
    // assert: c ∨ ¬c folds to True; c ∧ ¬c folds to False — no solver needed
    assert_eq!(simplify(taut), PathCond::True);
    assert_eq!(simplify(contra), PathCond::False);
    // assert: conditions fit §14.4 path_conditions: SmallVec<[PathCond; 4]>
    let seed = effects_for_function(&facts, PAY).unwrap();
    assert!(seed.path_conditions.len() <= 4);
}
```

---

### TASK-033: Interprocedural summary composition with fixpoint and wrapper hook

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-summary` |
| Depends on | TASK-030, TASK-032 |
| PRD reference | FR-5, FR-11 (§6); §7 (bi-abduction row, `cie-summary::biabduction`); §11 Stage 1 |
| Parallel group | G-4c |

**Constraints:**
- Composition maps callee reads/writes/effects/exceptions onto the caller through call/return edges and maps callee returns to caller outputs (FR-5: "summaries support composition").
- Fixpoint via the TASK-029 worklist; recursion starts at ⊥ (empty effects) and widens after a bounded iteration count, so analysis stays bounded (FR-7).
- A `WrapperDetector` hook fires per composed summary so Stage-1 detector crates can tag delegating wrappers (FR-11, §11 Stage 1 acceptance: "delegating wrappers") without core changes.
- Unresolved callees propagate upward into the caller's `unresolved` (FR-4) — composition never claims more precision than its inputs.
- Every composed summary is re-versioned at `sum:{rev}:{sym}` with fresh `compute_ns: NsDelta` (FR-21) and TASK-030 targeted invalidation applies.

**DoD:**
- `compose_all(rev)` reaches fixpoint on acyclic call graphs.
- Recursive cycles terminate via widening and are flagged partial — never silently complete.
- `WrapperDetector` hook fires and can tag a composed summary as delegating.
- Chain fixture: c's writes surface in a's summary; unresolved callees propagate to a.

Modular analysis lives or dies on composition: FR-5 requires summaries that compose, and FR-11's Stage-1 acceptance explicitly names delegating wrappers. This task composes callee summaries into callers across call/return edges, runs to fixpoint on a worklist with ⊥-start and bounded widening for recursive cycles, and exposes a `WrapperDetector` hook so later detector crates can tag delegating wrappers without touching the engine. Unresolved callees compose upward, so a caller never claims more precision than its callees had (FR-4).

**Validation test sketch (contract):**

```rust
#[test]
fn task_033_interprocedural_composition_fixpoint_and_wrappers() {
    // arrange: a → b → c where c writes LEDGER and b merely delegates to c (FR-11)
    let mut sums = SummaryStore::open(store());
    for sym in [A, B, C] { sums.put(seed_summary(sym, RevId(41))).expect("seed"); }
    let mut detector = DetectorChain::new().with(IsDelegatingWrapper);
    let mut composer = Composer::new(&mut sums, &mut detector);
    // act: fixpoint composition over the call graph (worklist, FR-7)
    let report = composer.compose_all(RevId(41)).expect("fixpoint");
    // assert: c's write composes through b into a (FR-5)
    let a = sums.get(RevId(41), A).unwrap().unwrap();
    assert!(a.writes.contains(&LEDGER), "composed through b");
    assert!(report.wrappers.contains(&B), "delegating wrapper hook fired (§11)");
    // assert: recursion terminates via widening, flagged partial — never silently complete
    let rec = composer.compose_all_recursive(RevId(41)).unwrap();
    assert_eq!(rec.status, SummaryStatus::Widened);
    assert!(rec.iterations <= MAX_FIXPOINT_ITERS, "bounded fixpoint (FR-7)");
}
```

---

### TASK-034: IFDS framework — exploded supergraph and tabulation algorithm

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-analysis` (`ifds` module) |
| Depends on | TASK-009, TASK-029 |
| PRD reference | §7 (Reps, Horwitz & Sagiv — IFDS, POPL 1995 → `cie-analysis::ifds`); FR-2; §5.2 |
| Parallel group | G-4b |

**Constraints:**
- The module path `cie-analysis::ifds` is fixed by the §7 algorithm map; module docs cite Reps, Horwitz & Sagiv (POPL 1995) and carry the §7 qualification that IFDS is precise only for its supported problem class.
- Implements tabulation over the exploded supergraph; the path-edge table guarantees termination on finite fact sets (Reps–Horwitz–Sagiv).
- `IfdsFactDomain` trait — `Fact: Copy + Eq + Hash + Ord`, `transfer(edge, fact) -> SmallVec<[Fact; 4]>` — is generic; concrete instances are TASK-035's job (§5.2 boundary: relations shared, fact domains algorithm-specific).
- Joins at control-flow merges are set union only (distributivity); no path-sensitivity is claimed beyond the IFDS class.
- Built on shared relations — `rel:csr:{rev}:{sym}` rows (§15.1) plus `CallFact` call/return edges (FR-2) — and runs under TASK-029 budgets/cancellation (FR-7).

**DoD:**
- `ifds::solve(rev, domain, seeds) -> IfdsSolution` with per-procedure exploded supergraph and memoized path edges.
- `IfdsFactDomain` is implementable by TASK-035's two instances without core changes (§5.2).
- Merge-join test: facts meeting at a join node equal the union of branch facts.
- Interprocedural test: facts cross the call edge, into the callee, and return to the call site's successors.

The PRD's §7 algorithm map pins Reps–Horwitz–Sagiv IFDS (POPL 1995) to `cie-analysis::ifds`, used to "follow facts across calls, returns, wrappers". This task implements the generic machinery: an exploded supergraph built from the shared CSR/call relations (§5.2 keeps relations shared while fact domains are algorithm-specific), path-edge tabulation with memoized termination, union joins at merges, and per-instance transfer functions behind an `IfdsFactDomain` trait. The module docs carry the PRD's qualification so nobody mistakes IFDS for a universal proof technique.

**Validation test sketch (contract):**

```rust
#[test]
fn task_034_ifds_tabulation_call_return_and_merge() {
    // arrange: caller f with an if/else merge calls g; identity domain, one seed fact
    let facts = TypedFacts::fixture("ifds_fixture", Lang::Rust);
    let solver = IfdsSolver::new(&facts, RevId(42));
    let seeds = smallvec![IfdsFact::at(facts.node_of("f.entry"))];
    // act: tabulate the exploded supergraph (Reps–Horwitz–Sagiv, §7)
    let sol = solver.solve(PassThroughDomain, &seeds).expect("finite facts ⇒ termination");
    // assert: facts cross the call edge into g and return to f's post-call node
    let call_site = facts.node_of("f.call_g");
    let ret_site = facts.node_of("f.after_g");
    assert!(!sol.facts_at(facts.node_of("g.entry")).is_empty(), "facts entered callee");
    assert_eq!(sol.facts_at(ret_site), sol.facts_at(call_site),
               "PassThrough returns exactly what it received");
    // assert: control-flow merge joins by union (distributive), not path-sensitivity
    let then_f = sol.facts_at(facts.node_of("f.then"));
    let else_f = sol.facts_at(facts.node_of("f.else"));
    assert_eq!(sol.facts_at(facts.node_of("f.join")), union(&then_f, &else_f));
}
```

---

### TASK-035: IFDS instances — unresolved-call propagation and taint-style value flow

| Field | Value |
|---|---|
| Phase | 4 — Execution Engine, Summaries & IFDS |
| Target crate(s) | `cie-analysis` (`ifds::instances`) |
| Depends on | TASK-034, TASK-033 |
| PRD reference | §7 (IFDS row: "follow facts across calls, returns, wrappers"); FR-11, FR-2; §14.9; §13 items 1, 4, 6 |
| Parallel group | G-4d |

**Constraints:**
- Exactly two `IfdsFactDomain` instances: `UnresolvedCallDomain` (propagates "target unresolved" facts from `CallFact.unresolved` sites, §14.2) and `TaintDomain` (value flow across calls, returns, and delegating wrappers, using TASK-033 composed summaries).
- Detector logic — print-only stub (§13 item 1) and ignored-rejection (§13 item 4) — stays in later crates; this task ships propagation facts only (§7 boundary, §5.2).
- Results become `DerivedRecord`s wrapped in §14.8 Evidence via the TASK-023 typestate — a record without evidence is unconstructable (FR-4).
- A budget trip during tabulation yields conclusion `Unknown`/`TimedOut`, never clean (FR-35, global rule 3).
- Transfer functions respect per-language semantics via adapters (e.g., Go error values, Rust `Result`) — no cross-language rule leakage (FR-3, global rule 6).

**DoD:**
- `UnresolvedCallDomain` and `TaintDomain` implement `IfdsFactDomain` and terminate on the §13-style fixtures.
- The ignored-rejection fixture produces a taint record citing the rejected return as it flows into the caller.
- The unresolved external (§13 item 6) is listed in `Evidence.unresolved`, not silently assumed.
- Records persist under `record:` with `deps:` links to their input summaries (§15.1).

Frameworks earn their keep through instances. This task ships the two §7-named instances CIE's Stage-1 detectors depend on: `UnresolvedCallDomain` propagates unresolved dynamic/reflective call sites, and `TaintDomain` follows value flow across calls, returns, and wrapper edges supplied by TASK-033's composed summaries. Results are wrapped into `DerivedRecord`s with full §14.8 Evidence via the TASK-023 typestate, so the print-only-stub and ignored-rejection detectors (§13 items 1 and 4) can later consume propagation facts with receipts attached.

**Validation test sketch (contract):**

```rust
#[test]
fn task_035_ifds_instances_unresolved_and_taint_flow() {
    // arrange: auth() is an external unresolved callee; pay() calls auth(), ignores
    // its rejection, and the value flows through wrapper log() (§13 items 4 + 6)
    let facts = TypedFacts::fixture("ignored_rejection", Lang::Go);
    let solver = IfdsSolver::new(&facts, RevId(42));
    let engine = Engine::new(store());
    let summaries = composed_summaries();               // from TASK-033
    // act: run both instances under engine budgets
    let unresolved = engine.ifds(&solver, UnresolvedCallDomain).unwrap();
    let taint = engine.ifds(&solver, TaintDomain::new(&summaries)).unwrap();
    // assert: the unresolved external propagates and is listed, not assumed (FR-4)
    let rec = engine.wrap_as_record(&unresolved, RecordKind::UnresolvedCall).unwrap();
    assert!(rec.evidence.unresolved.contains(&AUTH),
            "external auth() must appear in Evidence.unresolved");
    // assert: taint crosses call, return, and wrapper edges into the caller
    let after_auth = taint.facts_at(facts.node_of("pay.after_auth"));
    assert!(after_auth.iter().any(|f| f.is_tainted_from(AUTH_RESULT)),
            "the rejected value flows into pay() even though the return is ignored");
    // assert: record carries Evidence + deps; conclusions stay honest (FR-35)
    assert_eq!(rec.evidence.rev, RevId(42));
    assert!(!rec.deps.is_empty());
    assert!(matches!(rec.conclusion,
                     Conclusion::ExtractedFact | Conclusion::InferredCandidate));
}
```

## Phase 5 — Behavior Extraction, Contracts, FCA & Hierarchy

Phase 5 delivers PRD Stages 1–3: Stage-1 behavior detectors over summaries and relations (FR-11), the result-protocol store (FR-9) and contract registry (FR-6) that everything downstream consumes, Stage-2 candidate-rule inference and conformance checking (FR-12), and Stage-3 predicate checking, FCA concept lattices, and structural hierarchy (FR-13, FR-14). Throughout the phase, expected behavior (contracts) stays strictly separated from observed behavior (records), every derived result carries Evidence, inferred output can never masquerade as established (`Explicit`/`Verified`), and a timeout can never be reported as a clean result.

### TASK-036: Implement Stage-1 behavior detectors over summaries and relations

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-summary` (detectors), `cie-analysis` (detector registry) |
| Depends on | TASK-035, TASK-033 |
| PRD reference | §6 FR-11 / §11 Stage 1 / §12 Slice 0 / §13 items 1–3 |
| Parallel group | G-5a |

**Constraints:**
- Every detector output is a `DerivedRecord` (§14.9) whose `Evidence` cites summary/relation provenance (FR-4); a detector that cannot cite provenance emits nothing.
- Detectors report observed facts only ("prints a constant and returns without another visible operation"); turning a fact into a violation is deferred to conformance checking against contracts (FR-11, §8).
- The four required classes — print-only stub, unconditional success, ignored failure, delegating wrapper — run on normalized `FunctionSummary` + call relations, never on raw tree-sitter trees, so language semantics cannot leak across languages (FR-3).
- Unresolved callees are recorded in `Evidence.unresolved`, never guessed around (FR-4).
- Findings persist through the record protocol (`record:{record_id}`, `deps:{record_id}`, §15.1) with `created_ns`/`compute_ns` in `Ns` (FR-21).

**DoD:**
- Detector registry runs the four detectors deterministically over summary+relation inputs and emits classified records.
- §13 acceptance hooks: print-only `authorize()` → `SuspectedStub`; always-true path → `UnconditionalSuccess`; legitimate delegation → `DelegatingWrapper`, NOT a stub.
- All records round-trip through the TASK-037 store with correct deps/rev_deps linkage.
- Fixture suite covers all four classes plus a negative control (a genuinely checking function).

Build the Stage-1 detector set required by FR-11: each detector consumes composed function summaries and call
relations (TASK-033, TASK-035) and emits evidence-backed findings for print-only stubs, unconditional success,
ignored failures, and delegating wrappers. Detection is intentionally conservative — a print-only `authorize()`
yields a suspected-stub behavioral fact that becomes a violation only when a contract (TASK-038) establishes an
authorization requirement. These findings are the canonical inputs for Slice-1 conformance checking (TASK-042)
and gate acceptance items 1–3 of §13.

**Validation test sketch (contract):**

```rust
#[test]
fn task_036_stage1_behavior_detectors() {
    let fx = fixture_two_functions(); // (a) fn authorize(u: User) -> bool { println!("auth {u}"); true }
                                      // (b) fn check_access(u: User) -> bool { perms.contains(&u) }
    let sums = fx.summaries();        // FunctionSummary per symbol from the summary store (TASK-033)
    let rels = fx.call_relations();   // CSR call graph (TASK-035)

    let mut reg = DetectorRegistry::new();
    reg.register(PrintOnlyStub::default());
    reg.register(UnconditionalSuccess::default());
    reg.register(IgnoredFailure::default());
    reg.register(DelegatingWrapper::default());
    let records = reg.run(&sums, &rels, fx.rev);

    let stub = records.iter().find(|r| r.kind == RecordKind::SuspectedStub).expect("stub detected");
    assert_eq!(stub.subject, fx.a);
    assert!(stub.evidence.provenance.iter().any(|p| p.effect == EffectKind::Print));
    assert!(records.iter().all(|r| r.subject != fx.b));            // real check: no false stub
    assert!(records.iter().all(|r| r.evidence.rev == fx.rev));     // evidence invariant (FR-4)
    assert!(records.iter().all(|r| r.created_ns > 0));             // Ns timing (FR-21)
}
```

### TASK-037: Implement the result-protocol record store

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-store` (record trees), `cie-evidence` (evidence join) |
| Depends on | TASK-023 |
| PRD reference | §6 FR-9 / §14.9 / §15.1 / §15.2 / §8 |
| Parallel group | G-5a |

**Constraints:**
- Exactly the §14.9 shape: `DerivedRecord{kind, payload, evidence, deps, coverage, conclusion, counterexample, created_ns, compute_ns}`; `RecordPayload` covers findings, concepts, counterexamples, and status — one consistent interface for all analyzers (FR-9).
- Key namespaces exactly `record:{record_id}`, `deps:{record_id}`, `rev_deps:{symbol_id}`, `evidence:{record_id}` (§15.1) — no ad-hoc keys (global rule 7).
- `RecordId` is content-addressed (blake3 over payload+evidence+rev, `Hash = [u8; 32]`) so recomputation is idempotent and stale facts cannot survive silently (FR-8, global rule 1).
- Records are revision-scoped via `TimedRecord` (§15.2) so `--as-of-rev` time-travel reads work.
- A record whose analysis timed out stores `Conclusion::Unknown` + status `TimedOut`; the store rejects any "clean" conclusion carrying a timeout marker (§8, global rule 3).

**DoD:**
- put/get round-trip preserves every field; evidence is joined by `evidence:{record_id}`.
- `deps` and `rev_deps` are maintained transactionally on insert; `rev_deps(symbol)` returns the record.
- Recomputing the same result yields the same `RecordId` (idempotency).
- Time-travel read at rev N and N−1 returns the correct version.

Implement the single result protocol demanded by FR-9: every analyzer outcome — findings, inferred concepts,
counterexamples, and status — is a `DerivedRecord` (§14.9) persisted under the §15.1 key namespaces. Record ids
are content-addressed with blake3 so recomputation is idempotent and stale facts cannot survive a re-analysis
(FR-8). Revision-scoped storage via `TimedRecord` (§15.2) gives time-travel reads, and the store enforces the
status rules of §8 mechanically, including "timeout never becomes clean".

**Validation test sketch (contract):**

```rust
#[test]
fn task_037_record_protocol_store() {
    let mut store = RecordStore::open_in_memory();
    let ev = Evidence {
        rev: 41,
        provenance: smallvec![ProvenanceRef::summary(41, SYM_AUTHORIZE)],
        ..Evidence::empty(41)
    };
    let rec = DerivedRecord {
        kind: RecordKind::SuspectedStub,
        payload: RecordPayload::Finding(Finding::stub(SYM_AUTHORIZE)),
        evidence: ev.clone(),
        deps: smallvec![Dependency::summary(41, SYM_AUTHORIZE)],
        coverage: Coverage::within_scope(Scope::Symbol(SYM_AUTHORIZE)),
        conclusion: Conclusion::ExtractedFact,
        counterexample: None,
        created_ns: 1_700_000_000_000_000_000,
        compute_ns: 250_000,
    };
    let id = store.put(&rec).unwrap();   // writes record:{id}, deps:{id}, rev_deps:{sym}, evidence:{id}
    assert_eq!(store.get(id).unwrap(), rec);
    assert_eq!(store.evidence(id).rev, 41);
    assert!(store.rev_deps(SYM_AUTHORIZE).contains(&id));
    assert_eq!(store.put(&rec).unwrap(), id);   // content-addressed => idempotent recompute (FR-8)
    let timed_out = rec.with_conclusion(Conclusion::Unknown, Status::TimedOut);
    assert_eq!(store.put(&timed_out).unwrap().conclusion, Conclusion::Unknown); // rule 3: never "clean"
}
```

### TASK-038: Build the contract registry with authority separation

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-contract` |
| Depends on | TASK-023 |
| PRD reference | §6 FR-6 / §14.5 / §15.1 / §8 / §6 FR-28 |
| Parallel group | G-5a |

**Constraints:**
- `Contract{id, obligation, expr, authority, evidence}` exactly per §14.5, with `Obligation{Safety|Liveness|Protocol|Naming|Custom}` and `Authority{Explicit|InferredCandidate|Verified}`.
- Authority is settable only through guarded constructors: `insert_explicit` (user/source-annotated input), `verify` (requires mechanical checker evidence); no public path — and specifically no LLM path (FR-28, global rule 4) — can produce `Explicit`/`Verified` without that evidence.
- The registry holds expected behavior only; observed facts live in the record store (TASK-037); conformance comparisons emit new records and never mutate contracts (FR-6 acceptance).
- Keys `contract:{contract_id}` (§15.1, global rule 7); every contract carries an `EvidenceRef` (global rule 2).
- `InferredCandidate → Verified` transitions are explicit API calls, logged, and rejected without checker evidence (§8: an inferred rule must never become an established requirement).

**DoD:**
- Insert and query by id, subject symbol, obligation, and authority.
- Inserting an LLM-sourced contract with `Explicit` or `Verified` authority returns `Err(AuthorityError)`.
- `verify` without checker evidence is rejected; with it, `Explicit → Verified` succeeds and is logged.
- Struct shape matches §14.5 field-for-field.

Create the contract registry of FR-6 and §14.5, keyed as `contract:{contract_id}`. The registry is the home of
expected behavior and is kept strictly apart from observed facts (which live in the TASK-037 record store);
conformance comparisons emit new records and never mutate contracts. Authority is settable only through guarded
constructors, so no caller — and in particular no LLM extractor (FR-28) — can produce `Explicit` or `Verified`
contracts without mechanical evidence.

**Validation test sketch (contract):**

```rust
#[test]
fn task_038_contract_authority_invariants() {
    let mut reg = ContractRegistry::open_in_memory();
    let ev = reg.test_evidence(7);                       // mechanical checker evidence, rev 7
    let safety = Predicate::requires(Action::Fulfill, Action::Authorized);
    let c_exp = reg.insert_explicit(Obligation::Safety, safety.clone(), ev.src()).unwrap();
    let c_inf = reg.insert_inferred(Obligation::Naming,
                                    Predicate::name_matches("authorize", "checks"), ev.inf()).unwrap();
    assert_eq!(reg.authority_of(c_inf), Authority::InferredCandidate);

    let llm = Extractor::Llm { model_id: "llama3", prompt_hash: [0u8; 32] };
    assert!(reg.insert_with(llm, Obligation::Safety, safety.clone(), Authority::Explicit).is_err());
    assert!(reg.insert_with(llm, Obligation::Safety, safety, Authority::Verified).is_err()); // FR-28

    reg.verify(c_exp, ev.checker()).unwrap();            // Explicit -> Verified needs checker evidence
    assert_eq!(reg.authority_of(c_exp), Authority::Verified);
    assert!(reg.verify(c_inf, ev.inf()).is_err());       // non-checker evidence cannot verify
    assert!(reg.contracts_for(SYM_GATE).contains(&c_exp));
}
```

### TASK-039: Add the naming adapter for method-name/implementation mismatch

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-contract::naming` |
| Depends on | TASK-036, TASK-038 |
| PRD reference | §6 FR-10, FR-11 / §7 Høst & Østvold row / §8 |
| Parallel group | G-5b |

**Constraints:**
- Lexicon-driven verb analysis (`authorize`/`verify`/`check`/`delete`/…) compares the name's promised action with summary actions; a mismatch emits (a) a `SuspectedStub`/`NameMismatch` DerivedRecord and (b) an `Obligation::Naming` contract with `InferredCandidate` authority — the FR-11 conformance-finding example.
- Labels are immutable `Label{source, model_id, prompt_hash}` values written to `label:{record_id}`; they annotate records and NEVER influence detection, membership, or verification (FR-10, global rule 4).
- With the LLM disabled, the deterministic lexicon fallback still produces labels and findings (FR-28 acceptance).
- Per-language verb normalization lives behind the language adapter, which vouches that the naming rule applies to that language (FR-3, global rule 6).
- Evidence cites the name token, the summary action set, and the missing action (global rule 2).

**DoD:**
- `authorize()` that only prints → Naming obligation + suspected-stub finding with complete evidence.
- `authorize()` that actually consults a permission source → no finding.
- LLM-off path exercised end-to-end (deterministic fallback labels).
- Naming contracts enter the registry only as `InferredCandidate`.

Implement the naming adapter (`cie-contract::naming`, §7 Høst & Østvold "Debugging Method Names" row) that
compares what a method name promises with what its summary shows it does: an `authorize()` that only prints
becomes both a suspected-stub record and an `Obligation::Naming` candidate contract, exactly as the FR-11
conformance-finding example prescribes. Labels are immutable `Label{source, model_id, prompt_hash}` values
(FR-10) that never influence detection, membership, or verification decisions. A deterministic lexicon fallback
keeps the adapter fully functional with the LLM disabled (FR-28), and per-language verb normalization is
vouched for by the language adapter so rules do not leak across languages (FR-3).

**Validation test sketch (contract):**

```rust
#[test]
fn task_039_naming_mismatch() {
    let fx = fixture_naming(); // (a) fn authorize(u: User) -> bool { println!("auth {u}"); true }
                               // (b) fn authorize(u: User) -> bool { if !allowed(u) { return false; } true }
    let reg = contract_registry_with_naming_rules();
    let mut adapter = NamingAdapter::lexicon_default();       // deterministic fallback (FR-28)

    let recs = adapter.run(&fx.summaries(), &reg, fx.rev);
    let mis = recs.iter().find(|r| r.subject == fx.a).expect("mismatch on print-only authorize");
    assert!(matches!(mis.kind, RecordKind::NameMismatch | RecordKind::SuspectedStub));
    assert!(mis.evidence.provenance.iter()
        .any(|p| p.missing_action == Some(Action::CheckPermission)));
    assert!(recs.iter().all(|r| r.subject != fx.b));          // real check -> no mismatch

    let naming = reg.contracts_for(fx.a).iter()
        .find(|c| c.obligation == Obligation::Naming).unwrap();
    assert_eq!(naming.authority, Authority::InferredCandidate); // never Explicit via lexicon/LLM
    let lbl = adapter.label(mis, None).unwrap();               // None => LLM disabled
    assert_eq!(lbl.source, LabelSource::Deterministic);
}
```

### TASK-040: Infer candidate rules from deviant behavior

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-analysis::beliefs`, `cie-contract::beliefs` |
| Depends on | TASK-038 |
| PRD reference | §6 FR-12 / §7 Engler row + Kremenek row / §8 |
| Parallel group | G-5b |

**Constraints:**
- Signals: inconsistent checks (a condition honored at N−1 call sites), cleanup pairing (acquire/release), and API-usage regularities (Engler et al., `cie-analysis::beliefs`).
- Heterogeneous signals are combined in a Kremenek-style factor graph (`cie-contract::beliefs`) whose potentials are deterministic given the same input relations (no hidden nondeterminism).
- All outputs are contracts with `Authority::InferredCandidate` — the TASK-038 guarded API makes any other authority unreachable from this module (§8, global rule 4).
- Every candidate cites its support sites and the deviant sites in `Evidence.provenance` (global rule 2, FR-4).
- Beliefs are expected-behavior candidates: stored as contracts, never merged into observed-fact records (FR-6); `created_ns`/`compute_ns` in `Ns` (global rule 1).

**DoD:**
- Fixture: return value of `frob()` checked at 6 of 7 sites → candidate rule "retval of `frob` must be checked" with support 6 and the deviant site cited.
- Factor graph combines ≥ 2 signal families into one posterior.
- No candidate ever carries `Explicit`/`Verified`.
- Same inputs → byte-identical candidates (determinism).

Infer programmer beliefs the Engler way (§7): scan summaries and relations for inconsistent checks, acquire/
release cleanup pairing, and API-usage regularities, and treat the majority pattern as an implicit
specification. Heterogeneous signals are combined in a Kremenek-style factor graph (`cie-contract::beliefs`)
whose potentials are deterministic given the same input relations. Every output is an `InferredCandidate`
contract with its support sites and deviant sites cited in Evidence — deviant sites are the high-value audit
targets, and no belief is ever allowed to enter the registry as established (§8, FR-28).

**Validation test sketch (contract):**

```rust
#[test]
fn task_040_belief_inference() {
    let fx = fixture_checked_returns(); // 7 call sites of frob(); 6 check the status, 1 ignores it
    let signals = BeliefSignals::collect(&fx.summaries(), &fx.call_relations());
    assert_eq!(signals.check_consistency("frob").support(), 6);

    let mut graph = FactorGraph::new(); // Kremenek-style combination (cie-contract::beliefs)
    graph.add(signals.check_consistency("frob"));
    graph.add(signals.cleanup_pairing("lock", "unlock"));
    let beliefs = graph.infer();        // deterministic posterior over candidate rules

    let rule = beliefs.iter()
        .find(|b| b.predicate == Predicate::retval_checked("frob")).unwrap();
    assert_eq!(rule.authority, Authority::InferredCandidate);       // §8: inferred never established
    assert_eq!(rule.support_sites.len(), 6);
    assert!(rule.evidence.provenance.iter().any(|p| p.site == fx.deviant_site)); // Engler: deviant = suspect
    assert_eq!(graph.infer(), beliefs);                             // determinism
}
```

### TASK-041: Mine API protocols and refine candidates Houdini-style

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-contract::protocols`, `cie-contract::houdini` |
| Depends on | TASK-038 |
| PRD reference | §6 FR-12 / §7 Ammons row + Flanagan–Leino row / §12 Slice 1 |
| Parallel group | G-5b |

**Constraints:**
- Candidate sequencing obligations (`Obligation::Protocol`) are mined as automata over summary/relation-derived traces (Ammons, Bodík & Larus, `cie-contract::protocols`) and enter the registry as `InferredCandidate` only (global rule 4).
- Houdini loop (`cie-contract::houdini`): generate candidate annotations from mined protocols and summary predicates, then mechanically check each against all traces; refuted candidates are removed and logged with refuting evidence (global rule 2).
- Termination: bounded refinement rounds; every surviving candidate records the checks it survived.
- Unresolved calls are excluded from traces and recorded in `Evidence.unresolved` (FR-4).
- The refuted-candidate log is append-only and content-addressed for auditability (FR-8 spirit); keys follow §15.1 (global rule 7).

**DoD:**
- Fixture `begin`/`commit`/`rollback` API → candidate "commit requires prior begin" survives.
- A too-strong candidate ("commit never follows rollback") is refuted by an observed trace and removed.
- Surviving candidates stored as `Obligation::Protocol` contracts with `InferredCandidate` authority.
- Refuted log entry cites the exact refuting trace.

Mine candidate API protocols — sequencing obligations such as "commit requires prior begin" — as automata over
summary-derived traces, per the Ammons et al. row of §7 (`cie-contract::protocols`). Refine them Houdini-style
(`cie-contract::houdini`): generate candidate annotations from mined protocols and summary predicates, then
mechanically check each candidate against all traces and remove the refuted ones, logging each refutation with
its evidence. Surviving candidates are stored as `Obligation::Protocol` contracts with `InferredCandidate`
authority, and unresolved calls are excluded from traces but recorded in `Evidence.unresolved` (FR-4).

**Validation test sketch (contract):**

```rust
#[test]
fn task_041_protocol_mining_houdini() {
    let fx = fixture_txn_api();       // traces over begin/commit/rollback built from summaries + relations
    let mut miner = ProtocolMiner::bounded(2);              // Ammons et al., k-bounded automaton
    let cands = miner.mine(&fx.traces());
    assert!(cands.iter().any(|p| p.expr == Predicate::before(Action::Commit, Action::Begin)));

    let mut houdini = Houdini::new(miner.registry_view());  // Flanagan & Leino, cie-contract::houdini
    let survivors = houdini.refine(cands, &fx.traces());    // mechanically remove refuted candidates
    let keep = survivors.iter()
        .find(|c| c.expr == Predicate::before(Action::Commit, Action::Begin)).unwrap();
    assert_eq!(keep.authority, Authority::InferredCandidate);
    assert!(!survivors.iter()
        .any(|c| c.expr == Predicate::never_after(Action::Commit, Action::Rollback)));

    let refuted = houdini.refuted_log();                    // append-only, evidenced refutations
    assert!(refuted.iter().any(|r| r.candidate.expr == Predicate::never_after(Action::Commit, Action::Rollback)
        && r.by_trace == fx.trace_commit_after_rollback));
}
```

### TASK-042: Check conformance, distinguishing violations from suspicions

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-contract` (checker), `cie-analysis` (path realization) |
| Depends on | TASK-036, TASK-038, TASK-040, TASK-041 |
| PRD reference | §6 FR-11, FR-12 / §11 Stage 2 / §12 Slice 1 / §13 items 4–5 / §8 |
| Parallel group | G-5c |

**Constraints:**
- Authority governs the conclusion: breaching an `Explicit`/`Verified` contract ⇒ `Conclusion::Violated`; breaching only an `InferredCandidate` ⇒ `Conclusion::Suspected`; the two variants are distinct, never conflated or silently upgraded (§8, global rule 4).
- A suspected stub with no matching contract stays a behavioral fact; it becomes a violation only if a contract establishes the requirement (FR-11 conformance-finding example).
- Must detect fake authorization (no permission check behind the name), ignored rejection (failure status dropped by the caller), and swallowed exceptions (catch converted to success) — §13 items 4–5.
- Findings are DerivedRecords with Evidence; where a violating path is realized, a counterexample path is attached (global rule 2).
- A checker timeout yields `Conclusion::Unknown` with status `TimedOut`, never a clean verdict (global rule 3).

**DoD:**
- Fixture: fake authorization vs an Explicit contract → `Violated` record.
- Same code with only an InferredCandidate naming rule → `Suspected`, and no `Violated`.
- Ignored-rejection and swallowed-exception fixtures both detected (§13 items 4–5 hooks pass).
- Timeout path returns Unknown/TimedOut.

Build the Slice-1 conformance checker (FR-12, §11 Stage 2): realize paths from summaries and relations and check
them against the contract registry, attaching counterexample paths where a violating path is realized. The
checker's central duty is the authority split of §8: breaching an `Explicit`/`Verified` contract yields
`Conclusion::Violated`, while breaching only an `InferredCandidate` yields `Conclusion::Suspected` — never
conflated, never upgraded. It must detect fake authorization, ignored rejection, and swallowed exceptions
(§13 items 4–5), while a suspected stub with no matching contract stays a behavioral fact per FR-11, and a
checker timeout yields Unknown/TimedOut, never a clean verdict.

**Validation test sketch (contract):**

```rust
#[test]
fn task_042_conformance_authority_split() {
    let fx = fixture_fake_auth();    // fn authorize(u) -> bool { println!("auth {u}"); true }
    let mut reg = ContractRegistry::open_in_memory();
    reg.insert_explicit(Obligation::Safety,
        Predicate::requires(Action::Fulfill, Action::Authorized), ev_src()).unwrap();

    let out = ConformanceChecker::new(&reg, fx.summaries(), fx.call_relations()).run(fx.rev);
    let v = out.iter().find(|r| r.conclusion == Conclusion::Violated).expect("established violation");
    assert_eq!(v.subject, fx.authorize);                    // fake authorization detected (§13 #4)

    let mut reg2 = ContractRegistry::open_in_memory();      // same code, only an inferred naming rule
    reg2.insert_inferred(Obligation::Naming,
        Predicate::name_matches("authorize", "checks"), ev_inf()).unwrap();
    let out2 = ConformanceChecker::new(&reg2, fx.summaries(), fx.call_relations()).run(fx.rev);
    assert!(out2.iter().any(|r| r.conclusion == Conclusion::Suspected)); // suspicion, not violation
    assert!(out2.iter().all(|r| r.conclusion != Conclusion::Violated));

    for f in [fixture_ignored_rejection(), fixture_swallowed_exception()] { // §13 #4/#5
        assert!(ConformanceChecker::new(&reg, f.summaries(), f.call_relations()).run(f.rev)
            .iter().any(|r| matches!(r.conclusion, Conclusion::Violated | Conclusion::Suspected)));
    }
}
```

### TASK-043: Extract predicates and run bounded checks with budget semantics

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-analysis::predicates`, `cie-counterexample::smt` |
| Depends on | TASK-032, TASK-038 |
| PRD reference | §6 FR-13 / §12 Slice 2 / §7 Z3 row / §8 / §13 item 26 |
| Parallel group | G-5b |

**Constraints:**
- Predicates are extracted from contract `expr`s conjoined with path conditions derived from the relations/PDG layer (TASK-032), each predicate carrying provenance back to its contract and path (global rule 2).
- Checking is bounded by an explicit `Ns` budget and depth bound, both recorded as assumptions (FR-4, global rule 1).
- On budget exhaustion or solver timeout the result is `Conclusion::Unknown` with status `TimedOut` — never "no defect"/clean (§8, §13 item 26, global rule 3).
- A passing mechanical check can justify `InferredCandidate → Verified` only through the registry's guarded transition (TASK-038, global rule 4).
- Verdicts are sound within the stated bounds; out-of-bounds answers are forbidden and any unsupported construct is listed in `Evidence.unsupported` (FR-4).

**DoD:**
- Fixture contract "charge requires prior authorize": unauthorized path yields a violation trace with counterexample; authorized path checks clean.
- Budget-exhausted run returns Unknown/TimedOut for every predicate.
- Bounds and assumptions appear in the emitted Evidence.
- Verified upgrade flows only through the TASK-038 API.

Extract predicates from contract expressions conjoined with path conditions derived from the relations/PDG layer
(TASK-032) and run them through a bounded checker backed by SMT (`cie-counterexample::smt`, §7 Z3 row). The
check is bounded by an explicit nanosecond budget and depth recorded as assumptions (FR-4); on exhaustion the
result is `Conclusion::Unknown` with status `TimedOut` — the global rule that a timeout is never reported as
"no defect" (§8, §13 item 26). A passing mechanical check can upgrade a candidate to `Verified` only through the
registry's guarded transition (TASK-038, FR-28).

**Validation test sketch (contract):**

```rust
#[test]
fn task_043_bounded_predicate_check() {
    let fx = fixture_payment_paths();   // charge() with two paths: authorized / unauthorized
    let reg = registry_with(Contract::explicit(Obligation::Safety,
        Predicate::requires(Action::Charge, Action::Authorized), ev_src()));
    let preds = PredicateExtractor::new(&reg)
        .extract(&fx.summaries(), &fx.path_conditions());   // contracts + path conditions
    assert_eq!(preds.len(), 1);
    assert!(preds[0].evidence.provenance.iter().any(|p| p.from_contract.is_some()));

    let checker = BoundedChecker::new(&fx).budget_ns(5_000_000_000).depth(64);
    let r = checker.check(&preds);
    let viol = r.iter().find(|x| x.conclusion == Conclusion::Violated).expect("unauthorized path");
    assert!(viol.counterexample.as_ref().unwrap().path.ends_with(fx.unauthorized_call));

    let tight = BoundedChecker::new(&fx).budget_ns(1);       // exhausted immediately
    let r2 = tight.check(&preds);
    assert!(r2.iter().all(|x| x.conclusion == Conclusion::Unknown
        && x.status == Status::TimedOut));                   // rule 3: timeout ≠ no defect
}
```

### TASK-044: Implement the FCA core lattice builder

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-concepts::fca` |
| Depends on | TASK-037 |
| PRD reference | §6 FR-13 / §14.6 / §7 Wille row / §15.4 / §15.5 |
| Parallel group | G-5b |

**Constraints:**
- `FormalContext{objects, attributes, incidences: BitMatrix}` and `Concept{extent, intent, parent, children}` exactly per §14.6; extents/intents are BitSets (`fixedbitset`/`bitvec`, §15.5) — never `Vec<bool>`/`HashSet` (bitset-representation rule).
- Lattice construction (Next-Closure/In-Close style) emits concepts with a consistent cover relation: every concept except the top has exactly one parent; parent/children are mutual inverses.
- `[perf]` FCA closure step ≤ 2 µs × CI_TOLERANCE (§15.4); the perf test is tagged `#[perf]` and `#[ignore]`-gated and compares against the PRD target with a CI_TOLERANCE multiplier (global rule 5).
- Concepts are emitted through the TASK-037 record protocol (`concept:{concept_id}` + `record:{record_id}`, §15.1) with Evidence citing the verified attributes they close over (FR-13, global rules 2 & 7).
- Deterministic: same context ⇒ same lattice and same concept ordering; `created_ns`/`compute_ns` in `Ns` (global rule 1).

**DoD:**
- 4×4 fixture context yields exactly 6 concepts, including `({a,b},{1,2})`.
- Closure idempotence and the extent/intent fixed-point property hold.
- Cover relation validated on the fixture (parents/children inverse).
- Perf gate compiles, is `#[ignore]`-gated, and asserts the §15.4 target.

Implement the FCA core (`cie-concepts::fca`, Wille row of §7): `FormalContext{objects, attributes, incidences:
BitMatrix}` and `Concept{extent, intent, parent, children}` exactly per §14.6, with a Next-Closure/In-Close
lattice builder and a closure operation over BitSets (§15.5). Concepts are emitted through the TASK-037 record
protocol (`concept:{concept_id}`) with Evidence citing the verified attributes they close over, satisfying
FR-13's traceability requirement. The closure step carries the §15.4 performance target (≤ 2 µs) enforced by a
`#[perf]`-tagged, `#[ignore]`-gated test compared with a CI_TOLERANCE multiplier.

**Validation test sketch (contract):**

```rust
#[test]
fn task_044_fca_lattice_fixture() {
    // 4 objects x 4 attributes: a:{1} b:{1,2} c:{2,3} d:{2,4}  -> classic 6-concept lattice
    let ctx = FormalContext::from_rows(&[
        ("a", &[1]), ("b", &[1, 2]), ("c", &[2, 3]), ("d", &[2, 4]),
    ]);
    let lattice = LatticeBuilder::new().build(&ctx);            // Wille FCA, cie-concepts::fca

    assert_eq!(lattice.concepts().len(), 6);
    let ab = lattice.find_concept(&["a", "b"]).unwrap();
    assert_eq!(lattice.intent_of(ab), bitset(&[1, 2]));         // ({a,b}, {1,2}) is closed
    for c in lattice.concept_ids() {                            // cover relation: parent/children inverse
        for &ch in lattice.children_of(c) {
            assert_eq!(lattice.parent_of(ch), Some(c));
        }
    }
    let closed = ctx.closure(bitset(&[1]));
    assert_eq!(ctx.extent_of(&closed), bitset(&["a", "b"]));    // closure({1}) = extent {a,b} / intent {1,2}
}

#[test]
#[ignore] // #[perf]-gated: run with `cargo test -- --ignored` in CI (global rule 5)
fn perf_task_044_fca_closure_step() {
    let ctx = FormalContext::random(10_000, 500, 42);
    let t = bench_ns(|| ctx.closure(random_subset(&ctx, 64)));
    assert!(t <= 2_000 * CI_TOLERANCE, "closure step {t} ns exceeds §15.4 target (2 µs)");
}
```

### TASK-045: Add incremental FCA updates (Godin–Missaoui–Alaoui)

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-concepts::incremental_fca` |
| Depends on | TASK-044 |
| PRD reference | §6 FR-13 / §7 Godin row / §14.6 / §8 |
| Parallel group | G-5c |

**Constraints:**
- Godin–Missaoui–Alaoui incremental construction: inserting a new object (or attribute) updates only the local concepts whose intents intersect the new intent (AddIntent-style); untouched concept records are neither recomputed nor re-emitted (FR-8 spirit).
- Equivalence contract: incremental lattice ≡ from-scratch TASK-044 lattice (same concept set and same cover relation), enforced by a randomized property test (global rule 5: tests are contracts).
- The affected concept set is published through the record protocol's `rev_deps:{symbol_id}` (TASK-037) so downstream consumers invalidate exactly the changed concepts (global rule 7).
- Deletion is not claimed incrementally: it falls back to a local context rebuild with an explicit Evidence note (FR-4 honesty).
- `created_ns`/`compute_ns` in `Ns` (global rule 1).

**DoD:**
- Randomized property test: incremental == batch over 100 contexts (≤ 12×12).
- Single-object addition on the fixture touches strictly fewer concepts than a full rebuild.
- `rev_deps` lists exactly the recomputed concepts.
- Determinism: same insertion order ⇒ same lattice.

Add Godin–Missaoui–Alaoui incremental lattice construction (`cie-concepts::incremental_fca`, §7): inserting a
new object (or attribute) updates only the local concepts whose intents intersect the new intent, leaving
untouched concept records unmodified. Equivalence with the from-scratch TASK-044 lattice — same concept set and
same cover relation — is a binding randomized property test, and the affected set is published through the
record protocol's rev_deps so downstream consumers invalidate exactly the changed concepts (FR-8). Deletion is
not claimed incrementally: it falls back to a local rebuild with an explicit Evidence note (FR-4).

**Validation test sketch (contract):**

```rust
proptest! {
    #[test]
    fn task_045_incremental_equals_batch(rows in prop::collection::vec(
        prop::collection::hash_set(0u8..12, 1..8), 1..12))
    {
        let ctx = FormalContext::from_row_sets(&rows);
        let batch = LatticeBuilder::new().build(&ctx);          // TASK-044 from-scratch lattice
        let mut inc = IncrementalLattice::new();
        for (i, obj) in ctx.objects.iter().enumerate() {
            inc.add_object(*obj, ctx.attributes_of(*obj));      // Godin–Missaoui–Alaoui local update
            if i < 3 {
                let partial = LatticeBuilder::new().build(&ctx.prefix(i + 1));
                assert_eq!(inc.lattice(), &partial);            // holds after every step
            }
        }
        assert_eq!(inc.lattice().concepts(), batch.concepts()); // same concept set
        assert_eq!(inc.lattice().cover_edges(), batch.cover_edges()); // same hierarchy
    }
}

#[test]
fn task_045_local_update_scope() {
    let fx = fixture_4x4_context();                             // same 4x4 context as TASK-044
    let mut inc = IncrementalLattice::from(fx.ctx_without("d"));
    let touched = inc.add_object(fx.id_of("d"), &fx.attrs_of("d"));
    assert!(touched.len() < fx.full_rebuild_concept_count());   // strictly local update
    assert_eq!(inc.rev_deps_touched(), touched);                // rev_deps == recomputed set (FR-8)
}
```

### TASK-046: Produce clustering and hierarchy candidates

| Field | Value |
|---|---|
| Phase | 5 — Behavior Extraction, Contracts, FCA & Hierarchy |
| Target crate(s) | `cie-concepts::clustering`, `cie-concepts::hierarchy`, `cie-concepts::layered` |
| Depends on | TASK-009, TASK-044 |
| PRD reference | §6 FR-14, FR-10 / §14.7 / §7 Andritsos row + Peixoto rows / §12 Slice 3 |
| Parallel group | G-5c |

**Constraints:**
- Membership is structural only: information-loss-minimization clustering baseline (Andritsos–Tzerpos, `cie-concepts::clustering`), nested stochastic-block hierarchy for candidate scales (Peixoto 2014, `cie-concepts::hierarchy`), and a layered model keeping calls and ownership as distinct relation layers, never merged (Peixoto 2015, `cie-concepts::layered`).
- `HierarchyNode{label: Label, children, evidence}` exactly per §14.7; labels are immutable after write and NEVER influence membership or verification (FR-10, global rule 4).
- Labels come from `Label{source, model_id, prompt_hash}` (LLM) or a deterministic fallback name when the LLM is disabled (FR-28).
- Every candidate group carries an `EvidenceRef` citing the exact call/ownership edges that induced it (global rule 2, FR-14); results stay `InferredCandidate` until reviewed examples upgrade them (§12 Slice 3).
- Nodes are stored under `hier:{node_id}` with labels under `label:{record_id}` (§15.1, global rule 7).

**DoD:**
- Fixture: two 3-symbol modules with dense intra-calls → clustering baseline recovers both groups.
- Nested hierarchy yields ≥ 2 levels; layered model preserves calls and ownership edges distinctly.
- Label immutability enforced (second write rejected); writing an LLM label does not change membership.
- With LLM disabled, deterministic fallback labels are present.

Produce evidence-backed structural hierarchy candidates (FR-14, §12 Slice 3): an information-loss-minimization
clustering baseline (Andritsos–Tzerpos), a nested stochastic-block hierarchy for candidate scales (Peixoto 2014,
`cie-concepts::hierarchy`), and a layered model that keeps calls and ownership as distinct relation layers
(Peixoto 2015, `cie-concepts::layered`). Membership is decided purely by structure;
`HierarchyNode{label: Label, children, evidence}` (§14.7) carries immutable `Label{source, model_id,
prompt_hash}` annotations written afterwards that can never affect membership or verification (FR-10, FR-28).
Every candidate group cites the exact call/ownership edges that induced it, is stored under `hier:{node_id}`,
and remains `InferredCandidate` until reviewed examples upgrade it (§12 Slice 3).

**Validation test sketch (contract):**

```rust
#[test]
fn task_046_hierarchy_candidates() {
    let fx = fixture_two_modules(); // 6 symbols: dense intra-module calls, sparse cross-calls, ownership edges
    let base = ClusteringBaseline::new().minimize_information_loss(&fx.call_graph());
    assert_eq!(base.groups().len(), 2);                                  // Andritsos–Tzerpos baseline
    assert!(base.same_group(fx.a, fx.b) && !base.same_group(fx.a, fx.x));

    let hier = NestedSbm::with_levels(2).fit(&fx.layered_graph());       // Peixoto 2014/2015
    assert!(hier.depth() >= 2);
    assert!(hier.layer_edges(RelKind::Calls).count() > 0
         && hier.layer_edges(RelKind::Owns).count() > 0);                // layers kept distinct

    let nodes = hier.to_hierarchy_nodes(&base, &fx.evidence_for_groups()); // §14.7, evidence-backed
    assert!(nodes.iter().all(|n| !n.evidence.provenance.is_empty()));

    let mut store = HierStore::open_in_memory();
    let n0 = nodes[0].id;
    store.put_node(&nodes[0]).unwrap();
    let before = store.members(n0);
    let llm = Label { source: LabelSource::Llm, model_id: "llama3", prompt_hash: [9u8; 32], text: "auth module" };
    store.write_label(n0, llm).unwrap();
    assert_eq!(store.members(n0), before);                               // labels never affect membership
    assert!(store.write_label(n0, llm.with_text("changed")).is_err());   // labels immutable (FR-10)
}
```

## Phase 6 — Counterexamples: SMT, CEGAR & Symbolic Execution

Counterexample generation is what upgrades CIE's derived facts into decisive findings: instead of "suspected violation," the engine exhibits a concrete path, valuation, or executable test that demonstrates the defect. This phase builds the SMT foundation (budgeted Z3), the CEGAR refinement loop that removes spurious alarms, SLAM-style API protocol checking over the relation graph, and KLEE-style symbolic execution that emits runnable reproductions. All raw artifacts flow into TASK-051's packaging layer, which attaches them to `DerivedRecord`s and supports the §13 item 18 double-charge acceptance case.

### TASK-047: Encode predicates and path conditions into budgeted Z3 queries

| Field | Value |
|---|---|
| Phase | 6 — Counterexamples: SMT, CEGAR & Symbolic Execution |
| Target crate(s) | `cie-counterexample::smt` |
| Depends on | TASK-032, TASK-043 |
| PRD reference | §7 (de Moura & Bjørner — Z3, TACAS 2008) / FR-34 (Constraint checker) / FR-7 / §15.4 |
| Parallel group | G-6a |

**Constraints:**
- Every solver call takes an explicit `budget_ns: NsDelta`; expiry maps Z3's timeout to `SmtOutcome::Unknown(SmtUnknownReason::TimedOut)` — a timed-out query must never be reported feasible or clean (global rule 3, §13 item 26).
- Each encoded predicate keeps its `SymbolId` + `SourceLocation` provenance so extracted models map back into `Evidence` (global rule 2, FR-4).
- Budgets/deadlines are pure `Ns`/`NsDelta` arithmetic; no `chrono::DateTime` on the solver path (FR-21, global rule 1).
- SMT verdicts are mechanical facts: they may verify only `Authority::Explicit` obligations; LLM-sourced rules never gain authority from a solver result (FR-28, global rule 4).

**DoD:**
- `SmtEncoder` translates `PathCond`, extracted numeric ranges, and registry `Predicate`s into Z3 (int/bool/bitvec sorts).
- `solve_path(conds, obligations, budget_ns) -> Feasible(Model) | Infeasible | Unknown(reason)` with per-`ValueId` model extraction.
- Per-call Z3 timeout derived from `budget_ns`; cancellation polled between obligations (FR-7 bounded analysis).
- Unit tests cover feasible-with-model, infeasible, and forced-timeout paths; sketch below is the binding contract.

Build the SMT layer that every later counterexample algorithm calls: it turns extracted predicates (TASK-032) and bounded obligations (TASK-043) into Z3 formulas, answers path-feasibility questions, and extracts satisfying assignments as candidate counterexample valuations. Calls are strictly budgeted so unbounded solving can never stall ingestion. Downstream, TASK-048 uses it for concretization and TASK-050 for branch feasibility.

**Validation test sketch (contract):**

```rust
#[test]
fn task_047_smt_budgeted_feasibility_and_model_extraction() {
    let enc = SmtEncoder::new();
    // arrange: x has extracted range [0,100]; obligation x <= 99; branch condition x > 50
    let x = enc.declare_int(ValueId(7));
    enc.assume_range(x, 0, 100);
    let branch = enc.gt(x, enc.int_const(50));
    let obligation = enc.le(x, enc.int_const(99));

    // act/assert 1: branch feasible under the obligation -> model assigns x in 51..=99
    match enc.solve_path(&[branch.clone()], &[obligation.clone()], NsDelta(50_000_000)) {
        SmtOutcome::Feasible(model) => {
            let v = model.int_of(x).expect("model assigns x");
            assert!((51..=99).contains(&v));
        }
        _ => panic!("expected Feasible within 50 ms budget"),
    }

    // act/assert 2: `x > 100` contradicts the range fact -> Infeasible, not Unknown
    let contradiction = enc.gt(x, enc.int_const(100));
    assert!(matches!(
        enc.solve_path(&[contradiction], &[obligation], NsDelta(50_000_000)),
        SmtOutcome::Infeasible
    ));
    // act/assert 3: a 1 ns budget must yield Unknown(TimedOut) — never Feasible/Infeasible
    assert!(matches!(
        enc.solve_path(&[branch], &[], NsDelta(1)),
        SmtOutcome::Unknown(SmtUnknownReason::TimedOut)
    ));
}
```

### TASK-048: Refine abstractions with CEGAR until counterexamples are concrete

| Field | Value |
|---|---|
| Phase | 6 — Counterexamples: SMT, CEGAR & Symbolic Execution |
| Target crate(s) | `cie-counterexample::cegar` |
| Depends on | TASK-047 |
| PRD reference | §7 (Clarke et al. — CEGAR, CAV 2000) / FR-12 / §8 (status rules) |
| Parallel group | G-6b |

**Constraints:**
- Refinement is bounded: `max_rounds` + per-round `budget_ns`; exhaustion returns `CegarOutcome::Unknown(RefinementExhausted)` — the possibly-spurious alarm must never be emitted as a violation (global rule 3, §8).
- A counterexample is reported only after the concretization query (TASK-047 SMT) proves the trace feasible; spurious traces refine the abstraction and never leak out as findings.
- Every added predicate is recorded with provenance into `Evidence.assumptions` so refinement steps are auditable (global rule 2).
- `AbstractDomain` is language-neutral; per-language adapters vouch that domain operations are valid for their semantics (FR-3, global rule 6).

**DoD:**
- `AbstractDomain` trait (join/meet/concretize/encode-to-SMT) with a predicate-abstraction implementation.
- Loop: model-check abstraction → spurious? add distinguishing predicate : emit counterexample — with a false-alarm-reduction metric (`spurious_traces`, `rounds_used`).
- Deterministic refinement order (predicates sorted by `(SymbolId, SourceLocation)`) for reproducible outcomes.
- Metric + evidence exported on `CegarLoop::metrics()` / `CegarLoop::evidence()`; sketch below is binding.

Implement the counterexample-guided abstraction refinement loop over the bounded model-checking view of a function or flow. The abstraction is cheap but over-approximates; when the abstract run reaches an error state, SMT concretization decides whether the trace is real. Spurious traces contribute their distinguishing predicate back into the domain, so each round strictly sharpens the check and the false-alarm rate measurably drops.

**Validation test sketch (contract):**

```rust
#[test]
fn task_048_cegar_refines_spurious_counterexample() {
    // arrange: abstraction knows only predicate p; the error state needs `p && q`
    let mut cegar = CegarLoop::new(
        PredAbstractDomain::new(vec![pred_p()]),
        smt_backend_from_task_047(),
    );
    let program = fixture_cfa_with_error_needing(pred_q());

    // act: bounded refinement
    let outcome = cegar.check(
        &program,
        RefineLimits { max_rounds: 8, round_budget_ns: NsDelta(10_000_000) },
    );

    // assert: round-1 trace was spurious -> refined, never reported as a violation
    assert!(matches!(outcome, CegarOutcome::Safe(_) | CegarOutcome::Counterexample(_)));
    assert_eq!(cegar.metrics().spurious_traces, 1, "p-only trace must be classified spurious");
    assert!(cegar.refinement_predicates().contains(&pred_q()), "distinguishing predicate joined the domain");

    // assert: false-alarm reduction metric and refinement provenance in evidence
    assert!(cegar.metrics().false_alarm_reduction > 0.0);
    let ev = cegar.evidence();
    assert!(ev.assumptions.iter().any(|a| a.text.contains("refine:")), "refinement auditable");

    // assert: budget exhaustion degrades to Unknown, not Safe/Counterexample
    let starved = CegarLoop::new(PredAbstractDomain::new(vec![]), smt_backend_from_task_047())
        .check(&program, RefineLimits { max_rounds: 1, round_budget_ns: NsDelta(1) });
    assert!(matches!(starved, CegarOutcome::Unknown(_)));
}
```

### TASK-049: Check API protocol rules SLAM-style over the relation graph

| Field | Value |
|---|---|
| Phase | 6 — Counterexamples: SMT, CEGAR & Symbolic Execution |
| Target crate(s) | `cie-counterexample::slam` |
| Depends on | TASK-047, TASK-038 |
| PRD reference | §7 (Ball & Rajamani — SLAM, POPL 2002) / FR-12 / §14.5 (Obligation) |
| Parallel group | G-6b |

**Constraints:**
- Only `Obligation::Protocol` contracts with `Authority::Explicit` or `Verified` are checked as established rules; `InferredCandidate` hits are labeled "suspected" and never promoted (FR-12, §8, global rule 4).
- A violation is a feasible path in the product of the rule automaton and the CSR call graph; the emitted trace cites `SourceLocation` per transition (global rule 2).
- Per-rule `budget_ns`; on expiry the outcome is `RuleOutcome::Unknown(TimedOut)`, never "rule holds" (global rule 3).
- Machine events are language-agnostic (`authorize_ok`, `fulfill`, …); each language adapter maps its API calls onto events and vouches for the mapping (FR-3, global rule 6).

**DoD:**
- `ProtocolMachine` builder (states, events, error state) + `check_rule(machine, csr, budget_ns) -> Holds | Violated(Trace) | Unknown(r)`.
- Product construction over `CsrGraph` with DFS + on-stack visited set for cycles; spurious abstract hits handed to TASK-048 for refinement.
- Violation traces passed to `cie-counterexample::package` (TASK-051) with per-hop locations and evidence.
- Fixture reproduces the PRD protocol "failure must prevent fulfillment"; sketch below is binding.

Check protocol-shaped obligations — sequences like "a rejected authorization must prevent fulfillment" or "lock must not be re-acquired while held" — by model checking the rule automaton against the interprocedural relation graph from TASK-038. Rules come from the contract registry, so established and suspected rules stay separated. A violated rule yields a concrete call-sequence trace, which TASK-051 packages as the counterexample of a conformance finding.

**Validation test sketch (contract):**

```rust
#[test]
fn task_049_slam_detects_fulfillment_after_rejected_auth() {
    // arrange: rule "failure must prevent fulfillment": reject -> fulfill is the error edge
    let rule = ProtocolMachine::builder()
        .states(["Idle", "Authorized", "Rejected", "Error"])
        .on("authorize_ok", |s| s == &S::Idle).to(S::Authorized)
        .on("authorize_reject", |s| s == &S::Idle).to(S::Rejected)
        .on("fulfill", |s| s == &S::Rejected).to(S::Error) // violation edge
        .build();
    let csr = fixture_csr_caller_rejects_then_fulfills(); // from TASK-038

    // act
    let outcome = check_rule(&rule, &csr, NsDelta(50_000_000));

    // assert: violation with a concrete 2-hop trace citing both callsites
    match outcome {
        RuleOutcome::Violated(trace) => {
            assert_eq!(trace.events.len(), 2);
            assert_eq!(trace.events[0].name, "authorize_reject");
            assert_eq!(trace.events[1].name, "fulfill");
            assert!(trace.events.iter().all(|e| e.loc != SourceLocation::NONE),
                "every hop cites its callsite");
        }
        other => panic!("expected Violated, got {other:?}"),
    }

    // assert: an unrelated rule that cannot reach Error must hold, not time out
    let trivial = ProtocolMachine::builder().states(["Idle", "Error"]).build();
    assert!(matches!(check_rule(&trivial, &csr, NsDelta(50_000_000)), RuleOutcome::Holds));
}
```

### TASK-050: Explore paths symbolically and emit concrete reproduction tests

| Field | Value |
|---|---|
| Phase | 6 — Counterexamples: SMT, CEGAR & Symbolic Execution |
| Target crate(s) | `cie-counterexample::symbolic` |
| Depends on | TASK-047 |
| PRD reference | §7 (Cadar, Dunbar & Engler — KLEE, OSDI 2008) + §7 Qualification / FR-34 (Prohibition checker) / FR-7 |
| Parallel group | G-6b |

**Constraints:**
- Exploration is bounded by `max_steps` and `budget_ns`, whichever first; unexplored branches are recorded as incomplete coverage — never silently "no defect" (global rule 3, FR-7).
- Unresolved callees/externals end a path as `PathTermination::Unresolved` and land in `Evidence.unresolved`; no invented environment semantics (global rule 2, §7 qualification).
- Generated tests are deterministic and self-contained: concrete inputs plus an assertion of the violated prohibition, each citing the counterexample id for traceability.
- Symbolic semantics come only from the vouching language adapter's summaries; no cross-language rule reuse (FR-3, global rule 6).

**DoD:**
- `SymbolicExecutor::run(entry, limits) -> Vec<SymbolicPath>` with per-path condition, concrete model, and termination reason.
- `concretize(model) -> GeneratedTest` emits compilable test source (inputs + expected-violation assertion).
- §7 qualification (accuracy of semantics, environment model, exploration limits) documented in module docs and surfaced in `Evidence.assumptions`.
- Fixture reaches the double-charge error state and produces a runnable reproduction; sketch below is binding.

Provide KLEE-style bounded symbolic execution over CIE summaries: inputs are marked symbolic, branches fork, and TASK-047's SMT decides feasibility so the explorer never wanders down impossible paths. When a path reaches a prohibited state (e.g., a second charge of the same card), the concrete model is turned into an executable test that reproduces the defect. The module is explicitly qualified per §7 — it is sound only within accurate semantics, a supplied environment model, and its exploration limits.

**Validation test sketch (contract):**

```rust
#[test]
fn task_050_symbolic_exec_finds_and_reproduces_double_charge() {
    // arrange: charge(path) charges the card again whenever the `retry` flag holds
    let exe = SymbolicExecutor::from_summaries(fixture_double_charge_summaries());
    let limits = ExecLimits { max_steps: 512, budget_ns: NsDelta(200_000_000) };

    // act
    let paths = exe.run(SymbolId(9001), limits);

    // assert: some path reaches the prohibition; every path has a termination reason
    let bad = paths.iter().find(|p| p.reached_error(VIOLATION_DOUBLE_CHARGE))
        .expect("double-charge path must be discovered");
    assert_eq!(bad.termination, PathTermination::ErrorState);
    assert!(paths.iter().all(|p| p.termination != PathTermination::Running),
        "budgets force a terminal state on every explored path");
    assert_eq!(bad.evidence.unresolved.len(), 0, "fixture has no unresolved externals");

    // assert: generated test is a concrete, traceable reproduction
    let test = concretize(&bad.model);
    assert!(test.source.contains("retry = true"), "model assigns the retry flag");
    assert!(test.source.contains("charge_card"), "test exercises the charging call");
    assert!(test.source.contains(&format!("// cex:{}", bad.id)), "test cites its counterexample id");
}
```

### TASK-051: Package counterexamples into DerivedRecords with minimal reproductions

| Field | Value |
|---|---|
| Phase | 6 — Counterexamples: SMT, CEGAR & Symbolic Execution |
| Target crate(s) | `cie-counterexample::package` (+ `cie-core`) |
| Depends on | TASK-047, TASK-050 |
| PRD reference | §14.9 (Result Protocol) / FR-9 / §13 item 18 / §15.1 (`record:`, `evidence:`) |
| Parallel group | G-6c |

**Constraints:**
- `Counterexample { path, assignments, generated_test, evidence }` always carries a populated `EvidenceRef` (rev, provenance, assumptions, unresolved); a concluded violation without evidence is a contract violation (global rule 2, FR-4).
- Shrinking accepts a candidate minimal reproduction only if it still reaches the error state under the same model constraints — minimality never weakens the counterexample.
- `assignments` are concrete `ValueId → Valuation` pairs and packaging stamps `created_ns: Ns` (global rule 1).
- Packaging never upgrades authority: a counterexample for an `InferredCandidate` rule remains a suspected violation (FR-12, FR-28, global rule 4).

**DoD:**
- `package(record, raw_cex)` fills `DerivedRecord.counterexample` (§14.9) and writes `record:{id}` / `evidence:{id}` per §15.1.
- Trace shrinker reduces path length while re-validating error reachability after each candidate cut.
- Integration path for §13 item 18: the "must never double-charge" claim yields `Contradicting` with a counterexample path attached to `ClaimVerification` (§14.10).
- Round-trip test proves struct shape, storage keys, and shrink correctness; sketch below is binding.

Give every checker in Phases 6–7 one uniform output shape: this task defines the `Counterexample` struct and the packaging pipeline that turns raw SMT models, SLAM traces, and symbolic paths into the `counterexample: Option<Counterexample>` field of `DerivedRecord` (§14.9). It minimizes each reproduction — smallest path suffix plus concrete inputs that still triggers the violation — so reports stay readable and tests stay fast. Packaging is also the bridge to drift (§14.10): packaged counterexamples ride on `ClaimVerification` to support acceptance item 18.

**Validation test sketch (contract):**

```rust
#[test]
fn task_051_package_double_charge_counterexample() {
    // arrange: raw 5-step symbolic path from TASK-050 for the double-charge prohibition
    let raw = fixture_double_charge_raw_cex(); // 5 path steps, retry branch
    let mut record = DerivedRecord {
        kind: RecordKind::ProhibitionViolation,
        counterexample: None,
        ..base_record(REV_42)
    };

    // act: package + minimize
    package(&mut record, raw);
    let cex = record.counterexample.expect("§14.9: counterexample must be attached");

    // assert: struct shape and evidence invariants
    assert!(!cex.path.is_empty());
    assert!(cex.path.len() < 5, "minimal reproduction shrank the 5-step path");
    assert_eq!(cex.assignments[0].value, Valuation::Bool(true)); // retry = true
    assert!(cex.generated_test.source.contains("charge_card"));
    assert_eq!(cex.evidence.rev, REV_42);
    assert!(!cex.evidence.provenance.is_empty(), "concluded status requires evidence");
    // assert: storage keys written per §15.1 namespaces
    assert!(store_contains(&format!("record:{}", record.id)));
    assert!(store_contains(&format!("evidence:{}", record.id)));

    // assert: §13 item 18 — claim verification surfaces Contradicting with this path
    let ver = verify_claim(fixture_never_double_charge_claim(), REV_42);
    assert_eq!(ver.status, DriftStatus::Contradicting);
    assert!(ver.counterexamples.iter().any(|c| c.path == cex.path));
}
```

## Phase 7 — Recovery Execution & Fault Injection

Phase 7 turns the engine toward cross-service failure behavior: recovery and payment flows are modeled as explicit state machines whose safety and liveness obligations become checkable contracts (IronFleet-style), lineage-driven fault injection (LDFI) finds the message drops, duplicates, and reorders that break top-up/refund recovery, and error-handling detectors (Yuan et al., OSDI 2014) prioritize swallowed exceptions and misleading success. Together they reproduce the FR-15 acceptance evidence — ineffective refunds, interrupted schedulers, duplicate recovery — while keeping the safety/liveness distinction intact in every finding.

### TASK-052: Model recovery flows as checkable safety and liveness obligations

| Field | Value |
|---|---|
| Phase | 7 — Recovery Execution & Fault Injection |
| Target crate(s) | `cie-contract::obligations` |
| Depends on | TASK-038, TASK-042 |
| PRD reference | FR-15 / §7 (Hawblitzel et al. — IronFleet, SOSP 2015) / §14.5 (Contract) / §12 slice 4 |
| Parallel group | G-7a |

**Constraints:**
- Safety and liveness are distinct `Obligation` kinds; a liveness finding must never be reported as a safety violation and vice versa (FR-15 "Must distinguish").
- `refund_called` and `refund_effective` are separate events: a refund method being called does not establish that money was returned — only an externally-observed effective-refund transition satisfies liveness (FR-15 Important note).
- Transitions cite `EffectFact` provenance (reads/writes, `external` flag) so every machine edge is evidence-backed (global rule 2, FR-4).
- Obligations derived from inferred transitions are `Authority::InferredCandidate`; only Explicit contracts may reach Verified, and never via LLM output (FR-28, global rule 4).

**DoD:**
- `StateModelBuilder::from_effects(summaries) -> StateModel` with typed events (authorize, charge, refund_called, refund_effective, …).
- `obligations(model) -> Vec<Contract>` emitting at least the FR-15 templates: safety "fulfillment requires established authorization" and liveness "every debit eventually reaches fulfillment or an effective refund".
- Model edges stored under the contract registry with evidence refs; timestamps are `Ns` (global rule 1).
- Fixture proves the call-vs-effective refund distinction; sketch below is binding.

Build the obligation modeler for stage 4 (§12 slice 4): recovery, top-up, and payment flows are abstracted into explicit state machines whose transitions come from extracted effects rather than guesswork. Safety obligations ("fulfillment requires established authorization") and liveness obligations ("every debit eventually reaches fulfillment or an effective refund") are registered as checkable `Contract`s, giving TASK-053's fault injection and the SLAM checker concrete properties to attack. The module bakes in the PRD's critical distinction: calling a refund method is an event; money being returned is a different, externally-observable event.

**Validation test sketch (contract):**

```rust
#[test]
fn task_052_refund_call_does_not_establish_refund_effective() {
    // arrange: code path where refund() only logs — no external write ever occurs
    let summaries = fixture_refund_logs_only(); // EffectFact { external: false, writes: [log] }
    let model = StateModelBuilder::from_effects(&summaries);
    let obligations = obligations(&model);

    // assert: both FR-15 obligation kinds registered, distinct contracts
    let safety = obligations.iter().find(|c| c.obligation == Obligation::Safety).expect("safety obligation");
    let liveness = obligations.iter().find(|c| c.obligation == Obligation::Liveness).expect("liveness obligation");
    assert_ne!(safety.id, liveness.id);
    assert_eq!(safety.authority, Authority::InferredCandidate, "inferred transitions never auto-verify");

    // assert: the CRITICAL fact — refund *called* ≠ money *returned*
    let t = model.on(State::RefundRequested, Event::RefundCalled).expect("call transition exists");
    assert_ne!(t.to, State::Refunded,
        "a refund method being called does not establish that money was returned");

    // assert: only an effective-refund transition (external write evidence) reaches Refunded
    let eff = model.on(State::RefundPending, Event::RefundEffective).expect("effective transition");
    assert_eq!(eff.to, State::Refunded);
    assert!(eff.provenance.iter().all(|p| p.external_write), "effectiveness needs external-write evidence");

    // assert: every edge is provenance-backed (global rule 2)
    assert!(model.edges().iter().all(|e| !e.provenance.is_empty()));
}
```

### TASK-053: Plan lineage-driven fault injection over execution lineage

| Field | Value |
|---|---|
| Phase | 7 — Recovery Execution & Fault Injection |
| Target crate(s) | `cie-fault::ldfi` |
| Depends on | TASK-052 |
| PRD reference | FR-15 / §7 (Alvaro, Rosen & Hellerstein — LDFI, SIGMOD 2015) / §12 slice 4 / §7 Qualification |
| Parallel group | G-7b |

**Constraints:**
- Hypotheses derive only from observed lineage edges; no fabricated message/channel facts (§7 qualification "LDFI needs successful execution lineage"; missing lineage → `Unknown(NoLineage)`, never "recovery is sound" — global rules 2 and 3).
- Experiments are bounded: `budget_ns` wall clock + `max_faults` per round; overrun yields `ExperimentOutcome::Unknown(TimedOut)`, never a soundness verdict (global rule 3).
- Every applied fault is recorded with its lineage-edge provenance and `Ns` timestamps (global rules 1 and 2).
- Injection happens behind the `RuntimeFaultInjector` trait; the planner never touches real systems directly (FR-7 bounded, auditable execution).

**DoD:**
- `LineageGraph` ingestion + `LdfiPlanner::plan(property) -> Vec<FaultSet>` ranked minimal-faults-first.
- `RuntimeFaultInjector` trait: `drop`, `duplicate`, `reorder`, `delay`, `crash_scheduler` + a deterministic `SimInjector` for tests.
- Reproduces the FR-15 trio: dropped message breaking top-up recovery, duplicated refund causing double refund, crashed scheduler tick interrupting recovery.
- All experiment results persist with evidence for TASK-051 packaging; sketch below is binding.

Implement lineage-driven fault injection: from a successful execution's lineage graph — the dataflow/message events captured by the runtime adapter — enumerate minimal fault hypotheses whose negation breaks the TASK-052 liveness property, then run those hypotheses through an injectable target. Because LDFI reasons backward from the successful lineage, it finds targeted combinations (drop the credit event, duplicate the refund message) instead of random chaos. Per §7's qualification, the planner is honest about its precondition: without successful execution lineage it returns `Unknown(NoLineage)`.

**Validation test sketch (contract):**

```rust
#[test]
fn task_053_ldfi_finds_drop_that_breaks_topup_recovery() {
    // arrange: successful top-up lineage: debit -> ledger -> credit_event -> balance_update
    let lineage = fixture_topup_lineage();
    let property = liveness_from_task_052(); // every debit reaches fulfillment or effective refund

    // act: enumerate minimal fault sets, execute through the deterministic simulator
    let mut planner = LdfiPlanner::new(lineage, SimInjector::new());
    let rounds = planner.run(&property, RoundLimits { max_faults: 2, budget_ns: NsDelta(500_000_000) });

    // assert: a single dropped credit_event breaks liveness -> counterexample reported
    let hits: Vec<_> = rounds.results.iter().filter(|r| r.property_violated).collect();
    assert!(!hits.is_empty(), "LDFI must reproduce interrupted recovery");
    assert_eq!(hits[0].faults.len(), 1, "first failure is a minimal single-fault set");
    assert!(matches!(&hits[0].faults[0], Fault::Drop(e) if e.kind == LineageKind::Message));

    // assert: duplicate-refund hypothesis is also exercised (FR-15 target)
    assert!(rounds.results.iter().any(|r| r.faults.iter()
        .any(|f| matches!(f, Fault::Duplicate(e) if e.kind == LineageKind::Message))));

    // assert: no lineage input degrades to Unknown(NoLineage) — never "recovery is sound"
    let empty = LdfiPlanner::new(LineageGraph::empty(), SimInjector::new())
        .run(&property, RoundLimits::default());
    assert!(matches!(empty, Err(LdfiError::Unknown(LdfiUnknown::NoLineage))));
}
```

### TASK-054: Detect error-handling failures prioritized by safety and liveness impact

| Field | Value |
|---|---|
| Phase | 7 — Recovery Execution & Fault Injection |
| Target crate(s) | `cie-fault::error_handling` |
| Depends on | TASK-053, TASK-036 |
| PRD reference | §7 (Yuan et al. — Simple Testing Can Prevent Most Critical Failures, OSDI 2014) / FR-15 / FR-12 |
| Parallel group | G-7c |

**Constraints:**
- Findings carry an explicit `ThreatClass` of `Safety` or `Liveness` per FR-15; a swallowed refund failure is reported as a potential ineffective refund, never as "recovery handled" (global rule 3 spirit, FR-15 Important note).
- "Misleading success" requires two-sided evidence — error path taken plus success value returned — never naming alone (global rule 2, evidence-backed).
- Detector output is `Authority::InferredCandidate`; detectors never upgrade contract authority (FR-28, global rule 4).
- Exception semantics are per-language via adapter-provided `ExceptionFact` (Go error values, C errno, Rust `Result`); no cross-language rule reuse (FR-3, global rule 6).

**DoD:**
- Detectors: `SwallowedException`, `EmptyHandler`, `LogAndContinue`, `MisleadingSuccess`, `EarlyReturnInRetryLoop`.
- Each finding: severity, safety/liveness classification, `SourceLocation` chain, and evidence refs ready for TASK-051 packaging.
- Acceptance fixtures reproduce the FR-15 trio: ineffective refund, interrupted scheduler, duplicate recovery invocation.
- Findings route into `DerivedRecord`s with evidence; sketch below is binding.

Implement the Yuan et al.-derived static detectors that target the error-handling defects which dominate real outages: swallowed exceptions, empty catch/except handlers, errors logged then ignored, cleanup-only handlers, and misleading success returns. The detectors run over `ExceptionFact`s (TASK-036) and the TASK-052 obligation models so each finding is classified as a safety threat (e.g., refund errors swallowed → money may vanish silently) or a liveness threat (e.g., early return breaks the retry scheduler). The TASK-053 fault-injection results then confirm or clear each candidate dynamically.

**Validation test sketch (contract):**

```rust
#[test]
fn task_054_detects_ineffective_refund_and_interrupted_scheduler() {
    // arrange: fixture with (a) refund() swallowing its transfer error, (b) retry loop broken by early return
    let facts = fixture_error_handling_facts(); // ExceptionFacts from TASK-036
    let model = fixture_obligation_model();     // StateModel from TASK-052
    let detectors = ErrorHandlingDetectors::default();

    // act
    let findings = detectors.run(&facts, &model);

    // assert: swallowed refund failure -> safety-threatening potential ineffective refund
    let refund = findings.iter()
        .find(|f| f.detector == DetectorKind::SwallowedException && f.about() == sym("refund"))
        .expect("swallowed refund error must be flagged");
    assert_eq!(refund.classification, ThreatClass::Safety);
    assert!(!refund.evidence.locs.is_empty(), "finding cites handler location");

    // assert: early return inside the retry loop -> liveness threat (interrupted scheduler)
    let sched = findings.iter()
        .find(|f| f.detector == DetectorKind::EarlyReturnInRetryLoop)
        .expect("broken retry scheduler must be flagged");
    assert_eq!(sched.classification, ThreatClass::Liveness);

    // assert: candidates only — no authority upgrade; all findings carry evidence
    assert!(findings.iter().all(|f| f.authority == Authority::InferredCandidate));
    assert!(findings.iter().all(|f| !f.evidence.locs.is_empty()));
}
```

## Phase 8 — Incremental Maintenance

Incremental correctness belongs in every slice (§12); Phase 8 supplies the machinery that makes it true: a blake3 content-hash diff engine between revisions, reverse-dependency invalidation with a deterministic dirty worklist, differential dataflow maintenance of derived relations under adds and removals, and a property-test harness proving that incremental output equals clean recomputation — including deletions that invalidate earlier findings. The phase backs FR-8/FR-20 and encodes the §15.4 ≤ 200 µs per-changed-file invalidation budget as an executable `#[perf]` contract.

### TASK-055: Diff workspace revisions by blake3 content hash

| Field | Value |
|---|---|
| Phase | 8 — Incremental Maintenance |
| Target crate(s) | `cie-incremental` (diff module) |
| Depends on | TASK-025 |
| PRD reference | FR-8 / FR-20 / §15.1 (`file:{file_id}:hash`) / §15.4 |
| Parallel group | G-8a |

**Constraints:**
- Hashes are blake3 `Hash = [u8; 32]` over file content only — never paths or mtimes — stored strictly under `file:{file_id}:hash` per §15.1 (global rules 1 and 7).
- Classification is total: every file of rev N appears exactly once (`Changed | Added | Deleted | Unchanged`); unknown states are bugs, not defaults (FR-8 acceptance: no silently retained stale facts).
- Diffing streams file contents (no whole-file buffering) and uses `Ns` revision times only — no `chrono` on the hot path (FR-21, global rule 1).
- The diff is pure input to invalidation: it never writes derived facts, conclusions, or statuses (FR-8 separation; global rule 2).

**DoD:**
- `diff_revs(prev, next, store) -> RevDiff { changed, added, deleted, unchanged }` with per-file `FileId`s and reasons.
- Content-addressed resolution: identical content under a new path is recognized via hashes, feeding TASK-056's exact invalidation.
- Identical-rev diff is empty (the FR-20 ≤ 5% re-ingest path starts here).
- Deterministic output regardless of walk order; sketch below is binding.

Build the input stage of incremental maintenance: comparing revision N-1 and N by blake3 content hashes stored under `file:{file_id}:hash`, classifying each file as changed, added, deleted, or unchanged. This `RevDiff` is the single source of truth that TASK-056's invalidator and TASK-057's differential feed consume, so its correctness bounds the correctness of everything downstream. Path moves with identical content hash the same, keeping moves cheap — a direct contributor to FR-20's re-ingest target.

**Validation test sketch (contract):**

```rust
#[test]
fn task_055_diff_classifies_changed_added_deleted() {
    // arrange: rev 41 has a.rs (H1), b.rs (H2); rev 42 edits a.rs, deletes b.rs, adds c.rs and d.rs
    let store = fixture_store_with_rev(REV_41, &[("a.rs", H1), ("b.rs", H2)]);
    let next = fixture_rev_files(REV_42, &[("a.rs", H1_EDITED), ("c.rs", H3), ("d.rs", H4)]);

    // act
    let diff = diff_revs(REV_41, REV_42, &store, &next);

    // assert: classification per FR-8 — total and exact
    assert_eq!(diff.changed.len(), 1, "a.rs content edited");
    assert_eq!(diff.added.len(), 2, "c.rs and d.rs are new");
    assert_eq!(diff.deleted.len(), 1, "b.rs is gone from rev 42");
    assert_eq!(diff.changed[0].0, file_id("a.rs"));
    assert_eq!(diff.total(), 4, "classification is total over rev-42 files");

    // assert: blake3-width hashes stored under the §15.1 namespace
    for (fid, h) in next.hashes() {
        assert_eq!(h.len(), 32, "Hash = [u8; 32]");
        assert_eq!(store.get(&format!("file:{fid}:hash")).unwrap(), h);
    }

    // assert: identical revs produce an empty diff (FR-20 re-ingest entry point)
    assert!(diff_revs(REV_42, REV_42, &next, &next).is_empty());
}
```

### TASK-056: Invalidate exactly via reverse dependencies and a dirty worklist

| Field | Value |
|---|---|
| Phase | 8 — Incremental Maintenance |
| Target crate(s) | `cie-incremental` (invalidation module) |
| Depends on | TASK-055, TASK-037 |
| PRD reference | FR-8 / FR-20 / §15.1 (`deps:`, `rev_deps:`, `record:`) / §15.4 (≤ 200 µs per changed file) |
| Parallel group | G-8b |

**Constraints:**
- Invalidation is exact over recorded dependencies: a record stays valid unless a transitive dependency changed; over-invalidation breaks FR-20's ≤ 5% re-ingest target and is treated as a defect.
- `deps:{record_id}` / `rev_deps:{symbol_id}` are maintained transactionally with `record:{record_id}` writes so the index can never drift from the record store (global rule 7).
- Per-changed-file cost measured in `Ns` and asserted ≤ 200 µs × `CI_TOLERANCE` in the `#[perf]` `#[ignore]` test (global rules 1 and 5, §15.4).
- Deletion is a change: deleted files/symbols invalidate their dependents exactly like edits (full deletion correctness gated in TASK-058).

**DoD:**
- `Invalidator::on_diff(RevDiff) -> DirtyWorklist` with per-item `DirtyReason` (direct file change vs dep chain).
- Incremental `rev_deps:` maintenance on record add/remove; worklist ordering deterministic (stable by id) for reproducible reruns.
- `#[perf]` test: 1k-file synthetic graph, 10 changed files, per-file invalidation ≤ 200 µs × `CI_TOLERANCE`.
- Sketches below (correctness + perf) are binding.

Maintain the reverse-dependency index and run targeted invalidation: seed dirty files from the TASK-055 diff, expand transitively over `rev_deps:` through records and summaries, and enqueue only affected items for recomputation by the TASK-037 scheduler. Unchanged files contribute nothing, so re-ingesting a mostly stable folder touches only what actually moved — the mechanism behind FR-20. The §15.4 budget (≤ 200 µs per changed file) is enforced as an ignored perf test multiplied by `CI_TOLERANCE`, not an aspiration.

**Validation test sketch (contract):**

```rust
#[test]
fn task_056_invalidates_only_transitive_dependents() {
    // arrange: file F -> sym S1 <- record R1 <- record R3; file G -> sym S2 <- record R2 (independent)
    let (mut store, ids) = fixture_dep_graph(); // deps:/rev_deps: populated per §15.1
    let diff = RevDiff { changed: vec![(ids.file_f, H1_EDITED)], ..empty_diff() };
    let dirty = Invalidator::new(&mut store).on_diff(&diff);

    // assert: R1 and R3 dirty via dep chain; R2 untouched (exact invalidation)
    let dirty_recs: BTreeSet<_> = dirty.records().copied().collect();
    assert!(dirty_recs.contains(&ids.r1) && dirty_recs.contains(&ids.r3));
    assert!(!dirty_recs.contains(&ids.r2));
    assert_eq!(dirty.reason_of(ids.r3), Some(DirtyReason::DepChain));

    // assert: index stays consistent with record: after removal (no drift)
    store.remove_record(ids.r1);
    assert!(!store.rev_deps(ids.s1).contains(&ids.r1));
}

#[test]
#[ignore] // #[perf] — §15.4: incremental invalidation < 200 µs per changed file
fn task_056_perf_invalidation_under_200us_per_changed_file() {
    const CI_TOLERANCE: f64 = 2.0;
    let (mut store, files) = perf_fixture_1k_files_10_changed();
    let diff = diff_revs(REV_N, REV_N_PLUS_1, &store, &changed_rev(&files));
    let t0 = Ns::now();
    let dirty = Invalidator::new(&mut store).on_diff(&diff);
    let per_file_ns = (Ns::now() - t0).0 as f64 / 10.0;
    assert!(!dirty.is_empty());
    assert!(per_file_ns <= 200_000.0 * CI_TOLERANCE, "per changed file: {per_file_ns} ns");
}
```

### TASK-057: Maintain derived relations with differential dataflow

| Field | Value |
|---|---|
| Phase | 8 — Incremental Maintenance |
| Target crate(s) | `cie-incremental` (differential module) |
| Depends on | TASK-056 |
| PRD reference | FR-16 / FR-20 / §7 (McSherry et al. — Differential Dataflow, CIDR 2013) / §15.4 |
| Parallel group | G-8c |

**Constraints:**
- Relations are maintained as multisets of (input, output) updates; deletions retract deltas through the dataflow — retraction completeness is asserted, stale facts are defects (FR-8, global rule 2).
- Iterative operators (closure, fixpoints) run under iteration caps + `budget_ns`; exhaustion marks the affected relation `Unknown`, never a stale partial answer labeled complete (global rule 3).
- Trace epochs are `RevId`; compaction must never consume the frontier needed by `--as-of-rev` time-travel queries (FR-21/FR-23, global rule 1).
- No speculative caching layers unless TASK-061 measurements justify them — optimization is measurement-driven only (FR-16 discipline).

**DoD:**
- `RelationTrace` wrapper: `add/remove(rev, fact)`, `advance_to(rev)`, `contains(fact)`, `as_of(rev)` over arrangements/traces.
- Transitive-closure maintenance exercised with both additions and removals (delta propagation through iteration).
- Integration: TASK-056's dirty output drives `feed()`; outputs must equal clean recompute (equivalence gate in TASK-058).
- Sketch below (adds, retractions, as-of reads) is binding.

Integrate differential dataflow so derived relations — call edges, reachability closures, concept memberships — update incrementally under add/remove of inputs, including iterative computations where plain worklist invalidation degenerates into recomputation. Inputs arrive as revisioned deltas driven by TASK-056's invalidation output; arrangements keep traces compact and enable both current reads and `as_of(rev)` time-travel reads within the retained frontier. This is the FR-16/FR-20 engine room: cheap, correct, revisioned relation maintenance.

**Validation test sketch (contract):**

```rust
#[test]
fn task_057_differential_closure_tracks_adds_and_removals() {
    // arrange: edges a->b, b->c; iterative closure must derive a->c
    let mut trace = RelationTrace::transitive_closure();
    trace.add(RevId(41), (SYM_A, SYM_B));
    trace.add(RevId(41), (SYM_B, SYM_C));
    trace.advance_to(RevId(41));
    assert!(trace.contains((SYM_A, SYM_C)), "closure derives a->c at rev 41");

    // act: rev 42 removes b->c (its file was deleted)
    trace.remove(RevId(42), (SYM_B, SYM_C));
    trace.advance_to(RevId(42));

    // assert: retraction propagates through the iterative operator — no stale a->c
    assert!(!trace.contains((SYM_A, SYM_C)), "deletion must retract derived edges");
    assert!(trace.contains((SYM_A, SYM_B)), "unaffected edge survives");

    // assert: time-travel — the retained rev-41 frontier still answers as-of-41 (FR-23)
    assert!(trace.as_of(RevId(41)).contains((SYM_A, SYM_C)));
}
```

### TASK-058: Prove deletion correctness and incremental/clean equivalence

| Field | Value |
|---|---|
| Phase | 8 — Incremental Maintenance |
| Target crate(s) | `cie-incremental` (proptests) |
| Depends on | TASK-056 |
| PRD reference | FR-8 / §13 item 7 / §11 stage 5 acceptance / FR-20 |
| Parallel group | G-8c |

**Constraints:**
- Deletion invalidates dependent findings and removes affected `sym:`, `record:`, `deps:`, `rev_deps:`, `sum:` entries; any surviving orphan is a failing property (FR-8, §13 item 7, global rule 7).
- Equivalence is structural: incremental `DerivedRecord` set == clean-recompute set with `evidence`-stripped comparison; `created_ns`/`compute_ns` (`Ns`/`NsDelta`) are never compared (global rule 1).
- Generators include adversarial sequences: delete-then-readd identical content, delete dependency before dependent, edit during a pending worklist (global rule 5: tests are binding contracts).
- Failing seeds are reproducible: the harness prints the minimal shrinking edit sequence for triage.

**DoD:**
- `proptest` harness with an `EditSeq` strategy (≤ 64 ops: add/edit/delete) and a clean-recompute oracle after every step.
- §13 item 7 fixture: a finding on a deleted symbol disappears from the report at the next revision.
- `no_orphan_records()` invariant checked across all §15.1 namespaces touched by the phase.
- Harness runs in CI as the acceptance gate for any slice claiming incremental correctness; sketches below are binding.

Stand up the property-test harness that FR-16 demands in every slice: random edit sequences — additions, edits, and crucially deletions — are applied to fixture repos, and after every step the incremental result must equal a from-scratch recompute. Deletions get special scrutiny because they invalidate earlier findings rather than producing new ones, which is exactly §13 item 7. The harness also asserts that no stale facts survive in any store, so "recompute affected results without silently retaining stale facts" (FR-8 acceptance) is checked mechanically.

**Validation test sketch (contract):**

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn task_058_incremental_equals_clean_recompute(seq in edit_seq_strategy(64)) {
        let mut inc = IncrWorld::new(fixture_repo_small());
        for step in seq.steps() {
            inc.apply(step);                       // TASK-055 diff -> TASK-056 invalidate -> TASK-057 maintain
            let clean = clean_recompute(&inc.repo_snapshot()); // oracle: full rebuild
            prop_assert_eq!(inc.findings_stripped(), clean.findings_stripped(),
                "divergence after {step:?}");
            prop_assert!(inc.no_orphan_records(), "stale fact survived {step:?}");
        }
    }
}

#[test]
fn task_058_deletion_invalidates_earlier_finding() { // §13 item 7
    // arrange: a finding references sym 77, which lives in file f
    let mut w = world_with_finding_on(SymbolId(77), REV_41);

    // act: delete the file at rev 42
    w.apply(Edit::DeleteFile(file_of(SymbolId(77))));

    // assert: the earlier finding is gone; nothing stale remains anywhere
    assert!(!w.report(REV_42).findings.iter().any(|f| f.about() == SymbolId(77)),
        "deletion must invalidate the earlier finding");
    assert!(w.no_orphan_records(), "sym:/record:/rev_deps: entries for 77 purged");
    // time-travel still shows the old finding as-of rev 41 (FR-23)
    assert!(w.report_as_of(REV_41).findings.iter().any(|f| f.about() == SymbolId(77)));
}
```

## Phase 9 — Vector Store, Snapshots & Performance Hardening

Phase 9 hardens the storage plane and the performance envelope: an HNSW embedding store behind a backend-agnostic trait (FR-29), zero-copy revisioned graph snapshots via `rkyv` + `memmap2` (FR-30), the full §15.4 latency-target table encoded as executable `#[perf]` regression tests (FR-16 slice 5), and the L3 zstd + parquet cold archive for historical revisions. Every budget in this phase is a binding contract: an `#[ignore]`-gated test multiplied by `CI_TOLERANCE`, never a prose aspiration.

### TASK-059: Serve k-NN over embeddings behind an HNSW store trait

| Field | Value |
|---|---|
| Phase | 9 — Vector Store, Snapshots & Performance Hardening |
| Target crate(s) | `cie-store::hnsw` |
| Depends on | TASK-024 |
| PRD reference | FR-29 / §7 (Malkov & Yashunin — HNSW, 2018) / §15.3 (vector store row) / §15.4 (k-NN < 1 ms) |
| Parallel group | G-9a |

**Constraints:**
- Vectors are addressed strictly as `vec:{record_id}` per §15.1; record ids are stable across revisions — re-embedding overwrites in place, never orphans the old vector silently (global rule 7).
- Queries take `budget_ns`; expiry returns `KnnOutcome::Unknown(TimedOut)` — never an empty result that reads as "no similar vectors" (global rule 3).
- The `VectorStore` trait is backend-agnostic (`hnsw_rs`/`usearch` behind features); the default build compiles offline with no network dependency (FR-27 spirit).
- Dimension mismatches and lookups of removed records are explicit errors carrying evidence context, not silent zeros (global rule 2).

**DoD:**
- `VectorStore` trait: `insert(RecordId, &[f32])`, `remove(RecordId)`, `knn(&[f32], k, budget_ns)` + bulk-build path for 1M-vector fixtures.
- `HnswRs` implementation (Malkov–Yashunin hierarchy) selected as default backend; dimension pinned per store instance.
- `#[perf]` `#[ignore]` test: k-NN (k=10, 1M × 128-d vectors) p99 ≤ 1 ms × `CI_TOLERANCE` (§15.4).
- Serves concept similarity (FCA extents) and document→code mapping (FR-33); sketch below is binding.

Wrap an HNSW approximate-nearest-neighbor index behind a store trait so the rest of CIE depends on capabilities, not a vendor. The store holds `vec:{record_id}` embeddings used for concept similarity over the FCA lattice and for proposing document-claim→code mappings (FR-33). The FR-29 acceptance shape — k-NN k=10 over 1M vectors at p99 ≤ 1 ms — is enforced as an ignored perf test with the standard `CI_TOLERANCE` multiplier.

**Validation test sketch (contract):**

```rust
#[test]
fn task_059_knn_returns_known_neighbor_within_budget() {
    // arrange: 10k vectors; query exactly matches vector 0
    let mut vs = HnswRsStore::new(Dim(128));
    for i in 0..10_000u32 { vs.insert(RecordId(i), fixture_vector(i)); }
    let q = fixture_vector(0);
    // act
    let hits = vs.knn(&q, 10, NsDelta(10_000_000));
    // assert: exact vector ranks in top-k; results are (id, distance) pairs
    let hits = match hits { KnnOutcome::Hits(h) => h, other => panic!("unexpected {other:?}") };
    assert!(hits.iter().any(|(id, _)| *id == RecordId(0)), "exact match must be a top-10 neighbor");
    assert_eq!(hits.len(), 10);
}

#[test]
#[ignore] // #[perf] — FR-29: k-NN (k=10, 1M vectors) p99 <= 1 ms
fn task_059_perf_hnsw_1m_vectors_p99_under_1ms() {
    const CI_TOLERANCE: f64 = 2.0;
    let vs = bulk_fixture_1m_vectors_dim128(); // prebuilt offline fixture
    let queries = sample_queries(&vs, 1_000);

    let mut lat: Vec<u64> = queries.iter().map(|q| {
        let t0 = Ns::now();
        assert!(vs.knn(q, 10, NsDelta(5_000_000)).is_ok());
        (Ns::now() - t0).0
    }).collect();
    lat.sort_unstable();
    let p99 = lat[(lat.len() as f64 * 0.99) as usize - 1];
    assert!(p99 as f64 <= 1_000_000.0 * CI_TOLERANCE, "k-NN p99 {p99} ns");
}
```

### TASK-060: Load zero-copy revisioned graph snapshots with rkyv + memmap2

| Field | Value |
|---|---|
| Phase | 9 — Vector Store, Snapshots & Performance Hardening |
| Target crate(s) | `cie-store` (snapshot module) |
| Depends on | TASK-008 |
| PRD reference | FR-30 / §15.3 (L2 — graph snapshots) / §13 item 16 / §15.5 (memory layout rules) |
| Parallel group | G-9a |

**Constraints:**
- The read path performs zero deserialization: access via `Archived` accessors over the mmap; any `to_owned()` on hot structs is a review-blocking violation (FR-30, §15.5).
- Snapshots are versioned per `RevId` and content-addressed (blake3 of the archive); writing the same rev is idempotent (global rules 1 and 7).
- Corrupt, short, or mismatched mmap → explicit `SnapshotError::Corrupt`, never a silently empty graph (global rule 2).
- Archived layout follows §15.5: `#[repr(C)]`-friendly hot structs, `SmallVec`-bounded collections, interned strings, no allocation on the timing path.

**DoD:**
- `SnapshotWriter::write(dir, rev, graph)` emits a revisioned `.cie-snap` file; `SnapshotReader::open(dir, rev)` maps + validates it.
- Warm open of a 1M-symbol graph ≤ 10 µs × `CI_TOLERANCE` (`#[perf]` `#[ignore]`; §13 item 16).
- `open_latest_at(rev)` picks the newest snapshot ≤ rev, supporting `--as-of-rev` (FR-23).
- Round-trip fidelity test: archived node/edge views equal the source CSR graph; sketch below is binding.

Implement the L2 snapshot layer: each revision's graph is serialized with rkyv into an archive file that is memory-mapped on open, so reads touch pages directly instead of deserializing. Warm opens are pointer arithmetic plus validation — the §13 item 16 gate is a 1M-symbol graph read in ≤ 10 µs. Snapshots are versioned per revision and content-addressed, giving TASK-062's archive a stable artifact to compress and making time-travel reads (FR-23) a file-selection problem.

**Validation test sketch (contract):**

```rust
#[test]
fn task_060_snapshot_roundtrip_preserves_graph() {
    let dir = tempdir();
    let graph = fixture_csr_graph(1_000);
    SnapshotWriter::write(dir.path(), RevId(42), &graph).expect("write snapshot");
    // act + assert: zero-copy view equals the source graph structurally
    let reader = SnapshotReader::open(dir.path(), RevId(42)).expect("open snapshot");
    let g = reader.archived();
    assert_eq!(g.node_count(), 1_000);
    assert_eq!(g.edge_targets(SymbolId(7)), graph.edge_targets(SymbolId(7)));

    // assert: selection picks the newest snapshot <= requested rev (FR-23 support)
    SnapshotWriter::write(dir.path(), RevId(43), &graph).expect("write rev 43");
    assert_eq!(SnapshotReader::open_latest_at(dir.path(), RevId(42)).unwrap().rev(), RevId(42));
}

#[test]
#[ignore] // #[perf] — §13 item 16: warm 1M-symbol snapshot read <= 10 µs
fn task_060_perf_warm_snapshot_read_1m_symbols_under_10us() {
    const CI_TOLERANCE: f64 = 2.0;
    let reader = SnapshotReader::open(fixture_1m_snap_dir(), RevId(42)).expect("prebuilt fixture");
    std::hint::black_box(reader.archived().node_count()); // warm pass: page-in
    let mut worst = 0u64;
    for _ in 0..1_000 {
        let t0 = Ns::now();
        std::hint::black_box(reader.archived());
        worst = worst.max((Ns::now() - t0).0);
    }
    assert!(worst as f64 <= 10_000.0 * CI_TOLERANCE, "warm open worst {worst} ns");
}
```

### TASK-061: Encode the §15.4 latency table as executable perf regression tests

| Field | Value |
|---|---|
| Phase | 9 — Vector Store, Snapshots & Performance Hardening |
| Target crate(s) | `cie-bench` (new workspace bench crate) + benches in `cie-store`, `cie-incremental` |
| Depends on | TASK-056, TASK-060 |
| PRD reference | FR-16 / §12 slice 5 / §15.4 (full latency table) / §15.5 |
| Parallel group | G-9b |

**Constraints:**
- Every §15.4 row becomes an executable `#[perf]` + `#[ignore]` test comparing measured p99 against the target × `CI_TOLERANCE`; untagged benchmarks are rejected (global rule 5).
- Measurements are `Ns` deltas with no allocation on the timing path; fixtures are prebuilt offline so tests never measure network or build time (FR-21, global rule 1; FR-27 offline-first).
- Optimization happens only where profiles attribute cost — no speculative rewrites without a measurement citing the failing target (FR-16: "optimization based on measurements").
- A regression report is generated per run (target, measured p99, delta, verdict per row) and stored under the run's `RevId` with `Ns` timestamp (global rules 1 and 2).

**DoD:**
- Criterion (or manual harness) benches covering at minimum: symbol lookup ≤ 50 ns, hash→id ≤ 30 ns, CSR edge scan ≤ 20 ns, summary get ≤ 5 µs, snapshot read ≤ 10 µs, invalidation ≤ 200 µs/file, HNSW k-NN ≤ 1 ms, LLM cache hit ≤ 100 ns.
- `cargo test --ignored --features perf` runs the full gate in the dedicated CI stage and emits `perf_report.json` + Markdown summary.
- Regression detection: any row exceeding target × `CI_TOLERANCE` fails the stage with its delta in the report.
- Seed harness rows reusing TASK-056/TASK-060 perf fixtures; sketch below is binding.

Turn the §15.4 latency table from documentation into an executable gate: one `#[perf]` test per row, each measuring p99 over a prebuilt fixture and comparing against the target multiplied by `CI_TOLERANCE`. The harness also produces a regression report so slice-5 optimization work starts from measurements, not hunches — the exact discipline FR-16 prescribes. Rows whose implementations live in earlier phases (TASK-056 invalidation, TASK-060 snapshots) reuse those tasks' fixtures to keep the harness honest.

**Validation test sketch (contract):**

```rust
#[test]
#[ignore] // #[perf] — §15.4 gate: every row measured, compared with CI_TOLERANCE
fn task_061_perf_full_latency_table_gate() {
    const CI_TOLERANCE: f64 = 2.0;
    let rows = &[
        ("symbol_lookup",      50.0,      p99_of(|| bench_symbol_lookup(1_000))),
        ("hash_to_id",         30.0,      p99_of(|| bench_hash_to_id(1_000))),
        ("csr_edge_scan",      20.0,      p99_of(|| bench_csr_edge_scan(fixture_csr(1_000_000)))),
        ("summary_get_cold",   5_000.0,   p99_of(|| bench_summary_get_cold(100))),
        ("snapshot_read_1m",   10_000.0,  p99_of(|| bench_snapshot_warm_open(fixture_1m_snap_dir()))),
        ("invalidate_per_file",200_000.0, p99_of(|| bench_invalidate_per_file(fixture_1k_files()))),
        ("hnsw_knn_1m",        1_000_000.0, p99_of(|| bench_hnsw_knn(bulk_fixture_1m_vectors_dim128()))),
    ];

    let mut report = PerfReport::new(REV_42, Ns::now());
    for (name, target_ns, measured_ns) in rows {
        let verdict = if *measured_ns as f64 <= target_ns * CI_TOLERANCE { Verdict::Pass } else { Verdict::Regressed };
        report.push(Row { name, target_ns: *target_ns, measured_ns: *measured_ns, verdict });
        assert_eq!(verdict, Verdict::Pass, "{name}: {measured_ns} ns vs target {target_ns} ns");
    }

    write_perf_report(&report); // perf_report.json + Markdown, keyed by rev + Ns timestamp
    assert_eq!(report.rows.len(), rows.len(), "every §15.4 row measured");
}
```

### TASK-062: Archive historical revisions to an L3 zstd + parquet cold store

| Field | Value |
|---|---|
| Phase | 9 — Vector Store, Snapshots & Performance Hardening |
| Target crate(s) | `cie-store` (archive module) |
| Depends on | TASK-060 |
| PRD reference | §15.3 (L3 — cold archive: zstd + parquet, < 100 ms scan) / FR-23 (time-travel support) / §15.1 |
| Parallel group | G-9b |

**Constraints:**
- Archives are columnar (parquet) and zstd-compressed, written per revision range; scans of a rev's archive metadata complete ≤ 100 ms × `CI_TOLERANCE` (`#[perf]` `#[ignore]`, §15.3 L3 row).
- Archive/restore are explicit APIs: `archive_range(from_rev, to_rev)` and `restore(rev)` rebuild the L2 snapshot + L1 tables byte-equivalently; a silent partial restore is a defect (global rule 2).
- Archived records keep `Ns` timestamps and `RevId`s verbatim — the archive answers `--as-of-rev` / `--as-of-ns` queries for retired revisions (FR-21/FR-23, global rule 1).
- Only revisions explicitly marked evictable are archived; the hot (L0/L1/L2) frontier is never touched by the evictor (§15.3 layering discipline).

**DoD:**
- `ArchiveStore` with `archive_range`, `restore`, `scan_rev(rev) -> RevIndex`, and retention metadata (`valid_from_ns`/`valid_to_ns` per §15.2).
- Restored revisions pass snapshot equivalence against the original L2 artifacts (TASK-060 round-trip).
- `#[perf]` `#[ignore]` test: scan of a 100-rev archive index ≤ 100 ms × `CI_TOLERANCE`.
- CLI hooks (`cie archive --from-rev .. --to-rev ..`, `cie restore --rev ..`) exposed via `cie-cli` wiring points; sketch below is binding.

Implement the L3 cold archive: historical revisions are serialized to parquet columnar files, zstd-compressed, and indexed so old revisions stop paying hot-storage rent without disappearing from time-travel queries. `archive_range` evicts eligible revisions from the hot layers into the archive; `restore` materializes a revision back into L2/L1 exactly as it was, validated by snapshot equivalence. The §15.3 L3 budget — a scan of archived metadata in ≤ 100 ms — is enforced as an ignored perf test like every other storage budget.

**Validation test sketch (contract):**

```rust
#[test]
fn task_062_archive_restore_roundtrip_is_byte_equivalent() {
    // arrange: 10 revisions of history; revs 1..=5 marked evictable
    let world = fixture_history_10_revs();
    let mut arch = ArchiveStore::open(world.dir());

    // act: archive the old range, then restore one revision
    arch.archive_range(RevId(1), RevId(5)).expect("archive evictable range");
    assert!(!arch.contains_hot(RevId(3)), "archived rev evicted from hot layers");
    arch.restore(RevId(3)).expect("restore rev 3");
    // assert: restored revision matches the original snapshot + records byte-for-byte
    assert_eq!(arch.snapshot_digest(RevId(3)), world.original_digest(RevId(3)));
    assert_eq!(arch.records_as_of(RevId(3)), world.original_records(RevId(3)));
    // assert: as-of-ns queries still resolve against archived history (FR-23)
    let ns = world.rev_ns(RevId(3));
    assert_eq!(arch.as_of_ns(ns).rev(), RevId(3));
    assert!(arch.index().retains(RevId(1), RevId(5)));
}

#[test]
#[ignore] // #[perf] — §15.3 L3: archive scan < 100 ms
fn task_062_perf_archive_scan_under_100ms() {
    const CI_TOLERANCE: f64 = 2.0;
    let arch = fixture_archive_100_revs(); // prebuilt offline archive
    let t0 = Ns::now();
    let index = arch.scan_rev(RevId(50));
    let elapsed = (Ns::now() - t0).0 as f64;
    assert!(!index.is_empty());
    assert!(elapsed <= 100_000_000.0 * CI_TOLERANCE, "scan {elapsed} ns");
}
```

## Phase 10 — Native LLM Layer

Phase 10 adds the native LLM stack as a pure labeling and explanation layer (§18.1): a provider abstraction with an offline-first Ollama default, cloud adapters behind explicit opt-in, a content-addressed cache, and hard budget enforcement with graceful downgrade. The engine must remain fully functional with the LLM disabled, and every LLM artifact is a content-addressed `Label` record carrying `model_id` and `prompt_hash`; verification truth stays mechanical (FR-28, FR-39). All provider I/O is deadline-driven in `Ns`, and every prompt/response is recorded as `Evidence` (§18.5).

### TASK-063: Define LlmProvider trait with offline-first Ollama default

| Field | Value |
|---|---|
| Phase | 10 — Native LLM Layer |
| Target crate(s) | `cie-llm` (+ `cie-core` config types) |
| Depends on | TASK-002, TASK-004 |
| PRD reference | §18.3–18.7 FR-24, FR-27, FR-28 / §13 #12 / §15.1 `label:` |
| Parallel group | G-10a |

**Constraints:**
- Offline-first: default provider is Ollama at `http://localhost:11434`; nothing in the default config dials a cloud endpoint (FR-27, §18.7).
- The provider API is labeling-only: it returns `Label`/text payloads and can never mutate facts, contracts, or statuses (FR-28, §18.1).
- All deadlines and timeouts are `Ns = u64` (deltas `NsDelta = i64`); no `chrono` in the provider hot path (FR-21, global rule 1).
- Every LLM artifact carries `{source: Llm, model_id, prompt_hash}` and lands inside `Evidence` (§18.5, global rule 2).
- Tests are hermetic: no test opens a real socket; Ollama behavior is simulated by a no-network mock.

**DoD:**
- `LlmProvider` trait, `ProviderId`, `CompletionRequest`/`CompletionResponse`/`CompletionStream`, and `GenParams` match §18.3 and are object-safe (`dyn LlmProvider`).
- `LlmConfig` parses the §18.4 TOML shape (`provider`, `model`, `embed_model`, `endpoint`, `max_concurrency`, `timeout_ns`, `cache`, `budget_usd`); default build yields Ollama.
- Offline run with `MockLlmProvider::no_network()` produces LLM labels; `LlmMode::Off` run completes the engine and emits deterministic fallback labels.
- Labels persisted under `label:{record_id}` (§15.1) with `LabelSource::{Llm, Deterministic}` provenance.

This task pins the `LlmProvider` abstraction that every LLM consumer (symbol naming, concept naming, explanations, claim extraction, mapping) programs against, exactly as §18.3 specifies. Ollama is the compiled-in default so a fresh checkout runs labeling with zero network access and zero opt-in (§18.7); the §18.4 TOML selects model, endpoint, concurrency, timeout, cache, and budget. A no-network mock stands in for the local daemon in tests, and the `LlmMode::Off` path proves FR-28's fallback: the engine completes and labels degrade to deterministic names.

**Validation test sketch (contract):**

```rust
#[test]
fn task_063_offline_first_provider_and_llm_off_fallback() -> anyhow::Result<()> {
    // Default config selects Ollama with zero opt-in (§18.7, §18.4 TOML shape).
    let cfg = LlmConfig::from_toml(r#"[llm]
provider = "ollama"
model = "qwen2.5-coder:14b"
endpoint = "http://localhost:11434"
timeout_ns = 30_000_000_000
"#)?;
    assert_eq!(cfg.provider, ProviderId::Ollama);
    assert_eq!(cfg.timeout_ns, 30_000_000_000u64); // Ns, never chrono (FR-21)

    // Offline run: the mock refuses to dial any socket, so any network attempt fails the test.
    let llm = MockLlmProvider::no_network().replying("Authenticates a user against the session store");
    let mut engine = Engine::new(Store::in_memory()).with_llm(Arc::new(llm));
    let rev = engine.ingest_str("auth.py", "def authorize(u):\n    return u.is_active")?;
    let label = engine.label_of(RecordId::symbol(rev, "authorize"))?.expect("labeled");
    assert_eq!(label.source, LabelSource::Llm);
    assert_eq!(label.model_id, cfg.model);
    assert_ne!(label.prompt_hash, Hash::ZERO); // content-addressed, per global rule 4

    // LLM disabled: engine completes; labels fall back to deterministic names (FR-28, §13 #12).
    let mut engine = Engine::new(Store::in_memory()).with_llm_disabled();
    let rev = engine.ingest_str("auth.py", "def authorize(u):\n    return u.is_active")?;
    let label = engine.label_of(RecordId::symbol(rev, "authorize"))?.expect("labeled");
    assert_eq!(label.source, LabelSource::Deterministic);
    assert_eq!(label.text, "authorize"); // derived from the symbol name, no model involved
    Ok(())
}
```

### TASK-064: Implement cloud providers behind opt-in gating

| Field | Value |
|---|---|
| Phase | 10 — Native LLM Layer |
| Target crate(s) | `cie-llm` |
| Depends on | TASK-063 |
| PRD reference | §18.3 FR-24 / §18.4 |
| Parallel group | G-10b |

**Constraints:**
- All 8 §18.3 providers implement the exact `LlmProvider` trait as `Send + Sync` trait objects: OpenAI, Anthropic, Google, Ollama, llama.cpp, Azure OpenAI, Bedrock, OpenAI-compatible (FR-24).
- Cloud opt-in gate: selecting a cloud provider without a configured key source/profile is a config-build error, never a runtime surprise (FR-27).
- Timeout (`timeout_ns`) and bounded retries (`max_retries`) are uniform across adapters, driven by `Ns` deadlines, and recorded per response (FR-21, global rule 1).
- No provider-specific behavior escapes the adapter boundary: consumers branch on `LlmProvider`, never on vendor types.
- No real network in CI: every adapter is exercised against a mock HTTP transport only.

**DoD:**
- 8 adapters registered in `ProviderRegistry`; each implements `complete`, `stream`, `embed`, `max_context`, `cost_per_1k`.
- `ProviderRegistry::build(&LlmConfig)` rejects cloud selection lacking `api_key_env`/profile with `LlmError::CloudOptInRequired`.
- Retry policy proven by a mock transport that returns one `429` then succeeds; attempts recorded on `CompletionResponse`.
- The single `OpenAiCompatibleProvider` covers vLLM/LM Studio/Together/Groq-style endpoints via base URL (§18.3).

This task implements the remaining §18.3 providers — OpenAI, Anthropic, Google, Azure OpenAI, Bedrock, the catch-all OpenAI-compatible endpoint, and llama.cpp — all behind the same `LlmProvider` trait so consumers never branch on vendor. Cloud providers are strictly opt-in: config build fails unless a key source is present, keeping the offline-first Ollama default honest (FR-27). A shared `Ns`-deadline timeout plus bounded retry policy applies uniformly, and each adapter is exercised against a mock transport so CI never touches the network.

**Validation test sketch (contract):**

```rust
#[test]
fn task_064_all_providers_registered_and_opt_in_gated() -> anyhow::Result<()> {
    let mut reg = ProviderRegistry::new();
    reg.register(OllamaProvider::default());
    reg.register(OpenAiProvider::new());            // GPT-4o, o-series
    reg.register(AnthropicProvider::new());         // Claude
    reg.register(GoogleProvider::new());            // Gemini
    reg.register(AzureOpenAiProvider::new());
    reg.register(BedrockProvider::new());
    reg.register(OpenAiCompatibleProvider::new("http://127.0.0.1:8080/v1")); // vLLM, LM Studio, …
    reg.register(LlamaCppProvider::new("models/*.gguf"));
    assert_eq!(reg.ids().len(), 8, "FR-24 lists 8 built-in providers");

    // Cloud is opt-in: cloud selection without a key source must fail at config-build time.
    let bad = LlmConfig::from_toml("[llm]\nprovider = \"openai\"\n")?;
    assert!(matches!(reg.build(&bad), Err(LlmError::CloudOptInRequired(_))));

    // Uniform timeout/retry policy, Ns-based (FR-21); mock transport emits one 429 then 200.
    let cloud = LlmConfig::from_toml("[llm]\nprovider = \"openai\"\napi_key_env = \"OPENAI_API_KEY\"\ntimeout_ns = 30_000_000_000\nmax_retries = 2\n")?;
    let p = reg.build(&cloud)?;
    let req = CompletionRequest {
        prompt: "label `fn withdraw`", max_tokens: 64,
        deadline_ns: now_ns() + cloud.timeout_ns,
    };
    let resp = p.complete(req)?;
    assert_eq!(resp.attempts, 2, "retry policy retried exactly once");
    assert!(p.max_context() > 0 && p.cost_per_1k().0 >= 0.0);
    Ok(())
}
```

### TASK-065: Build content-addressed LLM cache

| Field | Value |
|---|---|
| Phase | 10 — Native LLM Layer |
| Target crate(s) | `cie-llm` (+ `cie-store`) |
| Depends on | TASK-063 |
| PRD reference | §18.5 FR-25 / §13 #13 / §15.1 `label:` / §15.4 |
| Parallel group | G-10b |

**Constraints:**
- Cache key is exactly `blake3(prompt + model + params)`; lookups are revision-scoped; changing the model invalidates prior entries (FR-25, §18.5).
- Entries are stored under `label:{record_id}` (§15.1, global rule 7) and reference `Evidence` with `generated_at_ns` (§18.5, global rule 2).
- The hit path performs zero network calls and no allocation beyond the returned `Label`.
- Perf contract: warm hit ≤ 100 ns × CI_TOLERANCE, tagged `#[perf]` and `#[ignore]`-gated (§13 #13, §15.4, global rule 5).

**DoD:**
- `LlmCache` with `key(prompt, model, params)`, `put_scoped`, `get_scoped`, and model-change invalidation.
- `sled://./cie-cache/llm` (§18.4 `cache` key) and in-memory backends behind one `CacheUri`.
- Repeated identical prompts return the identical `Label` bit-for-bit (content addressing verified).
- Perf test passes: measured ns/hit printed in the assertion message for CI diagnostics.

Every prompt/response pair is content-addressed: the cache key is `blake3(prompt + model + params)`, lookups are revision-scoped, and changing the model invalidates prior entries (FR-25, §18.5). Hits are served from the store under `label:{record_id}` without touching the network and must land under the §15.4 100 ns budget. This is what makes repeated ingests, `--watch` re-runs, and unchanged drift re-runs cheap and deterministic.

**Validation test sketch (contract):**

```rust
#[test]
fn task_065_cache_keying_scoping_invalidation() -> anyhow::Result<()> {
    let cache = LlmCache::open(CacheUri::parse("sled://:memory:")?)?;
    let params = GenParams { temperature: 0.2, max_tokens: 128 };
    let label = Label {
        source: LabelSource::Llm, model_id: "qwen2.5-coder:14b".into(),
        prompt_hash: Hash::ZERO, text: "Checks whether a user may withdraw.".into(),
    };
    // Key = blake3(prompt + model + params) (§18.5).
    let k1 = cache.key("label `fn withdraw`", "qwen2.5-coder:14b", &params);
    let k2 = cache.key("label `fn withdraw`", "llama3:8b", &params);
    cache.put_scoped(k1, &label, RevId(42))?; // stored under label:{record_id} (§15.1)
    assert_eq!(cache.get_scoped(k1, RevId(42))?, Some(label));
    assert_eq!(cache.get_scoped(k2, RevId(42))?, None, "model change invalidates (FR-25)");
    assert_eq!(cache.get_scoped(k1, RevId(41))?, None, "cache is revision-scoped");
    Ok(())
}

#[test]
#[ignore] // [perf] — run with: cargo test -- --ignored
fn task_065_cache_hit_latency_perf() -> anyhow::Result<()> {
    let cache = LlmCache::open(CacheUri::parse("sled://:memory:")?)?;
    let key = warm_cache(&cache, 10_000)?; // pre-warm, then measure steady state
    let t0 = Instant::now();
    let mut hits = 0u32;
    for _ in 0..10_000 { if cache.get_scoped(key, CURRENT_REV)?.is_some() { hits += 1; } }
    let ns_per_hit = t0.elapsed().as_nanos() as f64 / 10_000.0;
    assert_eq!(hits, 10_000, "no misses allowed on the measured path");
    // §15.4: LLM label cache hit < 100 ns, × CI_TOLERANCE on CI hardware (§13 #13).
    assert!(ns_per_hit <= 100.0 * CI_TOLERANCE, "cache hit {ns_per_hit:.0} ns > 100 ns × CI_TOLERANCE");
    Ok(())
}
```

### TASK-066: Enforce LLM budget with downgrade-continue semantics

| Field | Value |
|---|---|
| Phase | 10 — Native LLM Layer |
| Target crate(s) | `cie-llm` (+ `cie-contract`, `cie-evidence`) |
| Depends on | TASK-063, TASK-065 |
| PRD reference | §18.4–18.6 FR-26, FR-28 / §13 #14 |
| Parallel group | G-10c |

**Constraints:**
- `budget_usd` is a hard cap read from the `[llm]` config; spend accounting uses `cost_per_1k()` × measured token usage per call (FR-26, §18.4).
- On exceed: downgrade to the configured local model or disable labeling entirely, then CONTINUE — the ingest must not fail (FR-26, §13 #14).
- FR-28 invariant is non-negotiable: LLM output never writes `Contract.authority = Explicit` or `Verified` (global rule 4, §18.6).
- All LLM labels carry `{source: Llm, model_id, prompt_hash}` and are content-addressed + cached (global rule 4, FR-25).
- Downgrade/disable events are recorded as `Evidence`, never silent (global rule 2).

**DoD:**
- `BudgetMeter` accumulates USD spend per provider call and trips exactly at `budget_usd`.
- Trip action is configurable: `DowngradeToLocal(model)` or `DisableLabeling`; both keep the run green and are surfaced in the report.
- Store-wide invariant test passes: no `Contract` with `Authority::Explicit|Verified` has LLM provenance; every LLM `Label` has non-empty `model_id` + non-zero `prompt_hash`.
- §13 #14 scenario (budget exceeded mid-ingest → downgrade → completed report) is a passing integration test.

The engine meters spend from `cost_per_1k()` and token usage against the hard `budget_usd` from §18.4. On exceed it downgrades to the configured local model — or disables labeling entirely — and continues the run rather than failing (FR-26, §13 #14). The task also locks the FR-28 invariants as store-wide property checks: no LLM-written contract ever carries `Explicit`/`Verified` authority, and every LLM label carries `model_id` + `prompt_hash`.

**Validation test sketch (contract):**

```rust
const LOCAL_MODEL_ID: &str = "qwen2.5-coder:14b";

#[test]
fn task_066_budget_downgrade_and_authority_invariants() -> anyhow::Result<()> {
    // Cloud mock charges $0.01 per call; budget is $0.015 → trip after the 2nd call.
    let llm = MockLlmProvider::metered(Usd::per_call(0.01));
    let mut engine = Engine::new(Store::in_memory())
        .with_llm(Arc::new(llm))
        .with_budget_usd(0.015)
        .with_downgrade_model(LOCAL_MODEL_ID);
    let report = engine.ingest_folder(&repo_fixture("shop"))?; // must NOT error (FR-26)
    assert_eq!(report.llm_outcome, LlmOutcome::DowngradedToLocal);
    assert!(report.labels.iter().any(|l| l.model_id == LOCAL_MODEL_ID));
    assert!(report.evidence.iter().any(|e| e.mentions_downgrade()), "trip is evidenced");

    // FR-28 invariant sweep over the whole store (global rule 4):
    for contract in engine.store().contracts() {
        if matches!(contract.authority, Authority::Explicit | Authority::Verified) {
            assert!(!contract.evidence.provenance.iter().any(|p| p.is_llm()),
                    "LLM provenance behind Explicit/Verified authority");
        }
    }
    for label in engine.store().labels() {
        if label.source == LabelSource::Llm {
            assert!(!label.model_id.is_empty() && label.prompt_hash != Hash::ZERO);
        }
    }
    Ok(())
}
```

## Phase 11 — Folder Assimilation & CLI

Phase 11 turns the engine into a folder-point tool: a recursive, .gitignore-aware walk with parallel parsing (§17.2), the single end-to-end assimilation entry point demanded by FR-19, incremental re-ingestion bounded by FR-20, and the `cie ingest` CLI of §17.1. The polyglot end-to-end run (§13 #8) and the ≤ 5% unchanged re-ingest bound (§13 #10) are the phase's headline acceptance contracts. Every derived record still carries full `Evidence`, and language semantics remain confined to their adapters (FR-3).

### TASK-067: Implement folder walk with parallel adapter parsing

| Field | Value |
|---|---|
| Phase | 11 — Folder Assimilation & CLI |
| Target crate(s) | `cie-ingest` (+ `cie-adapters`) |
| Depends on | TASK-010, TASK-011 |
| PRD reference | §17.2 FR-19 / §16.1 |
| Parallel group | G-11a |

**Constraints:**
- Walk ignores exactly the §17.2 set (`.git`, `node_modules`, `target`, `vendor`, `.venv`, `dist`, `build`) plus `.gitignore` rules; binaries and files > `max_file_bytes` are skipped with recorded reasons.
- Language detection order is extension → shebang → tree-sitter probe (§17.2); the detected `LanguageId` is authoritative for adapter selection.
- Each file is parsed only by its own adapter; semantics never cross languages without the adapter vouching for them (FR-3, global rule 6).
- Per-file error isolation: a parse failure yields a `FileError` for that path only — the batch never aborts (§17.2).
- Parallelism via rayon; any walk/parse deadline from `--budget-ms` is tracked in `Ns`, not wall-clock chrono (FR-21, global rule 1).

**DoD:**
- `IngestPipeline::walk(root, WalkOptions)` → `detect_languages()` → `parse_parallel(pool)` emits the §17.2 fact stream (symbols, calls, CFG, DFG, effects, exceptions, async).
- Shebang-only scripts (no extension) are detected correctly; Python/Rust/Go/TypeScript fixtures resolve to the right adapters.
- Skipped and errored files are reported with `SkipReason::{Binary, Oversized}` / `FileError::Parse` and healthy siblings are unaffected.
- Walk honors nested `.gitignore` files and never descends into ignored directories.

This task implements the front half of the §17.2 pipeline: a recursive, .gitignore-aware walk with the PRD's ignore list, language detection by extension → shebang → tree-sitter probe, and rayon-parallel parsing through the registered `LanguageAdapter`s. Binary and oversized files are skipped with recorded reasons, and per-file errors are isolated so one broken file never fails the batch. Output is the typed fact stream (symbols, calls, CFG/DFG edges, effects, exceptions, async) that the rest of assimilation consumes.

**Validation test sketch (contract):**

```rust
#[test]
fn task_067_walk_detect_parallel_parse_isolation() -> anyhow::Result<()> {
    let root = fixture_dir(|fs| {
        fs.file("src/api.py", "def checkout(cart): ...\n");
        fs.file("src/lib.rs", "pub fn refund(order: &Order) -> Result<(), Pay> { todo!() }\n");
        fs.file("internal/svc/main.go", "func Authorize(u User) bool { return true }\n");
        fs.file("web/app.ts", "export function topUp(a: Account) { return a }\n");
        fs.file("tools/run", "#!/usr/bin/env python3\nimport sys\n"); // shebang detection
        fs.dir(".git/objects"); fs.dir("node_modules/left-pad"); fs.dir("target/debug");
        fs.binary("logo.png", b"\x89PNG\r\n\x1a\n\0\0");              // binary → skip
        fs.sparse_file("gen/big.log", 64 << 20);                      // oversized → skip
        fs.file("src/broken.py", "def oops(:\n");                     // parse error → isolated
    });

    let out = IngestPipeline::walk(&root, WalkOptions {
        respect_gitignore: true, max_file_bytes: 16 << 20, ..WalkOptions::default()
    })?
    .detect_languages() // extension + shebang + tree-sitter probe (§17.2)
    .parse_parallel(rayon::ThreadPoolBuilder::new().num_threads(4).build()?)?;

    assert_lang(&out, "src/api.py", Lang::Python);
    assert_lang(&out, "src/lib.rs", Lang::Rust);
    assert_lang(&out, "internal/svc/main.go", Lang::Go);
    assert_lang(&out, "web/app.ts", Lang::TypeScript);
    assert_eq!(out.lang_of("tools/run")?, Lang::Python, "shebang beats missing extension");
    assert!(out.ignored().contains(".git") && out.ignored().contains("node_modules") && out.ignored().contains("target"));
    assert!(out.skipped_as("logo.png", SkipReason::Binary));
    assert!(out.skipped_as("gen/big.log", SkipReason::Oversized));
    assert!(matches!(out.error_of("src/broken.py")?, Some(FileError::Parse(_))), "error isolated per file");
    assert_eq!(out.symbols_of("src/api.py")?.len(), 1, "healthy sibling unaffected by broken.py");
    Ok(())
}
```

### TASK-068: Wire end-to-end folder assimilation pipeline

| Field | Value |
|---|---|
| Phase | 11 — Folder Assimilation & CLI |
| Target crate(s) | `cie-ingest` (+ `cie-semantic`, `cie-summary`, `cie-contract`, `cie-concepts`) |
| Depends on | TASK-042, TASK-046, TASK-067 |
| PRD reference | §17.2 FR-19 / §13 #8 / §12 slice 8 |
| Parallel group | G-11b |

**Constraints:**
- One entry point, no further input: folder path → semantic model + summaries + contracts + concepts + hierarchy (FR-19, §17.2).
- Every derived record carries `Evidence { rev, provenance, assumptions, unsupported, unresolved }` (FR-4, global rule 2).
- Adapter-scoped semantics: `dispatch_kind`/`numeric_semantics`/`framework_conventions` are consulted per language, never cross-applied (FR-3, global rule 6).
- Unresolved externals and unresolved calls are recorded, never silently dropped (§11 stage 1).
- Results are persisted under a revision hash so re-runs are content-addressed (§17.2 final steps).

**DoD:**
- `assimilate_folder(path, AssimilateOptions) -> AssimilationReport` runs the full §17.2 chain and persists `rev:{rev_id}:meta`.
- Polyglot fixture (Python + Rust + Go + TypeScript) ingests end-to-end = §13 #8, producing non-empty facts for all four languages.
- Cross-file calls resolve within each language; unresolved calls are counted and exposed on the report.
- Summaries are computed bottom-up over the call graph; explicit contracts (docstrings/annotations) register; FCA concepts and hierarchy are populated.

This task wires the full §17.2 pipeline behind one entry point: from a folder path, produce the semantic model, function summaries, contracts, concepts, and hierarchy with no further user input (FR-19). The polyglot fixture (Python + Rust + Go + TypeScript) is the §13 #8 acceptance contract; every derived record carries `Evidence` per FR-4, and unresolved externals are recorded rather than dropped. Language semantics stay inside each adapter (FR-3) — dispatch and numeric semantics are consulted, never cross-applied.

**Validation test sketch (contract):**

```rust
#[test]
fn task_068_polyglot_folder_assimilation_end_to_end() -> anyhow::Result<()> {
    // §13 #8: one folder point, no further input → full model (FR-19).
    let repo = fixture_repo("polyglot"); // payments.py + ledger.rs + charge.go + billing.ts, cross-imports
    let report = assimilate_folder(&repo, AssimilateOptions { llm: LlmMode::Off, ..Default::default() })?;

    assert!(report.rev > 0 && report.file_count == 4 && report.symbol_count >= 12);
    for lang in [Lang::Python, Lang::Rust, Lang::Go, Lang::TypeScript] {
        assert!(report.symbols_in(lang) > 0, "{lang:?} produced no symbols");
    }
    // Cross-file relations resolved within each language's own adapter (FR-3: no semantic leakage).
    assert!(report.resolved_call("payments.charge", "ledger.post")?);
    assert!(report.unresolved_calls() * 2 < report.total_calls());
    // Summaries bottom-up, explicit contracts, concepts + hierarchy populated.
    assert!(report.summary_of("payments.charge")?.effects.len() >= 1);
    assert!(report.explicit_contracts() >= 1);
    assert!(report.concepts().len() >= 2 && report.hierarchy_root()?.children.len() >= 2);
    // Evidence protocol on every derived record (FR-4, global rule 2).
    for rec in report.records() {
        assert!(rec.evidence.is_complete(), "record {:?} lacks evidence", rec.kind);
    }
    Ok(())
}
```

### TASK-069: Implement incremental re-ingestion with hash-diff invalidation

| Field | Value |
|---|---|
| Phase | 11 — Folder Assimilation & CLI |
| Target crate(s) | `cie-ingest` (+ `cie-incremental`, `cie-store`) |
| Depends on | TASK-055, TASK-068 |
| PRD reference | §17.3 FR-20 / §13 #10 / §15.4 |
| Parallel group | G-11c |

**Constraints:**
- The skip decision is content-hash based (`file:{file_id}:hash`), never mtime-only (§17.3 step 2, FR-20).
- Invalidation covers changed files plus transitively affected summaries via reverse dependencies (§17.3 step 3).
- A delta against the previous revision is emitted; unchanged re-ingest produces an empty semantic diff (§17.3 step 5).
- Perf: unchanged re-ingest ≤ 5% of first-ingest wall time × CI_TOLERANCE, tagged `#[perf]` and `#[ignore]`-gated (§13 #10, global rule 5).
- Timestamps and durations are `Ns`/`NsDelta`; the §15.4 per-file invalidation target (≤ 200 µs) guides the hot path (FR-21, global rule 1).

**DoD:**
- `engine.ingest_folder(root)` diffs stored hashes, recomputes only affected slices, and returns `IngestDelta { recomputed_files, transitively_invalidated, changed_symbols, removed_symbols, .. }`.
- Touching one file recomputes exactly that file plus its transitive dependents — nothing else.
- Deleting a file invalidates its symbols and dependents (supports §13 #7 downstream).
- Perf test passes on a ≥ 1k-file fixture: second unchanged run ≤ 5% × CI_TOLERANCE of the first.

Re-pointing at the same folder must be nearly free: files are content-hash-diffed, unchanged files are skipped, and only changed files plus their transitively affected summaries are recomputed (§17.3, FR-20). A delta against the previous revision is emitted so downstream consumers see exactly what moved. The §13 #10 acceptance bound — unchanged re-ingest ≤ 5% of first-ingest wall time — is enforced as an `#[ignore]`-gated perf test with CI_TOLERANCE.

**Validation test sketch (contract):**

```rust
#[test]
fn task_069_incremental_reingest_only_affected() -> anyhow::Result<()> {
    let (root, mut engine) = ingest_once(repo_fixture("polyglot"))?;   // rev 1
    let again = engine.ingest_folder(&root)?;                          // rev 2, unchanged
    assert_eq!(again.recomputed_files, 0, "hash-diff must skip unchanged files");
    assert_eq!(again.transitively_invalidated, 0);
    assert!(again.semantic_diff().is_empty(), "unchanged re-ingest = empty diff");

    engine.write("src/api.py", "def checkout(cart, promo=None): ...\n")?; // change one file
    let delta = engine.ingest_folder(&root)?;                          // rev 3
    assert_eq!(delta.recomputed_files, 1);
    assert!(delta.transitively_invalidated >= 1, "callers of api.py are re-summarized");
    assert!(delta.changed_symbols.contains(&symbol("checkout")));
    assert!(delta.removed_symbols.is_empty());

    engine.remove("internal/svc/main.go")?;
    let del = engine.ingest_folder(&root)?;                            // deletion invalidates dependents
    assert!(del.removed_symbols.contains(&symbol("Authorize")));
    Ok(())
}

#[test]
#[ignore] // [perf] — run with: cargo test -- --ignored
fn task_069_reingest_unchanged_wall_time_perf() -> anyhow::Result<()> {
    let (root, mut engine) = ingest_timed(repo_fixture("large_mixed"))?; // ≥ 1k files
    let t0 = Instant::now();
    engine.ingest_folder(&root)?; // unchanged re-ingest (FR-20)
    let elapsed = t0.elapsed();
    // §13 #10: re-ingest ≤ 5% of first-ingest wall time, × CI_TOLERANCE (global rule 5).
    assert!(
        elapsed.as_nanos() as f64 <= 0.05 * FIRST_INGEST_NS.load(Ordering::Relaxed) as f64 * CI_TOLERANCE,
        "re-ingest {elapsed:?} exceeded 5% × CI_TOLERANCE of first ingest"
    );
    Ok(())
}
```

### TASK-070: Ship `cie ingest` CLI with flags, exit codes, config loading

| Field | Value |
|---|---|
| Phase | 11 — Folder Assimilation & CLI |
| Target crate(s) | `cie-cli` (+ `cie-ingest`) |
| Depends on | TASK-068 |
| PRD reference | §17.1 FR-19, FR-23 / §12 slice 8 |
| Parallel group | G-11c |

**Constraints:**
- Flags exactly per §17.1: `--watch`, `--langs`, `--budget-ms`, `--no-llm`, `--parallel`; unknown flags are clap errors (exit 2), not warnings.
- `--as-of-rev`/`--as-of-ns` parse into `RevId`/`Ns` and pass through to queries (FR-23, global rule 1).
- `--no-llm` forces `LlmMode::Off` regardless of config; the engine must still complete with deterministic labels (FR-28).
- Stable exit codes: 0 success; 2 usage/path error; 3 budget/deadline exceeded — documented and tested.
- Progress/status output is one parseable line per revision (`files=`, `symbols=`, `rev=`), suitable for CI logs.

**DoD:**
- `cie ingest /path/to/repo` performs full assimilation and exits 0 with a status line (FR-19 user surface).
- Config file loading: the `[llm]` TOML (§18.4) is honored, with CLI flags overriding file values.
- Missing folder and unknown flag produce exit 2 + a readable error, never a panic.
- `--langs py,rs,go` restricts the adapter set; `--parallel 16` bounds the rayon pool.

This task exposes assimilation as the §17.1 CLI: `cie ingest <path>` with `--watch`, `--langs`, `--budget-ms`, `--no-llm`, and `--parallel`, plus `--as-of-rev`/`--as-of-ns` passthrough for time-travel queries (FR-23). The command prints progress/status (files, symbols, rev), maps failures to stable exit codes, and loads the `[llm]` TOML config (§18.4) with CLI flags overriding file values. This is the user-facing surface Slice 8 promises: `cie ingest /path` works end-to-end.

**Validation test sketch (contract):**

```rust
#[test]
fn task_070_cie_ingest_cli() -> anyhow::Result<()> {
    use clap::Parser as _;
    // §17.1 flags parse into typed options.
    let cli = Cli::try_parse_from(["cie", "ingest", REPO.path(), "--langs", "py,rs,go",
                                   "--budget-ms", "60000", "--no-llm", "--parallel", "16"])?;
    let Command::Ingest { path, langs, budget_ms, no_llm, parallel, as_of_rev, as_of_ns, .. } =
        cli.command.clone() else { panic!("expected ingest subcommand") };
    assert_eq!(langs, vec![Lang::Python, Lang::Rust, Lang::Go]);
    assert_eq!(budget_ms, Some(60_000));
    assert!(no_llm);
    assert_eq!(parallel, Some(16));
    assert_eq!(as_of_rev, None);
    assert_eq!(as_of_ns, None); // FR-23 passthrough accepted and typed as RevId/Ns

    // Run against a temp repo: exit code 0, progress/status line, config file honored.
    let mut out = Vec::new();
    let code = cli.run_with_buffered_stdout(&mut out, config_toml("[llm]\nprovider = \"ollama\"\n"))?;
    assert_eq!(code, 0);
    let stdout = String::from_utf8(out)?;
    assert!(stdout.contains("files=") && stdout.contains("symbols=") && stdout.contains("rev="));

    // Failure paths: missing folder → exit 2, no panic.
    let bad = Cli::try_parse_from(["cie", "ingest", "/nonexistent/repo"])?;
    assert_eq!(bad.run_quiet()?, 2);
    Ok(())
}
```

## Phase 12 — Document Ingestion & Claims

Phase 12 brings documentation onto equal footing with code: FR-31 parsers normalize every supported document format into spans that preserve `DocSourceLocation`, and FR-32 turns those documents into atomic, typed claims with modality, confidence, and extractor provenance. The rule-based extractor always runs so the drift engine works with the LLM disabled (FR-39, §13 #25), while LLM-assisted candidates are content-addressed and cached. Storage follows §15.1 exactly: `doc:{doc_id}:meta`, `doc:{doc_id}:hash`, `claim:{claim_id}`, and `claim_by_doc:{doc_id}`.

### TASK-071: Implement document ingestion for all FR-31 formats

| Field | Value |
|---|---|
| Phase | 12 — Document Ingestion & Claims |
| Target crate(s) | `cie-docs` |
| Depends on | TASK-004 |
| PRD reference | FR-31 / §14.10 `DocSourceLocation` / §15.1 `doc:` |
| Parallel group | G-12a |

**Constraints:**
- All FR-31 formats have parsers: Markdown, plain text, PDF, DOCX, HTML, OpenAPI (YAML/JSON), Protobuf, GraphQL SDL, Confluence export, Notion export, Jira/Linear export, git commit messages, PR descriptions; a missing/failed parser is a registered error, never a silent skip.
- `DocSourceLocation { doc, page, section, paragraph, char_start, char_end }` spans are preserved verbatim — page/section/paragraph wherever the format provides them (§14.10).
- Keys `doc:{doc_id}:meta` and `doc:{doc_id}:hash` per §15.1 (global rule 7); identical bytes yield an identical hash (dedup path).
- Parsers run in parallel with per-document error isolation (§19.2 stage 1).
- No `chrono`: document timestamps normalize to `Ns` (FR-21, global rule 1).

**DoD:**
- `DocIngester::ingest_dir(path)` ingests a fixture set covering all 13 format families and returns `DocMeta` + span lists per document.
- PDF spans carry page numbers; Markdown/HTML/Confluence/Notion spans carry interned section names; commit-message parser consumes a git-log feed.
- `doc:{doc_id}:hash` round-trips: re-ingesting unchanged bytes reuses the same `DocumentId`.
- A malformed document yields `DocError` for that doc only; sibling documents are unaffected.

This task brings documents into the store on equal footing with code: parsers for all FR-31 formats normalize each source into `DocMeta` plus spans that preserve `DocSourceLocation { doc, page, section, paragraph, char_start, char_end }`. Content hashes under `doc:{doc_id}:hash` make document ingestion incremental, and a malformed document is isolated as a `DocError` without disturbing siblings. Everything downstream — claims, mappings, drift — addresses its documentary evidence back through these spans.

**Validation test sketch (contract):**

```rust
#[test]
fn task_071_document_ingestion_all_formats() -> anyhow::Result<()> {
    let dir = fixture_dir(|fs| {
        fs.file("PRD.md", "# Checkout\n\nThe system **must** never double-charge.\n");
        fs.file("notes.txt", "refund path is manual for now.\n");
        fs.file("spec.pdf", pdf_bytes(3));                       // page-preserving parser
        fs.file("design.docx", docx_bytes());
        fs.file("landing.html", "<h2>Plans</h2><p>Pro may scale to 1M users.</p>");
        fs.file("openapi.yaml", openapi_yaml());
        fs.file("openapi.json", openapi_json());                 // OpenAPI YAML/JSON
        fs.file("api.proto", "service Payments { rpc Charge (Req) returns (Resp); }");
        fs.file("schema.graphql", "type Order { id: ID! total: Money! }");
        fs.file("confluence.html", confluence_export());         // Confluence export
        fs.file("notion.md", notion_export());                   // Notion export
        fs.file("jira.json", jira_issues());                     // Jira/Linear export
        fs.file("commits.gitlog", "fix(pay): charge only once per order"); // git commit messages
    });
    let docs = DocIngester::with_parsers(default_doc_parsers()).ingest_dir(&dir)?; // parallel, isolated
    assert_eq!(docs.len(), 13, "every FR-31 fixture ingested");

    let gql = docs.by_file("schema.graphql")?;
    assert!(matches!(gql.meta.format, DocFormat::GraphqlSdl));
    // §14.10: spans preserve char ranges; PDF carries page numbers; MD carries sections.
    let span = docs.first_span(&gql, "id: ID!")?.expect("span");
    assert!(span.char_start < span.char_end);
    let pdf = docs.by_file("spec.pdf")?;
    assert!(pdf.spans_on_page(2).next().is_some());
    let md = docs.by_file("PRD.md")?;
    assert_eq!(md.section_of_span(md.spans()[0]), Some("Checkout"));

    // Storage keys + content addressing (§15.1): re-ingest of identical bytes is a no-op.
    let store = docs.into_store();
    assert!(store.doc_meta(gql.id).is_some());
    let reingested = DocIngester::with_parsers(default_doc_parsers()).ingest_dir(&dir)?;
    assert_eq!(reingested.by_file("schema.graphql")?.id, gql.id, "same bytes → same doc id");
    Ok(())
}
```

### TASK-072: Extract typed claims with rule-based plus LLM-assisted extractors

| Field | Value |
|---|---|
| Phase | 12 — Document Ingestion & Claims |
| Target crate(s) | `cie-claims` (+ `cie-llm`) |
| Depends on | TASK-063, TASK-071 |
| PRD reference | FR-32, FR-39 / §14.10 `Claim` / §13 #25 |
| Parallel group | G-12b |

**Constraints:**
- The rule-based extractor is unconditional: LLM-off must still extract claims and all verifications must remain runnable (FR-39, §13 #25).
- Exactly 8 `ClaimKind` values (Capability, Behavior, Constraint, Interface, DataModel, NonFunctional, Sequence, Prohibition) and 5 `Modality` values (shall|must|should|may|will); anything else is a parse error (FR-32, §14.10).
- LLM candidates are content-addressed, cached (FR-25), and tagged `Extractor::Llm { model_id, prompt_hash }` — candidates only, never verdicts (global rule 4).
- Every claim keeps `source_loc` with `char_start < char_end` and `confidence ∈ [0, 1]` (§14.10).
- Extraction runs in parallel per document with per-doc error isolation (§19.2 stage 2).

**DoD:**
- `extract_claims(&doc, ExtractOptions)` produces `Claim` records with `id, text, kind, modality, loc, confidence, extractor, created_ns` (§14.10).
- Modality/kind heuristics pass on the fixture: "must never double-charge" → `Prohibition`/`Must`; "should retry" → `Should`.
- LLM-assisted run yields a superset of the rule-based run; new candidates carry `Extractor::Llm` and are served from cache on the second run.
- §13 #25 asserted on the fixture: rule-only claims ≥ 60% of full-extraction claims.

This task turns documents into atomic, typed claims per FR-32: eight `ClaimKind`s, five modalities, source location, confidence, and extractor provenance. The rule-based extractor always runs (FR-39); the LLM extractor only proposes candidates, which are content-addressed, cached, and tagged `Extractor::Llm { model_id, prompt_hash }`. The §13 #25 bound — LLM-off still yields ≥ 60% of claims and all verifications still run — is asserted directly on the fixture.

**Validation test sketch (contract):**

```rust
#[test]
fn task_072_claim_extraction_rules_and_llm_off() -> anyhow::Result<()> {
    let doc = ingest_doc(fixture("prd_50_claims.md"))?; // 50 explicit claims (FR-32 fixture)
    // Rule-based extractor ALWAYS runs (FR-39); LLM is optional and content-addressed.
    let rules = extract_claims(&doc, ExtractOptions { llm: LlmMode::Off, ..Default::default() })?;
    assert!(!rules.is_empty());

    let c = rules.find("The system must never double-charge a card").expect("explicit claim found");
    assert_eq!(c.kind, ClaimKind::Prohibition);
    assert_eq!(c.modality, Modality::Must);
    assert!(matches!(c.extractor, Extractor::Rule));
    assert!(c.loc.char_start < c.loc.char_end && (0.0..=1.0).contains(&c.confidence));
    let kinds: HashSet<_> = rules.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&ClaimKind::Behavior) && kinds.contains(&ClaimKind::Constraint));

    // LLM-assisted extraction: candidates tagged, content-addressed, cached (FR-25, FR-39).
    let llm_mock = MockLlmProvider::claim_candidates();
    let full = extract_claims(&doc, ExtractOptions { llm: LlmMode::Provider(llm_mock.clone()), ..Default::default() })?;
    assert!(full.len() >= rules.len());
    assert!(full.iter().any(|c| matches!(c.extractor, Extractor::Llm { model_id, prompt_hash }
        if !model_id.is_empty() && prompt_hash != Hash::ZERO)));
    assert_eq!(llm_mock.transport_calls(), 0, "second identical run is served from cache");

    // §13 #25: LLM-off still yields ≥ 60% of claims; verifications remain runnable.
    assert!(rules.len() as f32 / full.len() as f32 >= 0.60,
        "rule-based extraction covers {}/{} claims", rules.len(), full.len());
    Ok(())
}
```

### TASK-073: Persist ClaimStore with dedup and provenance

| Field | Value |
|---|---|
| Phase | 12 — Document Ingestion & Claims |
| Target crate(s) | `cie-claims` (+ `cie-store`) |
| Depends on | TASK-072 |
| PRD reference | FR-32 / §15.1 `claim:`, `claim_by_doc:` / §13 #17 (downstream) |
| Parallel group | G-12c |

**Constraints:**
- Keys exactly `claim:{claim_id}` → `Claim` and `claim_by_doc:{doc_id}` → `SmallVec<[ClaimId; 16]>` (§15.1, global rule 7).
- Dedup on (doc, char span, text hash): re-extraction is idempotent and never duplicates claims (FR-32 atomicity).
- Per-claim provenance (extractor, doc, `created_ns` as `Ns`) is part of the stored record and survives store round-trips (global rule 2, FR-21).
- The ≥ 90% non-`Unknown` acceptance (§13 #17) is a downstream contract — this task prepares the 50-claim fixture but must not fake the result; it is checked by TASK-075/TASK-077.

**DoD:**
- `ClaimStore::open(store)` with `get(claim_id)`, `by_doc(doc_id)`, `put`, and idempotent `extract`.
- 50-claim PRD fixture wired as the extraction benchmark entry point (`benchmark_50_claims`).
- `claim_by_doc` returns claims ordered by `char_start` for stable downstream processing.
- Provenance round-trip test passes: reload from a forked store returns identical `extractor` and `created_ns`.

This task defines the claim persistence layer per §15.1: `claim:{claim_id}` → `Claim` and `claim_by_doc:{doc_id}` → `SmallVec<[ClaimId; 16]>`, with dedup so re-extraction is idempotent and per-claim provenance that survives store round-trips. The 50-claim PRD fixture is wired here as the extraction benchmark; the ≥ 90% non-`Unknown` acceptance (§13 #17) is noted as a downstream contract, checked once verification (TASK-075) and the harness (TASK-077) exist.

**Validation test sketch (contract):**

```rust
#[test]
fn task_073_claim_store_indexes_dedup_provenance() -> anyhow::Result<()> {
    let mut store = ClaimStore::open(Store::in_memory())?;
    let doc = ingest_doc(fixture("mini_prd.md"))?;
    store.extract(&doc, ExtractOptions::rules_only())?;

    // Index shapes per §15.1: claim:{claim_id} -> Claim, claim_by_doc:{doc_id} -> SmallVec<[ClaimId;16]>.
    let ids = store.by_doc(doc.id)?;
    assert!(!ids.is_empty(), "claims indexed per doc");
    for id in &ids {
        let claim = store.get(*id)?.expect("claim:{id} present");
        assert_eq!(claim.doc, doc.id);
        assert!(claim.created_ns > 0 && (0.0..=1.0).contains(&claim.confidence));
        assert!(matches!(claim.extractor, Extractor::Rule | Extractor::Llm { .. }));
    }
    // Claims come back ordered by source position.
    let starts: Vec<u32> = ids.iter().map(|id| store.get(*id)?.unwrap().loc.char_start).collect();
    let mut sorted = starts.clone(); sorted.sort_unstable();
    assert_eq!(starts, sorted);

    // Dedup: re-extraction of the same text at the same location must not duplicate.
    let before = store.by_doc(doc.id)?.len();
    store.extract(&doc, ExtractOptions::rules_only())?;
    assert_eq!(store.by_doc(doc.id)?.len(), before, "dedup by (doc, char_start, char_end, text-hash)");

    // Provenance survives a store round-trip (global rule 2).
    let first = store.get(ids[0])?.unwrap();
    let reloaded = ClaimStore::open(store.fork())?.get(ids[0])?.unwrap();
    assert_eq!(first.extractor, reloaded.extractor);
    assert_eq!(first.created_ns, reloaded.created_ns);
    Ok(())
}
```

## Phase 13 — Drift Engine

Phase 13 implements the document–code drift engine of §19: claim–code mapping proposals from four families of signals (FR-33), per-claim verification through the exact FR-34 checker table reduced to the eight FR-35 statuses, and the deliverable layer — versioned drift reports, reverse drift, multi-document reconciliation, and cross-revision deltas (FR-36–FR-38). The engine is an evidence-backed classifier, not an oracle (§19.1): every status carries evidence, LLM output stays candidate-only, and a timeout never masquerades as `Implemented`.

### TASK-074: Propose claim–code mappings from four signal families

| Field | Value |
|---|---|
| Phase | 13 — Drift Engine |
| Target crate(s) | `cie-drift` (+ `cie-store`, `cie-llm`) |
| Depends on | TASK-036, TASK-059, TASK-073 |
| PRD reference | FR-33 / §19.2 / §14.10 `ClaimMapping` / §15.1 `map:` |
| Parallel group | G-13a |

**Constraints:**
- Proposer set per FR-33: embedding similarity (HNSW), symbol-name/path matching, behavioral-signature matching, and LLM-proposed (optional).
- LLM proposals are candidate-only, never authoritative, and never the sole input to a verification decision (FR-39, global rule 4, §19.5).
- Every mapping records `{confidence, evidence, proposer}` and is persisted at `map:{claim_id}` → `SmallVec<[ClaimMapping; 4]>` (§14.10, §15.1, global rule 2).
- Mapping is deterministic given the same inputs and embedding index: re-running reproduces identical proposals.
- Incremental per §19.2: changing a document re-maps only affected claims, not the whole corpus.

**DoD:**
- `ClaimMapper` with pluggable `Proposer`s; all four FR-33 families implemented behind the trait.
- `map:{claim_id}` populated for every claim with `confidence ∈ [0, 1]`, non-empty `evidence`, and a tagged `proposer`.
- Name/path matching resolves obvious hits (claim mentions "checkout" → `CheckoutService` symbols); embedding matching resolves paraphrases with no name overlap.
- Re-mapping after a doc change touches only claims from that doc (asserted via recomputation counter).

This task implements FR-33 mapping: for each claim, propose `SymbolId`/`ConceptId`/`HierarchyNodeId` targets from four proposer families — HNSW embedding similarity, symbol-name/path matching, behavioral-signature matching over `FunctionSummary`s, and LLM proposals. Proposals are persisted at `map:{claim_id}` with confidence, evidence, and proposer, and LLM proposals are flagged candidate-only — never authoritative (FR-39, §19.5). Mapping is incremental per §19.2: changing a doc re-maps only affected claims.

**Validation test sketch (contract):**

```rust
#[test]
fn task_074_claim_mapping_proposers_and_evidence() -> anyhow::Result<()> {
    let code = assimilate_fixture("shop")?;   // semantic model with summaries + embeddings
    let doc = ingest_doc(fixture("shop_prd.md"))?;
    let claims = ClaimStore::open(code.store())?;
    claims.extract(&doc, ExtractOptions::default())?;

    let mapper = ClaimMapper::new(&code)
        .with_proposer(Proposer::EmbeddingSimilarity(EmbeddingIndex::hnsw()))
        .with_proposer(Proposer::NamePathMatch)         // symbol-name + path matching
        .with_proposer(Proposer::BehavioralSignature);  // FunctionSummary-based matching

    for claim in claims.iter() {
        let mappings = mapper.propose(claim)?;          // stored at map:{claim_id} (§15.1)
        assert!(!mappings.is_empty(), "claim {} has no proposal", claim.id.0);
        for m in &mappings {
            assert!((0.0..=1.0).contains(&m.confidence));
            assert!(!m.evidence.is_empty(), "FR-33: every mapping carries evidence");
            assert!(matches!(m.proposer, Extractor::Rule | Extractor::Llm { .. }));
            if matches!(m.proposer, Extractor::Llm { .. }) {
                assert!(m.candidate_only, "LLM proposals are never authoritative (FR-39)");
            }
        }
    }
    // Name match hits the obvious symbol; embedding match hits a paraphrase with no name overlap.
    let direct = mapper.propose(&claims.find("checkout must verify payment before fulfillment")?)?;
    assert!(direct.iter().any(|m| m.symbols.iter().any(|s| code.name_of(*s).contains("checkout"))));
    let paraphrase = mapper.propose(&claims.find("the buyer's card is validated before goods ship")?)?;
    assert!(paraphrase.iter().any(|m| m.confidence > 0.5), "embedding proposer fires without name overlap");
    Ok(())
}
```

### TASK-075: Verify claims via checker table into DriftStatus

| Field | Value |
|---|---|
| Phase | 13 — Drift Engine |
| Target crate(s) | `cie-drift` (+ `cie-counterexample`, `cie-analysis`) |
| Depends on | TASK-042, TASK-047, TASK-074 |
| PRD reference | FR-34, FR-35 / §14.10 `ClaimVerification` / §13 #20, #26 / §19.5 |
| Parallel group | G-13b |

**Constraints:**
- Checker table is exactly FR-34 and `CheckerKind` per §14.10 — no extra checkers, no missing checkers: Capability→Existence, Behavior→BehaviorMatch, Constraint→Smt, Interface/DataModel→Structural, NonFunctional→Runtime, Sequence→Protocol, Prohibition→PathSearch (+ KLEE-style counterexample).
- Exactly one of the 8 `DriftStatus` values per claim; the status constructor refuses `Implemented` without a non-empty `EvidenceRef` (FR-35, global rule 2).
- A timed-out checker yields `Unknown`, never `Implemented` (global rule 3, §13 #26); all checker deadlines are `Ns` (FR-21).
- `NonFunctional` claims without runtime evidence return `Unverifiable` with reason `requires_runtime_evidence` (§13 #20, §19.5).
- An inferred/LLM mapping alone never yields `Implemented` — verification is mechanical (FR-35 rules, global rule 4); results persist at `verify:{claim_id}:{rev}` (§15.1, global rule 7).

**DoD:**
- `verify_all(model, claims, VerifyOptions)` runs the FR-34 table in parallel under `deadline_ns` and returns `ClaimVerification` records with counterexamples, assumptions, and coverage (§14.10).
- All 8 statuses are reachable on the crafted fixture suite (FR-35 acceptance).
- §13 #20 and §13 #26 scenarios pass as dedicated assertions.
- `store.get_verify(claim_id, rev)` round-trips every persisted verification.

This task runs the FR-34 checker table — existence, behavior match, SMT, structural, protocol mining, runtime, path search + counterexample — against the mapped code and reduces each claim to exactly one of the eight FR-35 statuses. The FR-35 rules are enforced structurally: `Implemented` requires evidence, a timeout degrades to `Unknown` and never `Implemented` (§13 #26), and `NonFunctional` claims without runtime evidence come back `Unverifiable` with reason `requires_runtime_evidence` (§13 #20). Results persist at `verify:{claim_id}:{rev}` with counterexamples and assumptions.

**Validation test sketch (contract):**

```rust
#[test]
fn task_075_checker_table_statuses_and_rules() -> anyhow::Result<()> {
    // FR-34 checker table, exactly (§14.10 CheckerKind):
    assert_eq!(checker_for(ClaimKind::Capability), CheckerKind::Existence);
    assert_eq!(checker_for(ClaimKind::Behavior), CheckerKind::BehaviorMatch);
    assert_eq!(checker_for(ClaimKind::Constraint), CheckerKind::Smt);
    assert_eq!(checker_for(ClaimKind::Interface), CheckerKind::Structural);
    assert_eq!(checker_for(ClaimKind::DataModel), CheckerKind::Structural);
    assert_eq!(checker_for(ClaimKind::NonFunctional), CheckerKind::Runtime);
    assert_eq!(checker_for(ClaimKind::Sequence), CheckerKind::Protocol);
    assert_eq!(checker_for(ClaimKind::Prohibition), CheckerKind::PathSearch);

    let suite = fixture_suite(); // claims crafted to reach all 8 statuses (FR-35)
    let out = verify_all(&suite, VerifyOptions {
        deadline_ns: now_ns() + 5_000_000_000, ..VerifyOptions::default()
    })?;
    let statuses: HashSet<DriftStatus> = out.verifications().map(|v| v.status).collect();
    assert_eq!(statuses.len(), 8, "all statuses reachable: {statuses:?}");

    // §13 #20: p99-latency claim without runtime evidence → Unverifiable{requires_runtime_evidence}.
    let nf = out.by_text("p99 latency stays under 200 ms")?;
    assert_eq!(nf.status, DriftStatus::Unverifiable);
    assert!(nf.reason.contains("requires_runtime_evidence"));
    // §13 #26: a timed-out checker yields Unknown, never Implemented (global rule 3).
    let slow = out.by_text("synchronizes all replicas before ack")?;
    assert_eq!(slow.status, DriftStatus::Unknown);

    // FR-35 rules + persistence at verify:{claim_id}:{rev} (§15.1).
    for v in out.verifications() {
        if v.status == DriftStatus::Implemented {
            assert!(!v.evidence.is_empty(), "Implemented requires evidence");
        }
        assert_eq!(store.get_verify(v.claim, out.rev)?.status, v.status);
    }
    Ok(())
}
```

### TASK-076: Assemble drift reports, reverse drift, reconciliation, deltas

| Field | Value |
|---|---|
| Phase | 13 — Drift Engine |
| Target crate(s) | `cie-drift` (+ `cie-cli`, `cie-store`) |
| Depends on | TASK-075 |
| PRD reference | FR-36, FR-37, FR-38 / §19.3–19.5 / §13 #21–#24 / §15.1 `drift:`, `drift_delta:`, `reverse:` |
| Parallel group | G-13c |

**Constraints:**
- Keys exactly `drift:{rev}:{doc_id}` → `DriftReport`, `reverse:{rev}` → undocumented symbols, `drift_delta:{from_rev}:{to_rev}` → `DriftDelta` (§15.1, global rule 7).
- Reconciliation per FR-38: same claim + same status merges; same claim + different status → `Contradicting` citing both sources; superseded claims are marked with history (§13 #22, §19.5).
- `drift_delta` contains only claims whose status changed between the two revisions (FR-37, §13 #23).
- Every status, mapping, and counterexample in the report carries evidence; aggregates sum to `total_claims` (global rule 2, §19.4).
- Perf: unchanged re-run ≤ 5% of first-run wall time × CI_TOLERANCE, tagged `#[perf]` + `#[ignore]` (§13 #24, global rule 5).

**DoD:**
- `drift_report(model_rev, docs) -> DriftReport` per §14.10 with `DriftAggregate` (by-status counts, per-subsystem `DriftScore`) and YAML/JSON/Markdown renderers (§19.3–19.4).
- `reverse:{rev}` lists `Undocumented` symbols with reasons; a symbol with no claim lands there (FR-36, §13 #21).
- Multi-doc reconciliation scenario passes: two conflicting docs → one `Contradicting` claim citing both sources.
- `cie drift <code> <doc>` CLI with `--format`, `--status`, `--from-rev`, `--to-rev` flags (§19.3); `--from-rev 41 --to-rev 42` shows only changed statuses.

This task assembles and persists the deliverables: the versioned `DriftReport` at `drift:{rev}:{doc}` (FR-37, §19.4), the reverse-drift list of `Undocumented` code at `reverse:{rev}` (FR-36), multi-document reconciliation (FR-38), and cross-revision deltas at `drift_delta:{from_rev}:{to_rev}` so `--from-rev 41 --to-rev 42` shows only changed statuses (§13 #23). The `cie drift <code> <doc>` CLI renders YAML/JSON/Markdown (§19.3). Re-running drift with unchanged inputs must cost ≤ 5% of the first run (§13 #24, `#[perf]`-gated).

**Validation test sketch (contract):**

```rust
#[test]
fn task_076_report_reverse_reconcile_delta() -> anyhow::Result<()> {
    // Full report at drift:{rev}:{doc}; aggregates sum to total (FR-37, §19.4).
    let rep = drift_report(model_rev(42), &[doc_prd(), doc_api()])?;
    assert_eq!(rep.aggregate.by_status.iter().sum::<u32>(), rep.aggregate.total_claims);
    assert!(rep.verifications.iter().all(|v| v.evidence.complete()), "every status evidenced");

    // Reverse drift: symbol with no claim lands in reverse:{rev} as Undocumented (FR-36, §13 #21).
    assert!(rep.reverse_drift.contains(&symbol("Inventory.top_up")));

    // Multi-doc reconciliation (FR-38): conflict cites both sources (§13 #22).
    let conflict = rep.by_text("payment failure must prevent fulfillment")?;
    assert_eq!(conflict.status, DriftStatus::Contradicting);
    assert_eq!(conflict.sources.len(), 2);
    let merged = rep.by_text("orders are immutable after fulfillment")?; // same claim+status in both docs
    assert_eq!(merged.sources.len(), 2, "same claim, same status → merged, both cited");
    assert!(rep.superseded_history("refund window is 30 days").is_some(), "superseded keeps history");

    // Delta: only changed claim statuses between revisions (FR-37, §13 #23).
    let delta = drift_delta(41, 42)?;
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(delta.changed[0].before, DriftStatus::Implemented);
    assert_eq!(delta.changed[0].after, DriftStatus::Drifted);
    Ok(())
}

#[test]
#[ignore] // [perf] — run with: cargo test -- --ignored
fn task_076_drift_rerun_unchanged_perf() -> anyhow::Result<()> {
    let first = time_ns(|| drift_report(model_rev(7), &[doc_prd(), doc_api()]))?;
    let again = time_ns(|| drift_report(model_rev(7), &[doc_prd(), doc_api()]))?;
    // §13 #24: rerun with unchanged docs and code ≤ 5% × CI_TOLERANCE of first run.
    assert!(again <= first / 20 * CI_TOLERANCE, "rerun {again:?} > 5% × CI_TOLERANCE of first run");
    Ok(())
}
```

## Phase 14 — Acceptance Harness

Phase 14 closes the loop: a CI acceptance harness that registers all 26 PRD §13 items as named, individually runnable integration tests, wired so a failing item fails the build with its item number and PRD quote. The harness doubles as traceability documentation through a per-item evidence manifest naming the implementing tasks and test paths. Performance items are `#[ignore]`-gated and compare against the §15.4 latency targets multiplied by CI_TOLERANCE, keeping CI fast without weakening the contracts.

### TASK-077: Build acceptance harness registering all 26 PRD items

| Field | Value |
|---|---|
| Phase | 14 — Acceptance Harness |
| Target crate(s) | `cie-cli` (`tests/acceptance/`) + workspace `tests/` |
| Depends on | TASK-001…TASK-076 (full system) |
| PRD reference | §13 items 1–26 / §12 / §15.4 |
| Parallel group | G-14a |

**Constraints:**
- The harness registers exactly the 26 §13 items; each item names its implementing task(s) and test path in a per-item evidence manifest (global rule 2).
- `ACCEPTANCE_ITEM_COUNT = 26` is LOCKED: the constant's doc comment states that ONLY TASK-084 (Phase 15, FR-40–44 addendum) is authorized to raise it to 31 — no other change is permitted.
- `[perf]` items (§13 #10, #13, #15, #16, #24) are `#[ignore]`-gated and compare against §15.4 targets × CI_TOLERANCE (global rule 5).
- A failing item fails CI with item number, PRD text, and owning task; the harness never skips silently.
- All harness timing uses `Instant` measurement against `Ns`-denominated targets — no `chrono` (FR-21, global rule 1).

**DoD:**
- `AcceptanceSuite::registered()` exposes items 1–26, each with `implemented_by: Vec<TaskId>`, `test_path`, `is_perf`, `ignore_gated`.
- Meta-test `task_077_harness_registers_all_26_items` passes and asserts the manifest spot-checks (e.g., item 12 → TASK-063; item 17 → TASK-073 + TASK-075; item 23 → TASK-076; item 26 → TASK-075).
- `cargo test` runs all functional items on every push; `cargo test -- --ignored` runs the `[perf]` set in the nightly CI job.
- The LOCKED-constant note appears verbatim in source and any manifest gap (missing implementing task or test path) fails the meta-test.

This task builds the CI acceptance harness that registers all 26 §13 items as named, individually runnable integration tests, plus a meta-test asserting the registry itself. Each item carries an evidence manifest entry (implementing task ids + test path) so the suite doubles as traceability documentation for the whole deliverable. The `ACCEPTANCE_ITEM_COUNT = 26` constant is LOCKED: only TASK-084 (Phase 15, FR-40–44 addendum) is authorized to raise it to 31.

**Validation test sketch (contract):**

```rust
/// LOCKED constant. Change requires the Phase 15 addendum: ONLY TASK-084 is authorized to
/// raise ACCEPTANCE_ITEM_COUNT from 26 to 31 (items 27–31 = FR-40…FR-44). Any other change
/// to this constant must fail code review.
pub const ACCEPTANCE_ITEM_COUNT: usize = 26;

#[test]
fn task_077_harness_registers_all_26_items() {
    let suite = AcceptanceSuite::registered();
    assert_eq!(suite.len(), ACCEPTANCE_ITEM_COUNT, "must register exactly 26 items (PRD §13)");

    for n in 1..=ACCEPTANCE_ITEM_COUNT as u16 {
        let item = suite.item(n).unwrap_or_else(|| panic!("§13 item {n} missing from harness"));
        assert!(!item.implemented_by.is_empty(), "item {n} has no evidence manifest");
        assert!(item.implemented_by.iter().all(|t| t.0 <= 77), "item {n} credits a future task");
        assert!(!item.test_path.is_empty(), "item {n} is not wired to a test");
        if item.is_perf {
            assert!(item.ignore_gated, "[perf] item {n} must be #[ignore]-gated (CI_TOLERANCE applies)");
        }
    }

    // Manifest spot-checks for the phase 10–14 wiring.
    assert_eq!(suite.item(12).unwrap().implemented_by, &[TaskId(63)]);           // offline Ollama labeling
    assert_eq!(suite.item(13).unwrap().implemented_by, &[TaskId(65)]);           // cache hit ≤ 100 ns [perf]
    assert_eq!(suite.item(17).unwrap().implemented_by, &[TaskId(73), TaskId(75)]); // 50 claims ≥ 90% non-Unknown
    assert_eq!(suite.item(23).unwrap().implemented_by, &[TaskId(76)]);           // --from-rev/--to-rev delta
    assert_eq!(suite.item(26).unwrap().implemented_by, &[TaskId(75)]);           // timeout → Unknown
}

#[test]
#[ignore] // [perf] — nightly CI: cargo test -- --ignored
fn task_077_perf_items_within_tolerance() -> anyhow::Result<()> {
    // Every [perf] item runs under CI_TOLERANCE against its §15.4 target; any miss fails with
    // the item number and PRD target in the message.
    for item in AcceptanceSuite::registered().perf_items() {
        item.run_with(CI_TOLERANCE).map_err(|e| {
            anyhow!("§13 item {} ({target}) failed: {e}", item.id, target = item.prd_target)
        })?;
    }
    Ok(())
}
```

## Phase 15 — Hierarchical Concept Behavior (Implementation Addendum, FR-40…FR-44)

This phase implements the approved implementation addendum: hierarchical **concept nodes** that bind verified behaviors into a parent–child DAG, carry a per-edge reason with evidence, roll behavior status upward under weakest-link semantics, invalidate incrementally along parent edges only, and expose the result through a `cie concepts` CLI. It also extends the acceptance suite from 26 to 31 items. Nothing here relaxes any PRD invariant: LLM output still never sets `Explicit`/`Verified` authority (FR-28), every frozen conclusion still requires evidence (FR-4), and every rollup must equal a clean recomputation over its sub-DAG.

### TASK-078: Add ConceptNode types and the six concept_node storage keys

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-concepts`, `cie-store`, `cie-core` |
| Depends on | TASK-024, TASK-037, TASK-044, TASK-046 |
| PRD reference | Addendum FR-40; §14.6–14.7; §15.1 |
| Parallel group | G-15a |

**Constraints:**
- `ConceptNodeId(u32)` is minted by `cie-core`; node identity is content-addressed with `blake3(kind, sorted members, rev)` so identical node sets collapse to one id.
- Exactly six storage keys are introduced, prefixed `concept_node` and registered in the §15.1 namespace codec: `concept_node:{id}`, `concept_node_by_rev:{rev}:{id}`, `concept_node_children:{id}`, `concept_node_parent_edges:{id}`, `concept_node_bindings:{id}`, `concept_node_rollup:{rev}:{id}`.
- Every `ConceptNode` carries an `EvidenceRef` (FR-4); constructing or persisting a node without evidence is a compile-time or immediate runtime error.
- `label` is immutable after first write (§14.7). An LLM-sourced label must be a `Label { source: Llm, model_id, prompt_hash }` (FR-28) and never touches contract authority.

**DoD:**
- `ConceptNode`, `ConceptEdge`, `EdgeReason`, `BindingSource`, `ConceptBinding`, `ConceptRollup` defined and `rkyv`-archivable.
- All six keys implemented behind the `cie-store` codec with roundtrip tests for each.
- Reopening the store reloads identical nodes; rewriting a node leaves zero stale keys behind.

**Validation test sketch (contract):**

```rust
#[test]
fn task_078_concept_node_roundtrip_and_keys() {
    let store = Store::open_temp();
    let mut nodes = ConceptNodeStore::new(&store);
    let node = ConceptNode::builder()
        .kind(ConceptNodeKind::Fca)
        .members([sym(1), sym(2), sym(3)])
        .evidence(evidence_ref())
        .build(&mut intern);
    nodes.put(rev(41), &node).unwrap();

    let reopened = ConceptNodeStore::new(&Store::open_temp_same(&store));
    assert_eq!(reopened.get(rev(41), node.id).unwrap(), node);
    // six namespaces, no others prefixed concept_node
    for key in nodes.keys_prefixed("concept_node") { /* codec accepts exactly the 6 shapes */ }
    assert!(node.evidence.is_some(), "FR-4: no node without evidence");
}
```

### TASK-079: Bind concepts to behaviors from four evidence sources (+ `cart` fixture)

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-concepts`, `cie-contract` |
| Depends on | TASK-036, TASK-042, TASK-044, TASK-063, TASK-078 |
| PRD reference | Addendum FR-41; FR-28; §13 items 1–5 |
| Parallel group | G-15b |

**Constraints:**
- Bindings come from exactly four sources: `FcaCoOccurrence` (symbols sharing a concept extent), `CallReachability` (callee behavior within the bounded call closure), `ContractJoin` (a registry obligation attaches to a member symbol), `LlmCandidate` (proposed only).
- Every binding records `{ source, confidence, evidence }`; a binding without evidence cannot be inserted (FR-4).
- `LlmCandidate` bindings are inert: they are excluded from any `Verified`/`Explicit` counting, never change authority (FR-28), and are dropped automatically when the LLM is disabled.
- The shared fixture `cart` (small e-commerce repo: `authorize`, `top_up`, `debit`, `refund`) is added here and reused by TASK-080/081/083/084; it contains at least one print-only stub and one genuine implementation.

**DoD:**
- All four binders implemented; on `cart`, each mechanical source yields at least one binding and the stub binds with lower confidence than the genuine implementation.
- LLM-off mode still produces the three mechanical sources end-to-end.
- `cart` fixture committed under `cie-concepts/tests/fixtures/cart/` with a loader helper.

**Validation test sketch (contract):**

```rust
#[test]
fn task_079_four_binding_sources_on_cart() {
    let cart = load_fixture("cart"); // authorize, top_up, debit, refund
    let graph = analyze(cart.clone());
    let bindings = bind_concepts(&graph, &contracts, Llm::disabled());

    assert!(!bindings.with_source(BindingSource::FcaCoOccurrence).is_empty());
    assert!(!bindings.with_source(BindingSource::CallReachability).is_empty());
    assert!(!bindings.with_source(BindingSource::ContractJoin).is_empty());
    assert!(bindings.with_source(BindingSource::LlmCandidate).is_empty(),
        "FR-28: LLM off means zero LlmCandidate bindings");
    assert!(bindings.iter().all(|b| b.evidence.is_some()));
}
```

### TASK-080: Merge hierarchy sources into a single DAG with a reason on every edge

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-concepts` |
| Depends on | TASK-044, TASK-046, TASK-079 |
| PRD reference | Addendum FR-42; §14.7 |
| Parallel group | G-15c |

**Constraints:**
- Merge inputs are exactly: the FCA lattice (superconcept edges), clustering candidates (module/domain membership), package containment, and call ownership (symbol owning its callees' nodes).
- Each output edge is unique by `(parent, child)` and carries `reasons: SmallVec<[EdgeReason; 2]>` drawn from `{FcaLattice, ClusterMembership, PackageContainment, CallOwnership, Manual}` plus an `EvidenceRef` per reason — an edge with no reason is unrepresentable.
- The merged structure is strictly a DAG. If a proposed edge would create a cycle, merging fails with `ConceptCycle` and the store is left untouched (no partial write, atomic batch).
- Fixture `cyclic_concepts` (two nodes each justifying the other) is added here and must trigger the error path.

**DoD:**
- Merging over `cart` yields an acyclic DAG where every edge has ≥ 1 reason + evidence.
- `cyclic_concepts` returns `Err(ConceptCycle)` and a follow-up read proves the store byte-identical to before the attempt.
- Edge dedup verified: two sources justifying the same pair produce one edge with two reasons, not two edges.

**Validation test sketch (contract):**

```rust
#[test]
fn task_080_merge_is_dag_with_edge_reasons() {
    let dag = merge_hierarchy(&cart_graph(), &fca_lattice, &clusters, &packages);
    assert!(dag.is_acyclic());
    assert!(dag.edges().all(|e| !e.reasons.is_empty() && e.evidence.is_some()));

    let before = store.snapshot_bytes();
    let r = merge_hierarchy(&cyclic_concepts_graph(), &lat, &clu, &pkg);
    assert!(matches!(r, Err(ConceptCycle { .. })));
    assert_eq!(store.snapshot_bytes(), before, "no partial merge may persist");
}
```

### TASK-081: Compute rollups — union behaviors, intersect coverage, weakest-link confidence

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-concepts`, `cie-evidence` |
| Depends on | TASK-037, TASK-080 |
| PRD reference | Addendum FR-43; §14.8 |
| Parallel group | G-15d |

**Constraints:**
- Rollup of a node is a pure function of the node and its children's rollups: behaviors = union of own bindings and child rollups; coverage = intersection of all child coverages (and own); confidence = weakest link, i.e. the minimum confidence over own bindings and child rollups.
- `ConceptRollup` is persisted at `concept_node_rollup:{rev}:{id}` with `computed_ns: Ns` (FR-21); no hidden state may influence the value.
- A leaf node's rollup is exactly its own bindings; a node with no bindings and no children has an empty rollup, never a fabricated one.
- Rollup never promotes status: a union containing only `InferredCandidate` behaviors must not read as established (FR-28, FR-4).

**DoD:**
- Property test: `rollup(node) == clean_recompute(sub_dag(node))` holds for `cart` and for ≥ 100 generated random DAGs.
- Three-level example in the test asserts union, intersection, and min-confidence by hand.
- Confidence clamped to `[0.0, 1.0]`; empty-input behavior documented and tested.

**Validation test sketch (contract):**

```rust
#[test]
fn task_081_rollup_union_intersection_weakest_link() {
    let dag = cart_dag_three_levels(); // root -> mid -> leaf
    let r = dag.rollup(rev(41), root);
    let expected_behaviors: BTreeSet<_> =
        union(dag.bindings(root), union(dag.rollup(mid).behaviors, dag.rollup(leaf).behaviors));
    assert_eq!(r.behaviors, expected_behaviors);
    assert_eq!(r.coverage, intersect(dag.coverage(mid), dag.coverage(leaf)));
    assert_eq!(r.confidence, dag.bindings(root).conf.min(dag.rollup(mid).confidence));
}

#[test]
fn task_081_rollup_equals_clean_recompute() {
    proptest!(|dag in random_dag_strategy(100)| {
        assert_eq!(dag.rollup_all(rev(41)), clean_recompute(&dag, rev(41)));
    });
}
```

### TASK-082: Invalidate concept nodes incrementally, propagating only along parent edges

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-concepts`, `cie-incremental` |
| Depends on | TASK-056, TASK-058, TASK-081 |
| PRD reference | Addendum FR-44 (part 1); PRD §13 item 31 (post-addendum numbering); FR-8 |
| Parallel group | G-15e |

**Constraints:**
- When a member symbol's facts change, the owning concept node is marked dirty, its rollup recomputed, and invalidation propagates strictly child → parent along parent edges; it must never travel downward to children.
- Sibling branches of an invalidated node must remain untouched: their rollups and dirty flags are byte-identical before and after.
- Deleting a child removes the node and its outgoing edges, marks every ancestor's rollup dirty, and leaves no stale rollup readable (purge or tombstone, atomically).
- Per-changed-file invalidation inherits the TASK-056 budget of ≤ 200 µs × `CI_TOLERANCE` (`#[perf]`, `#[ignore]`-gated).

**DoD:**
- Sibling-isolation test on a three-branch `cart` DAG: touching branch A leaves B and C identical.
- Deletion test: removed child's rollup keys are gone and ancestors recompute; equivalence with full recompute asserted (reuses TASK-058 property harness).
- `[perf]` parent-edge propagation measured and gated.

**Validation test sketch (contract):**

```rust
#[test]
fn task_082_invalidation_walks_parent_edges_only() {
    let mut ws = cart_workspace_with_three_branches();
    ws.touch_member(branch_a_leaf, edit_member_symbol());
    let dirty = ws.invalidate_concepts(rev(42));

    assert!(dirty.contains(branch_a_leaf) && dirty.contains(root));
    assert!(!dirty.contains(branch_b_leaf) && !dirty.contains(branch_c_leaf),
        "sibling branches must not be touched");

    ws.delete_node(branch_b_leaf);
    assert!(ws.rollup(rev(43), root).is_dirty());
    assert!(ws.rollup_raw(rev(42), branch_b_leaf).is_none(), "no stale rollup survives");
}
```

### TASK-083: Ship the `cie concepts` CLI (md/json/dot, `--with-behaviors`, `--with-rules`, `--max-depth`)

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-cli`, `cie-concepts` |
| Depends on | TASK-070, TASK-082 |
| PRD reference | Addendum FR-44 (part 2); §19.3 (CLI conventions) |
| Parallel group | G-15f |

**Constraints:**
- `cie concepts <store>` renders in exactly three formats: `md` (default, human-readable indented tree), `json` (schema-versioned, machine-readable), `dot` (GraphViz edges labeled with `EdgeReason`).
- Flags: `--with-behaviors` joins binding behavior summaries, `--with-rules` joins contract obligations per member, `--max-depth N` prunes rendering below depth N — pruning is presentation-only and must not mutate the store.
- The command is strictly read-only; a store containing a cycle must exit non-zero with the `ConceptCycle` explanation rather than rendering a partial tree.
- All timestamps in output are `Ns` (FR-21); JSON output is deterministic for identical store bytes (stable key order) so golden files are usable.

**DoD:**
- Golden-file tests for all three formats over the `cart` store, including `--max-depth 1` and `--with-behaviors`.
- Determinism test: two invocations produce byte-identical JSON.
- Cycle-present store exits with code 3 and the `ConceptCycle` message.

**Validation test sketch (contract):**

```rust
#[test]
fn task_083_cli_formats_and_flags() {
    let out_md = cli(["cie", "concepts", store_path, "--format", "md", "--max-depth", "1"]);
    assert!(out_md.contains("cart") && !out_md.contains("leaf-grandchild"));

    let (json_a, json_b) = (cli_json(store_path), cli_json(store_path));
    assert_eq!(json_a, json_b, "deterministic output");
    assert!(json_a["schema_version"].is_string());

    let dot = cli(["cie", "concepts", store_path, "--format", "dot"]);
    assert!(dot.contains("label=\"FcaLattice\"") || dot.contains("label=\"ClusterMembership\""));

    assert_eq!(cli_exit_code(cyclic_store_path), 3); // ConceptCycle, non-zero
}
```

### TASK-084: Extend the acceptance harness from 26 to 31 items (meta-test constant update)

| Field | Value |
|---|---|
| Phase | 15 — Hierarchical Concept Behavior (Addendum FR-40…FR-44) |
| Target crate(s) | `cie-cli` (tests), `cie-concepts` |
| Depends on | TASK-077, TASK-083 |
| PRD reference | Addendum FR-44 (part 3); §13 items 27–31 (post-addendum numbering) |
| Parallel group | G-15g |

**Constraints:**
- This is the ONLY task authorized to change TASK-077's `ACCEPTANCE_ITEM_COUNT` meta-test constant, from `26` to `31`; any other task that edits the constant is a review-blocking defect.
- Five new items are registered, each wired to its implementing task and an executable integration test:
  - 27 — Concept nodes persist under `concept_node:*` keys with evidence and immutable labels (TASK-078).
  - 28 — All four binding sources fire on `cart`; `LlmCandidate` bindings are excluded from `Verified` counts (TASK-079).
  - 29 — Merge yields a DAG with a reason + evidence per edge; `cyclic_concepts` produces `ConceptCycle` with no partial write (TASK-080).
  - 30 — `rollup(node) == clean recompute` (union / coverage intersection / weakest-link confidence) on `cart` (TASK-081).
  - 31 — Concept invalidation propagates only along parent edges; no stale rollup survives a child deletion (TASK-082).
- Items 27–31 inherit the harness rules: evidence-backed status lines, `#[perf]` items `#[ignore]`-gated with `CI_TOLERANCE`.

**DoD:**
- `ACCEPTANCE_ITEM_COUNT = 31` with the five new harness entries and a completed `implemented_by` manifest (27→078 … 31→082).
- Full suite green: all 31 items, plus the meta-test asserting exactly 31 registrations.
- Diff review shows the only harness change is the constant + five appended entries.

**Validation test sketch (contract):**

```rust
#[test]
fn task_084_acceptance_suite_now_has_31_items() {
    let harness = AcceptanceHarness::load();
    assert_eq!(harness.len(), 31, "TASK-084 is the only task allowed to bump the constant");
    for (item, task) in [(27, 78), (28, 79), (29, 80), (30, 81), (31, 82)] {
        let e = harness.get(item).expect("item registered");
        assert_eq!(e.implemented_by, format!("TASK-{:03}", task));
        assert!(e.run().is_ok(), "item {} must pass", item);
    }
}
```
