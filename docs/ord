# PRD: Code Intelligence Engine (CIE) — Complete Specification v1.0

**Status:** Implementation-ready  
**Language:** Rust (core + analysis) + TypeScript (adapter + web)  
**Substrates:** tree-sitter (parsing) · SQLite (storage) · G6 + Mermaid + ReactFlow + JointJS (visualization) · Ollama (LLM) · OpenTelemetry (runtime)  
**Source basis:** All prior CIE PRD revisions, the multi-engine visualization work, the polyglot AST/IR analysis, and the hierarchy corrections.  
**Purpose:** A polyglot, evidence-first code intelligence engine that ingests folders, documents, runtime traces, and multi-repo graphs; produces hierarchical concept behavior, drift reports, impact/duplicate analysis, and 76 diagram types; renders through a negotiable multi-engine substrate; and never displays an unevidenced claim.

---

# Part I — Overview

## 1. Executive summary

CIE is not a linter, not a docs generator, not a code search tool. It is a **shared analysis foundation** that turns code, documents, runtime traces, and repositories into a single evidence-bound model, and then surfaces that model as **honest, hierarchical, multi-engine visualizations** and **verifiable findings**.

The engine answers, with evidence and explicit uncertainty:

- What does the code actually do?
- What is it expected to do?
- Does it conform?
- What are the concepts and how do they nest?
- What breaks if I change this?
- What copies of this logic exist elsewhere?
- What is implemented vs. missing vs. drifted vs. undocumented?
- What is the runtime behavior, and how does it differ from the static picture?
- What state machines and workflows exist?
- What C4 containers and components does this polyglot repo decompose into?

## 2. Vision and principles

| Principle | Meaning |
|---|---|
| **Evidence over assertion** | Every conclusion carries revision, provenance, assumptions, coverage |
| **Hypothesis ≠ truth** | Inferred rules, LLM mappings, naming conventions are candidates, never established requirements |
| **Explicit uncertainty** | `Unverifiable`, `Unknown`, and `Fog` are first-class states |
| **Timeout ≠ no defect** | Analysis completion and conclusion are separate dimensions |
| **LLM labels, never decides** | Membership, verification, truth are mechanical |
| **Incremental by construction** | Content hashes, reverse deps, deletion handling in every slice |
| **Nanosecond time** | Every timestamp in `u64` ns |
| **Language-agnostic core** | New language = one adapter + one registration |
| **Honest visual law** | Fact = solid · Inference = dashed · Hypothesis = dotted · Fog = hatched |
| **One contract, many engines** | CIE emits `ViewSpec`; engines translate to their own visual vocabularies |
| **No file paths as concepts** | Files are evidence metadata; concepts are semantic units |

## 3. Non-goals

- Automatically extracting arbitrary business semantics from code alone
- Treating IFDS, bi-abduction, SMT, symbolic execution, LDFI, or IronFleet as universal automatic proof
- Ground truth for vague claims ("user-friendly", "scalable")
- Certainty when code is dynamic, reflective, generated, or unresolved
- Complete dynamic analysis without runtime evidence
- Sound security analysis without heap modeling
- Predicting bugs in code never executed
- A fixed enumeration of chart types (the registry is the enumeration)
- Reimplementing layout algorithms (engines own geometry)

---

# Part II — System architecture

## 4. Layered architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│ L6  Output                                                       │
│     CLI · JSON · Markdown · DOT · SARIF · WebSocket · HTTP       │
│     ViewSpec → G6 / Mermaid / ReactFlow / JointJS                │
├──────────────────────────────────────────────────────────────────┤
│ L5  Capability                                                   │
│     17 capability domains · 76 chart types · combinatorial       │
├──────────────────────────────────────────────────────────────────┤
│ L4  Analysis                                                     │
│     Behavior · Conformance · FCA · Concept DAG · C4 · State     │
│     machines · Workflows · Impact · Duplicates · Drift           │
├──────────────────────────────────────────────────────────────────┤
│ L3  Foundation                                                   │
│     AST · UAST · IR/CFG · Symbols · Calls · Summaries ·         │
│     Contracts · Evidence · Incremental engine                    │
├──────────────────────────────────────────────────────────────────┤
│ L2  Ingestion                                                    │
│     Code (10 languages) · Documents · Runtime (OTel) ·          │
│     Multi-repo                                                   │
├──────────────────────────────────────────────────────────────────┤
│ L1  Storage                                                      │
│     Hot: DashMap + CSR · Warm: SQLite · Cold: rkyv/parquet       │
│     HNSW vectors · Time-indexed tables · Revision chain          │
└──────────────────────────────────────────────────────────────────┘
```

## 5. Actor and decision authority

### 5.1 Engine selection

```text
1. Capability gate       (hard, non-overridable)
2. User preference       (bounded by 1)
3. Graph size limits     (hard)
4. Deterministic order   (g6 → jointjs → reactflow → mermaid)
```

### 5.2 Layout selection

```text
1. User layout choice    (explicit UI selection — highest)
2. ViewSpec.layoutHint   (CIE shape-based suggestion)
3. Adapter auto-select   (graph-shape analysis)
4. Adapter substitution  (if unavailable — always disclosed)
```

Layout precedence is inverted from engine precedence. Every override is disclosed in the UI as `Preferred: X ✗ (reason) · Rendered: Y`.

### 5.3 What each actor may decide

| Actor | May | May not |
|---|---|---|
| **CIE** | Emit `layoutHint` (strategy name only) | Name an engine, name a layout library |
| **Negotiator** | Reject by capability/size, honour preference when eligible | Override preference silently |
| **User** | Pin engine (subject to capability), pin layout, toggle fisheye | Bypass capability gate silently |
| **Adapter** | Translate layout hint, substitute and report | Select its own engine; claim a substituted layout was honoured |

## 6. Hash split (cache correctness)

```rust
pub struct ViewSpec {
    pub semantic_hash: Hash,   // elements, edges, gaps, footer, scope
    pub render_hash:   Hash,   // semantic_hash + layoutHint + preferences + engine_id
}
```

- **Cache key:** `semantic_hash` — layout swap doesn't invalidate content
- **Render artifact key:** `render_hash`
- **Audit baseline:** keyed by `(semantic_hash, engine_id, layout_hint_hash)`

Without this split, the first-chosen layout becomes the only stable one, and multi-engine support is nominal.

---

# Part III — Ingestion paths

## 7. Code ingestion — folder-point assimilation

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
  → persist AST to SQLite (§13)
  → build UAST map (§14)
  → lower to IR + CFG (§15)
  → extract symbols, calls, framework annotations
  → build call graph (bottom-up summaries)
  → run FCA → concept lattice (§22)
  → merge into ConceptNode DAG (C4 + FCA + classes) (§24)
  → extract state machines (§25) and workflows (§26)
  → run impact-radius and duplicate analysis (§27)
  → compute function summaries (§20)
  → populate contract registry (§21)
  → run LLM naming adapter (if enabled)
  → persist with revision hash
  → emit coverage + evidence + conclusions
```

**Incremental re-ingest:** content-hash diff → invalidate changed + transitively affected → re-run affected slices → emit delta.

**Acceptance:** Re-ingest of unchanged folder ≤ 5% of first-ingest wall time.

## 8. Document ingestion

Formats: Markdown, plain text, PDF, DOCX, HTML, OpenAPI, Protobuf, GraphQL SDL, Confluence, Notion, Jira/Linear, Git commit messages, PR descriptions.

Each claim preserves `DocSourceLocation { doc, page, section, paragraph, char_start, char_end }`.

## 9. Runtime ingestion (OTel)

**Inputs:** OpenTelemetry traces, logs, metrics via OTLP; APM exports (Jaeger, Tempo, Datadog, New Relic); log files; metric snapshots.

**Pipeline:**

```text
OTel / logs / metrics
  → parse spans (trace_id, span_id, parent, service, operation, duration, status)
  → map service/operation to SymbolId (via summary names + framework adapters)
  → emit RuntimeEvidence { trace_id, span_id, symbol, path, duration_ns, status, attrs }
  → aggregate: hit counts, observed effects, observed exceptions, observed paths
  → join with static summaries → observed-vs-static delta
  → persist to runtime evidence table, indexed by symbol and by ns
```

