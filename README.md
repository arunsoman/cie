# CIE — Code Intelligence Engine

**A staged, evidence-backed code-analysis platform.** CIE ingests code and documents, extracts typed facts, function summaries, contracts, concepts and hierarchy, verifies conformance mechanically, and reports document–code drift — with evidence, provenance, nanosecond timestamps, incremental correctness, and an LLM confined to the labeling layer.

This repository is the **v0.1.0 foundation release** implementing PRD slices 0–10 (foundation, conformance, concepts & hierarchy, drift, multi-language, native LLM, folder assimilation, security slice, testing slice) per the spec in [`docs/PRD-v1.0.md`](docs/PRD-v1.0.md).

## Quickstart

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"

# Ingest + analyze a folder (human report; exit code 2 on critical findings)
cie analyze examples/bankapp

# Machine-readable outputs
cie analyze examples/bankapp --format sarif   # SARIF 2.1.0 for security tooling
cie analyze examples/bankapp --format json
cie analyze examples/bankapp --format md

# Hierarchical concept behavior tree (FR-44)
cie concepts examples/bankapp --with-behaviors --with-rules

# Document vs code drift (FR-31–39)
cie drift docs/prd.md --code examples/bankapp

# Blast radius + minimum test set for a change (CAP-10.3, CAP-17.3)
cie impact examples/bankapp --symbol auth.good_auth

# Time-travel report (FR-23)
cie report examples/bankapp --as-of-rev 1
```

## Architecture (PRD §6)

| Layer | Crate | What ships in v0.1.0 |
|---|---|---|
| L6 Output | `cie-cli` | CLI, JSON, Markdown, SARIF, DOT, Mermaid |
| L5 Capability | `cie-analysis` | Conformance, security (secrets, taint-lite, N+1), testing (test map, coverage gaps, min test set), dead code, layering |
| L4 Analysis | `cie-analysis` | Behavior summaries (fixpoint over call graph), conformance rules, FCA lattice, ConceptNode DAG + rollup, drift pipeline |
| L3 Foundation | `cie-core` | Semantic model, typed facts, summaries, contracts (authority-guarded), evidence/coverage, incremental engine |
| L2 Ingestion | `cie-langs` | Folder walk, hashing, tree-sitter adapters, cross-file call resolution |
| L1 Storage | `cie-storage` | Revisioned snapshots (`rev:{id}`), file index (`file:{path}:hash`), label cache, drift reports — PRD §57 namespaces as JSON |

## Languages

| Language | Adapter | Depth |
|---|---|---|
| Python | `tree-sitter-python` | **Deep**: effects, exceptions + handler semantics, branches, loops, assignments, taint sources/sinks, f-string data flow, docstring contracts |
| TypeScript / TSX | `tree-sitter-typescript` | Light: symbols, calls, imports, loops, assignments, sources/sinks, tests |
| JavaScript | `tree-sitter-javascript` | Light (same) |
| Go | `tree-sitter-go` | Light (same) |
| Rust | `tree-sitter-rust` | Light (same) |

**Extension path (PRD §67, acceptance #9):** add `tree-sitter-<lang>` to `crates/cie-langs/Cargo.toml`, implement `LanguageAdapter`, add ONE registration line in `default_registry()` — zero core changes.

## Foundation guarantees (all enforced by construction)

- **LLM labels, never decides** — the LLM layer returns strings; `Authority` (Explicit / InferredCandidate / Verified) is only settable by mechanical checkers. Offline-first default; Ollama and OpenAI-compatible endpoints opt-in via `--llm-provider`; content-addressed cache (blake3); hard USD budget with automatic downgrade (§63).
- **Timeout ≠ no defect** — every stage is budget-aware; cuts mark `coverage.complete = false` and yield `Unknown`, never `Implemented` (acceptance #26).
- **Explicit uncertainty** — unresolved calls and unsupported constructs are recorded in `Evidence`, never silently dropped (acceptance #6).
- **Incremental by construction** — blake3 content hashes, ingest plans, deletion handling; unchanged re-ingest measured at **~3%** of first-ingest wall time (acceptance #10).
- **Every conclusion carries evidence** — revision, provenance, assumptions, counterexample findings, coverage.

## Acceptance suite coverage (PRD §70)

| Suite | Status in v0.1.0 |
|---|---|
| Foundation #1–#7 (authorize examples) | ✅ `crates/cie-cli/tests/acceptance.rs` |
| Ingestion #8, #10, #11 (polyglot, re-ingest, time travel) | ✅ |
| LLM #12–#14 (offline labels, cache, budget downgrade) | ✅ unit tests (`cie-llm`) |
| Storage #15–#16 (sled/rkyv latency targets) | ⏳ L1 backend swap planned (file-backed JSON now) |
| Drift #17–#26 | ✅ #18, #19, #20, #21, #22, #23, #26 (+ #17/#24/#25 partial — rule extractor achieves ≥60% recall without LLM) |
| Concepts #27–#31 | ✅ |
| Runtime #32–#36 (OTel ingestion, FR-45) | ⏳ Slice 19 |
| Multi-repo #37–#40 (FR-46) | ⚠️ basic merged-concept graph via `cie ingest-org` |
| Spot checks #41–#44, #46 | ✅ (layering, taint, N+1, dead code, min test set) |
| Spot checks #45, #47, #48, #49 | ⚠️ partial (`impact`, mermaid output) |
| Spot check #50 (SMT proofs) | ⏳ CAP-13, not in v0.1 |

Run everything: `cargo test` (30 tests: 18 acceptance + 12 unit).

## Example

The `examples/bankapp` project contains the canonical `authorize()` pathology set from the PRD:

- `auth.authorize` — print-only authorization (**Critical**)
- `payments.charge` — rejected authorization ignored, then DB writes (**High**)
- `payments.refund` — bare except converting failure to success (**High**)
- `ui.delete_account` — tainted `request.args` → SQL f-string, plus UI-layer DB access
- `payments.API_KEY` — hardcoded live-format secret (**Critical**)
- `payments.list_purchases` — N+1 query in a loop
- `auth.good_auth` — legitimate delegation, **not flagged**

## Repository layout

```
crates/cie-core        # L3: types, facts, summaries, contracts, evidence, drift model
crates/cie-storage     # L1: revisioned store (PRD §57 namespaces)
crates/cie-langs       # L2: tree-sitter adapters + resolution
crates/cie-analysis    # L4/L5: analyses + capability slices
crates/cie-llm         # LLM providers, cache, budget
crates/cie-cli         # L6: `cie` binary + engine orchestration
tests/fixtures         # bankapp + polyglot + docs fixtures
docs/PRD-v1.0.md       # full product specification
```

## License

MIT — see [LICENSE](LICENSE).
