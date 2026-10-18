# PRD: Code Intelligence Engine (CIE) — v1.0 Complete Specification

**Status:** Final  
**Implementation language:** Rust (2021/2024 edition)  
**Sources:** `implementation-oriented.txt` + Addendum 1 (Multi-Language, Storage, Assimilation, LLM) + Addendum 2 (Drift) + Hierarchical Concept Behavior (FR-40–44) + Full Capability Inventory  
**Purpose:** A staged, evidence-backed code-analysis platform that ingests code, documents, runtime traces, and multiple repositories; extracts behavior, specifications, concepts, and hierarchy; verifies conformance; detects drift; and powers sixteen categories of downstream capability — all with evidence, provenance, nanosecond timestamps, incremental correctness, and an LLM confined to the labeling layer.

---

# Part I — Overview

## 1. Executive Summary

CIE is not a single analyzer. It is a **shared analysis foundation** plus **algorithm-specific extensions**, organized into four ingestion paths, six foundation primitives, eight core analyses, sixteen capability domains, and one output protocol.

The engine answers, with evidence and explicit uncertainty:

- What does the code actually do?
- What is it expected to do?
- Does it conform?
- What are the concepts, and how do they nest?
- What is implemented vs. missing vs. drifted vs. undocumented?
- What breaks if this changes?
- What security, performance, and recovery failures exist?
- How has the architecture evolved?

The first foundation release must be validated immediately against the `authorize()` examples.

## 2. Vision & Principles

| Principle | Meaning |
|---|---|
| **Evidence over assertion** | Every conclusion carries revision, provenance, assumptions, coverage |
| **Hypothesis ≠ truth** | Inferred rules, LLM mappings, and naming conventions are candidates, never established requirements |
| **Explicit uncertainty** | `Unverifiable` and `Unknown` are first-class; never silently dropped |
| **Timeout ≠ no defect** | Analysis completion and conclusion are separate dimensions |
| **LLM labels, never decides** | Membership, verification, and truth are mechanical; LLM is confined to labeling and explanation |
| **Incremental by construction** | Content hashes, reverse dependencies, deletion handling in every slice |
| **Nanosecond time** | Every timestamp, duration, deadline, and budget in `u64`/`i64` ns |
| **Language-agnostic core** | New language = one crate + one registry line + zero core changes |
| **Grounded LLM** | RAG over typed facts, not raw text; verifier checks every suggestion |
| **Shared vs specific** | Foundation reused; algorithms replaceable without rebuilding ingestion, evidence, or UI |

## 3. Goals

- Build a small shared analysis foundation.
- Extend through working vertical slices.
- Support ten languages from day one with easy extension.
- Provide minimal-latency storage with nanosecond time units.
- Enable folder-point assimilation with incremental re-ingestion.
- Integrate LLMs natively with caching, budgets, offline-first defaults.
- Detect document–code drift with evidence-backed statuses.
- Deliver hierarchical concept behavior as a first-class artifact.
- Enable sixteen capability domains (architecture, security, testing, performance, refactoring, docs, compliance, observability, ops, productivity, AI-native, modernization, formal assurance, business, archaeology, multi-repo) plus combinatorial capabilities.
- Add runtime ingestion (OpenTelemetry) and multi-repo ingestion as first-class paths.

## 4. Non-Goals

- Automatically extracting arbitrary business semantics from code alone.
- Treating IFDS, bi-abduction, SMT, symbolic execution, LDFI, or IronFleet as universal automatic proof techniques.
- Forcing every concept into one tree.
- Letting an LLM decide membership or verification.
- Letting a timeout become "no defect."
- Letting an inferred rule become an established requirement.
- Ground truth for vague claims ("should be fast," "user-friendly").
- Guarantees when environment, data, or runtime is not modeled.
- Certainty when code is dynamic, reflective, generated, or unresolved.
- Complete dynamic analysis without runtime evidence.
- Sound security analysis without heap modeling.
- Predicting bugs in code never executed.

## 5. Glossary

| Term | Meaning |
|---|---|
| CIE | Code Intelligence Engine |
| PDG | Program Dependence Graph |
| IFDS | Interprocedural Finite Distributive Subset |
| FCA | Formal Concept Analysis |
| CEGAR | Counterexample-Guided Abstraction Refinement |
| SMT | Satisfiability Modulo Theories |
| LDFI | Lineage-Driven Fault Injection |
| Safety | Fulfillment requires established authorization |
| Liveness | Every debit eventually reaches fulfillment or effective refund |
| Evidence | Revision, provenance, assumptions, unsupported constructs, unresolved calls |
| Coverage | What was actually analyzed |
| Drift | Mismatch between document claims and code reality |
| Ns | Nanosecond, base time unit (`u64` since epoch) |
| ConceptNode | Unified node binding concept + hierarchy + behavior + rules |
| Claim | Atomic typed assertion extracted from a document |

---

# Part II — System Architecture