**Outputs:**
- Observed behavior per summary
- Runtime drift: static summary differs from observed
- Hot paths (frequency-weighted call graph)
- Flaky operations (status varies across traces)
- PII flow evidence (field names in attributes)

**Acceptance:** A span can be mapped to a `SymbolId` or explicitly marked `Unresolved` with reason.

## 10. Multi-repo ingestion

```bash
cie ingest-org <manifest.yaml>
cie ingest-org <path1> <path2> <path3>
```

**Pipeline:**

```text
multiple repos
  → ingest each repo independently
  → resolve cross-repo references (imports, package names, API specs)
  → merge SymbolIds via package + signature + name + semantic similarity
  → merge ConceptNode DAGs across repos
  → detect shared subgraphs (extraction candidates)
  → detect API compatibility across services
  → emit org-wide concept graph + compatibility matrix
```

**Acceptance:** Two repos with the same library produce one merged concept with two sources; a breaking API change appears as `Contradicting`.

---

# Part IV — Storage layer

## 11. The polyglot AST schema

### 11.1 Core tables

```sql
CREATE TABLE files (
    id           INTEGER PRIMARY KEY,
    repo_id      INTEGER NOT NULL,               -- multi-repo
    path         TEXT    NOT NULL,
    content_hash BLOB    NOT NULL,               -- blake3
    language     TEXT    NOT NULL,
    revision     INTEGER NOT NULL,
    size_bytes   INTEGER NOT NULL,
    tree_sitter_grammar_version TEXT NOT NULL,
    UNIQUE (repo_id, path, revision)
);
CREATE INDEX idx_files_rev ON files(revision);
CREATE INDEX idx_files_hash ON files(content_hash);

CREATE TABLE node_kinds (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL,                   -- 'function_definition', 'If', ...
    language TEXT,                               -- NULL = cross-language
    UNIQUE (name, language)
);

CREATE TABLE node_text (
    id       INTEGER PRIMARY KEY,
    content  TEXT    NOT NULL,
    hash     BLOB    NOT NULL UNIQUE,
    kind_hint TEXT
);

CREATE TABLE ast_nodes (
    id          INTEGER PRIMARY KEY,
    file_id     INTEGER NOT NULL REFERENCES files(id),
    parent_id   INTEGER REFERENCES ast_nodes(id),
    kind_id     INTEGER NOT NULL REFERENCES node_kinds(id),
    child_index INTEGER NOT NULL DEFAULT 0,
    start_byte  INTEGER NOT NULL,
    end_byte    INTEGER NOT NULL,
    start_row   INTEGER NOT NULL,
    start_col   INTEGER NOT NULL,
    end_row     INTEGER NOT NULL,
    end_col     INTEGER NOT NULL,
    is_named    INTEGER NOT NULL DEFAULT 1,
    is_error    INTEGER NOT NULL DEFAULT 0,
    is_missing  INTEGER NOT NULL DEFAULT 0,
    text_id     INTEGER REFERENCES node_text(id),
    shape_hash  BLOB,
    uast_kind_id INTEGER REFERENCES uast_kinds(id)
);
CREATE INDEX idx_ast_span ON ast_nodes(file_id, start_byte, end_byte);
CREATE INDEX idx_ast_children ON ast_nodes(parent_id, child_index);
CREATE INDEX idx_ast_kind ON ast_nodes(file_id, kind_id);
CREATE INDEX idx_ast_errors ON ast_nodes(file_id, is_error, is_missing);
CREATE INDEX idx_ast_shape ON ast_nodes(shape_hash);
```

**Why the byte-span index matters:** because AST spans are strictly nested, the entire subtree of any node is a contiguous range in a table sorted by `(file_id, start_byte, end_byte)`. Subtree retrieval is a range scan — microseconds, not recursive walk.

### 11.2 UAST layer

```sql
CREATE TABLE uast_kinds (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE  -- 'TypeDecl' | 'FunctionDecl' | 'CallExpr' | ...
);

CREATE TABLE uast_map (
    id            INTEGER PRIMARY KEY,
    ast_kind_id   INTEGER NOT NULL REFERENCES node_kinds(id),
    uast_kind_id  INTEGER NOT NULL REFERENCES uast_kinds(id),
    confidence    REAL    NOT NULL DEFAULT 1.0,
    provenance    TEXT    NOT NULL,
    UNIQUE (ast_kind_id, uast_kind_id)
);
```

Mapping examples:

| Grammar kind | Language | UAST kind |
|---|---|---|
| `function_definition` | python | `FunctionDecl` |
| `function_item` | rust | `FunctionDecl` |
| `function_declaration` | go | `FunctionDecl` |
| `method_declaration` | java | `MethodDecl` |
| `class_definition` | python | `TypeDecl` |
| `struct_item` | rust | `TypeDecl` |
| `trait_item` | rust | `TraitDecl` |
| `interface_declaration` | java | `TraitDecl` |
| `call` | python | `CallExpr` |
| `method_invocation` | java | `CallExpr` |

### 11.3 Derived tables — symbols

```sql
CREATE TABLE symbols (
    id              INTEGER PRIMARY KEY,
    file_id         INTEGER NOT NULL REFERENCES files(id),
    ast_node_id     INTEGER NOT NULL REFERENCES ast_nodes(id),
    name            TEXT    NOT NULL,
    qual_name       TEXT    NOT NULL,
    kind            TEXT    NOT NULL,
    parent_symbol_id INTEGER REFERENCES symbols(id),
    start_byte      INTEGER NOT NULL,
    end_byte        INTEGER NOT NULL,
    sig_end_byte    INTEGER,
    subtree_hash    BLOB    NOT NULL,
    annotations     TEXT    -- JSON: ["Transactional","Async","RequestMapping"]
);
CREATE INDEX idx_sym_file   ON symbols(file_id);
CREATE INDEX idx_sym_parent ON symbols(parent_symbol_id);
CREATE INDEX idx_sym_name   ON symbols(name);
CREATE INDEX idx_sym_qual   ON symbols(qual_name);
CREATE INDEX idx_sym_kind   ON symbols(kind);
CREATE INDEX idx_sym_span   ON symbols(file_id, start_byte, end_byte);
```

### 11.4 Calls table (with all six sequence-diagram additions)

```sql
CREATE TABLE calls (
    id                INTEGER PRIMARY KEY,
    file_id           INTEGER NOT NULL REFERENCES files(id),
    call_node_id      INTEGER NOT NULL REFERENCES ast_nodes(id),
    caller_symbol_id  INTEGER NOT NULL REFERENCES symbols(id),
    callee_symbol_id  INTEGER REFERENCES symbols(id),
    callee_text       TEXT,
    resolution        TEXT NOT NULL,       -- 'PARSED' | 'RESOLVED' | 'UNRESOLVED'
    stmt_index        INTEGER NOT NULL,
    -- sequence-diagram additions
    block_id          INTEGER REFERENCES ir_blocks(id),
    call_kind         TEXT NOT NULL DEFAULT 'sync',  -- 'sync'|'async_spawn'|'async_await'|'callback'|'stream'
    throws_set        BLOB,                -- serialized exception list from callee summary
    guard_summary     TEXT                 -- human-readable condition, e.g. "x > 10"
);
CREATE INDEX idx_calls_caller ON calls(caller_symbol_id, stmt_index);
CREATE INDEX idx_calls_callee ON calls(callee_symbol_id);
CREATE INDEX idx_calls_block  ON calls(block_id);
CREATE INDEX idx_calls_unres  ON calls(resolution) WHERE resolution = 'UNRESOLVED';
```

### 11.5 IR tables

```sql
CREATE TABLE ir_blocks (
    id            INTEGER PRIMARY KEY,
    symbol_id     INTEGER NOT NULL REFERENCES symbols(id),
    block_index   INTEGER NOT NULL,
    kind          TEXT    NOT NULL     -- 'entry' | 'normal' | 'exit'
);

CREATE TABLE ir_instructions (
    id            INTEGER PRIMARY KEY,
    block_id      INTEGER NOT NULL REFERENCES ir_blocks(id),
    inst_index    INTEGER NOT NULL,
    op            TEXT    NOT NULL,    -- 'Param' | 'Const' | 'Cmp' | 'Call' | 'Br' | 'Ret' | ...
    operands      BLOB    NOT NULL,
    ast_node_id   INTEGER REFERENCES ast_nodes(id),   -- evidence back-pointer
    span_start    INTEGER NOT NULL,
    span_end      INTEGER NOT NULL
);

CREATE TABLE ir_edges (
    from_block_id INTEGER NOT NULL REFERENCES ir_blocks(id),
    to_block_id   INTEGER NOT NULL REFERENCES ir_blocks(id),
    kind          TEXT    NOT NULL,   -- 'branch_true' | 'branch_false' | 'jump' | 'exception'
    PRIMARY KEY (from_block_id, to_block_id, kind)
);
CREATE INDEX idx_ir_ast    ON ir_instructions(ast_node_id);
CREATE INDEX idx_ir_symbol ON ir_blocks(symbol_id, block_index);
```

**The `ast_node_id` on every instruction** is the honesty invariant — every IR op traces back to a byte span.

### 11.6 FCA inputs

```sql
CREATE TABLE fca_objects (
    symbol_id INTEGER NOT NULL REFERENCES symbols(id),
    attribute TEXT    NOT NULL,
    evidence  INTEGER REFERENCES ast_nodes(id),
    PRIMARY KEY (symbol_id, attribute)
);
CREATE INDEX idx_fca_attr ON fca_objects(attribute);
```

### 11.7 Concept DAG (output)

```sql
CREATE TABLE concept_nodes (
    id            INTEGER PRIMARY KEY,
    label         TEXT    NOT NULL,
    kind          TEXT    NOT NULL,   -- 'Domain'|'Container'|'Service'|'Aggregate'|'Entity'|'Value'|'UseCase'|'Rule'|'Invariant'
    revision      INTEGER NOT NULL,
    confidence    REAL    NOT NULL,
    provenance    TEXT    NOT NULL,
    primary_edge  TEXT    NOT NULL,
    semantic_hash BLOB    NOT NULL
);

CREATE TABLE concept_edges (
    parent_id     INTEGER NOT NULL REFERENCES concept_nodes(id),
    child_id      INTEGER NOT NULL REFERENCES concept_nodes(id),
    reason        TEXT    NOT NULL,
    is_primary    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (parent_id, child_id, reason)
);

CREATE TABLE concept_node_symbols (
    concept_id    INTEGER NOT NULL REFERENCES concept_nodes(id),
    symbol_id     INTEGER NOT NULL REFERENCES symbols(id),
    PRIMARY KEY (concept_id, symbol_id)
);
```

### 11.8 Cross-language bindings

```sql
CREATE TABLE cross_language_bindings (
    id                INTEGER PRIMARY KEY,
    source_symbol_id  INTEGER REFERENCES symbols(id),
    target_symbol_id  INTEGER REFERENCES symbols(id),
    protocol          TEXT NOT NULL,           -- 'http' | 'grpc' | 'graphql' | 'amqp' | 'kafka'
    spec_file_id      INTEGER REFERENCES files(id),
    spec_pointer      TEXT,
    confidence        REAL NOT NULL,
    resolution        TEXT NOT NULL            -- 'DECLARED' | 'URL_MATCHED' | 'RUNTIME_OBSERVED'
);
CREATE INDEX idx_xlang_source ON cross_language_bindings(source_symbol_id);
CREATE INDEX idx_xlang_target ON cross_language_bindings(target_symbol_id);
```

### 11.9 Framework adapters

```sql
CREATE TABLE framework_adapters (
    id           INTEGER PRIMARY KEY,
    language     TEXT NOT NULL,
    framework    TEXT NOT NULL,
    feature      TEXT NOT NULL,    -- 'route' | 'service' | 'repository' | 'entity' | ...
    ast_kind     TEXT NOT NULL,
    match_value  TEXT NOT NULL,
    confidence   REAL NOT NULL DEFAULT 1.0
);
CREATE INDEX idx_fw_lang ON framework_adapters(language, feature);
```

### 11.10 Source cache

```sql
CREATE TABLE file_source_cache (
    file_id    INTEGER PRIMARY KEY REFERENCES files(id),
    zstd_bytes BLOB NOT NULL,
    revision   INTEGER NOT NULL
);
```

## 12. Latency targets

| Operation | Target |
|---|---|
| Symbol lookup | < 50 ns |
| Hash → SymbolId | < 30 ns |
| CSR edge scan | < 20 ns |
| Summary get (warm) | < 500 ns |
| Summary get (cold L1) | < 5 µs |
| Subtree retrieval (range scan) | < 5 µs |
| CFG query (per function) | < 20 µs |
| Sequence query (per 100 calls) | < 50 µs |
| Snapshot read (L2, mmap) | < 10 µs |
| FCA closure step | < 2 µs |
| HNSW k-NN (k=10, 1M) | < 1 ms |
| LLM label cache hit | < 100 ns |
| Runtime evidence get | < 10 µs |

---

# Part V — AST → UAST → IR pipeline

## 13. Parsing

Per file:
1. Detect language (extension + shebang + grammar probe)
2. Parse with tree-sitter grammar pinned by `tree_sitter_grammar_version`
3. Persist `files` row + `ast_nodes` rows in one transaction
4. Compute `shape_hash` for every node (ANL-06, pre-order grammar kinds only)

**Empty scope never certifies absence.** Missing parse → `is_error` set on root; file still indexed but flagged.

## 14. UAST normalization

Per file, after AST:
1. For each `ast_node`, look up `uast_map` by `(kind_id, language)`
2. Populate `ast_nodes.uast_kind_id`
3. If no mapping exists: leave `NULL` — the node stays grammar-specific, only visible to language-specific queries

**Why UAST and not just AST:** A query like "give me all functions in this polyglot repo" is a single join through `uast_kinds`. Without UAST it would need per-language queries.

## 15. IR lowering

Per symbol, after UAST:
1. Build CFG from AST using the lowering pass (§15.1)
2. Emit `ir_blocks`, `ir_instructions`, `ir_edges`
3. Populate `calls.block_id`, `calls.call_kind`, `calls.throws_set`, `calls.guard_summary` during the same pass
4. Compute `ir_hash` (ANL-05): rename-of-locals-stable
5. Compute `ir_vec` (sparse embedding for cosine similarity)

### 15.1 The lowering algorithm

```rust
pub fn lower_node(&mut self, node: Node, src: &str) -> Value {
    match node.kind() {
        "integer_literal" => Value::Const(parse(node.text(src))),
        "identifier" => Value::Slot(intern(node.text(src))),
        "binary_expression" => {
            let l = self.lower_node(node.child(0)?, src);
            let r = self.lower_node(node.child(1)?, src);
            Value::BinaryOp(op_of(node), l, r)
        }
        "if_statement" => {
            let cond = self.lower_node(node.child(0)?, src);
            let then_bb = self.new_block();
            let else_bb = self.new_block();
            self.emit(Branch { cond, then: then_bb, else: else_bb });
            self.enter_block(then_bb);
            self.lower_node(node.child(1)?, src);
            self.enter_block(else_bb);
            if let Some(else_node) = node.child(2) { self.lower_node(else_node, src); }
            Value::Unit
        }
        "call" => {
            let callee = self.lower_node(node.child(0)?, src);
            let args = /* lower each arg */;
            let call_kind = classify_call(node, src);  // sync | async_await | async_spawn | callback
            let guard = self.current_guard();          // walk backward to branch, render symbolically
            self.emit_call(callee, args, call_kind, guard);
            Value::CallResult
        }
        // ...
    }
}
```

**What lowering erases:** variable names, parentheses, loop form (`for`/`while`/`do` all become one `Loop`), compound assignment form (`+=` and `= x + 1` become the same `Add`).

**What lowering preserves:** byte spans (per instruction), external names (for cross-file resolution), literal values (blanked from `ir_dump` for privacy, but present in IR for analysis).

## 16. AST vs IR — the separation of concerns

| Dimension | AST | IR |
|---|---|---|
| Question | "What does this look like?" | "What does this do?" |
| Structure | Tree | Graph (blocks + edges) |
| Language-specific | Yes | No (normalized instruction set) |
| Names preserved | Yes | Locals blanked, externals kept |
| Control flow | Implicit in tree | Explicit as CFG |
| Data flow | Implicit | Explicit as SSA def-use |
| Idiom equivalent | No | Yes |
| Used for | Evidence spans, tree-sitter queries, symbol extraction | Effects, taint, shape hash, PDG, **flowcharts, sequence diagrams** |