## 6. Layered Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│ L6  Output Layer                                                │
│     CLI · JSON · Markdown · DOT · SARIF · WebSocket · HTTP      │
├─────────────────────────────────────────────────────────────────┤
│ L5  Capability Layer                                            │
│     16 capability domains (architecture, security, testing, …)  │
│     Combinatorial capabilities                                  │
├─────────────────────────────────────────────────────────────────┤
│ L4  Analysis Layer                                              │
│     Behavior · Conformance · Rules/Counterexamples · Concepts   │
│     Hierarchy · Hierarchical Concept Behavior · Recovery        │
│     Drift                                                       │
├─────────────────────────────────────────────────────────────────┤
│ L3  Foundation Layer                                            │
│     Semantic model · Relations · Summaries · Contracts          │
│     Evidence/Coverage · Incremental engine                      │
├─────────────────────────────────────────────────────────────────┤
│ L2  Ingestion Layer                                             │
│     Code · Documents · Runtime (OTel) · Multi-Repo              │
├─────────────────────────────────────────────────────────────────┤
│ L1  Storage Layer                                               │
│     Hot (DashMap/CSR) · Warm (sled/redb) · Cold (rkyv/parquet)  │
│     HNSW vectors · Time-indexed tables · Revision chain         │
└─────────────────────────────────────────────────────────────────┘
```

**Boundary:** Shared foundation (L1–L3) is reused by every capability. Algorithm-specific representation lives in L4. Adding a capability in L5 must not require changes in L3.

## 7. Ingestion Layer

Four ingestion paths. All four are incremental, revisioned, and emit evidence.

| Path | Input | Produces | New in |
|---|---|---|---|
| Code | Folder path | Symbols, calls, CFG, DFG, effects, exceptions, async | FR-19 |
| Documents | Files (MD, PDF, DOCX, …) | Claims, mappings | FR-31 |
| Runtime | OTel traces, logs, metrics | Execution evidence, observed behavior | FR-45 |
| Multi-Repo | Multiple folder paths | Merged concept graph | FR-46 |

## 8. Foundation Layer

Ten shared components (unchanged from the original PRD, extended by two new artifacts).

| Component | Provides |
|---|---|
| Semantic program model | Stable IDs for symbols, operations, values, resources, locations |
| Typed relations | Calls, CFG, DFG, reads/writes, exceptions, ownership |
| Language semantics adapters | Numeric behavior, dispatch, exceptions, async, framework conventions |
| Evidence and coverage | Revision, provenance, assumptions, unsupported, unresolved |
| Summary store | Versioned function effects and dependencies |
| Contract registry | Explicit obligations, inferred candidates, authority |
| Analysis execution | Scheduling, worklists, cancellation, budgets, checkpoints |
| Incremental maintenance | Content hashes, reverse deps, invalidation, deletion |
| Result protocol | Findings, concepts, counterexamples, status |
| Naming adapter | Labels immutable analysis records |

**New artifacts:**
- **ConceptNode DAG** (FR-40–44) — unified hierarchy with behavior rollup.
- **Runtime evidence store** (FR-45) — OTel traces, logs, metrics keyed to summaries.

## 9. Analysis Layer

Eight core analyses.

| Analysis | Produces |
|---|---|
| Behavior extraction | Function summaries: inputs, outputs, effects, exceptions, path conditions |
| Behavioral conformance | Established vs inferred conformance; contract registry checking |
| Rules & counterexamples | Predicates, bounded checks, counterexample traces |
| Concepts (FCA) | Extent/intent pairs; overlapping concepts |
| Hierarchy | Structural clusters merged with FCA and package structure |
| Hierarchical concept behavior | ConceptNode DAG with behaviors, rules, rollup |
| Recovery & fault injection | State models, LDFI plans, recovery failures |
| Drift | Per-claim status: implemented, missing, partial, contradicting, drifted, undocumented, unverifiable, unknown |

## 10. Capability Layer

Sixteen capability domains, each built on L4 primitives. See Part VI.

## 11. Output Layer

| Format | Consumers |
|---|---|
| CLI (human) | Engineers |
| JSON | CI, dashboards |
| Markdown | Docs, PRs |
| DOT / Mermaid | Diagrams |
| SARIF | Security tooling |
| WebSocket / HTTP | IDE, web UI |
| Parquet | Analytics |

---

# Part III — Ingestion Paths

## 12. Code Ingestion (FR-19, FR-20)

```bash
cie ingest <path>                        # full assimilation
cie ingest <path> --watch                # continuous incremental
cie ingest <path> --langs py,rs,go       # restrict languages
cie ingest <path> --budget-ms 60000      # hard wall-clock budget
cie ingest <path> --no-llm               # skip LLM labeling
cie ingest <path> --parallel 16          # worker threads
```

**Pipeline:**

```text
folder path
  → walk (ignore .git, node_modules, target, vendor, .venv, dist, build)
  → detect languages per file (extension + shebang + tree-sitter probe)
  → parse each file in parallel (rayon)
  → emit typed facts (symbols, calls, CFG, DFG, effects, exceptions, async)
  → build cross-file relations (resolve calls, imports, ownership)
  → compute function summaries (bottom-up over call graph)
  → populate contract registry (explicit + inferred candidates)
  → run FCA + clustering → merge into ConceptNode DAG
  → bind behaviors and rules to ConceptNodes; rollup to ancestors
  → run LLM naming adapter (if enabled)
  → persist with revision hash
  → emit coverage + evidence + conclusions
```

**Incremental re-ingest:** content hash diff → invalidate changed + transitively affected → re-run affected slices → emit delta.

**Acceptance:** Re-ingest of unchanged folder ≤ 5% of first-ingest wall time.

## 13. Document Ingestion (FR-31)

**Formats:** Markdown, plain text, PDF, DOCX, HTML, OpenAPI (YAML/JSON), Protobuf, GraphQL SDL, Confluence export, Notion export, Jira/Linear export, Git commit messages, PR descriptions.

**Preserves:** `DocSourceLocation { doc, page, section, paragraph, char_start, char_end }`.

**Pipeline:** parse → extract claims (rule + LLM) → embed → propose mappings → verify (mechanical) → reconcile multi-doc → emit drift report.

## 14. Runtime Ingestion (FR-45) — *new*

**Inputs:** OpenTelemetry traces, logs, metrics; APM exports (Jaeger, Tempo, Datadog, New Relic); log files; metric snapshots.

**Pipeline:**

```text
OTel / logs / metrics
  → parse spans (trace_id, span_id, parent, service, operation, duration, status)
  → map service/operation to SymbolId (via summary names + framework conventions)
  → emit ExecutionEvidence { trace_id, span_id, symbol, path, duration_ns, status, attrs }
  → aggregate: hit counts, observed effects, observed exceptions, observed paths
  → join with static summaries → observed-vs-static delta
  → persist to runtime evidence store, indexed by symbol and by ns
```

**Outputs:**
- Observed behavior per summary.
- Runtime drift: static summary differs from observed.
- Hot paths (frequency-weighted call graph).
- Flaky operations (status varies across traces).
- PII flow evidence (field names in attributes).

**Enables:** §37 (Observability), §38 (Ops), runtime half of §42 (Formal Assurance), incident replay.

**Acceptance:** A span can be mapped to a `SymbolId` or explicitly marked `Unresolved` with reason.

## 15. Multi-Repo Ingestion (FR-46) — *new*

**Input:** Multiple folder paths + optional org manifest.

```bash
cie ingest-org <manifest.yaml>           # multi-repo
cie ingest-org <path1> <path2> <path3>   # ad-hoc multi-repo
```

**Pipeline:**

```text
multiple repos
  → ingest each repo independently (Code Ingestion)
  → resolve cross-repo references (imports, package names, API specs)
  → merge SymbolIds via package + signature + name + semantic similarity
  → merge ConceptNode DAGs across repos
  → detect shared subgraphs (extraction candidates)
  → detect API compatibility across services
  → emit org-wide concept graph + compatibility matrix