**Both are indispensable.** AST for `@RestController` detection; IR for `authorize()` print-only detection. AST for "where is this text"; IR for "what does this text mean".

---

# Part VI — Semantic analysis

## 17. Behavior extraction

Extract function effects, paths, and interprocedural summaries (bottom-up over call graph).

`FunctionSummary { inputs, outputs, reads, writes, effects, exceptions, path_conditions, callees, unresolved }`

Detect: print-only stubs, unconditional success, ignored failure, delegating wrappers.

**Papers:** PDG (Ferrante 1987), IFDS (Reps 1995), Abstract Interpretation (Cousot 1977), Bi-Abduction (Calcagno 2009).

## 18. Framework detection

Populated via `framework_adapters` table (one row per language-framework-feature). Required frameworks:

| Language | Frameworks |
|---|---|
| Java | Spring Boot, Spring Cloud Gateway |
| Rust | Actix, Axum, Rocket |
| Go | Gin, Echo, Chi |
| Python | Django, FastAPI, Flask |
| TypeScript | Express, NestJS, Next.js |
| C# | ASP.NET Core |

Framework roles detected: routes, services, repositories, entities, config classes, boot mains, DI-injected classes, `@Transactional`, `@Async`.

## 19. State machine and workflow detection

### 19.1 Three tiers

| Tier | Signal | Confidence |
|---|---|---|
| **T1 — Explicit** | `createMachine({...})` (XState), `StateMachineBuilder` (Spring), `@Workflow` (Temporal), AWS Step Functions YAML | **Fact** |
| **T2 — Structural** | Enum + `switch`/`match` arms assigning state variable | **Inference** |
| **T3 — Behavioral** | Call-graph ordering across components, conditional branches, queue boundaries | **Hypothesis** (with counter-argument) |

### 19.2 Detection rules

- **T1**: query for known library calls with structured literal; extract states and transitions verbatim
- **T2**: enum with name matching `state|status|phase|mode`; walk `switch`/`match` cases where assignment target equals the switch subject; extract guards from `if` wrappers; mark as `Inference` when incomplete (`4 of 6 enum members appear in switch arms`)
- **T3**: forward DFS from entry points (HTTP routes, queue consumers, cron jobs, webhooks); stop at async boundaries (mark as boundary node); follow conditional branches as gateways; collapse cycles as loop nodes; label by framework role

### 19.3 Epistemic encoding

- T1 → `Fact` (solid)
- T2 → `Inference` (dashed); incomplete → `Fog` (hatched) with reason
- T3 → `Hypothesis` (dotted) with counter-argument ("static call order; runtime branching may differ")

## 20. C4 container and component detection

### 20.1 Container identification

One container per manifest root (pom.xml, build.gradle, Cargo.toml, go.mod, package.json, pyproject.toml). Detection sources:

1. Build manifests
2. Deployment configs (Dockerfile, docker-compose, K8s manifests)
3. Entry points (main.rs, main.go, `@SpringBootApplication`, app.py, server.js)

### 20.2 Component identification

Inside each container, group UAST nodes:

| Signal | Rule | Confidence |
|---|---|---|
| Framework annotation | All `@Service` classes → "Service Layer" | Fact |
| Naming suffix | `*Repository`, `*Controller`, `*Handler` | Inference |
| Call-graph cohesion | SCCs of internal calls | Inference |
| Directory structure | DDD segments: `domain/`, `application/`, `infrastructure/` | Inference |
| Import boundaries | No cross-imports → separate components | Inference |

### 20.3 Relationship derivation

| UAST edge | C4 relationship |
|---|---|
| `CallExpr` cross-component | `uses` |
| `ImportDecl` cross-component | `depends-on` |
| `@Autowired Service` | `uses (DI)` |
| `cross_language_bindings` DECLARED | `calls` (Fact) |
| `cross_language_bindings` URL_MATCHED | `calls` (Inference) |

## 21. FCA concept extraction

Objects = functions. Attributes = effects, tags, framework roles, stack facts.

```rust
let objects: Vec<(SymbolId, BTreeSet<String>)> = summaries
    .iter()
    .filter(|(_, s)| !s.is_test())
    .map(|(id, s)| {
        let mut attrs = s.tags.clone();
        for e in &s.effects { attrs.insert(e.as_str()); }
        // Add framework facts from C4 (§20)
        if route_files.contains(path) { attrs.insert("http-route"); }
        if effects.contains(DbWrite) { attrs.insert("db-write"); }
        if di_files.contains(path) { attrs.insert("di-injected"); }
        (*id, attrs)
    })
    .collect();

let lattice = fca::build_lattice(&objects);
```

**The lattice is the only algorithm whose output IS a concept** (in the mathematical sense, per Wille 1982 / Godin 1995). Everything else produces attributes or facts. This is why FCA runs over the *corpus*, not over a single AST.

## 22. ConceptNode DAG — the corrected construction

### 22.1 The correction that eliminated the "wall of file paths"

**Original (broken) code created a `ConceptNode` for every file:**
```rust
// ❌ WRONG — creates file nodes as concepts
for path in store.files() {
    let n = ConceptNode::new(0, path.clone(), ConceptKind::Module, now);
    dag.link_child(module_parent(&path, root), id);
}
```

**Corrected:**
```rust
// ✓ RIGHT — classes link directly to C4 components; files are evidence metadata
for sym in store.type_likes() {
    let n = ConceptNode::new(0, sym.name.clone(), kind_of(sym), now);
    n.symbols = vec![sym.id];
    let id = dag.push(n);
    let parent = c4.component_of(path).unwrap_or(root);
    dag.link_child(parent, id);
}
```

### 22.2 Hierarchy structure

```text
Project (Domain)
├── checkout-api (Container, from pom.xml)
│   ├── CheckoutService (Service, from @Service)
│   │   ├── CheckoutController (Entity)
│   │   ├── Order (Aggregate, from repo_entity + route reach)
│   │   └── Payment (Entity)
│   └── Persistence (Service, from *Repository suffix)
│       └── OrderRepository (Entity)
├── pricing-engine (Container, from Cargo.toml)
│   └── Pricing (Service, from module)
│       └── PriceCalculator (Entity)
└── ml-fraud (Container, from pyproject.toml)
    └── FraudScoring (Service, from FastAPI route)
        └── Scorer (Entity)
```

**Files never appear as ConceptNodes.** They appear as `symbols.file_id` and as `Evidence.file`.

### 22.3 Edge reasons (every edge traceable)

| Reason | Source | Meaning |
|---|---|---|
| `PackageContainment` | C4 build | Container contains component |
| `ClusterCohesion` | Call-graph clustering | Component contains class by cohesion |
| `FcaSubsumption` | FCA lattice | UseCase spans classes sharing attributes |
| `CallOwnership` | Call graph | Rule applies to class that owns the contract |

### 22.4 Behavior and rule rollup (FR-43)

Per node:
- **behaviors** = union of own + descendants
- **rules** = union of own + descendants
- **effects** = union of attached summaries
- **coverage** = intersection of descendants
- **confidence** = weakest-link

**Acceptance:** Rollup matches clean recomputation from leaves.

### 22.5 The user-facing output

```markdown
## Cart (Aggregate)
- Concept: extent {Cart, CartItem}, intent {mutable, per-user, has-items}
- Symbols: cart.py:Cart, cart.py:CartItem
- Behaviors:
  - Add item to cart — cart.py:Cart.add_item
  - Calculate cart total — cart.py:Cart.total
- Rules:
  - Quantity must be > 0 (Invariant, Explicit) — C-001
- Children:
  - CartItem (Entity)
  - Quantity (Value)
```

## 23. Impact radius and duplicate detection

The visual with shared center, callers on the left by ring, duplicates on the right by similarity tier.

### 23.1 Semantic shape