```

**Enables:** §45 (Multi-Repo & Ecosystem), org-wide concept graph, internal library usage, API matrix.

**Acceptance:** Two repos with the same library produce one merged concept with two sources; a breaking API change appears as `Contradicting` in the compatibility matrix.

---

# Part IV — Foundation Primitives

## 16. Semantic Model (FR-1)

Stable identities for symbols, operations, values, resources, source locations. Every algorithm references the same `SymbolId`, `OperationId`, `ValueId`, `ResourceId`, `SourceLocation`.

## 17. Typed Relations (FR-2)

Calls, control flow, data flow, reads/writes, exceptions, dependency bindings, ownership. CSR representation for hot scans; `petgraph::StableGraph` for algorithmic work.

## 18. Summaries (FR-5)

`FunctionSummary { inputs, outputs, reads, writes, effects, exceptions, path_conditions, unresolved, rev, compute_ns }`. Bottom-up over call graph. Versioned. Invalidatable.

## 19. Contracts (FR-6)

`Contract { id, obligation, expr, authority, evidence }`. Authority ∈ {Explicit, InferredCandidate, Verified}. Expected behavior is kept separate from observed behavior.

## 20. Evidence & Coverage (FR-4)

`Evidence { rev, provenance, assumptions, unsupported, unresolved, generated_at_ns }`. Every result carries it. Timeouts and unsupported constructs are explicit.

## 21. Incremental Engine (FR-8)

Content hashes (`blake3`), reverse dependencies, invalidation, deletion handling. Differential dataflow for iteration. Time-travel via `TimedRecord<T>`.

---

# Part V — Core Analyses

## 22. Behavior Extraction (FR-11)

Extract function effects, paths, and interprocedural summaries. Detect print-only stubs, unconditional success, ignored failure, delegating wrappers.

**Papers:** PDG (Ferrante 1987), IFDS (Reps 1995), Abstract Interpretation (Cousot 1977), Bi-Abduction (Calcagno 2009).

## 23. Behavioral Conformance (FR-12)

Contract registry, candidate-rule inference, checking. Distinguish established violations from inferred suspicions. Detect fake authorization, ignored rejection, swallowed exceptions.

**Papers:** Naming (Høst 2009), Beliefs (Engler 2001, Kremenek 2006), Protocols (Ammons 2002), Houdini (Flanagan 2001).

## 24. Rules & Counterexamples (FR-13, FR-34)

Predicate extraction, bounded checking, counterexample traces. Per-claim checkers (existence, behavior match, SMT, structural, protocol, path search, runtime).

**Papers:** CEGAR (Clarke 2000), SLAM (Ball & Rajamani 2002), Z3 (de Moura 2008), KLEE (Cadar 2008).

## 25. Concepts — FCA (FR-13, §14.6)

Formal contexts (objects × attributes). Concepts as extent/intent pairs. Overlapping, not forced into a tree.

**Papers:** FCA (Wille 2008), Incremental FCA (Godin 1995).

## 26. Hierarchy (FR-14)

Structural clusters merged with FCA subsumption, package structure, call ownership. DAG, not just tree. Every edge traceable.

**Papers:** Peixoto 2014, 2015; Andritsos & Tzerpos 2003.

## 27. Hierarchical Concept Behavior (FR-40–44) — *new*

### FR-40: ConceptNode

Unified node binding concept + hierarchy + behaviors + rules + symbols + evidence.

```rust
pub struct ConceptNode {
    pub id: ConceptNodeId,
    pub label: Label,
    pub kind: ConceptKind,   // Entity | Value | Aggregate | Service | Module | Domain | Subdomain | UseCase | Rule | Invariant
    pub concept: Option<ConceptId>,
    pub cluster: Option<ClusterId>,
    pub parent: Option<ConceptNodeId>,
    pub children: SmallVec<[ConceptNodeId; 8]>,
    pub behaviors: SmallVec<[BehaviorRef; 8]>,
    pub rules: SmallVec<[RuleRef; 8]>,
    pub symbols: SmallVec<[SymbolId; 16]>,
    pub evidence: EvidenceRef,
    pub coverage: Coverage,
    pub created_ns: Ns,
    pub compute_ns: NsDelta,
}
```

### FR-41: Concept–Behavior Binding

Attach behaviors and rules via FCA co-occurrence, call reachability, contract join, LLM proposals (candidate only).

**Acceptance:** Every behavior and rule attaches to ≥1 node or is listed as unattached.

### FR-42: Hierarchy Construction

Merge FCA lattice + structural clusters + packages + call ownership. DAG. Every parent–child edge traceable.

**Acceptance:** Every edge has a reason (FCA subsumption, cluster cohesion, package containment, or call ownership).

### FR-43: Behavior Rollup

Per node: union of descendants' behaviors, rules, effects; intersection of coverage; weakest-link confidence.

**Acceptance:** Rollup matches clean recomputation.

### FR-44: Output

```bash
cie concepts <path> --format md --with-behaviors --with-rules
```

Example output matches the original user-facing artifact: a nested tree where each node carries concepts, behaviors, rules, symbols, evidence.

### Hierarchical Concept Behavior rollup example

```markdown
# Hierarchical Concept Behavior — rev 42

## Cart (Aggregate)
- Concept: extent {Cart, CartItem}, intent {mutable, per-user, has-items}
- Symbols: cart.py:Cart, cart.py:CartItem
- Behaviors:
  - Add item to cart — summary cart.py:Cart.add_item
  - Calculate cart total — summary cart.py:Cart.total
- Rules:
  - Quantity must be > 0 (Invariant, Explicit) — C-001
- Children:
  - CartItem (Entity)
  - Quantity (Value)
    - Rules: Quantity > 0 (Invariant, Explicit)