```typescript
{
  elements: [
    { id: 'center', elementKind: 'symbol', displayMode: 'Fact',
      label: 'Cart.add_item', fields: { role: 'center', ring: 0 } },
    { id: 'c2', elementKind: 'symbol', displayMode: 'Fact',
      label: 'API.addToCart', fields: { role: 'caller', ring: 1 } },
    { id: 't2', elementKind: 'symbol', displayMode: 'Inference',
      label: 'Worker.syncCart', claimId: 'C-310',
      fields: { role: 'caller', ring: 2 } },
    { id: 'fog1', elementKind: 'symbol', displayMode: 'Fog',
      label: 'getattr() dispatch',
      fogReason: 'dynamic dispatch — 3 potential callers not resolved',
      fields: { role: 'caller', ring: 3 } },
    { id: 'dup1', elementKind: 'symbol', displayMode: 'Fact',
      label: 'SavedCart.add_item',
      fields: { role: 'duplicate', ring: 1, similarity: 0.99,
                isCanonical: true, detection: 'ir-hash-match' } },
    { id: 'dup2', elementKind: 'symbol', displayMode: 'Inference',
      label: 'Wishlist.add_item', claimId: 'C-311',
      fields: { role: 'duplicate', ring: 1, similarity: 0.87,
                drift: 'diverged', driftDetail: 'missing null-check',
                detection: 'ir-shingle' } },
  ],
  edges: [
    { source: 'c2',    target: 'center', edgeKind: 'calls', displayMode: 'Fact' },
    { source: 't2',    target: 'c2',     edgeKind: 'calls', displayMode: 'Inference' },
    { source: 'center', target: 'dup1',  edgeKind: 'similar', displayMode: 'Fact',
      fields: { similarity: 0.99 } },
    { source: 'center', target: 'dup2',  edgeKind: 'similar', displayMode: 'Inference',
      fields: { similarity: 0.87 } },
  ],
  layoutHint: { strategy: 'cie-impact-duplicate' },
}
```

### 23.2 Detection algorithm

| Tier | Method | Confidence |
|---|---|---|
| Canonical duplicate | `ir_hash` exact match | Fact |
| High-similarity duplicate | `ir_vec` cosine ≥ 0.85 | Inference |
| Medium-similarity duplicate | `ir_vec` cosine 0.60–0.85 + shape hash match | Inference |
| Low-similarity candidate | `ir_vec` cosine 0.40–0.60 | Hypothesis (with counter-argument) |
| Below threshold | Hidden, counted in gaps | — |

### 23.3 Canonical detection

A duplicate is canonical if it has the most callers AND the most tests. Detection runs in **both directions** — the visual swaps which side is canonical if the original has fewer.

### 23.4 Encoding

| Property | Encoding |
|---|---|
| Solid stroke | Fact |
| Dashed stroke | Inference |
| Dotted stroke | Hypothesis |
| Hatched fill | Fog |
| Purple tint | Duplicate hemisphere |
| Similarity % badge | Score |
| ◆ canonical badge | Most callers + tests |
| ⚠ drifted badge | Diverged since last index |
| Dashed similarity edge | Bidirectional, never a call arrow |

## 24. Drift analysis

### 24.1 Claim extraction

Types: Capability, Behavior, Constraint, Interface, DataModel, NonFunctional, Sequence, Prohibition. Modality: shall, must, should, may, will.

### 24.2 Per-claim verification

| Claim type | Checker |
|---|---|
| Capability | Existence check over SummaryStore + ConceptLattice |
| Behavior | Behavior match vs FunctionSummary |
| Constraint | SMT check vs extracted ranges (Z3) |
| Interface | Structural match vs extracted signatures |
| DataModel | Structural match vs extracted types |
| NonFunctional | Requires runtime evidence; else Unverifiable |
| Sequence | Protocol mining (Ammons 2002) |
| Prohibition | Path search + KLEE-style counterexample |

### 24.3 Drift statuses

```rust
pub enum DriftStatus {
    Implemented,      // verified within assumptions
    Partial,          // some sub-claims verified
    Missing,          // no matching behavior found
    Contradicting,    // behavior exists, violates claim
    Drifted,          // behavior exists, differs from claim
    Undocumented,     // code exists, no claim (reverse drift)
    Unverifiable,     // needs runtime/env/data
    Unknown,          // insufficient evidence
}
```

**Rules:** Implemented requires evidence. Timeout never becomes Implemented. Inferred mapping never becomes Implemented without verification. Unverifiable must be explicit.

### 24.4 Unresolvable — the handoff

`Unresolvable` is not a computation result; it is a **scope declaration + handoff**. Output:

```rust
pub struct UnresolvableClaim {
    pub claim: ClaimId,
    pub required_evidence: EvidenceClass,
    pub coverage_checked: CoverageScope,       // revisions × envs × docs × authorities
    pub coverage_result: CoverageResult,       // zero of class C
    pub gap_reason: GapReason,                 // closed set
    pub what_would_resolve: ResolveRequirement,
    pub owner: Owner,
    pub next_action: HandoffAction,
    pub scope_correction: Option<ScopeCorrection>,
}
```

**The scope is user-owned and correctable.** Without a declared scope, `Unresolvable` collapses into `Missing` and becomes a dishonest absence claim.

---

# Part VII — Visualization

## 25. ViewSpec contract

```typescript
export interface ViewSpec {
    elements:    ViewElement[];
    edges:       ViewEdge[];
    groups?:     ViewGroup[];
    footer:      EpistemicFooter;
    gaps:        Gap[];
    hidden:      HiddenCandidate[];
    scope:       ScopeAttestation;
    layoutHint?: LayoutHint;
    preferences?: Preferences;
    semantic_hash: string;
    render_hash:   string;
}

export interface ViewElement {
    id:               string;
    elementKind:      ElementKind;
    displayMode:      'Fact' | 'Inference' | 'Hypothesis' | 'Fog';
    label:            string;
    evidence:         EvidenceRef;          // REQUIRED
    claimId?:         string;               // REQUIRED iff displayMode ≠ Fact
    counterArgument?: string;               // REQUIRED iff Hypothesis
    fogReason?:       string;               // REQUIRED iff Fog
    salience:         { factors: string[]; weight: number; whyShown: string };
    fields:           Record<string, unknown>;
}
```

## 26. The gate (single honesty enforcement point)

```typescript
export function gate(spec: ViewSpec): void {
    for (const el of spec.elements) {
        if (!el.evidence) throw new VizError('ELEMENT_WITHOUT_EVIDENCE', el.id);
        if (el.displayMode !== 'Fact' && !el.claimId)
            throw new VizError('NON_FACT_WITHOUT_CLAIM', el.id);
        if (el.displayMode === 'Hypothesis' && !el.counterArgument)
            throw new VizError('HYPOTHESIS_WITHOUT_COUNTER', el.id);
        if (el.displayMode === 'Fog' && !el.fogReason)
            throw new VizError('FOG_WITHOUT_REASON', el.id);
    }
    for (const e of spec.edges) {
        if (!e.evidence) throw new VizError('EDGE_WITHOUT_EVIDENCE', e.id);
    }
}
```

**`gate()` is called by every adapter's `adapt`.** There is no render path that bypasses it.

## 27. EngineAdapter interface

```typescript
export interface EngineAdapter {
    readonly id: EngineId;
    readonly capabilities: CapabilitySet;
    readonly limits: EngineLimits;
    gate(spec: ViewSpec): void;
    adapt(spec: ViewSpec): unknown;
    configure(spec: ViewSpec, options: RenderOptions): unknown;
    render(container: HTMLElement, spec: ViewSpec, options: RenderOptions): Promise<RenderHandle>;
    relayout(handle: RenderHandle, layout: LayoutHint): Promise<LayoutOutcome>;
    audit(handle: RenderHandle, spec: ViewSpec): AuditReport;
}
```

## 28. Engine capability matrix

| Capability | G6 | JointJS | ReactFlow | Mermaid |
|---|---|---|---|---|
| fisheye | ✅ native | ❌ | ❌ | ❌ |
| customNodes | ✅ | ✅ | ✅ (React) | ❌ |
| customEdges | ✅ | ✅ | ✅ | ❌ |
| customLayouts | ✅ | ✅ | ❌ external | ❌ |
| plugins | ✅ | ✅ | ✅ | ❌ |
| interactive | ✅ | ✅ | ✅ | ❌ |
| combos | ✅ | ✅ | ⚠ parent-nodes | ❌ |
| maxNodes | 10,000 | 5,000 | 2,000 | 500 |
| Layouts | 20+ | directed-graph, spring, force | external Dagre/ELK | dagre, elk |

## 29. Epistemic encoding across engines

| Engine | Fact | Inference | Hypothesis | Fog | Fidelity |
|---|---|---|---|---|---|
| G6 | solid | `[6,4]` | `[2,4]` | hatched | **Full** |
| JointJS | solid | `strokeDasharray` | finer dash | pattern fill | **Full** |
| ReactFlow | solid | dashed CSS | dotted CSS | hatched node | **Full** (custom node) |
| Mermaid | solid | `classDef` dashed | fine dash | grey fill only | **Degraded** |

**Requirement:** Every adapter's `audit()` returns `EncodingFidelity: Full | Degraded | Unsupported`. The UI must show degraded fidelity as a banner. The text outline is authoritative when degraded.

## 30. Fisheye

Only G6 has native fisheye. Configuration is **one plugin entry** — no custom lens geometry, no coordinate transform, no hit-testing:

```typescript
plugins: [
    { type: 'fisheye', key: 'cie-fisheye',
      trigger: 'pointermove',
      d: 1.5, r: 120, maxR: 300, minR: 60, maxD: 5, minD: 1,
      scaleRBy: 'wheel', scaleDBy: 'drag',
      showDPercent: true }
]
```

For engines without fisheye, the adapter declares `fisheye: false` and the negotiator excludes them when fisheye is required. ReactFlow may approximate with contextual zoom, but must label it `FOCUS_APPROXIMATION`, never `FISHEYE`.

## 31. Layout translation

| Engine | Mechanism |
|---|---|
| G6 | Pass `layoutHint.strategy` verbatim to `layout.type` |
| Mermaid | `dagre` or `elk` only; substitute otherwise and report |
| ReactFlow | Run Dagre or ELK **before** rendering |
| JointJS | Map to nearest JointJS layout; report substitution |

Every adapter reports `LayoutOutcome { requested, actual, fidelity }`. UI shows `Layout: dagre (substituted from force)`.

## 32. The 76 chart catalog

| Category | Count | G6 Native | G6 Custom | Alt-engine |
|---|---|---|---|---|
| A Structural / UML | 13 | 10 | 3 | 0 |
| B Behavioral / UML | 7 | 5 | 0 | 2 |
| C Architecture | 9 | 9 | 0 | 0 |
| D Data | 6 | 6 | 0 | 0 |
| E Code-level | 10 | 10 | 0 | 0 |
| F Runtime | 7 | 4 | 1 | 2 |
| G Quality | 5 | 5 | 0 | 0 |
| H Decision | 5 | 5 | 0 | 0 |
| I Epistemic | 7 | 7 | 0 | 0 |
| J Journey | 4 | 4 | 0 | 0 |
| K Organizational | 3 | 3 | 0 | 0 |
| **Total** | **76** | **68** | **4** | **4** |

**G6 extension budget for all 76:** 4 custom node types (`uml-class-node`, `actor-icon`, `flame-frame`, `impact-dup-node`), 2 custom edge types (`uml-edge`, `swimlane-edge`), 2 custom layouts (`cie-flame`, `cie-impact-duplicate`), 2 custom plugins (gap overlay, footer overlay).

## 33. The flame graph — worked example of a custom chart

G6 has no flame layout. Built by registering **one custom layout** and **one custom node**:

```typescript
class FlameLayout extends BaseLayout {
    id = 'cie-flame';
    async execute(model) {
        const childrenOf = {};
        for (const e of model.edges) {
            (childrenOf[e.source] ??= []).push(e.target);
        }
        const root = model.nodes.find(n => !model.edges.some(e => e.target === n.id));
        const rootSamples = root.data.fields.samples;
        const visit = (nodeId, depth, xOffset) => {
            const node = model.nodes.find(n => n.id === nodeId);
            const width = (node.data.fields.samples / rootSamples) * 960;
            node.style = { x: xOffset + width / 2, y: depth * 24 + 12, size: [width, 24] };
            let childX = xOffset;
            for (const childId of childrenOf[nodeId] ?? []) {
                const childW = (model.nodes.find(n => n.id === childId).data.fields.samples / rootSamples) * 960;
                visit(childId, depth + 1, childX);
                childX += childW;
            }
        };
        visit(root.id, 0, 0);
    }
}
register(ExtensionCategory.LAYOUT, 'cie-flame', FlameLayout);
```

**This is the proof that the architecture generalizes:** a chart G6 does not ship is implemented in ~60 lines, with no schema change and no adapter change.

## 34. The sequence diagram — IR-based

A sequence diagram is built from `calls` (§11.4), which now has all four sequence-diagram fields populated during lowering:

```sql
SELECT c.stmt_index, c.block_id, c.guard_summary, c.call_kind, c.throws_set,
       s1.name AS caller, s2.name AS callee
FROM calls c
JOIN symbols s1 ON s1.id = c.caller_symbol_id
LEFT JOIN symbols s2 ON s2.id = c.callee_symbol_id
WHERE c.caller_symbol_id IN (:trace_symbols)
ORDER BY c.caller_symbol_id, c.stmt_index;
```

**Why IR, not AST:** the IR's instruction stream is ordered; the CFG's branches are explicit; the SSA temporaries make data flow between calls explicit. The AST's tree structure requires you to re-derive all of that.

**The AST is still needed for enrichment:** `symbols.annotations` gives lifeline badges (`@Async`, `@Transactional`); `file_source_cache` gives hover snippets.

## 35. The flowchart — IR is the answer

A flowchart *is* a control flow graph. The IR *is* a control flow graph.

```sql
-- The complete CFG of function @S in one transaction
SELECT * FROM ir_blocks WHERE symbol_id = :S;
SELECT * FROM ir_instructions WHERE block_id IN (SELECT id FROM ir_blocks WHERE symbol_id = :S);
SELECT * FROM ir_edges WHERE from_block_id IN (SELECT id FROM ir_blocks WHERE symbol_id = :S);
```

Three queries, zero reconstruction. Every block is a box, every edge is an arrow, every instruction carries its byte span for hover.

---

# Part VIII — Native LLM

## 36. Provider abstraction

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

## 37. Integration points

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

## 38. Safety

- Content-addressed cache by `blake3(prompt + model + params)`
- Revision-scoped; invalidated on model change
- Hard budget `budget_usd`; downgrade to local or disable on exceed
- All prompts/responses recorded as `Evidence` with `generated_at_ns`
- LLM never writes `Contract.authority = Explicit` or `Verified`
- Labels marked `Label { source: Llm, model_id, prompt_hash }`
- Disabling LLM: engine runs fully; deterministic fallbacks apply
- **Default provider: Ollama** — offline-first

---

# Part IX — Language support

## 39. Top-10 languages

| # | Language | Parser | Framework detection |
|---|---|---|---|
| 1 | Python | `tree-sitter-python` | Django, FastAPI, Flask |
| 2 | JavaScript | `tree-sitter-javascript` | Express, Koa |
| 3 | TypeScript | `tree-sitter-typescript` | Express, NestJS, Next.js |
| 4 | Java | `tree-sitter-java` | Spring Boot |
| 5 | C | `tree-sitter-c` | — (no framework detection in v1) |
| 6 | C++ | `tree-sitter-cpp` | — |
| 7 | C# | `tree-sitter-c-sharp` | ASP.NET Core |
| 8 | Go | `tree-sitter-go` | Gin, Echo, Chi |
| 9 | Rust | `tree-sitter-rust` | Actix, Axum, Rocket |
| 10 | PHP | `tree-sitter-php` | Laravel, Symfony |

**Optional v1.1:** Ruby, Kotlin, Swift, Scala, SQL (embedded DSL).

## 40. Adapter trait

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

## 41. Extension path

Adding a language requires:
1. Add `tree-sitter-<lang>` dependency
2. Implement `LanguageAdapter`
3. Register in `LanguageRegistry` (one line)

**Acceptance:** ≤ 1 crate, ≤ 1 registration line, 0 core changes.

## 42. Polyglot specifics

The AST schema is polyglot by construction:
- `files.language` selects grammar
- `node_kinds.language` namespaces grammar kinds
- `node_text` is content-addressed (shared across languages)
- `uast_map` normalizes grammar kinds → UAST kinds