```

**Acceptance:** Output matches this shape.

## 28. Recovery & Fault Injection (FR-15)

State models, LDFI plans, recovery failures. Distinguish Safety (fulfillment requires authorization) from Liveness (every debit reaches fulfillment or effective refund). A refund method being called does not establish money returned.

**Papers:** Yuan 2014, LDFI (Alvaro 2015), IronFleet (Hawblitzel 2015).

## 29. Drift Analysis (FR-31–39)

### FR-32: Claim Extraction

Types: Capability, Behavior, Constraint, Interface, DataModel, NonFunctional, Sequence, Prohibition. Modality: shall, must, should, may, will.

### FR-33: Claim–Code Mapping

Embedding + name/path + behavioral-signature + LLM-proposed (never authoritative).

### FR-35: Drift Statuses

```rust
pub enum DriftStatus {
    Implemented, Partial, Missing, Contradicting,
    Drifted, Undocumented, Unverifiable, Unknown,
}
```

**Rules:** Implemented requires evidence; timeout ≠ Implemented; inferred mapping ≠ Implemented without verification; Unverifiable explicit.

### FR-36: Reverse Drift

Code with no matching claim → `Undocumented`.

### FR-37: Drift Report

Machine + human readable. Per claim: status, mapping, evidence, counterexamples, coverage. Per subsystem: aggregate. Reverse drift list. Deltas across revisions.

### FR-38: Multi-Document Reconciliation

Same claim same status → merge. Same claim different status → `Contradicting` with both cited. Superseded → `Superseded`.

### FR-39: LLM Role

Extracts candidate claims and proposes mappings. Never sets Implemented/Contradicting/Missing. Disabling LLM still yields ≥60% claims.

### Acceptance tests

17–26 in §70.

---

# Part VI — Capability Layer

Each capability is a vertical slice in L5 built on L4 primitives. **Status** indicates implementation phase:
- **Core** — v1.0, no new ingestion path
- **Runtime** — needs FR-45 (OTel ingestion)
- **Multi-Repo** — needs FR-46

## 30. Architecture & Design (CAP-1)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-1.1 | Layering violation detection | ConceptNode kind + call edges | Core |
| CAP-1.2 | Cyclic dependency detection | CSR graph + SCC | Core |
| CAP-1.3 | Coupling/cohesion metrics | CSR + module structure | Core |
| CAP-1.4 | Dead code detection | No reachable callers + no claims | Core |
| CAP-1.5 | Orphaned code (no caller, no claim, no test) | Reverse deps + claims + test map | Core |
| CAP-1.6 | God object / large class detection | Fan-out + summary size | Core |
| CAP-1.7 | Architectural drift over time | ConceptNode DAG diff across revs | Core |
| CAP-1.8 | Module boundary erosion | Package structure vs call edges | Core |
| CAP-1.9 | Dependency direction enforcement | Contracts over call edges | Core |
| CAP-1.10 | Semantic diff of architecture | ConceptNode DAG at rev N vs N-1 | Core |
| CAP-1.11 | Feature envy / shotgun surgery | Change history + call graph | Runtime + VCS |

## 31. Security (CAP-2)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-2.1 | Taint tracking (source → sink) | DFG + IFDS | Core |
| CAP-2.2 | Injection (SQL, command, path, XSS) | Taint + effects + literals | Core |
| CAP-2.3 | Hardcoded secrets | Summaries + literal scan | Core |
| CAP-2.4 | Insecure deserialization | Effects + call resolution | Core |
| CAP-2.5 | AuthZ bypass | Contracts (safety: fulfillment requires authorization) | Core |
| CAP-2.6 | Crypto misuse | Call resolution + adapters | Core |
| CAP-2.7 | SSRF / XXE | Taint + framework conventions | Core |
| CAP-2.8 | Privilege escalation paths | Call graph + authZ contracts | Core |
| CAP-2.9 | Information flow (non-interference) | IFDS + abstract domains | Core |
| CAP-2.10 | SBOM generation | Unresolved symbols + manifests | Core |
| CAP-2.11 | SARIF export | Result protocol | Core |

## 32. Testing & Quality (CAP-3)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-3.1 | Coverage gap analysis | Summaries + test map | Core |
| CAP-3.2 | Mutation testing targeting | Summaries + risk scores | Core |
| CAP-3.3 | Counterexample → test generation | KLEE-style | Core |
| CAP-3.4 | Property-based tests from contracts | Contracts → generators | Core |
| CAP-3.5 | Test-to-code mapping | Call graph from tests | Core |
| CAP-3.6 | Regression risk scoring | Affected summaries × coverage × contracts | Core |
| CAP-3.7 | Test oracles from specs | Contracts + inferred rules | Core |
| CAP-3.8 | Minimum test set for a change | Reverse deps + test map | Core |
| CAP-3.9 | Flaky test detection | Execution evidence | Runtime |

## 33. Performance (CAP-4)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-4.1 | Complexity estimation per summary | CFG loop depth + call depth | Core |
| CAP-4.2 | N+1 query detection | Effects repeated inside loops | Core |
| CAP-4.3 | Unbounded loop / missing pagination | CFG + bounds (abstract interp) | Core |
| CAP-4.4 | Memory leak candidates | Effects (alloc without free) | Core |
| CAP-4.5 | Blocking calls in async contexts | Async facts + resolution | Core |
| CAP-4.6 | Lock contention candidates | Shared resource writes in concurrent paths | Core |
| CAP-4.7 | Hot path identification | Call frequency | Runtime |
| CAP-4.8 | Cache miss analysis | Effects + call graph | Core |

## 34. Refactoring & Migration (CAP-5)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-5.1 | Safe rename | Typed relations + stable IDs | Core |
| CAP-5.2 | Extract function suggestions | Cohesive subgraphs (FCA/clustering) | Core |
| CAP-5.3 | Move class to module | Coupling metrics | Core |
| CAP-5.4 | Dead parameter / unused import | DFG + summaries | Core |
| CAP-5.5 | Deprecation impact analysis | Callers of deprecated symbols | Core |
| CAP-5.6 | API surface diff between revs | Signatures at rev N vs N-1 | Core |
| CAP-5.7 | Breaking change detection | Signature + behavior + effects diff | Core |
| CAP-5.8 | Framework migration (Flask → FastAPI) | Framework conventions + ConceptNode | Core |
| CAP-5.9 | Cross-language migration | ConceptNode shared across adapters | Core |
| CAP-5.10 | Automated codemods with evidence | Every change traced to a finding | Core |
| CAP-5.11 | Strangler-fig extraction plan | Coupling × ConceptNode kind | Core |
| CAP-5.12 | Golden-master testing | Summaries at old vs new rev | Core |
| CAP-5.13 | Behavior preservation verification | Contracts at old vs new | Core |

## 35. Documentation (CAP-6)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-6.1 | Auto-generated architecture docs | ConceptNode DAG → Markdown/Mermaid | Core |
| CAP-6.2 | Auto-generated API docs | Summaries + contracts | Core |
| CAP-6.3 | Data flow diagrams | Typed relations → diagrams | Core |
| CAP-6.4 | Sequence diagrams | Call facts → diagrams | Core |
| CAP-6.5 | Onboarding guide | Top concepts by centrality + dependency order | Core |
| CAP-6.6 | Glossary | Concept labels + definitions | Core |
| CAP-6.7 | Runbook generation | Contracts + recovery facts | Core |
| CAP-6.8 | Inline comment drift | Comment vs summary | Core |
| CAP-6.9 | ADR suggestions | Change history + concept diffs | Runtime + VCS |
| CAP-6.10 | "Explain this code" | LLM narration grounded in summaries | Core |

## 36. Compliance & Governance (CAP-7)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-7.1 | License scanning | Imports + manifests | Core |
| CAP-7.2 | PII handling detection | Effects + field names + claims | Core |
| CAP-7.3 | Data residency / PII flow | Taint + effects | Core |
| CAP-7.4 | GDPR/CCPA data subject paths | PII facts + call graph | Core |
| CAP-7.5 | Policy enforcement (no DB from controllers) | Contracts + layering | Core |
| CAP-7.6 | Audit trail | Rev + evidence + VCS | Core |
| CAP-7.7 | SBOM | Unresolved symbols + manifests | Core |
| CAP-7.8 | Schema-vs-impl drift | OpenAPI/Protobuf/GraphQL vs summaries | Core |
| CAP-7.9 | Continuous compliance | Drift on every commit | Core |

## 37. Runtime & Observability (CAP-8)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-8.1 | Trace-to-code mapping | OTel → summaries | Runtime |
| CAP-8.2 | Log-to-code mapping | Log lines → emitters | Runtime |
| CAP-8.3 | Metric-to-code mapping | Metric names → emitters | Runtime |
| CAP-8.4 | Error-to-code mapping | Stack traces → summaries + contracts | Runtime |
| CAP-8.5 | Incident root-cause candidates | Trace + contract violations | Runtime |
| CAP-8.6 | SLO impact analysis | Claims + concept → SLO | Core + Runtime |
| CAP-8.7 | Runtime behavior drift | Observed vs static summary | Runtime |
| CAP-8.8 | Shadow mode comparison | Two impls, same contracts | Runtime |

## 38. DevOps & Operations (CAP-9)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-9.1 | Deployment impact analysis | ConceptNode diff between revs | Core |
| CAP-9.2 | Rollback safety | Contracts that would break | Core |
| CAP-9.3 | Feature flag impact | Gated code paths | Core |
| CAP-9.4 | Config dependency mapping | Reads of env vars / secrets | Core |
| CAP-9.5 | Secret rotation impact | Readers of a secret | Core |
| CAP-9.6 | Service dependency graph | External calls + unresolved symbols | Core |
| CAP-9.7 | Capacity planning inputs | Complexity × call frequency | Runtime |
| CAP-9.8 | IaC drift | Terraform vs code expectations | Runtime |

## 39. Developer Productivity (CAP-10)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-10.1 | "Who owns this?" | VCS blame + summaries | Runtime + VCS |
| CAP-10.2 | "What breaks if I change this?" | Reverse deps + contracts | Core |
| CAP-10.3 | Blast radius of a PR | Symbol → summary → concept → claim | Core |
| CAP-10.4 | PR risk scoring | Change size × contracts × coverage | Core |
| CAP-10.5 | Semantic code search | HNSW over embeddings | Core |
| CAP-10.6 | Natural-language query | LLM + grounded retrieval | Core |
| CAP-10.7 | Code review bot (grounded) | Drift + contracts + coverage | Core |
| CAP-10.8 | "Explain this error" | Stack trace → summaries → LLM | Runtime |
| CAP-10.9 | Ticket-to-code mapping | Jira claim → ConceptNode | Core |

## 40. AI-Native Layer (CAP-11)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-11.1 | NL query over codebase | Embeddings + ConceptNode + LLM | Core |
| CAP-11.2 | NL behavioral diff between revs | ConceptNode diff + LLM narration | Core |
| CAP-11.3 | Grounded code review | Evidence-bound LLM | Core |
| CAP-11.4 | Conversational architecture walk | ConceptNode DAG traversal + LLM | Core |
| CAP-11.5 | Grounded code generation | Generate → verify against contracts | Core |
| CAP-11.6 | Auto PR description | Change impact + ConceptNode | Core |
| CAP-11.7 | RAG grounded in typed facts | Facts store as retrieval corpus | Core |

## 41. Reverse Engineering & Modernization (CAP-12)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-12.1 | Bounded context inference | FCA + clustering + coupling | Core |
| CAP-12.2 | Monolith → microservice candidates | ConceptNode kind + coupling | Core |
| CAP-12.3 | Shared library extraction | Repeated subgraphs across services | Multi-Repo |
| CAP-12.4 | Strangler-fig planning | Coupling × risk | Core |
| CAP-12.5 | Behavior-preserving rewrite | Contracts at old vs new | Core |
| CAP-12.6 | Data migration planning | Summaries → tables written | Core |
| CAP-12.7 | Legacy decomposition roadmap | ConceptNode centrality + coupling | Core |

## 42. Formal Assurance (CAP-13)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-13.1 | Contract → proof pipeline | Contracts → SMT → proofs | Core |
| CAP-13.2 | Invariant discovery | Abstract interpretation | Core |
| CAP-13.3 | Refinement checking | Spec vs impl, same contracts | Core |
| CAP-13.4 | Termination analysis | Ranking functions | Core |
| CAP-13.5 | Concurrency bug detection | Lock order + data races | Core |
| CAP-13.6 | Memory safety (unsafe Rust, C, C++) | Bi-abduction + shape analysis | Core |
| CAP-13.7 | Equivalence of two implementations | Same contracts, different code | Core |

## 43. Business & Product (CAP-14)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-14.1 | Feature inventory | ConceptNode + claims | Core |
| CAP-14.2 | Reachability from entry points | Call graph | Core |
| CAP-14.3 | Tier/packaging analysis | Config reads + ConceptNode | Core |
| CAP-14.4 | Customer customization detection | Config reads | Core |
| CAP-14.5 | IP boundary analysis | Multi-product ConceptNode overlap | Multi-Repo |

## 44. Cross-Cutting / Archaeology (CAP-15)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-15.1 | "When did this behavior change?" | Summary diff across revs | Core |
| CAP-15.2 | "Who introduced this violation?" | Drift + VCS blame | Runtime + VCS |
| CAP-15.3 | "What did this do 6 months ago?" | Summary at rev N | Core |
| CAP-15.4 | Regression attribution | Which rev broke which claim | Core |
| CAP-15.5 | Coupling evolution over time | Graph metrics across revs | Core |
| CAP-15.6 | Concept birth/death tracking | ConceptNode DAG diff | Core |

## 45. Multi-Repo & Ecosystem (CAP-16)

| ID | Capability | Primitives | Status |
|---|---|---|---|
| CAP-16.1 | Cross-repo concept linking | Shared ConceptNode labels | Multi-Repo |
| CAP-16.2 | Org-wide concept catalog | Merged ConceptNode DAG | Multi-Repo |
| CAP-16.3 | API compatibility matrix | Summaries across services | Multi-Repo |
| CAP-16.4 | Dependency update impact | Unresolved symbols + reverse deps | Core |
| CAP-16.5 | Internal library usage tracking | Imports + ConceptNode | Multi-Repo |

## 46. Combinatorial Capabilities (CAP-17)

Emerge from combining primitives.

| ID | Capability | Combination | Status |
|---|---|---|---|
| CAP-17.1 | Semantic architecture diff | ConceptNode DAG + time travel + evidence | Core |
| CAP-17.2 | Blast radius for any change | Reverse deps + ConceptNode + claims | Core |
| CAP-17.3 | Minimum test set | Changed symbols + reverse deps + test map | Core |
| CAP-17.4 | Risk-ordered refactoring backlog | Coupling × coverage gap × contract violations | Core |
| CAP-17.5 | Concept-based access control | ConceptNode + ownership | Core |
| CAP-17.6 | Onboarding curriculum | Centrality + topological order | Core |
| CAP-17.7 | Incident replay | Trace + ConceptNode + contracts | Runtime |
| CAP-17.8 | What-if analysis | "Remove module X → which contracts break?" | Core |
| CAP-17.9 | Grounded AI pair programmer | LLM + contracts + checker | Core |
| CAP-17.10 | Self-documenting codebase | ConceptNode + contracts → docs | Core |
| CAP-17.11 | Continuous compliance | Drift on every commit | Core |
| CAP-17.12 | Org-wide concept graph | Multi-repo ConceptNode merge | Multi-Repo |

---

# Part VII — Data Model

## 47. Core Types

```rust
pub type Ns = u64;                    // ns since epoch
pub type NsDelta = i64;               // signed ns
pub type RevId = u64;                 // monotonic
pub type Hash = [u8; 32];             // blake3

pub struct SymbolId(pub u32);
pub struct OperationId(pub u32);
pub struct ValueId(pub u32);
pub struct ResourceId(pub u32);
pub struct SourceLocation { pub file: FileId, pub start: u32, pub end: u32 }

pub struct Interner<T> { map: FxHashMap<Box<str>, u32>, vec: Vec<T> }
```

## 48. Fact Types

```rust
pub struct SymbolFact { id, kind, name, loc, lang, rev }
pub struct CallFact { caller, callee, kind, loc, unresolved }
pub struct EffectFact { op, reads, writes, external }
pub struct ExceptionFact { op, kind, handler }
pub struct AsyncFact { op, kind }
pub struct ExecutionEvidence { trace_id, span_id, symbol, path, duration_ns, status, attrs }
```

## 49. Relations

```rust
pub enum RelKind { Calls, Returns, ControlFlow, DataFlow, Reads, Writes, Throws, Catches, Owns, DependsOn }
pub type RelationGraph = StableGraph<SymbolId, RelKind, Directed, u32>;
pub struct CsrGraph { pub offsets: Vec<u32>, pub edges: Vec<(u32, RelKind)> }
```

## 50. Summary

```rust
pub struct FunctionSummary {
    sym, inputs, outputs, reads, writes, effects, exceptions,
    path_conditions, unresolved, rev, compute_ns,
}
```

## 51. Contract

```rust
pub struct Contract { id, obligation, expr, authority, evidence }
```

## 52. Concepts & Hierarchy

```rust
pub struct FormalContext { objects, attributes, incidences: BitMatrix }
pub struct Concept { extent: BitSet, intent: BitSet, parent, children }
pub struct HierarchyNode { id, label, children, evidence }

pub struct ConceptNode {
    id, label, kind, concept, cluster, parent, children,
    behaviors, rules, symbols, evidence, coverage, created_ns, compute_ns,
}
pub struct BehaviorRef { summary, kind, confidence }
pub struct RuleRef { contract, kind, authority }
```

## 53. Evidence

```rust
pub struct Evidence {
    rev, provenance, assumptions, unsupported, unresolved, generated_at_ns,
}
```

## 54. Drift

```rust
pub struct DocSourceLocation { doc, page, section, paragraph, char_start, char_end }
pub struct Claim { id, doc, text, kind, modality, loc, confidence, extractor, created_ns }
pub struct ClaimMapping { claim, symbols, concepts, nodes, confidence, proposer, evidence }
pub struct ClaimVerification {
    claim, status, checker, evidence, counterexamples, assumptions,
    coverage, verified_ns, compute_ns,
}
pub struct DriftReport {
    rev, docs, claims, mappings, verifications, reverse_drift,
    aggregate, generated_ns,
}
pub enum DriftStatus {
    Implemented, Partial, Missing, Contradicting,
    Drifted, Undocumented, Unverifiable, Unknown,
}
```

## 55. Runtime & Multi-Repo Types

```rust
pub struct RuntimeEvidence {
    pub symbol: SymbolId,
    pub trace_id: Hash,
    pub span_id: Hash,
    pub duration_ns: NsDelta,
    pub status: SpanStatus,
    pub attrs: SmallVec<[(InternedStr, InternedStr); 8]>,
    pub observed_ns: Ns,
}