Cross-language resolution uses three tables:
- `uast_map` (grammar → UAST)
- `framework_adapters` (framework → role)
- `cross_language_bindings` (three-tier: DECLARED / URL_MATCHED / RUNTIME_OBSERVED)

---

# Part X — Capability domains

17 domains, each a vertical slice in L5:

| # | Domain | Key capabilities |
|---|---|---|
| 1 | Architecture & Design | Layering, cycles, coupling, dead code, orphans, semantic diff, architecture drift |
| 2 | Security | Taint, injection, secrets, authZ bypass, SBOM, SARIF export |
| 3 | Testing & Quality | Coverage gaps, mutation targeting, counterexample → test, min test set |
| 4 | Performance | Complexity, N+1, unbounded loops, memory leaks, hot paths |
| 5 | Refactoring & Migration | Safe rename, extract, breaking change, framework migration, cross-language |
| 6 | Documentation | Auto-generated architecture docs, API docs, diagrams, onboarding |
| 7 | Compliance | License, PII, policy, SBOM, schema-vs-impl drift |
| 8 | Runtime & Observability | Trace-to-code, incident replay, SLO impact, runtime drift |
| 9 | DevOps & Operations | Deployment impact, rollback safety, config mapping, service graph |
| 10 | Developer Productivity | Blast radius, PR risk, semantic search, review bot, ticket-to-code |
| 11 | AI-Native | NL query, behavioral diff, grounded review, code generation with verification |
| 12 | Reverse Engineering | Bounded contexts, monolith decomposition, shared library extraction |
| 13 | Formal Assurance | Contract proofs, invariants, termination, concurrency, equivalence |
| 14 | Business & Product | Feature inventory, reachability, tier analysis, IP boundary |
| 15 | Archaeology | When did this change, who introduced, regression attribution |
| 16 | Multi-Repo | Cross-repo concepts, org catalog, API matrix, library usage |
| 17 | Combinatorial | Semantic diff, blast radius, min test set, incident replay, org-wide graph |

---

# Part XI — Runtime behavior (complementary tools)

## 43. Open-source runtime tools

| Category | Tools | Purpose |
|---|---|---|
| **Tracing & APM** | OpenTelemetry Java Agent, Spring Boot Actuator + Micrometer, Jaeger, Zipkin, SigNoz, Grafana Tempo | Request flows, service dependencies |
| **Profiling** | async-profiler, Java Flight Recorder (JFR), Glowroot | CPU/memory bottlenecks |
| **Dynamic debugging** | Arthas, BTrace | Real-time method inspection |
| **Specialized** | Kieker, ExplorViz | Architectural runtime analysis |

## 44. ExplorViz as a reference

ExplorViz's architecture is the blueprint for the runtime evidence layer:

```text
Instrumented app (OTel agent)
  → OTel Collector
  → Adapter Service (validates, decomposes into structural + dynamic)
  → Landscape Service (rebuilds static tree)
  → Trace Service (aggregates dynamic traces)
  → Cassandra DB
  → Frontend (3D city visualization)
```

**Key takeaway for CIE:** the structural/dynamic data split is essential for storage efficiency. CIE's `runtime_evidence` table (§45) applies the same decomposition.

## 45. Runtime evidence table

```sql
CREATE TABLE runtime_evidence (
    id           INTEGER PRIMARY KEY,
    symbol_id    INTEGER NOT NULL REFERENCES symbols(id),
    trace_id     BLOB NOT NULL,
    span_id      BLOB NOT NULL,
    parent_span  BLOB,
    duration_ns  INTEGER NOT NULL,
    status       TEXT NOT NULL,      -- 'OK' | 'ERROR' | 'UNSET'
    observed_ns  INTEGER NOT NULL,
    attrs        BLOB                -- serialized key-value map
);
CREATE INDEX idx_rt_symbol ON runtime_evidence(symbol_id, observed_ns);
CREATE INDEX idx_rt_trace  ON runtime_evidence(trace_id);
```

---

# Part XII — Implementation

## 46. Stages

| Stage | Deliverable | Acceptance |
|---|---|---|
| 1 | Behavior extraction | Print-only stubs, unconditional success, ignored failure, delegating wrappers |
| 2 | Behavioral conformance | Established vs inferred violations |
| 3 | Rules & concepts (FCA) | Traceable rules; overlapping concepts with LLM labels |
| 3b | Hierarchical concept behavior | Every ConceptNode has label, parents/children, behaviors, rules, symbols; rollup correct; **no file path nodes** |
| 4 | Structural hierarchy | Evidence-backed module/domain candidates; every edge traceable |
| 5 | Recovery & fault injection | Reproduce failed refunds, interrupted recovery, duplicate processing |
| 6 | Drift analysis | Per-claim status with evidence |
| 7 | Multi-language expansion | All 10 adapters; new language ≤ 1 crate |
| 8 | Native LLM | Offline-first; cache; budget |
| 9 | Folder assimilation | `cie ingest` end-to-end; incremental ≤ 5% |
| 10 | Runtime ingestion | OTel traces mapped to summaries |
| 11 | Multi-repo ingestion | Merged concept graph; API matrix |
| 12 | Capability layer buildout | 17 domains; combinatorial |
| 13 | Visualization — G6 | ViewSpec + G6 adapter + 68 native charts |
| 14 | Visualization — multi-engine | JointJS, ReactFlow, Mermaid adapters |
| 15 | Visualization — custom charts | Flame graph, impact/dup, UML class |
| 16 | Performance hardening | All latency targets met |

## 47. Slices

| Slice | Scope | Outcome |
|---|---|---|
| 0 | Foundation + first behavior extractor | Minimum schema; effects, evidence, coverage |
| 1 | Behavioral conformance | Fake auth, ignored rejection, swallowed exceptions |
| 2 | Rules & concepts | Predicate extraction; FCA |
| 2b | Hierarchical concept behavior | ConceptNode DAG (corrected — no file nodes) |
| 3 | Structural hierarchy | Clustering vs reviewed examples |
| 4 | Recovery execution | State models; targeted fault injection |
| 5 | Drift | Claim extraction, mapping, verification, report |
| 6 | Multi-language | All 10 adapters; extension path validated |
| 7 | Native LLM | Provider trait; cache; budget; offline |
| 8 | Folder assimilation | Full ingest; incremental |
| 9 | SQLite schema — AST | ast_nodes, node_kinds, node_text, symbols |
| 10 | SQLite schema — IR | ir_blocks, ir_instructions, ir_edges, calls with all 6 fields |
| 11 | UAST normalization | uast_kinds, uast_map; polyglot queries |
| 12 | C4 detection | Containers + components from manifests + framework adapters |
| 13 | State machine detection | T1/T2/T3 tiers with epistemic encoding |
| 14 | Workflow detection | Entry-point forward DFS |
| 15 | Impact radius + duplicates | Detection + ViewSpec |
| 16 | ViewSpec + G6 adapter | gate() + adapt() + graphOptions() |
| 17 | Fisheye | Built-in plugin enabled by config |
| 18 | JointJS / ReactFlow / Mermaid adapters | Capability declared; negotiation |
| 19 | Custom G6 extensions | uml-class-node, flame layout, impact-dup |
| 20 | Runtime ingestion | OTel + ExplorViz-style decomposition |
| 21 | Multi-repo | Merged concept graph |
| 22 | Performance | All latency targets met |

**Incremental correctness belongs in every slice.**

## 48. Acceptance suite

### Foundation (1–7)

1. Print-only authorization detected
2. Unconditional success detected
3. Legitimate delegation not flagged
4. Rejected authorization ignored by caller detected
5. Exceptions converted to success detected
6. Unresolved external behavior marked
7. Code deletion invalidates earlier finding

### Ingestion & language (8–11)

8. Polyglot repo (Python + Rust + Go + TS) ingested end-to-end
9. New language (Ruby) added in ≤ 1 crate + ≤ 1 registration line
10. Unchanged folder re-ingest ≤ 5% of first-ingest wall time
11. Time-travel query returns correct results for rev N and N-1

### LLM (12–14)

12. Ollama offline run produces labels with no network access
13. LLM cache hit ≤ 100 ns
14. LLM budget exceeded → downgrade, continue

### Storage (15–16)

15. Symbol lookup p99 ≤ 50 ns; CSR edge scan p99 ≤ 20 ns; summary get p99 ≤ 5 µs
16. Subtree retrieval ≤ 5 µs; CFG query ≤ 20 µs

### Drift (17–26)

17. PRD with 50 claims → ≥ 90% non-Unknown
18. "Must never double-charge" → Contradicting with counterexample
19. Claim with no matching code → Missing with scope evidence
20. p99 latency claim → Unverifiable
21. Code without doc claim → reverse_drift
22. Two docs conflicting → Contradicting citing both
23. `--from-rev 41 --to-rev 42` shows only changed statuses
24. Unchanged drift re-run ≤ 5% of first run
25. LLM disabled → ≥ 60% claims extracted
26. Timeout → Unknown, never Implemented

### Hierarchical concept behavior (27–31)

27. `cie concepts --with-behaviors` emits nested tree with concepts, behaviors, rules
28. **No file path appears as a ConceptNode** — files are only evidence
29. Rollup at parent = union of descendants
30. Concept under multiple parents (DAG); each edge traceable
31. Removing symbol invalidates only referencing nodes

### Runtime (32–36)

32. OTel span mapped to SymbolId or Unresolved
33. Observed behavior differs from static → runtime drift reported
34. Hot paths ranked by span frequency
35. Incident replay: trace + contracts → root-cause candidates
36. Error stack trace → summaries + violated contracts

### Multi-repo (37–40)

37. Two repos with same library → one merged concept, two sources
38. Breaking API change → Contradicting
39. Shared subgraph detected as extraction candidate
40. Org-wide concept graph emitted

### Visualization (41–60)

41. `gate()` rejects unevidenced elements before any engine runs
42. `semantic_hash` unchanged across layout and engine swaps
43. `render_hash` changes across layout swap
44. Negotiator is a pure function
45. User preference honoured when eligible
46. User preference rejected with disclosed reason when ineligible
47. `nodes > 2000` excludes Mermaid and ReactFlow
48. `fisheye: true` restricts to G6
49. CIE output contains no engine id
50. G6 renders UML class diagram
51. G6 renders flame graph via custom layout + node
52. Mermaid renders sequence diagram
53. JointJS renders architecture with full encoding fidelity
54. ReactFlow renders hypothesis graph with dashed borders
55. Same ViewSpec rendered by two engines has identical element ids
56. Layout substitution is disclosed
57. Encoding fidelity is `Degraded` for Mermaid and surfaced
58. Text outline is authoritative when fidelity is degraded
59. Fisheye works on all G6 layouts
60. Adding a new chart requires no adapter change

### Capability spot checks (61–70)

61. Layering violation detected
62. Taint from `req.query` to SQL execution detected
63. N+1 query inside loop detected
64. Dead code with no callers, claims, or tests detected
65. Blast radius of a change lists affected summaries, concepts, claims
66. Minimum test set for a change computed
67. Architecture docs generated with mermaid diagrams
68. PR risk score correlates with actual regressions
69. Monolith decomposition candidates ranked by coupling
70. Contract proof pipeline produces SMT-verified obligation

### State machines & workflows (71–76)

71. XState `createMachine` extracted as Fact
72. Spring `StateMachineBuilder` extracted as Fact
73. Enum + switch extracted as Inference with partial-coverage gap
74. Call-graph workflow extracted as Hypothesis with counter-argument
75. Async boundary crossing marked as Fog with reason
76. State machine and enclosing workflow appear in the same ViewSpec

### Polyglot (77–80)

77. `uast_map` maps Python `function_definition` and Rust `function_item` to `FunctionDecl`
78. Cross-language binding DECLARED from OpenAPI has `confidence = 1.0`
79. Cross-language binding URL_MATCHED has `confidence = 0.7`
80. Framework adapters detect routes in Spring Boot, FastAPI, and Actix

---

# Part XIII — Appendix

## 49. Risks and mitigations

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
| R-11 | **File paths become concepts** | Classes link to C4 components, never to file nodes |
| R-12 | **Polyglot undercounting** | `framework_adapters` covers every language, not just JVM |
| R-13 | **Sequence diagrams need IR reconstruction** | The four `calls` additions (`block_id`, `call_kind`, `throws_set`, `guard_summary`) |
| R-14 | **Multi-engine fidelity loss** | `EncodingFidelity` declared per adapter; text outline authoritative |
| R-15 | **Lens geometry in adapter** | Fisheye is a G6 plugin, not custom code |

## 50. Open questions

- Which persistence backend: SQLite (v1) vs. Postgres (v2)?
- Which SMT integration: `z3` crate or external process?
- Which symbolic execution approach: KLEE FFI or bounded Rust implementation?
- OTel version: OTLP/gRPC, OTLP/HTTP, or both?
- Multi-repo identity: which merge signals dominate when they conflict?
- Minimum reviewed hierarchy set for Slice 3?
- Minimum acceptable claim extraction recall without LLM?

## 51. Limitations

| Not achievable | Why |
|---|---|
| Ground truth for vague requirements | No operational definition |
| Complete dynamic analysis without execution | Static only; runtime needs OTel |
| Sound security analysis without heap modeling | Bi-abduction is approximate |
| Correct concurrency analysis without happens-before model | Needs memory-model adapter |
| Business value estimation | Not derivable from code or docs |
| Predicting bugs in code never executed | Requires execution evidence |
| Perfect drift for runtime-dependent claims | Marked Unverifiable by design |
| Human intent | Hypothesis only |
| Merge of two independent codebases without shared identity | Best-effort with confidence |

## 52. Traceability matrix

| Source | PRD | Paper |
|---|---|---|
| Extract what code does | §17 | PDG, IFDS, Abstract Interp, Bi-Abduction |
| Discover expected behavior | §24 | Naming, Beliefs, Protocols, Houdini |
| Check rules & counterexamples | §24 | CEGAR, SLAM, Z3, KLEE |
| Derive concepts & hierarchy | §21–22 | FCA, Incremental FCA, Peixoto, Andritsos |
| Cross-service recovery | §19 | Yuan, LDFI, IronFleet |
| Incremental performance | §7 | Differential Dataflow |
| Top-10 languages | §39 | Tree-sitter |
| Easy extension | §40–41 | Tree-sitter |
| Folder assimilation | §7 | — |
| Nanosecond time | §12 | ntime |
| Time travel | §12 | — |
| Native LLM | §36–38 | LLM code analysis |
| Document ingestion | §8 | Doc-code drift review |
| Claim extraction / mapping | §24 | Requirements traceability |
| Per-claim verification | §24 | — |
| Drift statuses | §24 | — |
| Reverse drift | §24 | — |
| Multi-doc reconciliation | §24 | — |
| Hierarchical concept behavior | §22 | FCA + clustering |
| **AST schema** | §11 | — |
| **UAST normalization** | §11.2, §14 | — |
| **IR lowering** | §15 | SSA construction, rustc HIR/MIR |
| **C4 detection** | §20 | — |
| **State machine detection** | §19 | XState, Spring Statemachine, starchainx |
| **Workflow detection** | §19 | bpmn-skill, Temporal |
| **Impact radius + duplicates** | §23 | IR hashing, WL subtree kernel |
| **ViewSpec contract** | §25 | — |
| **Multi-engine** | §27–32 | G6, JointJS, ReactFlow, Mermaid |
| **Fisheye** | §30 | G6 fisheye plugin |
| **Flame graph** | §33 | G6 custom layout |
| **Sequence diagram** | §34 | IR + calls table |
| **Flowchart** | §35 | IR is CFG |
| **Runtime tools** | §43 | OTel, Arthas, async-profiler, ExplorViz |

## 53. One-sentence summary

**A polyglot, evidence-first code intelligence engine that ingests folders, documents, runtime traces, and multi-repo graphs into a SQLite-backed AST/UAST/IR model; produces a corrected hierarchical ConceptNode DAG (no file path nodes) with behavior rollup, C4 containers and components, state machines and workflows, impact/duplicate analysis, and drift reports with eight honest statuses; emits a semantic ViewSpec whose honesty gate prevents unevidenced elements from ever reaching a renderer; negotiates among G6, JointJS, ReactFlow, and Mermaid by capability → preference → size → order, with fisheye as a G6 configuration entry, custom layouts as registrations, and 76 chart types bounded only by what the extension systems can draw — never promoting a timeout to a defect, an inference to a fact, or a file path to a concept.**