pub struct RepoId(pub u16);
pub struct MergedConcept {
    pub merged_id: ConceptNodeId,
    pub sources: SmallVec<[(RepoId, ConceptNodeId); 8]>,
    pub confidence: f32,
}
```

---

# Part VIII — Storage & Performance

## 56. Storage Layers

| Layer | Structure | Purpose | Latency target |
|---|---|---|---|
| L0 — Hot facts | `DashMap<SymbolId, SymbolFact>` | in-process fact cache | < 50 ns |
| L0 — Relations | `petgraph::Csr` | CSR adjacency | < 200 ns edge scan |
| L0 — Index | `FxHashMap<Hash, SymbolId>` | content-hash → id | < 30 ns |
| L0 — LRU | `quick_cache::sync::Cache` | bounded hot set | < 100 ns |
| L1 — Summaries | `sled::Tree` or `redb` | versioned summaries | < 5 µs get |
| L2 — Snapshots | `memmap2` + `rkyv` | revisioned graph | < 10 µs cold read |
| L3 — Cold | `zstd` + `parquet` | historical | < 100 ms scan |
| Vector | `hnsw_rs` or `usearch` | embeddings | < 1 ms k-NN |
| Runtime | `sled::Tree` keyed by symbol + ns | OTel evidence | < 10 µs get |

## 57. Key Namespaces

```text
rev:{rev_id}:meta                 -> RevMeta
file:{file_id}:hash               -> Hash
file:{file_id}:lang               -> LanguageId
sym:{symbol_id}                   -> SymbolFact
sym_by_hash:{hash}                -> SymbolId
rel:csr:{rev_id}:{sym_id}         -> CsrGraph
sum:{rev_id}:{sym_id}             -> FunctionSummary
contract:{contract_id}            -> Contract
concept:{concept_id}              -> Concept
hier:{node_id}                    -> HierarchyNode
concept_node:{node_id}            -> ConceptNode
concept_node_parents:{node_id}    -> SmallVec<[ConceptNodeId; 4]>
concept_node_children:{node_id}   -> SmallVec<[ConceptNodeId; 8]>
concept_node_behaviors:{node_id}  -> SmallVec<[BehaviorRef; 8]>
concept_node_rules:{node_id}      -> SmallVec<[RuleRef; 8]>
concept_node_symbols:{node_id}    -> SmallVec<[SymbolId; 16]>
label:{record_id}                 -> Label
vec:{record_id}                   -> Vec<f32>
evidence:{record_id}              -> Evidence
record:{record_id}                -> DerivedRecord
deps:{record_id}                  -> SmallVec<[RecordId; 8]>
rev_deps:{symbol_id}              -> SmallVec<[RecordId; 8]>
doc:{doc_id}:meta                 -> DocMeta
doc:{doc_id}:hash                 -> Hash
claim:{claim_id}                  -> Claim
claim_by_doc:{doc_id}             -> SmallVec<[ClaimId; 16]>
map:{claim_id}                    -> SmallVec<[ClaimMapping; 4]>
verify:{claim_id}:{rev_id}        -> ClaimVerification
drift:{rev_id}:{doc_id}           -> DriftReport
drift_delta:{from_rev}:{to_rev}   -> DriftDelta
reverse:{rev_id}                  -> SmallVec<[SymbolId; 32]>
runtime:{symbol_id}:{ns}          -> RuntimeEvidence
runtime_by_trace:{trace_id}       -> SmallVec<[RuntimeEvidence; 32]>
repo:{repo_id}:meta               -> RepoMeta
merged_concept:{merged_id}        -> MergedConcept
api_matrix:{from_repo}:{to_repo}  -> CompatibilityMatrix
```

## 58. Time-Indexed Tables

```rust
pub struct TimedRecord<T> {
    pub value: T,
    pub valid_from_ns: Ns,
    pub valid_to_ns: Option<Ns>,
    pub rev: RevId,
}
```

Enables `--as-of-rev` and `--as-of-ns`.

## 59. Latency Targets

| Operation | Target |
|---|---|
| Symbol lookup | < 50 ns |
| Hash → SymbolId | < 30 ns |
| CSR edge scan | < 20 ns |
| Summary get (warm) | < 500 ns |
| Summary get (cold L1) | < 5 µs |
| Snapshot read (L2) | < 10 µs |
| FCA closure step | < 2 µs |
| HNSW k-NN (k=10, 1M) | < 1 ms |
| Incremental invalidation per file | < 200 µs |
| LLM label cache hit | < 100 ns |
| Runtime evidence get | < 10 µs |
| ConceptNode get | < 1 µs |

## 60. Memory Layout Rules

- Hot structs `#[repr(C)]`, cache-line aligned where scanned.
- `SmallVec` for bounded collections.
- Strings interned once; references via `SymbolId` / `InternedStr`.
- `fixedbitset` / `bitvec` for FCA extents/intents.
- Zero-copy via `rkyv` for snapshots.
- All timestamps `u64` ns; no allocation on the timing path.

---

# Part IX — LLM Integration

## 61. Provider Abstraction

```rust
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    fn stream(&self, req: CompletionRequest) -> Result<CompletionStream>;
    fn max_context(&self) -> usize;
    fn cost_per_1k(&self) -> (f64, f64);
}
```

**Providers (v1):** OpenAI, Anthropic, Google, Ollama, llama.cpp, Azure OpenAI, AWS Bedrock, any OpenAI-compatible endpoint.

## 62. Integration Points

| Point | Purpose | Bound |
|---|---|---|
| Symbol naming | Labels for symbols, modules, concepts | Never decides membership |
| Concept naming | Labels for FCA concepts and ConceptNodes | Never decides membership |
| Finding explanation | NL explanation of DerivedRecord | Grounded in evidence |
| Contract suggestion | Candidate obligations | Never sets Explicit or Verified |
| Counterexample narration | Prose explanation | Grounded in trace |
| NL query | Query over store | Grounded in retrieval |
| Embeddings | HNSW similarity | Content-addressed cache |
| Claim extraction | Candidate claims | Confidence scored |
| Claim mapping | Proposed mappings | Never authoritative |

## 63. Safety & Budget

- Content-addressed cache by `blake3(prompt + model + params)`.
- Revision-scoped; invalidated on model change.
- Hard budget `budget_usd`; downgrade to local or disable on exceed.
- All prompts/responses recorded as `Evidence` with `generated_at_ns`.
- LLM never writes `Contract.authority = Explicit` or `Verified`.
- Labels marked `Label { source: Llm, model_id, prompt_hash }`.
- Disabling LLM: engine runs fully; deterministic fallbacks apply.

## 64. Local-First

Default provider: **Ollama**. Cloud providers opt-in. LLM labeling works offline. Acceptance: offline Ollama run with no network access produces labels.

---

# Part X — Language Support

## 65. Top-10 Languages

| # | Language | Parser | Notes |
|---|---|---|---|
| 1 | Python | `tree-sitter-python` | dynamic dispatch, generators, async, exceptions |
| 2 | JavaScript | `tree-sitter-javascript` | prototype, promises, closures, `this` |
| 3 | TypeScript | `tree-sitter-typescript` | structural typing, generics, decorators |
| 4 | Java | `tree-sitter-java` | JVM dispatch, checked exceptions, annotations |
| 5 | C | `tree-sitter-c` | pointers, UB, manual memory, preprocessor |
| 6 | C++ | `tree-sitter-cpp` | templates, RAII, overloads, exceptions |
| 7 | C# | `tree-sitter-c-sharp` | CLR dispatch, LINQ, async, attributes |
| 8 | Go | `tree-sitter-go` | goroutines, channels, interfaces, errors |
| 9 | Rust | `tree-sitter-rust` | ownership, traits, `Result`, async, macros |
| 10 | PHP | `tree-sitter-php` | dynamic types, magic methods, exceptions |

**Optional v1.1:** Ruby, Kotlin, Swift, Scala, SQL (embedded DSL).

## 66. Adapter Trait

```rust
pub trait LanguageAdapter: Send + Sync {
    fn id(&self) -> LanguageId;
    fn extensions(&self) -> &'static [&'static str];
    fn tree_sitter_language(&self) -> tree_sitter::Language;
    fn extract_symbols(&self, tree: &Tree, src: &[u8]) -> Vec<SymbolFact>;
    fn extract_calls(&self, tree: &Tree, src: &[u8]) -> Vec<CallFact>;
    fn extract_control_flow(&self, tree: &Tree, src: &[u8]) -> Vec<CfgEdge>;
    fn extract_data_flow(&self, tree: &Tree, src: &[u8]) -> Vec<DfgEdge>;
    fn extract_reads_writes(&self, tree: &Tree, src: &[u8]) -> Vec<EffectFact>;
    fn extract_exceptions(&self, tree: &Tree, src: &[u8]) -> Vec<ExceptionFact>;
    fn extract_async(&self, tree: &Tree, src: &[u8]) -> Vec<AsyncFact>;
    fn dispatch_kind(&self) -> DispatchKind;
    fn numeric_semantics(&self) -> NumericSemantics;
    fn framework_conventions(&self) -> &'static [FrameworkRule];
}
```

## 67. Extension Path

Adding a language requires:

1. Add `tree-sitter-<lang>` dependency.
2. Implement `LanguageAdapter`.
3. Register in `LanguageRegistry` (one line).

**Acceptance:** ≤ 1 crate, ≤ 1 registration line, 0 core changes.

---

# Part XI — Implementation

## 68. Staged Roadmap

| Stage | Deliverable | Acceptance |
|---|---|---|
| 1 | Behavior extraction | Print-only stubs, unconditional success, ignored failure, delegating wrappers |
| 2 | Behavioral conformance | Established vs inferred violations |
| 3 | Rules & concepts (FCA) | Traceable rules; overlapping concepts with LLM labels |
| 3b | Hierarchical concept behavior | Every ConceptNode has label, parents/children, behaviors, rules, symbols, evidence; rollup correct |
| 4 | Structural hierarchy | Evidence-backed module/domain candidates; every edge traceable |
| 5 | Recovery & fault injection | Reproduce failed refunds, interrupted recovery, duplicate processing |
| 6 | Drift analysis | Per-claim status with evidence |
| 7 | Multi-language expansion | All 10 adapters; new language ≤ 1 crate |
| 8 | Native LLM | Offline-first; cache; budget |
| 9 | Folder assimilation | `cie ingest` end-to-end; incremental ≤ 5% |
| 10 | Runtime ingestion | OTel traces mapped to summaries |
| 11 | Multi-repo ingestion | Merged concept graph; API matrix |
| 12 | Capability layer buildout | 16 domains; combinatorial capabilities |
| 13 | Performance hardening | Measured latency, memory, correctness |

## 69. Slices

| Slice | Scope | Outcome |
|---|---|---|
| 0 | Foundation + first behavior extractor | Minimum schema; effects, evidence, coverage, invalidation |
| 1 | Behavioral conformance | Explicit contracts; branch analysis; fake auth, ignored rejection, swallowed exceptions |
| 2 | Rules & concepts | Predicate extraction; bounded checking; local FCA |
| 2b | Hierarchical concept behavior | ConceptNode DAG; `cie concepts --with-behaviors` |
| 3 | Structural hierarchy | Clustering compared against reviewed examples |
| 4 | Recovery execution | State models; targeted fault injection |
| 5 | Drift | Claim extraction, mapping, verification, report |
| 6 | Multi-language | All 10 adapters; extension path validated |
| 7 | Native LLM | Provider trait; cache; budget; offline |
| 8 | Folder assimilation | Full ingest; incremental |
| 9 | Security capabilities | Taint, injection, secrets, SBOM |
| 10 | Testing capabilities | Coverage gaps, mutation targeting, min test set |
| 11 | Performance capabilities | Complexity, N+1, unbounded loops |
| 12 | Refactoring capabilities | Safe rename, extract, breaking change detection |
| 13 | Documentation capabilities | Architecture docs, API docs, diagrams |
| 14 | Compliance capabilities | License, PII, policy, SBOM, schema drift |
| 15 | Developer productivity | Blast radius, PR risk, review bot |
| 16 | AI-native | NL query, grounded review, code generation with verification |
| 17 | Modernization | Bounded contexts, monolith decomposition, strangler-fig |
| 18 | Formal assurance | Contract proofs, invariants, equivalence |
| 19 | Runtime ingestion | OTel traces; hot paths; runtime drift |
| 20 | Observability & ops | Trace/log/metric mapping; incident replay |
| 21 | Multi-repo | Merged concepts; API matrix; org-wide graph |
| 22 | Performance hardening | All targets met |

**Incremental correctness belongs in every slice.**

## 70. Acceptance Suite

**Foundation (1–7):**
1. Print-only authorization detected.
2. Unconditional success detected.
3. Legitimate delegation not flagged.
4. Rejected authorization ignored by caller detected.
5. Exceptions converted to success detected.
6. Unresolved external behavior explicitly marked.
7. Code deletion invalidates earlier finding.

**Ingestion & language (8–11):**
8. Polyglot repo (Python + Rust + Go + TS) ingested end-to-end by folder path.
9. New language (Ruby) added in ≤ 1 crate + ≤ 1 registry line.
10. Unchanged folder re-ingest ≤ 5% of first-ingest wall time.
11. Time-travel query returns correct results for rev N and N-1.

**LLM (12–14):**
12. Ollama offline run produces labels with no network access.
13. LLM cache hit ≤ 100 ns.
14. LLM budget exceeded → downgrade, continue.

**Storage (15–16):**
15. Symbol lookup p99 ≤ 50 ns; CSR edge scan p99 ≤ 20 ns; summary get p99 ≤ 5 µs.
16. Zero-copy snapshot read of 1M-symbol graph ≤ 10 µs warm.

**Drift (17–26):**
17. PRD with 50 claims → ≥ 90% non-Unknown.
18. "Must never double-charge" → Contradicting with counterexample.
19. Claim with no matching code → Missing with evidence of complete search.
20. p99 latency claim → Unverifiable (requires_runtime_evidence).
21. Code without doc claim → reverse_drift.
22. Two docs conflicting → Contradicting citing both.
23. `--from-rev 41 --to-rev 42` shows only changed statuses.
24. Unchanged drift re-run ≤ 5% of first run.
25. LLM disabled → ≥ 60% claims extracted; all verifications run.
26. Timeout during verification → Unknown, never Implemented.

**Hierarchical concept behavior (27–31):**
27. `cie concepts --with-behaviors` emits nested tree with concepts, behaviors, rules.
28. Every behavior and rule attaches to ≥ 1 node or is listed unattached.
29. Rollup at parent = union of descendants.
30. Concept under multiple parents (DAG); each edge traceable.
31. Removing symbol invalidates only referencing nodes.

**Runtime ingestion (32–36):**
32. OTel span mapped to SymbolId or marked Unresolved with reason.
33. Observed behavior differs from static → runtime drift reported.
34. Hot paths ranked by span frequency.
35. Incident replay: trace + contracts → root-cause candidates.
36. Error stack trace → summaries + violated contracts.

**Multi-repo (37–40):**
37. Two repos with same library → one merged concept, two sources.
38. Breaking API change → Contradicting in compatibility matrix.
39. Shared subgraph detected as extraction candidate.
40. Org-wide concept graph emitted.

**Capability spot checks (41–50):**
41. Layering violation detected (CAP-1.1).
42. Taint from `req.query` to SQL execution detected (CAP-2.1, CAP-2.2).
43. N+1 query inside loop detected (CAP-4.2).
44. Dead code with no callers, claims, or tests detected (CAP-1.4, CAP-1.5).
45. Blast radius of a change lists affected summaries, concepts, claims (CAP-10.3).
46. Minimum test set for a change computed (CAP-17.3).
47. Architecture docs generated with mermaid diagrams (CAP-6.1).
48. PR risk score correlated with actual regressions in fixture (CAP-10.4).
49. Monolith decomposition candidates ranked by coupling (CAP-12.2).
50. Contract proof pipeline produces SMT-verified obligation (CAP-13.1).

## 71. Traceability Matrix

| Source | PRD | Paper |
|---|---|---|
| Extract what code does | FR-11 | PDG, IFDS, Abstract Interp, Bi-Abduction |
| Discover expected behavior | FR-12 | Naming, Beliefs, Protocols, Houdini |
| Check rules & counterexamples | FR-13, FR-24 | CEGAR, SLAM, Z3, KLEE |
| Derive concepts & hierarchy | FR-13, FR-14, FR-40–44 | FCA, Incremental FCA, Peixoto, Andritsos |
| Cross-service recovery | FR-15 | Yuan, LDFI, IronFleet |
| Incremental performance | FR-8, FR-16 | Differential Dataflow |
| Top-10 languages | FR-17 | Tree-sitter |
| Easy language extension | FR-18 | Tree-sitter |
| Folder assimilation | FR-19 | — |
| Incremental re-ingest | FR-20 | Differential Dataflow |
| Nanosecond time | FR-21 | ntime |
| Minimal-latency storage | FR-22 | — |
| Time travel | FR-23 | — |
| Native LLM | FR-24–30 | LLM code analysis |
| Document ingestion | FR-31 | Doc-code drift review |
| Claim extraction / mapping | FR-32, FR-33 | Requirements traceability |
| Per-claim verification | FR-34 | — |
| Drift statuses | FR-35 | — |
| Reverse drift | FR-36 | — |
| Drift report | FR-37 | — |
| Multi-doc reconciliation | FR-38 | — |
| LLM role in drift | FR-39 | — |
| Hierarchical concept behavior | FR-40–44 | FCA + clustering |
| Runtime ingestion | FR-45 | OTel |
| Multi-repo ingestion | FR-46 | — |
| 17 capability domains | §30–46 | See capability tables |

---

# Part XII — Appendix

## 72. Risks & Mitigations

| ID | Risk | Mitigation |
|---|---|---|
| R-1 | Building generic framework too long | Validate first foundation against `authorize()` examples |
| R-2 | Timeout mistaken for no defect | Separate completion from conclusion |
| R-3 | Inferred rule mistaken for requirement | Contract authority field |
| R-4 | Spurious counterexamples | CEGAR refinement |
| R-5 | Stale incremental facts | Content hashes, reverse deps, deletion handling |
| R-6 | LLM overreach | Naming adapter only; verification mechanical |
| R-7 | Language-specific rules misapplied | Language semantics adapters |
| R-8 | Vague document claims | Mark Unverifiable explicitly |
| R-9 | Multi-doc conflicts | Surface as Contradicting |
| R-10 | LLM cost overrun | Hard budget, offline-first |
| R-11 | Runtime ingestion volume | Sampling, aggregation, TTL |
| R-12 | Multi-repo identity collision | Signature + package + semantic similarity; confidence-scored |
| R-13 | Capability scope creep | Vertical slices; each capability independently validated |
| R-14 | Performance regression | Measured targets; profiling in every slice |

## 73. Open Questions

- Which persistence backend: `sled`, `redb`, or SQLite?
- Which SMT integration: `z3` crate or external process?
- Which symbolic execution approach: KLEE FFI or bounded Rust implementation?
- Which execution lineage source feeds LDFI?
- Minimum reviewed hierarchy set for Slice 3?
- Additional document formats for v1.1?
- Minimum acceptable claim extraction recall without LLM?
- OTel version and protocol: OTLP/gRPC, OTLP/HTTP, or both?
- Multi-repo identity: which merge signals dominate when they conflict?
- Capability prioritization after §71 acceptance: which of the 17 domains ship first?

## 74. Limitations (Explicit)

| Not achievable | Why |
|---|---|
| Ground truth for vague requirements ("user-friendly") | No operational definition |
| Semantic equivalence with arbitrary external systems | Requires modeling them |
| Complete dynamic analysis without execution | Static only; runtime needs OTel |
| Sound security analysis without heap modeling | Bi-abduction is approximate |
| Correct concurrency analysis without happens-before model | Needs memory-model adapter |
| Business value estimation | Not derivable from code or docs |
| Predicting bugs in code never executed | Requires execution evidence |
| Perfect drift for runtime-dependent claims | Marked Unverifiable by design |
| Human intent | Hypothesis only |
| Merge of two independent codebases without shared identity | Best-effort with confidence |

## 75. Top-10 Highest-Leverage Additions (Priority Guide)

Ranked by value ÷ cost, once foundation exists:

1. **Taint tracking** — unlocks most of CAP-2. One IFDS instantiation.
2. **Blast radius for any change** — CAP-10.3. Pure combination.
3. **Semantic architecture diff** — CAP-1.10, CAP-17.1. Pure combination.
4. **Auto-generated architecture docs** — CAP-6.1. Pure combination.
5. **N+1 / unbounded loop detection** — CAP-4.2, CAP-4.3. Small add.
6. **Dead code / orphan detection** — CAP-1.4, CAP-1.5. Small add.
7. **Grounded code review bot** — CAP-10.7. Small add.
8. **Cross-language migration assistance** — CAP-5.9. Small add.
9. **Minimum test set per change** — CAP-17.3. Small add.
10. **Incident root-cause candidates** — CAP-8.5. Needs OTel, then pure combination.

## 76. One-Line Summary

Given any folder, any document, any runtime trace stream, and any set of repositories, CIE extracts typed facts, function summaries, contracts, concepts, hierarchy, and runtime evidence; verifies conformance with mechanical checkers; emits a revisioned, nanosecond-indexed **drift report** and **hierarchical concept behavior tree**; and powers sixteen capability domains — with evidence, assumptions, and explicit uncertainty — never promoting vague, runtime-dependent, or unresolved claims to `Implemented`, and never letting the LLM decide membership, verification, or truth.

---

**End of PRD.**